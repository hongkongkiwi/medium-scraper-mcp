use crate::extract;
use crate::md::{html_to_markdown, MdOptions};
use scraper::{Html, Selector};

/// Parsed Medium article: metadata plus rendered markdown body.
#[derive(Debug, Clone)]
pub struct Article {
    pub meta: ArticleMeta,
    pub body_markdown: String,
    /// How the body was obtained (direct, freedium, readmedium, archive).
    pub via: String,
    pub paywalled: bool,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ArticleMeta {
    pub title: String,
    pub author: String,
    pub publication: Option<String>,
    pub published: Option<String>,
    pub tags: Vec<String>,
    pub url: String,
    pub canonical: Option<String>,
    pub post_id: Option<String>,
    pub reading_time: Option<String>,
    pub excerpt: Option<String>,
    pub word_count: usize,
}

/// True for medium.com, *.medium.com, /p/<id> paths, and custom-domain posts
/// whose slug ends in the canonical 12-hex-char Medium suffix.
pub fn is_medium_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    if host == "medium.com" || host.ends_with(".medium.com") {
        // tag streams and other non-post paths are not articles
        return !parsed.path().starts_with("/tag/");
    }
    if parsed.path().starts_with("/p/") {
        return true;
    }
    slug_post_id(url).is_some()
}

/// 12-hex-char post id from a `-a1b2c3d4e5f6` slug suffix or a /p/<id> path.
pub fn slug_post_id(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let last = parsed.path_segments()?.next_back()?.to_string();
    if parsed.path().starts_with("/p/") {
        let id = last.trim();
        if id.len() == 8 || id.len() == 12 {
            return Some(id.to_string());
        }
    }
    let after_last_dash = last.rsplit('-').next()?;
    if after_last_dash.len() == 12 && after_last_dash.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(after_last_dash.to_string())
    } else {
        None
    }
}

pub fn slug(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    parsed
        .path_segments()?
        .next_back()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// Medium markup selector list for the article body, in preference order.
const BODY_SELECTORS: &[&str] = &[
    "article",
    "div[data-testid='storyContainer']",
    "section[data-field='body']",
    "div[class*='postArticle-content']",
    ".article-content",
    ".post-content",
    "main",
];

pub fn parse_article(html: &str, url: &str, opts: MdOptions) -> (ArticleMeta, String, bool) {
    let doc = Html::parse_document(html);
    let mut meta = extract::parse_meta(&doc);
    extract::parse_jsonld(&doc, &mut meta);

    let mut tags: Vec<String> = Vec::new();
    if let Ok(sel) = Selector::parse("a[href*='/tag/']") {
        for a in doc.select(&sel) {
            if let Some(href) = a.value().attr("href") {
                if let Some(tag) = href.split("/tag/").nth(1) {
                    let tag = tag.trim_matches('/').split('?').next().unwrap_or(tag);
                    if !tag.is_empty() && tag.len() < 40 && !tags.contains(&tag.to_string()) {
                        tags.push(tag.to_string());
                    }
                }
            }
        }
    }
    if tags.is_empty() {
        if let Some(kw) = &meta.keywords {
            for k in kw.split(',').map(str::trim).filter(|k| !k.is_empty()) {
                if !tags.contains(&k.to_string()) {
                    tags.push(k.to_string());
                }
            }
        }
    }
    tags.truncate(8);

    // reading time: Medium markup or estimate from body
    let reading_time = doc
        .select(&Selector::parse("span[data-testid='readingTime']").unwrap())
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty());

    let (_body_root, body_md) = render_body(&doc, opts);
    let word_count = body_md.split_whitespace().count();

    let paywalled = detect_paywall(html, word_count);

    let canonical = meta.canonical.clone().unwrap_or_else(|| url.to_string());
    let post_id = slug_post_id(url).or_else(|| slug_post_id(&canonical));

    let article_meta = ArticleMeta {
        title: meta.title.clone().unwrap_or_else(|| "Untitled".to_string()),
        author: meta.author.clone().unwrap_or_else(|| "Unknown".to_string()),
        publication: meta.publisher.clone().or_else(|| publication_hint(url)),
        published: meta.published.clone(),
        tags,
        url: url.to_string(),
        canonical: Some(canonical),
        post_id,
        reading_time,
        excerpt: meta.description.clone(),
        word_count,
    };
    (article_meta, body_md, paywalled)
}

/// Find the main content element and convert; falls back to whole-document conversion.
fn render_body(doc: &Html, opts: MdOptions) -> (Option<scraper::ElementRef<'_>>, String) {
    for selector in BODY_SELECTORS {
        if let Ok(sel) = Selector::parse(selector) {
            if let Some(el) = doc.select(&sel).next() {
                let md = html_to_markdown(el, opts);
                if md.split_whitespace().count() >= 30 {
                    return (Some(el), md);
                }
            }
        }
    }
    if let Some(main) = extract::select_main(doc) {
        let md = html_to_markdown(main, opts);
        return (Some(main), md);
    }
    let md = html_to_markdown(doc.root_element(), opts);
    (None, md)
}

/// Member-only posts render only a preview: short body plus Medium's wall markers.
pub fn detect_paywall(html: &str, body_word_count: usize) -> bool {
    const MARKERS: &[&str] = &[
        "data-testid=\"paywall\"",
        "class=\"paywall\"",
        "metered-content",
        "premium-content",
        "Sign up for Early Access",
        "member-only story",
        "Read the full story",
        "Subscribe to read the full story",
        "Create a free Medium account",
    ];
    let lowered = html.to_ascii_lowercase();
    let marker_hit = MARKERS
        .iter()
        .any(|m| lowered.contains(&m.to_ascii_lowercase()));
    let thin_preview = body_word_count > 0 && body_word_count < 120;
    marker_hit
        && (thin_preview
            || html.contains("isLocked")
            || html.contains("lockedPostSource")
            || body_word_count < 250)
}

fn publication_hint(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_string();
    if host == "medium.com" || host.ends_with(".medium.com") {
        None
    } else {
        Some(host)
    }
}

/// Compose the final markdown document with frontmatter, ported from
/// ZMediumToMarkdown's output shape via mcp-medium-reader.
pub fn with_frontmatter(article: &Article) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("title: {}\n", yaml_str(&article.meta.title)));
    fm.push_str(&format!("author: {}\n", yaml_str(&article.meta.author)));
    if let Some(pub_name) = &article.meta.publication {
        fm.push_str(&format!("publication: {}\n", yaml_str(pub_name)));
    }
    if let Some(date) = &article.meta.published {
        fm.push_str(&format!("published: {}\n", yaml_str(date)));
    }
    if !article.meta.tags.is_empty() {
        fm.push_str(&format!(
            "tags: [{}]\n",
            article
                .meta
                .tags
                .iter()
                .map(|t| yaml_str(t))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    fm.push_str(&format!(
        "url: {}\n",
        yaml_str(article.meta.canonical.as_deref().unwrap_or(&article.meta.url))
    ));
    if let Some(rt) = &article.meta.reading_time {
        fm.push_str(&format!("reading_time: {}\n", yaml_str(rt)));
    }
    fm.push_str("---\n\n");
    fm.push_str(&article.body_markdown);
    fm
}

fn yaml_str(s: &str) -> String {
    if s.contains(": ") || s.contains('#') || s.contains('"') || s.starts_with('\'') {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_url_detection() {
        assert!(is_medium_url("https://medium.com/@user/post-123-a1b2c3d4e5f6"));
        assert!(is_medium_url("https://user.medium.com/post-123-a1b2c3d4e5f6"));
        assert!(is_medium_url("https://medium.com/p/055527a739dd"));
        assert!(is_medium_url(
            "https://towardsdatascience.com/some-title-here-a1b2c3d4e5f6"
        ));
        assert!(!is_medium_url("https://example.com/blog/post"));
        assert!(!is_medium_url("https://medium.com/tag/rust"));
    }

    #[test]
    fn post_id_extraction() {
        assert_eq!(
            slug_post_id("https://medium.com/@u/title-goes-here-a1b2c3d4e5f6"),
            Some("a1b2c3d4e5f6".to_string())
        );
        assert_eq!(
            slug_post_id("https://medium.com/p/055527a739dd"),
            Some("055527a739dd".to_string())
        );
        assert_eq!(slug_post_id("https://example.com/nope"), None);
    }

    #[test]
    fn parses_minimal_article() {
        let html = r#"<html><head>
            <meta property="og:title" content="Test Post">
            <meta name="author" content="Tester">
            <meta property="article:published_time" content="2025-03-01T00:00:00Z">
        </head><body>
            <article>
                <h1>Test Post</h1>
                <p>First paragraph with plenty of words to pass the threshold check.</p>
                <p>Second paragraph also with plenty of words to be safe here.</p>
                <a href="https://medium.com/tag/rust">rust</a>
            </article>
        </body></html>"#;
        let (meta, md, paywalled) = parse_article(html, "https://medium.com/@t/test-post-a1b2c3d4e5f6", MdOptions::default());
        assert_eq!(meta.title, "Test Post");
        assert_eq!(meta.author, "Tester");
        assert!(meta.tags.contains(&"rust".to_string()));
        assert!(md.contains("First paragraph"));
        assert!(!paywalled);
        assert_eq!(meta.post_id.as_deref(), Some("a1b2c3d4e5f6"));
    }

    #[test]
    fn frontmatter_shape() {
        let article = Article {
            meta: ArticleMeta {
                title: "T".into(),
                author: "A".into(),
                publication: Some("P".into()),
                published: Some("2025-01-01".into()),
                tags: vec!["x".into()],
                url: "https://medium.com/x".into(),
                canonical: None,
                post_id: None,
                reading_time: Some("2 min read".into()),
                excerpt: None,
                word_count: 10,
            },
            body_markdown: "Body".into(),
            via: "direct".into(),
            paywalled: false,
        };
        let md = with_frontmatter(&article);
        assert!(md.starts_with("---\ntitle: T\nauthor: A\npublication: P\npublished: 2025-01-01\ntags: [x]\nurl: https://medium.com/x\nreading_time: 2 min read\n---\n\nBody"));
    }
}
