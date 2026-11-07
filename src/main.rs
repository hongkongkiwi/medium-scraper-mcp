use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use medium_scraper_mcp::config::Config;
use medium_scraper_mcp::crawl::{Crawler, FetchOptions, OutputFormat};
use medium_scraper_mcp::http::HttpFetcher;
use medium_scraper_mcp::medium::search::MediumSearch;
use medium_scraper_mcp::server::MediumScraperServer;
use medium_scraper_mcp::VERSION;
use rmcp::ServiceExt;

#[derive(Parser)]
#[command(
    name = "medium-scraper-mcp",
    version = VERSION,
    about = "MCP server for Medium research and agent-friendly web extraction",
    long_about = "Runs an MCP stdio server by default. Also exposes one-shot CLI modes: fetch a URL, search Medium, search the web."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the MCP stdio server (default when no subcommand is given)
    Serve,
    /// Fetch one URL and print the extracted content (JSON) - one-shot crawl4agent-style
    Fetch {
        url: String,
        /// Output format: markdown or html
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Truncate content to this length
        #[arg(long)]
        max_chars: Option<usize>,
        /// Enable LLM cleanup (requires OPENAI_* env vars)
        #[arg(long, default_value_t = false)]
        use_llm: bool,
    },
    /// Search Medium articles - one-shot
    Search {
        query: String,
        /// Medium tag to filter by
        #[arg(long)]
        tag: Option<String>,
        /// Maximum results
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Search the web across engines - one-shot
    SearchWeb {
        query: String,
        /// Engine: auto, google, bing, duckduckgo, baidu, sogou, so360, yandex
        #[arg(long, default_value = "auto")]
        engine: String,
        /// Maximum results
        #[arg(long, default_value_t = 10)]
        max_results: usize,
    },
    /// Print dependency/environment status as JSON
    Doctor,
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = if std::env::var("MEDIUM_SCRAPER_VERBOSE").ok().as_deref() == Some("1") {
        "info"
    } else {
        "warn"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let cli = Cli::parse();
    let cfg = Config::from_env();

    match cli.command {
        None | Some(Command::Serve) => {
            let service = MediumScraperServer::new(cfg).context("build MCP server")?;
            let running = service
                .serve(rmcp::transport::stdio())
                .await
                .context("serve MCP stdio transport")?;
            running.waiting().await?;
            Ok(())
        }
        Some(Command::Fetch {
            url,
            format,
            max_chars,
            use_llm,
        }) => {
            let http = HttpFetcher::new(std::sync::Arc::new(cfg.clone()))?;
            let crawler = Crawler {
                cfg: std::sync::Arc::new(cfg),
                http: std::sync::Arc::new(http),
            };
            let opts = FetchOptions {
                format: OutputFormat::parse(&format),
                max_chars: max_chars.unwrap_or(crawler.cfg.max_content_chars),
                use_llm,
                llm_instruction: None,
            };
            let result = crawler.fetch_one(&url, opts).await;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some(Command::Search { query, tag, limit }) => {
            let http = HttpFetcher::new(std::sync::Arc::new(cfg))?;
            let search = MediumSearch { http: &http };
            let hits = search.search(&query, tag.as_deref(), limit).await?;
            println!("{}", serde_json::to_string_pretty(&hits)?);
            Ok(())
        }
        Some(Command::SearchWeb {
            query,
            engine,
            max_results,
        }) => {
            let http = HttpFetcher::new(std::sync::Arc::new(cfg))?;
            let outcome = medium_scraper_mcp::search_web::search(&http, &query, &engine, max_results, "").await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Some(Command::Doctor) => {
            let status = serde_json::json!({
                "version": VERSION,
                "proxy_configured": cfg.proxy.is_some(),
                "medium_cookies_configured": cfg.medium_cookies.is_some(),
                "browser_fallback_enabled": cfg.use_browser_fallback,
                "chrome_found": medium_scraper_mcp::browser::find_chrome().is_some(),
                "yt_dlp_found": which::which("yt-dlp").is_ok(),
                "llm_cleanup_configured": cfg.openai.is_some(),
                "openai_model": cfg.openai.as_ref().map(|o| o.model.clone()),
            });
            println!("{}", serde_json::to_string_pretty(&status)?);
            Ok(())
        }
    }
}
