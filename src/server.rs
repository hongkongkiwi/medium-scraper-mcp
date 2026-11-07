use crate::config::Config;
use crate::crawl::{Crawler, FetchOptions, OutputFormat};
use crate::http::HttpFetcher;
use crate::medium::download::{resolve_output_dir, Downloader};
use crate::medium::fetch::{MediumFetcher, ProxyKind, ReadOptions};
use crate::medium::parse::{self, with_frontmatter, Article};
use crate::medium::search::{MediumSearch, SearchHit};
use crate::search_web;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::*,
    schemars,
    tool, tool_handler, tool_router,
    ErrorData as McpError,
    ServerHandler,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UnifiedArgs {
    /// Operation to perform: search (find articles), convert (article to markdown), info (get metadata)
    pub operation: String,
    /// Search query for articles (required for search operation)
    #[serde(default)]
    pub query: Option<String>,
    /// Medium article URL (required for convert and info operations)
    #[serde(default)]
    pub url: Option<String>,
    /// Medium tag to filter by, e.g. "python" (search only)
    #[serde(default)]
    pub tag: Option<String>,
    /// Maximum number of search results (1-50, default 10)
    #[serde(default)]
    pub limit: Option<u32>,
    /// Include image references in markdown (default true)
    #[serde(default)]
    pub include_images: Option<bool>,
    /// Preserve code blocks (default true)
    #[serde(default)]
    pub include_code: Option<bool>,
    /// Attempt paywall bypass via proxy services (default false)
    #[serde(default)]
    pub bypass_paywall: Option<bool>,
    /// Preferred proxy service: freedium, readmedium, archive, auto (default auto)
    #[serde(default)]
    pub preferred_proxy: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadPostArgs {
    /// Medium post URL. Accepts medium.com, *.medium.com, and Medium publications on custom domains (URLs ending in a 12-char hex slug suffix like `-a1b2c3d4e5f6` are Medium regardless of host).
    pub url: String,
    /// Include image references (default true)
    #[serde(default)]
    pub include_images: Option<bool>,
    /// Attempt paywall bypass via freedium/readmedium/archive when the post is member-only (default true)
    #[serde(default)]
    pub bypass_paywall: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DownloadPostArgs {
    /// Medium post URL to download.
    pub url: String,
    /// Directory to write into. Files land under `<output_dir>/Output/medium-scraper-mcp/`. Accepts `~/...`. Defaults to the server's working directory. Re-runs are skip-if-unchanged.
    #[serde(default)]
    pub output_dir: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ArticleInfoArgs {
    /// Medium article URL.
    pub url: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchMediumArgs {
    /// Search query string
    pub query: String,
    /// Medium tag to filter by, e.g. "python", "technology"
    #[serde(default)]
    pub tag: Option<String>,
    /// Maximum number of results (default 10, max 50)
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FetchUrlsArgs {
    /// List of URLs to fetch (one or more)
    pub urls: Vec<String>,
    /// Output format: "markdown" (default) or "html"
    #[serde(default)]
    pub format: Option<String>,
    /// Truncate content to this length (default 200000)
    #[serde(default)]
    pub max_chars: Option<usize>,
    /// Parallel fetch limit (1-10, default 3)
    #[serde(default)]
    pub concurrency: Option<u32>,
    /// Enable LLM cleanup (default false, requires OPENAI_BASE_URL/OPENAI_API_KEY/OPENAI_MODEL)
    #[serde(default)]
    pub use_llm: Option<bool>,
    /// Custom instruction for LLM cleanup (only when use_llm=true)
    #[serde(default)]
    pub llm_instruction: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchWebArgs {
    /// Search query string
    pub query: String,
    /// Engine: auto (default), google, bing, duckduckgo, baidu, sogou, so360, yandex
    #[serde(default)]
    pub engine: Option<String>,
    /// Maximum number of results (default 10, max 100)
    #[serde(default)]
    pub max_results: Option<u32>,
    /// Language hint, e.g. "en", "zh-CN" (auto-detects Chinese from query text)
    #[serde(default)]
    pub lang: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateSetupArgs {
    /// Optional Medium post URL to read as a live test; reports whether full content was returned.
    #[serde(default)]
    pub test_url: Option<String>,
}

pub struct MediumScraperServer {
    cfg: Arc<Config>,
    http: Arc<HttpFetcher>,
}

#[tool_router]
impl MediumScraperServer {
    pub fn new(cfg: Config) -> anyhow::Result<Self> {
        let cfg = Arc::new(cfg);
        let http = Arc::new(HttpFetcher::new(cfg.clone())?);
        Ok(Self {
            cfg,
            http,
        })
    }

    fn read_opts(
        &self,
        include_images: Option<bool>,
        include_code: Option<bool>,
        bypass: Option<bool>,
        proxy: Option<&str>,
    ) -> ReadOptions {
        ReadOptions {
            include_images: include_images.unwrap_or(true),
            include_code: include_code.unwrap_or(true),
            bypass_paywall: bypass.unwrap_or(true),
            preferred_proxy: proxy
                .map(ProxyKind::parse)
                .unwrap_or(ProxyKind::Auto),
        }
    }

    fn article_payload(&self, article: &Article, include_frontmatter: bool) -> String {
        if include_frontmatter {
            with_frontmatter(article)
        } else {
            article.body_markdown.clone()
        }
    }

    #[tool(
        description = "Unified Medium scraper tool for searching, converting, and getting article info with paywall bypass (compatibility tool). operation=search requires query; operation=convert|info requires url."
    )]
    async fn medium_scraper(
        &self,
        Parameters(args): Parameters<UnifiedArgs>,
    ) -> Result<CallToolResult, McpError> {
        match args.operation.as_str() {
            "search" => {
                let query = args.query.ok_or_else(|| McpError::invalid_params("query is required for search operation", None))?;
                let hits = self
                    .run_search(&query, args.tag.as_deref(), args.limit.unwrap_or(10) as usize)
                    .await?;
                self.json_result(&hits)
            }
            "convert" => {
                let url = args.url.ok_or_else(|| McpError::invalid_params("url is required for convert operation", None))?;
                let opts = self.read_opts(args.include_images, args.include_code, args.bypass_paywall, args.preferred_proxy.as_deref());
                let article = self.read_article(&url, opts).await?;
                let mut md = self.article_payload(&article, true);
                if article.via != "direct" {
                    md.push_str(&format!("\n\n---\n*Content retrieved via {} proxy (paywall bypass)*\n", article.via));
                }
                if article.paywalled {
                    md.push_str(&paywall_notice());
                }
                self.text_result(md)
            }
            "info" => {
                let url = args.url.ok_or_else(|| McpError::invalid_params("url is required for info operation", None))?;
                let article = self.read_article(&url, ReadOptions::default()).await?;
                self.json_result(&article.meta)
            }
            other => Err(McpError::invalid_params(
                format!("unknown operation: {other} (expected search, convert, or info)"),
                None,
            )),
        }
    }

    #[tool(
        description = "Read a Medium post by URL and return clean Markdown with frontmatter (title/author/publication/published/tags/url/reading_time) plus the full body. Handles medium.com, *.medium.com, and custom-domain publications. Member-only posts use freedium/readmedium/archive fallbacks; when bypass fails, a recovery notice is included."
    )]
    async fn read_medium_post(
        &self,
        Parameters(args): Parameters<ReadPostArgs>,
    ) -> Result<CallToolResult, McpError> {
        let opts = self.read_opts(
            args.include_images,
            Some(true),
            args.bypass_paywall,
            Some("auto"),
        );
        let article = self.read_article(&args.url, opts).await?;
        let mut md = with_frontmatter(&article);
        if article.paywalled {
            md.push_str(&paywall_notice());
        }
        self.text_result(md)
    }

    #[tool(
        description = "Download a Medium post AND its images to local disk. Markdown lands in `<output_dir>/Output/medium-scraper-mcp/<date>-<slug>.md`; images go to `Output/medium-scraper-mcp/assets/<post_id>/`. Re-runs are skip-if-unchanged. Use read_medium_post instead when the user just wants to read the post in chat."
    )]
    async fn download_medium_post(
        &self,
        Parameters(args): Parameters<DownloadPostArgs>,
    ) -> Result<CallToolResult, McpError> {
        let dir = resolve_output_dir(args.output_dir.as_deref())
            .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
        let fetcher = MediumFetcher {
            http: &self.http,
            cfg: &self.cfg,
        };
        let downloader = Downloader { fetcher };
        let report = downloader
            .download(&args.url, self.read_opts(None, None, Some(true), Some("auto")), &dir)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let summary = format!(
            "Downloaded post to `{}`.\nAssets dir: `{}`\nImages downloaded: {} (failed: {})\n{}",
            report.markdown_path,
            report.assets_dir,
            report.images_downloaded,
            report.images_failed,
            if report.skipped_unchanged { "(already up to date)" } else { "" }
        );
        self.text_result(summary)
    }

    #[tool(
        description = "Get Medium article metadata as JSON: title, author, publication, published date, tags, reading time, word count, canonical URL, post id, paywall status. No body content."
    )]
    async fn get_article_info(
        &self,
        Parameters(args): Parameters<ArticleInfoArgs>,
    ) -> Result<CallToolResult, McpError> {
        let article = self.read_article(&args.url, ReadOptions::default()).await?;
        self.json_result(&article.meta)
    }

    #[tool(
        description = "Search Medium articles by query and optional tag. Falls back from Medium's own search HTML to tag pages to a DuckDuckGo site:medium.com query. Returns [{title, url, author, snippet}]."
    )]
    async fn search_medium(
        &self,
        Parameters(args): Parameters<SearchMediumArgs>,
    ) -> Result<CallToolResult, McpError> {
        let limit = args.limit.unwrap_or(10).clamp(1, 50) as usize;
        let hits = self.run_search(&args.query, args.tag.as_deref(), limit).await?;
        self.json_result(&hits)
    }

    #[tool(
        description = "Fetch and extract content from one or more URLs. Real HTTP fetch with automatic headless-Chrome fallback for blocked or JS-heavy pages. Supports YouTube/Bilibili transcript extraction, blocked-page detection, and optional LLM cleanup. Returns per URL: {url, final_url, title, content, content_format, links, extracted_at, blocked, llm_used?, llm_error?, video_metadata?}."
    )]
    async fn fetch_urls(
        &self,
        Parameters(args): Parameters<FetchUrlsArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.urls.is_empty() {
            return Err(McpError::invalid_params("urls must not be empty", None));
        }
        if args.urls.len() > 20 {
            return Err(McpError::invalid_params("urls is limited to 20 per call", None));
        }
        let format = args.format.as_deref().map(OutputFormat::parse).unwrap_or(OutputFormat::Markdown);
        let opts = FetchOptions {
            format,
            max_chars: args.max_chars.unwrap_or(self.cfg.max_content_chars),
            use_llm: args.use_llm.unwrap_or(false),
            llm_instruction: args.llm_instruction.as_deref().map(|s| Box::leak(s.to_string().into_boxed_str())).map(|s| &*s),
        };
        let crawler = Crawler {
            cfg: self.cfg.clone(),
            http: self.http.clone(),
        };
        let results = crawler
            .fetch_many(&args.urls, opts, args.concurrency.unwrap_or(3) as usize)
            .await;
        self.json_result(&results)
    }

    #[tool(
        description = "Search the web across 7 engines (google, bing, duckduckgo, baidu, sogou, so360, yandex) with automatic fallback. engine=auto picks a plan by query language: English starts on DuckDuckGo, Chinese starts on Baidu. Results are deduplicated by URL and fuzzy title (85% threshold). Returns {engine, query, results: [{title, url, snippet, engine}], total, engines_tried}."
    )]
    async fn search_web(
        &self,
        Parameters(args): Parameters<SearchWebArgs>,
    ) -> Result<CallToolResult, McpError> {
        let max_results = args.max_results.unwrap_or(10).clamp(1, 100) as usize;
        let engine = args.engine.as_deref().unwrap_or("auto").to_string();
        let lang = args.lang.unwrap_or_default();
        match search_web::search(&self.http, &args.query, &engine, max_results, &lang).await {
            Ok(outcome) => self.json_result(&outcome),
            Err(e) => {
                let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
                    "Search failed: {e}"
                ))]);
                result.is_error = Some(true);
                Ok(result)
            }
        }
    }

    #[tool(
        description = "Inspect this server's dependency state: Medium reachability, freedium proxy reachability, configured cookies, yt-dlp availability, Chrome availability for the browser fallback, and optional LLM config. Optionally runs a live read against test_url."
    )]
    async fn validate_setup(
        &self,
        Parameters(args): Parameters<ValidateSetupArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut report = serde_json::Map::new();
        report.insert("version".into(), serde_json::Value::String(crate::VERSION.into()));
        report.insert(
            "medium_cookies_configured".into(),
            serde_json::Value::Bool(self.cfg.medium_cookies.is_some()),
        );
        report.insert(
            "proxy_configured".into(),
            serde_json::Value::Bool(self.cfg.proxy.is_some()),
        );
        report.insert(
            "browser_fallback_enabled".into(),
            serde_json::Value::Bool(self.cfg.use_browser_fallback),
        );
        report.insert(
            "chrome_found".into(),
            serde_json::Value::Bool(crate::browser::find_chrome().is_some()),
        );
        report.insert(
            "yt_dlp_found".into(),
            serde_json::Value::Bool(which::which("yt-dlp").is_ok()),
        );
        report.insert(
            "llm_cleanup_configured".into(),
            serde_json::Value::Bool(self.cfg.openai.is_some()),
        );
        let medium_status = self.http.probe("https://medium.com/").await;
        report.insert(
            "medium_reachable".into(),
            serde_json::Value::Bool(medium_status.as_ref().map(|s| *s < 500).unwrap_or(false)),
        );
        let freedium_status = self.http.probe("https://freedium.cfd/").await;
        report.insert(
            "freedium_reachable".into(),
            serde_json::Value::Bool(freedium_status.as_ref().map(|s| *s < 500).unwrap_or(false)),
        );
        if let Some(test_url) = args.test_url {
            match self.read_article(&test_url, ReadOptions::default()).await {
                Ok(article) => {
                    report.insert("test_url".into(), serde_json::Value::String(test_url));
                    report.insert(
                        "test_full_content".into(),
                        serde_json::Value::Bool(!article.paywalled && article.meta.word_count > 120),
                    );
                    report.insert(
                        "test_word_count".into(),
                        serde_json::Value::Number(article.meta.word_count.into()),
                    );
                }
                Err(e) => {
                    report.insert("test_url".into(), serde_json::Value::String(test_url));
                    report.insert("test_error".into(), serde_json::Value::String(e.to_string()));
                }
            }
        }
        self.json_result(&serde_json::Value::Object(report))
    }
}

impl MediumScraperServer {
    async fn read_article(&self, url: &str, opts: ReadOptions) -> Result<Article, McpError> {
        if !parse::is_medium_url(url) {
            return Err(McpError::invalid_params(
                format!("not a recognized Medium URL: {url}"),
                None,
            ));
        }
        let fetcher = MediumFetcher {
            http: &self.http,
            cfg: &self.cfg,
        };
        fetcher
            .read(url, opts)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    async fn run_search(
        &self,
        query: &str,
        tag: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, McpError> {
        let search = MediumSearch { http: &self.http };
        search
            .search(query, tag, limit)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    fn text_result(&self, text: String) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    fn json_result<T: serde::Serialize>(
        &self,
        value: &T,
    ) -> Result<CallToolResult, McpError> {
        let json = serde_json::to_string_pretty(value)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
    }
}

fn paywall_notice() -> String {
    "\n\n---\n\
    **This post is member-only and only a preview could be retrieved.**\n\n\
    Recovery options for full content:\n\n\
    1. Set the `MEDIUM_SCRAPER_COOKIES` environment variable to your signed-in Medium cookie header \
    (at minimum `sid=...; uid=...`, plus `cf_clearance=...` if Medium challenges you) and restart \
    this MCP server. Signed-in cookies unlock member-only posts, the same role the ZMediumToMarkdown \
    cookie cache plays for the mcp-medium-reader server.\n\
    2. Ensure freedium.cfd is reachable from this machine (it is the first bypass proxy tried).\n\
    3. As a last resort, open the URL in a browser with your Medium session and copy the text."
        .to_string()
}

#[tool_handler]
impl ServerHandler for MediumScraperServer {
    fn get_info(&self) -> ServerConfig {
        let implementation: Implementation = serde_json::from_value(serde_json::json!({
            "name": "medium-scraper-mcp",
            "version": crate::VERSION,
            "title": "Medium Scraper MCP"
        }))
        .expect("static implementation descriptor");
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(implementation)
            .with_instructions(
                "Medium research server: read/download Medium posts as markdown with paywall-aware fetching, \
                 search Medium, batch-fetch web pages with a browser fallback, and multi-engine web search. \
                 Paste a medium.com URL and the LLM will route to read_medium_post."
                    .to_string(),
            )
    }
}
