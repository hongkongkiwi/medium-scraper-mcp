use std::time::Duration;

/// OpenAI-compatible endpoint for optional LLM cleanup.
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Upstream HTTP/SOCKS5 proxy for all outbound HTTP.
    pub proxy: Option<String>,
    /// Raw Cookie header applied to medium.com and custom-domain requests.
    pub medium_cookies: Option<String>,
    /// Launch the browser fallback headless (default true).
    pub headless: bool,
    /// Per-request HTTP timeout.
    pub timeout: Duration,
    /// Truncation cap applied by fetch_urls.
    pub max_content_chars: usize,
    /// Enable the headless-browser fallback for blocked or thin pages.
    pub use_browser_fallback: bool,
    /// OpenAI-compatible config for optional LLM cleanup; None disables it.
    pub openai: Option<OpenAiConfig>,
    /// yt-dlp cookie source, e.g. "chrome" or "firefox:default".
    pub ytdlp_cookies_from_browser: Option<String>,
    /// Netscape cookies.txt path for yt-dlp.
    pub ytdlp_cookiefile: Option<String>,
    /// Pin a single user agent; otherwise requests rotate a UA pool.
    pub user_agent: Option<String>,
    /// Accept-Language header value.
    pub accept_language: String,
}

impl Config {
    pub fn from_env() -> Self {
        let timeout = env_duration("MEDIUM_SCRAPER_TIMEOUT_MS", 30_000);
        let openai = match (
            env_str(&["OPENAI_BASE_URL"]),
            env_str(&["OPENAI_API_KEY"]),
        ) {
            (Some(base), Some(key)) => Some(OpenAiConfig {
                base_url: base.trim_end_matches('/').to_string(),
                api_key: key,
                model: env_str(&["OPENAI_MODEL"]).unwrap_or_else(|| "gpt-4o-mini".to_string()),
            }),
            _ => None,
        };
        Self {
            proxy: env_str(&["MEDIUM_SCRAPER_PROXY", "CRAWL4AI_MCP_PROXY"])
                .map(|raw| normalize_proxy(&raw)),
            medium_cookies: env_str(&["MEDIUM_SCRAPER_COOKIES"]),
            headless: env_bool("MEDIUM_SCRAPER_HEADLESS", true),
            timeout,
            max_content_chars: env_usize("MEDIUM_SCRAPER_MAX_CONTENT_CHARS", 200_000),
            use_browser_fallback: !env_bool("MEDIUM_SCRAPER_NO_BROWSER", false),
            openai,
            ytdlp_cookies_from_browser: env_str(&[
                "MEDIUM_SCRAPER_YTDLP_COOKIES_FROM_BROWSER",
                "CRAWL4AI_MCP_YTDLP_COOKIES_FROM_BROWSER",
            ]),
            ytdlp_cookiefile: env_str(&[
                "MEDIUM_SCRAPER_YTDLP_COOKIEFILE",
                "CRAWL4AI_MCP_YTDLP_COOKIEFILE",
            ]),
            user_agent: env_str(&["MEDIUM_SCRAPER_USER_AGENT"]),
            accept_language: env_str(&["MEDIUM_SCRAPER_ACCEPT_LANGUAGE"])
                .unwrap_or_else(|| "en-US,en;q=0.9".to_string()),
        }
    }

    pub fn is_medium_request(&self, url: &str) -> bool {
        url.contains("medium.com") || crate::medium::parse::is_medium_url(url)
    }
}

/// Accept the proxy formats crawl4ai-mcp accepts: scheme://host:port, host:port, port-only.
pub fn normalize_proxy(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    if raw.parse::<u16>().is_ok() {
        return format!("http://127.0.0.1:{raw}");
    }
    if let Some(rest) = raw
        .strip_prefix("socket5://")
        .or_else(|| raw.strip_prefix("socket4://"))
    {
        return format!("socks5://{rest}");
    }
    if raw.contains("://") {
        return raw.to_string();
    }
    // host:port (or host only) -> http scheme
    if !raw.starts_with("[") && raw.matches(':').count() == 1 {
        return format!("http://{raw}");
    }
    raw.to_string()
}

fn env_str(names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(v) = std::env::var(name) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    match std::env::var(name) {
        Ok(v) => v.trim().parse().unwrap_or(default),
        Err(_) => default,
    }
}

fn env_duration(name: &str, default_ms: u64) -> Duration {
    Duration::from_millis(env_u64(name, default_ms))
}

fn env_u64(name: &str, default: u64) -> u64 {
    match std::env::var(name) {
        Ok(v) => v.trim().parse().unwrap_or(default),
        Err(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_proxy;

    #[test]
    fn proxy_formats_normalize() {
        assert_eq!(normalize_proxy("7890"), "http://127.0.0.1:7890");
        assert_eq!(normalize_proxy("127.0.0.1:7890"), "http://127.0.0.1:7890");
        assert_eq!(normalize_proxy("socket5://10.0.0.1:1080"), "socks5://10.0.0.1:1080");
        assert_eq!(normalize_proxy("socks5://10.0.0.1:1080"), "socks5://10.0.0.1:1080");
        assert_eq!(normalize_proxy("https://proxy.example.com:8443"), "https://proxy.example.com:8443");
        assert_eq!(normalize_proxy(""), "");
    }
}
