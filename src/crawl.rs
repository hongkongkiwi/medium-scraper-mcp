use crate::browser;
use crate::config::Config;
use crate::extract::{self, is_blocked};
use crate::http::HttpFetcher;
use crate::llm;
use crate::md::{html_to_markdown, MdOptions};
use crate::video;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Markdown,
    Html,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Self {
        match s {
            "html" => Self::Html,
            _ => Self::Markdown,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FetchOptions {
    pub format: OutputFormat,
    pub max_chars: usize,
    pub use_llm: bool,
    pub llm_instruction: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct FetchResult {
    pub url: String,
    pub final_url: String,
    pub title: Option<String>,
    pub content: String,
    pub content_format: String,
    pub links: Vec<(String, String)>,
    pub extracted_at: String,
    pub blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_used: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_metadata: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct Crawler {
    pub cfg: Arc<Config>,
    pub http: Arc<HttpFetcher>,
}

impl Crawler {
    /// Fetch many URLs with bounded concurrency and per-request jitter.
    pub async fn fetch_many(
        &self,
        urls: &[String],
        opts: FetchOptions,
        concurrency: usize,
    ) -> Vec<FetchResult> {
        let sem = Arc::new(Semaphore::new(concurrency.clamp(1, 10)));
        let mut handles = Vec::new();
        for url in urls {
            let permit = sem.clone().acquire_owned().await.ok();
            let this = self.clone();
            let url = url.clone();
            handles.push(tokio::spawn(async move {
                let _permit = permit;
                let jitter = 50 + rand::random::<u64>() % 250;
                tokio::time::sleep(std::time::Duration::from_millis(jitter)).await;
                this.fetch_one(&url, opts).await
            }));
        }
        let mut out = Vec::new();
        for h in handles {
            out.push(h.await.unwrap_or_else(|e| error_result("join error", &e.to_string())));
        }
        out
    }

    pub async fn fetch_one(&self, url: &str, opts: FetchOptions) -> FetchResult {
        if video::is_supported_video_url(url) {
            return self.fetch_video(url).await;
        }
        match self.fetch_page(url, opts).await {
            Ok(mut result) => {
                if opts.use_llm {
                    let content_for_llm = result.content.clone();
                    self.apply_llm(&mut result, &content_for_llm, opts).await;
                }
                truncate_result(&mut result, opts.max_chars);
                result
            }
            Err(e) => error_result(url, &e.to_string()),
        }
    }

    async fn fetch_video(&self, url: &str) -> FetchResult {
        match video::transcript(&self.cfg, &self.http, url).await {
            Ok(t) => {
                let video_metadata = serde_json::json!({
                    "title": t.title,
                    "uploader": t.uploader,
                    "duration_seconds": t.duration_secs,
                    "track_lang": t.track_lang,
                });
                FetchResult {
                    url: url.to_string(),
                    final_url: url.to_string(),
                    title: t.title,
                    content: t.transcript_markdown,
                    content_format: "markdown".into(),
                    links: Vec::new(),
                    extracted_at: now_iso(),
                    blocked: false,
                    error: None,
                    llm_used: None,
                    llm_error: None,
                    video_metadata: Some(video_metadata),
                }
            }
            Err(e) => {
                // fall back to plain page fetch when transcript extraction fails
                let mut result = self
                    .fetch_page_plain(url, FetchOptions {
                        format: OutputFormat::Markdown,
                        max_chars: self.cfg.max_content_chars,
                        use_llm: false,
                        llm_instruction: None,
                    })
                    .await
                    .unwrap_or_else(|page_err| error_result(url, &page_err.to_string()));
                result.error = Some(format!("transcript extraction failed: {e}"));
                result
            }
        }
    }

    /// Fast HTTP path with browser escalation for blocked/thin pages.
    async fn fetch_page(&self, url: &str, opts: FetchOptions) -> anyhow::Result<FetchResult> {
        let mut result = self.fetch_page_plain(url, opts).await?;
        let text_thin = opts.format == OutputFormat::Markdown
            && result.content.split_whitespace().count() < 60;
        if (result.blocked || text_thin) && self.cfg.use_browser_fallback {
            if let Ok(rendered) = browser::render(&self.cfg, url).await {
                if !is_blocked(&rendered.html) {
                    let new_result = build_result(url, &rendered.final_url, &rendered.html, opts);
                    if !new_result.blocked {
                        result = new_result;
                    }
                }
            }
        }
        Ok(result)
    }

    async fn fetch_page_plain(&self, url: &str, opts: FetchOptions) -> anyhow::Result<FetchResult> {
        let fetched = self.http.get(url, None).await?;
        if fetched.status >= 400 {
            return Err(anyhow::anyhow!("HTTP {} for {url}", fetched.status));
        }
        Ok(build_result(url, &fetched.final_url, &fetched.body, opts))
    }

    async fn apply_llm(&self, result: &mut FetchResult, content: &str, opts: FetchOptions) {
        let Some(openai) = &self.cfg.openai else {
            result.llm_used = Some(false);
            result.llm_error = Some(
                "OpenAI-compatible config missing (OPENAI_BASE_URL/OPENAI_API_KEY/OPENAI_MODEL)"
                    .into(),
            );
            return;
        };
        match llm::cleanup(
            openai,
            result.final_url.as_str(),
            result.title.as_deref(),
            content,
            opts.llm_instruction,
        )
        .await
        {
            Ok(cleaned) => {
                result.llm_used = Some(true);
                if let Some(t) = cleaned.title {
                    result.title = Some(t);
                }
                result.content = cleaned.markdown;
                result.content_format = "markdown".into();
            }
            Err(e) => {
                result.llm_used = Some(false);
                result.llm_error = Some(e.to_string());
            }
        }
    }
}

fn build_result(url: &str, final_url: &str, html: &str, opts: FetchOptions) -> FetchResult {
    let blocked = is_blocked(html);
    let doc = scraper::Html::parse_document(html);
    let mut meta = extract::parse_meta(&doc);
    extract::parse_jsonld(&doc, &mut meta);
    let links = extract::extract_links(&doc);

    let (content, content_format) = match opts.format {
        OutputFormat::Html => (html.to_string(), "html".to_string()),
        OutputFormat::Markdown => {
            let body_root = extract::select_main(&doc);
            let md = match body_root {
                Some(root) => html_to_markdown(root, MdOptions::default()),
                None => html_to_markdown(doc.root_element(), MdOptions::default()),
            };
            (cleanup_markdown(&md, final_url), "markdown".to_string())
        }
    };

    let title = meta.title.clone().or_else(|| extract::extract_title(&doc));
    FetchResult {
        url: url.to_string(),
        final_url: final_url.to_string(),
        title,
        content,
        content_format,
        links,
        extracted_at: now_iso(),
        blocked,
        error: None,
        llm_used: None,
        llm_error: None,
        video_metadata: None,
    }
}

/// Port of crawl4ai-mcp's markdown post-processing: strip Medium chrome,
/// data-image placeholders, squeeze blanks, ensure a title h1.
pub fn cleanup_markdown(md: &str, source_url: &str) -> String {
    let is_medium = source_url.contains("medium.com");
    let mut out_lines: Vec<String> = Vec::new();
    for line in md.lines() {
        let s = line.trim();
        if is_medium
            && (matches!(
                s,
                "Follow" | "Subscribe" | "Listen" | "Share" | "Remember me for faster sign in"
            ) || s.contains("/m/signin")
                || s.starts_with("Join Medium")
                || (s.starts_with("## Get ") && s.contains("stories in your inbox"))
                || s.contains("Open in app")
                || s.contains("Sign up to discover"))
        {
            continue;
        }
        if s.contains("data:image") && s.starts_with("![") {
            continue;
        }
        out_lines.push(line.to_string());
    }
    let mut out = String::new();
    let mut blanks = 0;
    for line in &out_lines {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    trim_to_first_h1(&out)
}

fn trim_to_first_h1(md: &str) -> String {
    let lines: Vec<&str> = md.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("# ") {
            return lines[i..].join("\n");
        }
    }
    md.to_string()
}

fn truncate_result(result: &mut FetchResult, max_chars: usize) {
    if max_chars > 0 && result.content.len() > max_chars {
        result.content = result.content.chars().take(max_chars).collect();
    }
}

fn error_result(url: &str, msg: &str) -> FetchResult {
    FetchResult {
        url: url.to_string(),
        final_url: url.to_string(),
        title: None,
        content: String::new(),
        content_format: "markdown".into(),
        links: Vec::new(),
        extracted_at: now_iso(),
        blocked: false,
        error: Some(msg.to_string()),
        llm_used: None,
        llm_error: None,
        video_metadata: None,
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_cleanup_drops_chrome() {
        let md = "# Title\n\nFollow\n\nSubscribe\n\nOpen in app\n\nReal content line\n\n## Get the best stories in your inbox\n\nTail\n";
        let cleaned = cleanup_markdown(md, "https://medium.com/x");
        assert!(cleaned.contains("Real content line"));
        assert!(!cleaned.contains("Subscribe"));
        assert!(!cleaned.contains("Get the best stories"));
        assert!(cleaned.starts_with("# Title"));
    }

    #[test]
    fn generic_cleanup_keeps_content() {
        let md = "line one\n\n\n\n\nline two\n\n![x](data:image/png;base64,AAA)\nline three";
        let cleaned = cleanup_markdown(md, "https://example.com/post");
        assert!(cleaned.contains("line one"));
        assert!(cleaned.contains("line two"));
        assert!(!cleaned.contains("data:image"));
    }

    #[test]
    fn format_parse() {
        assert_eq!(OutputFormat::parse("html"), OutputFormat::Html);
        assert_eq!(OutputFormat::parse("markdown"), OutputFormat::Markdown);
        assert_eq!(OutputFormat::parse("bogus"), OutputFormat::Markdown);
    }
}
