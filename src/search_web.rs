use crate::http::HttpFetcher;
use anyhow::Result;
use scraper::{Html, Selector};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Google,
    Bing,
    DuckDuckGo,
    Baidu,
    Sogou,
    So360,
    Yandex,
}

impl Engine {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "google" => Some(Self::Google),
            "bing" => Some(Self::Bing),
            "duckduckgo" | "ddg" => Some(Self::DuckDuckGo),
            "baidu" => Some(Self::Baidu),
            "sogou" => Some(Self::Sogou),
            "so360" | "360" => Some(Self::So360),
            "yandex" => Some(Self::Yandex),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Bing => "bing",
            Self::DuckDuckGo => "duckduckgo",
            Self::Baidu => "baidu",
            Self::Sogou => "sogou",
            Self::So360 => "so360",
            Self::Yandex => "yandex",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WebSearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub engine: String,
}

#[derive(Debug, Serialize)]
pub struct SearchOutcome {
    pub engine: String,
    pub query: String,
    pub results: Vec<WebSearchHit>,
    pub total: usize,
    pub engines_tried: Vec<String>,
}

const ENGINE_TIMEOUT: Duration = Duration::from_secs(12);

fn cjk_ratio(q: &str) -> f64 {
    let chars: Vec<char> = q.chars().collect();
    if chars.is_empty() {
        return 0.0;
    }
    let cjk = chars
        .iter()
        .filter(|c| (**c as u32) >= 0x4E00 && (**c as u32) <= 0x9FFF)
        .count();
    cjk as f64 / chars.len() as f64
}

/// Auto fallback plans, ported from crawl4ai-mcp: English starts on
/// DuckDuckGo, Chinese starts on Baidu/Sogou.
pub fn auto_plan(query: &str) -> Vec<Engine> {
    if cjk_ratio(query) > 0.1 {
        vec![
            Engine::Baidu,
            Engine::Sogou,
            Engine::So360,
            Engine::Bing,
            Engine::DuckDuckGo,
        ]
    } else {
        vec![
            Engine::DuckDuckGo,
            Engine::Bing,
            Engine::Google,
            Engine::Yandex,
        ]
    }
}

pub async fn search(
    http: &HttpFetcher,
    query: &str,
    engine: &str,
    max_results: usize,
    _lang: &str,
) -> Result<SearchOutcome> {
    let plan: Vec<Engine> = match Engine::parse(engine) {
        Some(e) => vec![e],
        None => auto_plan(query),
    };
    let mut engines_tried = Vec::new();
    for e in plan {
        engines_tried.push(e.name().to_string());
        match search_with(http, query, e, max_results).await {
            Ok(outcome) if !outcome.results.is_empty() => {
                let total = outcome.results.len();
                return Ok(SearchOutcome {
                    engine: e.name().to_string(),
                    query: query.to_string(),
                    results: outcome.results,
                    total,
                    engines_tried,
                });
            }
            Ok(_) => continue,
            Err(err) => {
                tracing::warn!(engine = e.name(), error = %err, "search engine failed");
            }
        }
    }
    Err(anyhow::anyhow!(
        "all search engines failed; engines tried: {}",
        engines_tried.join(", ")
    ))
}

pub async fn search_with(
    http: &HttpFetcher,
    query: &str,
    engine: Engine,
    max_results: usize,
) -> Result<SearchOutcome> {
    let (mut results, engine_used) = match engine {
        Engine::DuckDuckGo => (duckduckgo(http, query).await?, "duckduckgo"),
        Engine::Bing => (bing(http, query).await?, "bing"),
        Engine::Google => (google(http, query).await?, "google"),
        Engine::Baidu => (baidu(http, query).await?, "baidu"),
        Engine::Sogou => (sogou(http, query).await?, "sogou"),
        Engine::So360 => (so360(http, query).await?, "so360"),
        Engine::Yandex => (yandex(http, query).await?, "yandex"),
    };
    results = dedupe(results);
    results.truncate(max_results);
    Ok(SearchOutcome {
        engine: engine_used.to_string(),
        query: query.to_string(),
        total: results.len(),
        results,
        engines_tried: vec![engine_used.to_string()],
    })
}

fn decode_ddg_href(href: &str) -> String {
    if let Some(rest) = href.split("uddg=").nth(1) {
        let until = rest.split('&').next().unwrap_or(rest);
        return percent_decode(until);
    }
    if href.starts_with("http") {
        href.to_string()
    } else {
        String::new()
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

async fn duckduckgo(http: &HttpFetcher, query: &str) -> Result<Vec<WebSearchHit>> {
    let f = http
        .post_form(
            "https://html.duckduckgo.com/html/",
            &[("q", query), ("b", "")],
        )
        .await?;
    let doc = Html::parse_document(&f.body);
    let mut hits = Vec::new();
    if let (Ok(link_sel), Ok(snippet_sel)) = (
        Selector::parse("a.result__a"),
        Selector::parse("a.result__snippet, .result__snippet"),
    ) {
        for a in doc.select(&link_sel) {
            let href = a.value().attr("href").unwrap_or("");
            let url = decode_ddg_href(href);
            if !url.starts_with("http") {
                continue;
            }
            let title = a.text().collect::<String>().trim().to_string();
            let snippet = doc
                .select(&snippet_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            hits.push(WebSearchHit {
                title,
                url,
                snippet,
                engine: "duckduckgo".into(),
            });
        }
    }
    Ok(hits)
}

async fn bing(http: &HttpFetcher, query: &str) -> Result<Vec<WebSearchHit>> {
    let url = format!(
        "https://www.bing.com/search?q={}&count=30",
        crate::medium::search::urlencode(query)
    );
    let f = http.get(&url, Some(ENGINE_TIMEOUT)).await?;
    let doc = Html::parse_document(&f.body);
    let mut hits = Vec::new();
    if let Ok(item_sel) = Selector::parse("li.b_algo") {
        for item in doc.select(&item_sel) {
            let Some(a) = item.select(&Selector::parse("h2 a").unwrap()).next() else {
                continue;
            };
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            if !href.starts_with("http") {
                continue;
            }
            let title = a.text().collect::<String>().trim().to_string();
            let snippet = item
                .select(&Selector::parse(".b_caption p, p").unwrap())
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            hits.push(WebSearchHit {
                title,
                url: href.to_string(),
                snippet,
                engine: "bing".into(),
            });
        }
    }
    Ok(hits)
}

async fn google(http: &HttpFetcher, query: &str) -> Result<Vec<WebSearchHit>> {
    let url = format!(
        "https://www.google.com/search?q={}&num=20&hl=en",
        crate::medium::search::urlencode(query)
    );
    let f = http.get(&url, Some(ENGINE_TIMEOUT)).await?;
    let doc = Html::parse_document(&f.body);
    let mut hits = Vec::new();
    if let Ok(a_sel) = Selector::parse("div.g a[href], div[data-sokoban-container] a[href]") {
        for a in doc.select(&a_sel) {
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let href = if let Some(rest) = href.strip_prefix("/url?q=") {
                decode_ddg_href(&format!("uddg={rest}"))
            } else {
                href.to_string()
            };
            if !href.starts_with("http") || href.contains("google.") {
                continue;
            }
            let title = a
                .select(&Selector::parse("h3").unwrap())
                .next()
                .map(|h| h.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            hits.push(WebSearchHit {
                title,
                url: href,
                snippet: String::new(),
                engine: "google".into(),
            });
        }
    }
    Ok(hits)
}

async fn baidu(http: &HttpFetcher, query: &str) -> Result<Vec<WebSearchHit>> {
    let url = format!(
        "https://www.baidu.com/s?wd={}&rn=30",
        crate::medium::search::urlencode(query)
    );
    let f = http.get(&url, Some(ENGINE_TIMEOUT)).await?;
    let doc = Html::parse_document(&f.body);
    let mut hits = Vec::new();
    if let Ok(item_sel) = Selector::parse("div.result, div.result-op") {
        for item in doc.select(&item_sel) {
            let Some(a) = item.select(&Selector::parse("h3 a").unwrap()).next() else {
                continue;
            };
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let snippet = item
                .select(&Selector::parse(".c-abstract, [class*='content-right']").unwrap())
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            hits.push(WebSearchHit {
                title,
                url: href.to_string(),
                snippet,
                engine: "baidu".into(),
            });
        }
    }
    Ok(hits)
}

async fn sogou(http: &HttpFetcher, query: &str) -> Result<Vec<WebSearchHit>> {
    let url = format!(
        "https://www.sogou.com/web?query={}",
        crate::medium::search::urlencode(query)
    );
    let f = http.get(&url, Some(ENGINE_TIMEOUT)).await?;
    let doc = Html::parse_document(&f.body);
    let mut hits = Vec::new();
    if let Ok(item_sel) = Selector::parse(".vrwrap, .rb") {
        for item in doc.select(&item_sel) {
            let Some(a) = item.select(&Selector::parse("h3 a").unwrap()).next() else {
                continue;
            };
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let full = if href.starts_with('/') {
                format!("https://www.sogou.com{href}")
            } else {
                href.to_string()
            };
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            hits.push(WebSearchHit {
                title,
                url: full,
                snippet: String::new(),
                engine: "sogou".into(),
            });
        }
    }
    Ok(hits)
}

async fn so360(http: &HttpFetcher, query: &str) -> Result<Vec<WebSearchHit>> {
    let url = format!(
        "https://www.so.com/s?q={}",
        crate::medium::search::urlencode(query)
    );
    let f = http.get(&url, Some(ENGINE_TIMEOUT)).await?;
    let doc = Html::parse_document(&f.body);
    let mut hits = Vec::new();
    if let Ok(item_sel) = Selector::parse("li.res-list, .result") {
        for item in doc.select(&item_sel) {
            let Some(a) = item.select(&Selector::parse("h3 a").unwrap()).next() else {
                continue;
            };
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            hits.push(WebSearchHit {
                title,
                url: href.to_string(),
                snippet: String::new(),
                engine: "so360".into(),
            });
        }
    }
    Ok(hits)
}

async fn yandex(http: &HttpFetcher, query: &str) -> Result<Vec<WebSearchHit>> {
    let url = format!(
        "https://yandex.com/search/?text={}",
        crate::medium::search::urlencode(query)
    );
    let f = http.get(&url, Some(ENGINE_TIMEOUT)).await?;
    let doc = Html::parse_document(&f.body);
    let mut hits = Vec::new();
    if let Ok(a_sel) = Selector::parse(".organic a.organic__url, .serp-item a[href^='http']") {
        for a in doc.select(&a_sel) {
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() || !href.starts_with("http") {
                continue;
            }
            hits.push(WebSearchHit {
                title,
                url: href.to_string(),
                snippet: String::new(),
                engine: "yandex".into(),
            });
        }
    }
    Ok(hits)
}

fn dedupe(hits: Vec<WebSearchHit>) -> Vec<WebSearchHit> {
    let mut seen_url = std::collections::HashSet::new();
    let mut seen_title: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for h in hits {
        let norm_url = h
            .url
            .split('#')
            .next()
            .unwrap_or(&h.url)
            .trim_end_matches('/')
            .to_string();
        if !seen_url.insert(norm_url) {
            continue;
        }
        let norm_title = norm_title(&h.title);
        if seen_title.iter().any(|t| title_similarity(t, &norm_title) >= 0.85) {
            continue;
        }
        seen_title.push(norm_title);
        out.push(h);
    }
    out
}

fn norm_title(t: &str) -> String {
    t.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .map(|w| w.to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Word-set Jaccard similarity, port of crawl4ai-mcp's 85% fuzzy-title dedupe.
fn title_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let sb: std::collections::HashSet<&str> = b.split_whitespace().collect();
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        return 0.0;
    }
    inter as f64 / union as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_plan_language_split() {
        assert_eq!(auto_plan("rust async tips")[0], Engine::DuckDuckGo);
        assert_eq!(auto_plan("如何学习编程")[0], Engine::Baidu);
    }

    #[test]
    fn engine_parse() {
        assert_eq!(Engine::parse("duckduckgo"), Some(Engine::DuckDuckGo));
        assert_eq!(Engine::parse("so360"), Some(Engine::So360));
        assert_eq!(Engine::parse("auto"), None);
    }

    #[test]
    fn ddg_href_decode() {
        assert_eq!(
            decode_ddg_href("//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&rut=abc"),
            "https://example.com/a"
        );
        assert_eq!(decode_ddg_href("https://example.com/direct"), "https://example.com/direct");
        assert_eq!(decode_ddg_href("/relative"), "");
    }

    #[test]
    fn title_dedupe() {
        let hits = vec![
            WebSearchHit { title: "Rust Async Guide".into(), url: "https://a.com/1".into(), snippet: String::new(), engine: "bing".into() },
            WebSearchHit { title: "Guide Async Rust".into(), url: "https://b.com/2".into(), snippet: String::new(), engine: "bing".into() },
            WebSearchHit { title: "Totally Different Topic".into(), url: "https://c.com/3".into(), snippet: String::new(), engine: "bing".into() },
        ];
        let out = dedupe(hits);
        assert_eq!(out.len(), 2);
    }
}
