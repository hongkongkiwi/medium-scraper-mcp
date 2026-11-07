use super::parse::{self, Article, ArticleMeta};
use crate::config::Config;
use crate::extract::is_blocked;
use crate::http::HttpFetcher;
use crate::md::MdOptions;
use anyhow::{anyhow, Result};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKind {
    Freedium,
    Readmedium,
    Archive,
    Auto,
}

impl ProxyKind {
    pub fn parse(s: &str) -> Self {
        match s {
            "freedium" => Self::Freedium,
            "readmedium" => Self::Readmedium,
            "archive" => Self::Archive,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReadOptions {
    pub include_images: bool,
    pub include_code: bool,
    pub bypass_paywall: bool,
    pub preferred_proxy: ProxyKind,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            include_images: true,
            include_code: true,
            bypass_paywall: false,
            preferred_proxy: ProxyKind::Auto,
        }
    }
}

pub struct MediumFetcher<'a> {
    pub http: &'a HttpFetcher,
    pub cfg: &'a Config,
}

const PROXY_TIMEOUT: Duration = Duration::from_secs(15);
/// Minimum body words before a proxy result is trusted as the full article.
const MIN_PROXY_WORDS: usize = 200;

impl<'a> MediumFetcher<'a> {
    pub async fn read(&self, url: &str, opts: ReadOptions) -> Result<Article> {
        let fetched = self.http.get(url, None).await?;
        if fetched.status == 404 || fetched.status == 410 {
            return Err(anyhow!("article not found (HTTP {}): {url}", fetched.status));
        }
        let md_opts = MdOptions {
            include_images: opts.include_images,
            include_code: opts.include_code,
        };
        let (meta, body, paywalled) = parse::parse_article(&fetched.body, &fetched.final_url, md_opts);
        let direct_words = body.split_whitespace().count();

        let direct_usable = !paywalled && direct_words >= 30 && !is_blocked(&fetched.body);
        if direct_usable {
            return Ok(Article {
                meta,
                body_markdown: body,
                via: "direct".to_string(),
                paywalled: false,
            });
        }

        if opts.bypass_paywall {
            let chain: Vec<ProxyKind> = match opts.preferred_proxy {
                ProxyKind::Auto => vec![
                    ProxyKind::Freedium,
                    ProxyKind::Readmedium,
                    ProxyKind::Archive,
                ],
                other => vec![other],
            };
            for kind in chain {
                if let Some(article) = self.try_proxy(kind, &fetched.final_url, md_opts).await {
                    return Ok(article);
                }
            }
            // proxies failed: return the direct preview marked paywalled rather than nothing
            if direct_words >= 30 {
                return Ok(Article {
                    meta,
                    body_markdown: body,
                    via: "direct".to_string(),
                    paywalled: true,
                });
            }
            return Err(anyhow!(
                "could not retrieve full content for {url}: paywall bypass failed for all proxies and the direct fetch returned only a preview"
            ));
        }

        if direct_words >= 30 {
            return Ok(Article {
                meta,
                body_markdown: body,
                via: "direct".to_string(),
                paywalled,
            });
        }
        Err(anyhow!(
            "could not extract article content from {url} (HTTP {}); enable bypass_paywall for member-only posts",
            fetched.status
        ))
    }

    async fn try_proxy(
        &self,
        kind: ProxyKind,
        url: &str,
        md_opts: MdOptions,
    ) -> Option<Article> {
        let proxy_url = match kind {
            ProxyKind::Freedium => format!("https://freedium.cfd/{url}"),
            ProxyKind::Readmedium => {
                let slug = parse::slug(url).unwrap_or_default();
                format!("https://readmedium.com/{slug}")
            }
            ProxyKind::Archive => format!("https://archive.ph/newest/{url}"),
            ProxyKind::Auto => return None,
        };
        let fetched = match self.http.get(&proxy_url, Some(PROXY_TIMEOUT)).await {
            Ok(f) if f.status == 200 => f,
            Ok(f) => {
                tracing::warn!(proxy = ?kind, status = f.status, "proxy returned non-200");
                return None;
            }
            Err(e) => {
                tracing::warn!(proxy = ?kind, error = %e, "proxy fetch failed");
                return None;
            }
        };
        if is_blocked(&fetched.body) {
            return None;
        }
        let (meta, body, _pw) = parse::parse_article(&fetched.body, url, md_opts);
        let words = body.split_whitespace().count();
        if words < MIN_PROXY_WORDS {
            return None;
        }
        let via = match kind {
            ProxyKind::Freedium => "freedium",
            ProxyKind::Readmedium => "readmedium",
            ProxyKind::Archive => "archive",
            ProxyKind::Auto => "proxy",
        };
        Some(Article {
            meta: fill_missing(meta, url),
            body_markdown: body,
            via: via.to_string(),
            paywalled: false,
        })
    }
}

/// Proxy pages re-render the article; keep the original URL metadata stable.
fn fill_missing(mut meta: ArticleMeta, original_url: &str) -> ArticleMeta {
    if meta.url.is_empty() || meta.url != original_url {
        meta.url = original_url.to_string();
    }
    if meta.post_id.is_none() {
        meta.post_id = parse::slug_post_id(original_url);
    }
    meta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_urls_match_original_scheme() {
        // freedium takes the full URL; readmedium the slug; archive the full URL
        let url = "https://medium.com/@u/some-title-a1b2c3d4e5f6";
        assert_eq!(
            format!("https://freedium.cfd/{url}"),
            "https://freedium.cfd/https://medium.com/@u/some-title-a1b2c3d4e5f6"
        );
        assert_eq!(parse::slug(url).unwrap(), "some-title-a1b2c3d4e5f6");
    }

    #[test]
    fn proxy_kind_parse() {
        assert_eq!(ProxyKind::parse("freedium"), ProxyKind::Freedium);
        assert_eq!(ProxyKind::parse("archive"), ProxyKind::Archive);
        assert_eq!(ProxyKind::parse("auto"), ProxyKind::Auto);
        assert_eq!(ProxyKind::parse("garbage"), ProxyKind::Auto);
    }
}
