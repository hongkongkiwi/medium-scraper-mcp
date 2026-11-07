use crate::http::HttpFetcher;
use crate::search_web;
use anyhow::Result;
use scraper::{Html, Selector};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub author: String,
    pub snippet: String,
}

pub struct MediumSearch<'a> {
    pub http: &'a HttpFetcher,
}

impl<'a> MediumSearch<'a> {
    /// Search Medium posts. Three paths, merged and deduped:
    /// 1. medium.com/search/posts HTML
    /// 2. medium.com/tag/<tag> stream (tag-only searches)
    /// 3. DuckDuckGo `site:medium.com` fallback
    pub async fn search(
        &self,
        query: &str,
        tag: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchHit>> {
        let mut hits: Vec<SearchHit> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if let Ok(f) = self.http.get(&format!("https://medium.com/search/posts?q={}", urlencode(query)), None).await {
            for hit in parse_search_html(&f.body) {
                if seen.insert(hit.url.clone()) {
                    hits.push(hit);
                }
            }
        }

        if let Some(tag) = tag.filter(|_| hits.len() < limit) {
            if let Ok(f) = self
                .http
                .get(&format!("https://medium.com/tag/{}", urlencode(tag)), None)
                .await
            {
                for hit in parse_tag_page(&f.body) {
                    if seen.insert(hit.url.clone()) {
                        hits.push(hit);
                    }
                }
            }
        }

        if hits.len() < limit {
            let web_query = match tag {
                Some(t) => format!("site:medium.com {} {}", query, t),
                None => format!("site:medium.com {}", query),
            };
            if let Ok(outcome) = search_web::search_with(
                self.http,
                &web_query,
                search_web::Engine::DuckDuckGo,
                limit.saturating_sub(hits.len()).max(5),
            )
            .await
            {
                for r in outcome.results {
                    if seen.insert(r.url.clone()) {
                        let author = author_from_url(&r.url);
                        hits.push(SearchHit {
                            title: r.title,
                            url: r.url,
                            author,
                            snippet: r.snippet,
                        });
                    }
                }
            }
        }

        hits.truncate(limit);
        Ok(hits)
    }
}

/// Old markup: div[data-test-id="postPreview"]; generic fallback: links to posts.
pub fn parse_search_html(html: &str) -> Vec<SearchHit> {
    let doc = Html::parse_document(html);
    let mut hits = Vec::new();
    if let Ok(sel) = Selector::parse("div[data-test-id='postPreview'], div[data-testid='postPreview']") {
        for el in doc.select(&sel) {
            let title = el
                .select(&Selector::parse("h2, h3").unwrap())
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let Some(a) = el.select(&Selector::parse("a[href]").unwrap()).next() else {
                continue;
            };
            let href = normalize_medium_href(a.value().attr("href").unwrap_or(""));
            if href.is_empty() {
                continue;
            }
            let author = el
                .select(&Selector::parse("a[data-testid='authorName'], a[data-test-id='authorName']").unwrap())
                .next()
                .map(|a| a.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| author_from_url(&href));
            hits.push(SearchHit {
                title: if title.is_empty() { "Untitled".into() } else { title },
                url: href.clone(),
                author,
                snippet: String::new(),
            });
        }
    }
    if hits.is_empty() {
        // newer server-rendered search markup: h2/h3 wrapping a post link
        if let Ok(sel) = Selector::parse("a[href*='-a'], a[href*='/p/']") {
            for a in doc.select(&sel) {
                let href = normalize_medium_href(a.value().attr("href").unwrap_or(""));
                if href.is_empty() {
                    continue;
                }
                let title = collapse(&a.text().collect::<String>());
                if title.len() < 8 {
                    continue;
                }
                hits.push(SearchHit {
                    title,
                    url: href.clone(),
                    author: author_from_url(&href),
                    snippet: String::new(),
                });
            }
        }
    }
    dedupe(hits)
}

/// Tag stream pages embed JSON-LD ItemLists or h2 article links.
pub fn parse_tag_page(html: &str) -> Vec<SearchHit> {
    let doc = Html::parse_document(html);
    let mut hits = Vec::new();
    if let Ok(sel) = Selector::parse("article h2, article h3, h2") {
        for h in doc.select(&sel) {
            let title = collapse(&h.text().collect::<String>());
            if title.len() < 8 {
                continue;
            }
            let Some(a) = h
                .select(&Selector::parse("a[href]").unwrap())
                .next()
                .or_else(|| h.parent().and_then(scraper::ElementRef::wrap).and_then(|p| p.select(&Selector::parse("a[href]").unwrap()).next()))
            else {
                continue;
            };
            let href = normalize_medium_href(a.value().attr("href").unwrap_or(""));
            if href.is_empty() {
                continue;
            }
            hits.push(SearchHit {
                title,
                url: href.clone(),
                author: author_from_url(&href),
                snippet: String::new(),
            });
        }
    }
    dedupe(hits)
}

fn normalize_medium_href(href: &str) -> String {
    let href = href.split('?').next().unwrap_or(href);
    if href.starts_with('/') {
        format!("https://medium.com{href}")
    } else if href.starts_with("http") {
        href.to_string()
    } else {
        String::new()
    }
}

fn author_from_url(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|mut segs| segs.next().map(|s| s.to_string()))
        })
        .filter(|s| s.starts_with('@') || !s.is_empty())
        .unwrap_or_else(|| "Unknown".to_string())
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn dedupe(hits: Vec<SearchHit>) -> Vec<SearchHit> {
    let mut seen = std::collections::HashSet::new();
    hits.into_iter()
        .filter(|h| seen.insert(h.url.clone()))
        .collect()
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_post_preview_markup() {
        let html = r#"<html><body>
            <div data-test-id="postPreview">
                <a href="/@writer/some-post-a1b2c3d4e5f6"><h3>A Great Post</h3></a>
                <a data-testid="authorName" href="/@writer">Writer Name</a>
            </div>
        </body></html>"#;
        let hits = parse_search_html(html);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "A Great Post");
        assert_eq!(hits[0].url, "https://medium.com/@writer/some-post-a1b2c3d4e5f6");
        assert_eq!(hits[0].author, "Writer Name");
    }

    #[test]
    fn urlencode_matches() {
        assert_eq!(urlencode("machine learning"), "machine%20learning");
        assert_eq!(urlencode("a&b"), "a%26b");
    }
}
