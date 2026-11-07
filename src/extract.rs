use scraper::{ElementRef, Html, Selector};

/// Metadata harvested from meta tags and JSON-LD.
#[derive(Debug, Default, Clone)]
pub struct PageMeta {
    pub title: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub published: Option<String>,
    pub canonical: Option<String>,
    pub publisher: Option<String>,
    pub keywords: Option<String>,
}

pub fn parse_meta(doc: &Html) -> PageMeta {
    let mut meta = PageMeta::default();
    let get = |sel_name: &str, attr: &str| -> Option<String> {
        let sel = Selector::parse(sel_name).ok()?;
        doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr(attr))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    meta.title = get("meta[property='og:title']", "content")
        .or_else(|| get("meta[name='twitter:title']", "content"))
        .or_else(|| get("title", "x-nonexistent"));
    if meta.title.is_none() {
        let tsel = Selector::parse("title").ok();
        if let Some(tsel) = tsel {
            meta.title = doc
                .select(&tsel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty());
        }
    }
    meta.description = get("meta[property='og:description']", "content")
        .or_else(|| get("meta[name='description']", "content"));
    meta.author = get("meta[name='author']", "content")
        .or_else(|| get("meta[property='article:author']", "content"));
    meta.published = get("meta[property='article:published_time']", "content")
        .or_else(|| get("meta[name='parsely-pub-date']", "content"));
    meta.canonical = get("link[rel='canonical']", "href")
        .or_else(|| get("meta[property='og:url']", "content"));
    meta.keywords = get("meta[name='keywords']", "content")
        .or_else(|| get("meta[property='article:tag']", "content"));
    meta
}

/// Extract the headline/author/publisher/date from JSON-LD blocks when present.
pub fn parse_jsonld(doc: &Html, meta: &mut PageMeta) {
    let sel = match Selector::parse("script[type='application/ld+json']") {
        Ok(s) => s,
        Err(_) => return,
    };
    for script in doc.select(&sel) {
        let raw = script.text().collect::<String>();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        // flatten @graph arrays
        let mut items: Vec<serde_json::Value> = Vec::new();
        match &value {
            serde_json::Value::Array(arr) => items.extend(arr.iter().cloned()),
            v @ serde_json::Value::Object(_) => items.push(v.clone()),
            _ => {}
        }
        let mut graph_items = Vec::new();
        for it in &items {
            if let Some(graph) = it.get("@graph").and_then(|g| g.as_array()) {
                graph_items.extend(graph.iter().cloned());
            }
        }
        items.extend(graph_items);
        for item in &items {
            let ty = item
                .get("@type")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let is_article = ty.contains("article") || ty == "blogposting" || ty == "newsarticle";
            if is_article {
                if meta.title.is_none() {
                    meta.title = str_field(item, "headline");
                }
                if meta.description.is_none() {
                    meta.description = str_field(item, "description");
                }
                if meta.published.is_none() {
                    meta.published = str_field(item, "datePublished");
                }
                if meta.author.is_none() {
                    meta.author = item.get("author").and_then(jsonld_name);
                }
                if meta.publisher.is_none() {
                    meta.publisher = item.get("publisher").and_then(jsonld_name);
                }
                if meta.keywords.is_none() {
                    meta.keywords = item.get("keywords").and_then(|k| match k {
                        serde_json::Value::String(s) => Some(s.clone()),
                        serde_json::Value::Array(a) => Some(
                            a.iter()
                                .filter_map(|x| x.as_str())
                                .collect::<Vec<_>>()
                                .join(","),
                        ),
                        _ => None,
                    });
                }
            }
            if meta.title.is_none() {
                meta.title = str_field(item, "name");
            }
        }
    }
}

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn jsonld_name(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(o) => o
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string()),
        serde_json::Value::Array(a) => a.first().and_then(jsonld_name),
        _ => None,
    }
}

fn try_select<'a>(doc: &'a Html, selector: &str) -> Vec<ElementRef<'a>> {
    Selector::parse(selector)
        .map(|s| doc.select(&s).collect())
        .unwrap_or_default()
}

/// Readability-lite: pick the element with the most paragraph text.
pub fn select_main(doc: &Html) -> Option<ElementRef<'_>> {
    let mut candidates: Vec<(usize, ElementRef)> = Vec::new();
    for el in try_select(doc, "article") {
        candidates.push((text_score(el), el));
    }
    for el in try_select(doc, "main, [role='main']") {
        candidates.push((text_score(el), el));
    }
    for el in try_select(doc, "div, section") {
        let cls = el.value().classes().collect::<Vec<_>>().join(" ");
        let looks_like_content = cls.contains("article")
            || cls.contains("post-content")
            || cls.contains("entry-content")
            || cls.contains("story")
            || cls.contains("body-content");
        if looks_like_content {
            candidates.push((text_score(el), el));
        }
    }
    candidates.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
    let best = candidates.into_iter().next()?;
    if best.0 >= 40 {
        Some(best.1)
    } else {
        None
    }
}

fn text_score(el: ElementRef) -> usize {
    let sel = match Selector::parse("p, h1, h2, h3, h4, li, pre, blockquote") {
        Ok(s) => s,
        Err(_) => return 0,
    };
    el.select(&sel)
        .map(|p| p.text().collect::<String>().trim().len())
        .sum()
}

pub fn extract_title(doc: &Html) -> Option<String> {
    let sel = Selector::parse("title").ok()?;
    doc.select(&sel)
        .next()
        .map(|t| t.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// (text, href) pairs, normalized, deduped, no mailto/javascript.
pub fn extract_links(doc: &Html) -> Vec<(String, String)> {
    let sel = match Selector::parse("a[href]") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for a in doc.select(&sel) {
        let href = a.value().attr("href").unwrap_or("").trim();
        if href.is_empty()
            || href.starts_with("mailto:")
            || href.starts_with("javascript:")
            || href.starts_with("tel:")
        {
            continue;
        }
        if seen.insert(href.to_string()) {
            let text = collapse_ws(&a.text().collect::<String>());
            out.push((text, href.to_string()));
        }
    }
    out
}

pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn text_len(el: ElementRef) -> usize {
    el.text().collect::<String>().trim().len()
}

pub fn visible_text_len(html: &str) -> usize {
    let doc = Html::parse_fragment(html);
    doc.root_element().text().collect::<String>().trim().len()
}

/// Port of crawl4ai-mcp's blocked/interstitial detection.
pub fn is_blocked(content: &str) -> bool {
    let s = content.trim();
    if s.is_empty() {
        return true;
    }
    const STRONG: &[&str] = &[
        "当前环境异常",
        "完成验证后即可继续访问",
        "执行安全验证",
        "Verify you are human",
        "Verifying you are human",
        "Checking your browser",
        "Just a moment",
        "Attention Required! | Cloudflare",
        "请输入验证码",
    ];
    const WEAK: &[&str] = &[
        "去验证",
        "请开启 JavaScript",
        "Please enable JavaScript",
        "Access Denied",
        "正在等待",
        "Cloudflare",
        "captcha",
        "CAPTCHA",
    ];
    if STRONG.iter().any(|m| s.contains(m)) {
        return true;
    }
    if s.len() < 4_000 {
        let weak_hits = WEAK.iter().filter(|m| s.contains(**m)).count();
        if weak_hits >= 2 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_from_og_tags() {
        let html = r#"<html><head>
            <title>Doc</title>
            <meta property="og:title" content="Real Title">
            <meta property="og:description" content="Desc here">
            <meta name="author" content="Jane Writer">
            <meta property="article:published_time" content="2025-01-30T10:00:00Z">
            <link rel="canonical" href="https://example.com/canonical">
        </head><body></body></html>"#;
        let doc = Html::parse_document(html);
        let meta = parse_meta(&doc);
        assert_eq!(meta.title.as_deref(), Some("Real Title"));
        assert_eq!(meta.description.as_deref(), Some("Desc here"));
        assert_eq!(meta.author.as_deref(), Some("Jane Writer"));
        assert_eq!(meta.published.as_deref(), Some("2025-01-30T10:00:00Z"));
        assert_eq!(meta.canonical.as_deref(), Some("https://example.com/canonical"));
    }

    #[test]
    fn jsonld_article_fields() {
        let html = r#"<html><head><script type="application/ld+json">
            {"@type":"BlogPosting","headline":"LD Title","datePublished":"2025-02-01",
             "author":{"@type":"Person","name":"LD Author"},
             "publisher":{"@type":"Organization","name":"LD Pub"},
             "keywords":["rust","mcp"]}
        </script></head><body></body></html>"#;
        let doc = Html::parse_document(html);
        let mut meta = PageMeta::default();
        parse_jsonld(&doc, &mut meta);
        assert_eq!(meta.title.as_deref(), Some("LD Title"));
        assert_eq!(meta.author.as_deref(), Some("LD Author"));
        assert_eq!(meta.publisher.as_deref(), Some("LD Pub"));
        assert_eq!(meta.keywords.as_deref(), Some("rust,mcp"));
    }

    #[test]
    fn select_main_prefers_article() {
        let html = r#"<html><body>
            <nav><p>tiny nav</p></nav>
            <article><p>{big}</p><p>{second long paragraph here for scoring}</p></article>
            <footer><p>footer</p></footer>
        </body></html>"#;
        let doc = Html::parse_document(html);
        let main = select_main(&doc);
        assert!(main.is_some(), "article with real text should be selected");
    }

    #[test]
    fn blocked_detection() {
        assert!(is_blocked("Just a moment..."));
        assert!(is_blocked("Verify you are human"));
        assert!(!is_blocked("An article about Cloudflare architecture with lots of text. ".repeat(50).as_str()));
        assert!(is_blocked(""));
    }

    #[test]
    fn weak_markers_need_cooccurrence() {
        let small = "Please enable JavaScript. Cloudflare";
        assert!(is_blocked(small));
    }
}
