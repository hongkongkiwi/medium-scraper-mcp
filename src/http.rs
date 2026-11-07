use crate::config::Config;
use anyhow::{anyhow, Context, Result};
use rand::Rng;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, REFERER, USER_AGENT};
use std::sync::Arc;
use std::time::Duration;

/// Modern user agents used for rotation when no UA is pinned.
pub const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:133.0) Gecko/20100101 Firefox/133.0",
];

pub fn random_user_agent() -> &'static str {
    let idx = rand::thread_rng().gen_range(0..USER_AGENTS.len());
    USER_AGENTS[idx]
}

pub struct Fetched {
    pub final_url: String,
    pub status: u16,
    pub body: String,
}

/// Thin reqwest wrapper: UA rotation, optional proxy, optional Medium cookies.
pub struct HttpFetcher {
    client: reqwest::Client,
    cfg: Arc<Config>,
}

impl HttpFetcher {
    pub fn new(cfg: Arc<Config>) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .timeout(cfg.timeout)
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(10))
            .gzip(true)
            .brotli(true)
            .user_agent(cfg.user_agent.clone().unwrap_or_else(|| random_user_agent().to_string()));
        if let Some(proxy) = &cfg.proxy {
            let proxy = reqwest::Proxy::all(proxy)
                .with_context(|| format!("invalid proxy {proxy}"))?;
            builder = builder.proxy(proxy);
        }
        let client = builder.build()?;
        Ok(Self { client, cfg })
    }

    fn base_headers(&self, url: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_str(&self.cfg.accept_language).unwrap_or(HeaderValue::from_static("en-US,en;q=0.9")));
        let is_medium = self.cfg.is_medium_request(url);
        headers.insert(ACCEPT, HeaderValue::from_static(if is_medium {
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"
        } else {
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
        }));
        if let Some(cookies) = &self.cfg.medium_cookies {
            if is_medium {
                if let Ok(v) = HeaderValue::from_str(cookies) {
                    headers.insert(COOKIE, v);
                }
            }
        }
        if let Ok(host) = url::Url::parse(url).map(|u| u.host_str().unwrap_or("").to_string()) {
            if let Ok(v) = HeaderValue::from_str(&format!("https://{host}/")) {
                headers.insert(REFERER, v);
            }
        }
        headers
    }

    /// GET with one retry on transport errors; returns Err on final failure.
    pub async fn get(&self, url: &str, timeout: Option<Duration>) -> Result<Fetched> {
        let mut last_err = None;
        for attempt in 0..2 {
            if attempt > 0 {
                let jitter = 400 + rand::random::<u64>() % 600;
                tokio::time::sleep(Duration::from_millis(jitter)).await;
            }
            let ua = self.cfg.user_agent.clone().unwrap_or_else(|| random_user_agent().to_string());
            let mut req = self.client.get(url).headers(self.base_headers(url)).header(USER_AGENT, &ua);
            if let Some(t) = timeout {
                req = req.timeout(t);
            }
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let final_url = resp.url().to_string();
                    let body = resp.text().await.unwrap_or_default();
                    return Ok(Fetched { final_url, status, body });
                }
                Err(e) => {
                    last_err = Some(anyhow!("GET {url} failed: {e}"));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("request failed")))
    }

    /// POST a form (used by search engines).
    pub async fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<Fetched> {
        let ua = self.cfg.user_agent.clone().unwrap_or_else(|| random_user_agent().to_string());
        let resp = self
            .client
            .post(url)
            .headers(self.base_headers(url))
            .header(USER_AGENT, &ua)
            .form(form)
            .send()
            .await?;
        let status = resp.status().as_u16();
        let final_url = resp.url().to_string();
        let body = resp.text().await.unwrap_or_default();
        Ok(Fetched { final_url, status, body })
    }

    /// Fetch binary content (images, subtitle tracks).
    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(url)
            .header(USER_AGENT, self.cfg.user_agent.clone().unwrap_or_else(|| random_user_agent().to_string()))
            .timeout(self.cfg.timeout)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow!("GET {url} -> HTTP {}", resp.status().as_u16()));
        }
        Ok(resp.bytes().await?.to_vec())
    }

    /// HEAD-style probe used by validate_setup.
    pub async fn probe(&self, url: &str) -> Result<u16> {
        let resp = self
            .client
            .get(url)
            .headers(self.base_headers(url))
            .header(USER_AGENT, self.cfg.user_agent.clone().unwrap_or_else(|| random_user_agent().to_string()))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        Ok(resp.status().as_u16())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ua_pool_is_modern() {
        assert!(USER_AGENTS.len() >= 4);
        for ua in USER_AGENTS {
            assert!(ua.starts_with("Mozilla/5.0"));
        }
    }

    #[tokio::test]
    async fn client_builds_without_proxy() {
        let cfg = Arc::new(Config {
            proxy: None,
            medium_cookies: None,
            headless: true,
            timeout: Duration::from_secs(5),
            max_content_chars: 1000,
            use_browser_fallback: false,
            openai: None,
            ytdlp_cookies_from_browser: None,
            ytdlp_cookiefile: None,
            user_agent: None,
            accept_language: "en-US,en;q=0.9".into(),
        });
        assert!(HttpFetcher::new(cfg).is_ok());
    }
}
