use super::fetch::{MediumFetcher, ReadOptions};
use super::parse::{self, with_frontmatter};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, serde::Serialize)]
pub struct DownloadReport {
    pub markdown_path: String,
    pub assets_dir: String,
    pub images_downloaded: usize,
    pub images_failed: usize,
    pub skipped_unchanged: bool,
}

pub struct Downloader<'a> {
    pub fetcher: MediumFetcher<'a>,
}

const MAX_IMAGES: usize = 60;

impl<'a> Downloader<'a> {
    /// Save article markdown + images under `<output_dir>/Output/medium-scraper-mcp/`.
    /// Re-runs with unchanged content are skipped.
    pub async fn download(
        &self,
        url: &str,
        opts: ReadOptions,
        output_dir: &Path,
    ) -> Result<DownloadReport> {
        let article = self.fetcher.read(url, opts).await?;
        let base = output_dir.join("Output").join("medium-scraper-mcp");
        tokio::fs::create_dir_all(&base)
            .await
            .context("create output directory")?;

        let date = article
            .meta
            .published
            .as_deref()
            .and_then(|d| d.get(0..10))
            .map(|d| d.replace('-', ""))
            .unwrap_or_else(|| chrono::Utc::now().format("%Y%m%d").to_string());
        let slug = parse::slug(url)
            .or_else(|| parse::slug_post_id(url).map(|id| format!("p-{id}")))
            .unwrap_or_else(|| "article".to_string());
        let md_path = base.join(format!("{date}-{slug}.md"));

        let mut markdown = with_frontmatter(&article);

        // download images and rewrite references
        let post_id = article
            .meta
            .post_id
            .clone()
            .unwrap_or_else(|| "no-post-id".to_string());
        let assets_dir = base.join("assets").join(&post_id);
        let (downloaded, failed, markdown2) =
            self.fetch_images(&markdown, &assets_dir, &base).await;
        markdown = markdown2;

        // skip-if-unchanged
        if let Ok(existing) = tokio::fs::read_to_string(&md_path).await {
            if existing == markdown {
                return Ok(DownloadReport {
                    markdown_path: md_path.display().to_string(),
                    assets_dir: assets_dir.display().to_string(),
                    images_downloaded: 0,
                    images_failed: 0,
                    skipped_unchanged: true,
                });
            }
        }
        tokio::fs::write(&md_path, &markdown)
            .await
            .context("write markdown file")?;

        Ok(DownloadReport {
            markdown_path: md_path.display().to_string(),
            assets_dir: assets_dir.display().to_string(),
            images_downloaded: downloaded,
            images_failed: failed,
            skipped_unchanged: false,
        })
    }

    async fn fetch_images(
        &self,
        markdown: &str,
        assets_dir: &Path,
        base: &Path,
    ) -> (usize, usize, String) {
        let re = img_re();
        let mut found: Vec<(String, String)> = Vec::new();
        for cap in re.captures_iter(markdown) {
            let whole = cap.get(0).map(|m| m.as_str().to_string());
            let url = cap.get(2).map(|m| m.as_str().to_string());
            if let (Some(whole), Some(url)) = (whole, url) {
                if url.starts_with("http") && !found.iter().any(|(_, u)| *u == url) {
                    found.push((whole, url));
                }
            }
            if found.len() >= MAX_IMAGES {
                break;
            }
        }
        if found.is_empty() {
            return (0, 0, markdown.to_string());
        }
        if tokio::fs::create_dir_all(assets_dir).await.is_err() {
            return (0, found.len(), markdown.to_string());
        }
        let mut downloaded = 0;
        let mut failed = 0;
        let mut out = markdown.to_string();
        for (whole, url) in &found {
            let ext = url
                .split('?')
                .next()
                .and_then(|p| p.rsplit('.').next())
                .filter(|e| matches!(*e, "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif"))
                .unwrap_or("png");
            let fname = format!("{:016x}.{}", fnv1a(url), ext);
            match self.fetcher.http.get_bytes(url).await {
                Ok(bytes) if !bytes.is_empty() => {
                    let path = assets_dir.join(&fname);
                    if tokio::fs::write(&path, &bytes).await.is_ok() {
                        let rel = path
                            .strip_prefix(base)
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|_| fname.clone());
                        out = out.replace(
                            whole.as_str(),
                            &whole.replace(url.as_str(), &format!("../{rel}")),
                        );
                        downloaded += 1;
                    } else {
                        failed += 1;
                    }
                }
                _ => {
                    failed += 1;
                }
            }
        }
        (downloaded, failed, out)
    }
}

fn img_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(!\[[^\]]*\])\((https?://[^)\s]+)\)").unwrap())
}

fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn resolve_output_dir(explicit: Option<&str>) -> Result<PathBuf> {
    match explicit {
        Some(dir) => {
            let expanded = if let Some(rest) = dir.strip_prefix("~/") {
                dirs::home_dir()
                    .ok_or_else(|| anyhow!("cannot resolve home directory"))?
                    .join(rest)
            } else {
                PathBuf::from(dir)
            };
            Ok(expanded)
        }
        None => std::env::current_dir().context("no working directory for output"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_regex_finds_urls() {
        let md = "text\n![alt](https://miro.medium.com/max/700/a.png)\nmore\n![x](https://b.com/c.jpg)";
        let re = img_re();
        let urls: Vec<String> = re
            .captures_iter(md)
            .filter_map(|c| c.get(2).map(|m| m.as_str().to_string()))
            .collect();
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://miro.medium.com/max/700/a.png");
    }

    #[test]
    fn fnv1a_stable() {
        assert_eq!(fnv1a("abc"), fnv1a("abc"));
        assert_ne!(fnv1a("abc"), fnv1a("abd"));
    }
}
