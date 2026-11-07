use scraper::{ElementRef, Html, Selector};

/// Options for HTML to markdown conversion.
#[derive(Debug, Clone, Copy)]
pub struct MdOptions {
    pub include_images: bool,
    pub include_code: bool,
}

impl Default for MdOptions {
    fn default() -> Self {
        Self {
            include_images: true,
            include_code: true,
        }
    }
}

pub fn html_to_markdown(root: ElementRef, opts: MdOptions) -> String {
    let mut r = Renderer {
        out: String::new(),
        list_stack: Vec::new(),
        quote_depth: 0,
        opts,
    };
    r.render_children_block(root);
    r.finish()
}

pub fn parse_fragment(md_html: &str) -> String {
    let doc = Html::parse_fragment(md_html);
    match doc.root_element().children().find(|c| {
        ElementRef::wrap(*c)
            .map(|e| e.value().name() != "html" && e.value().name() != "head")
            .unwrap_or(false)
    }) {
        Some(root) => html_to_markdown(ElementRef::wrap(root).unwrap(), MdOptions::default()),
        None => String::new(),
    }
}

struct Renderer {
    out: String,
    list_stack: Vec<(char, u64)>,
    quote_depth: usize,
    opts: MdOptions,
}

impl Renderer {
    fn finish(self) -> String {
        let mut out = String::new();
        let mut blanks = 0;
        for line in self.out.lines() {
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
        out.trim().to_string() + "\n"
    }

    fn push_block(&mut self, s: &str) {
        let s = s.trim();
        if s.is_empty() {
            return;
        }
        self.out.push('\n');
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn render_children_block(&mut self, el: ElementRef) {
        for child in el.children() {
            match child.value() {
                scraper::node::Node::Text(t) => {
                    let text = collapse_ws(t);
                    if !text.is_empty() {
                        self.push_block(&text);
                    }
                }
                scraper::node::Node::Element(_) => {
                    if let Some(child_ref) = ElementRef::wrap(child) {
                        self.render_element_block(child_ref);
                    }
                }
                _ => {}
            }
        }
    }

    fn inline_children(&mut self, el: ElementRef) -> String {
        let mut inline = InlineCollector {
            depth: 0,
            opts: self.opts,
        };
        inline.collect(el)
    }
}

const BLOCK_TAGS: &[&str] = &[
    "p", "div", "section", "article", "header", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol",
    "li", "blockquote", "pre", "figure", "figcaption", "table", "hr", "main",
];

impl Renderer {
    fn render_element_block(&mut self, el: ElementRef) {
        let name = el.value().name().to_string();
        match name.as_str() {
            "script" | "style" | "noscript" | "svg" | "button" | "nav" | "footer" | "aside"
            | "form" | "input" | "select" | "label" => {}
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level: usize = name[1..2].parse().unwrap_or(2);
                let text = self.inline_children(el);
                let t = text.trim();
                if !t.is_empty() {
                    self.push_block(&format!("{} {}", "#".repeat(level), t));
                }
            }
            "p" => {
                let text = self.inline_children(el);
                self.push_block(&text);
            }
            "blockquote" => {
                self.quote_depth += 1;
                let saved = std::mem::take(&mut self.out);
                self.render_children_block(el);
                let inner = std::mem::replace(&mut self.out, saved);
                self.quote_depth -= 1;
                let _ = self.quote_depth;
                for line in inner.lines() {
                    self.out.push_str("> ");
                    self.out.push_str(line);
                    self.out.push('\n');
                }
            }
            "pre" => {
                if !self.opts.include_code {
                    return;
                }
                let code_el = el.select(&sel("code")).next();
                let lang = code_el
                    .and_then(|c| c.value().attr("class").map(|s| s.to_string()))
                    .and_then(|cls| extract_lang(&cls));
                let text = code_el
                    .map(|c| c.text().collect::<String>())
                    .unwrap_or_else(|| el.text().collect::<String>());
                self.push_block(&format!(
                    "```{}\n{}\n```",
                    lang.unwrap_or_default(),
                    text.trim_end()
                ));
            }
            "ul" | "ol" => {
                let marker = if name == "ul" { '-' } else { '1' };
                let counter_start = if name == "ol" {
                    el.value()
                        .attr("start")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(1)
                } else {
                    1
                };
                self.list_stack.push((marker, counter_start));
                for li in el.children() {
                    if let Some(li_el) =
                        ElementRef::wrap(li).filter(|l| l.value().name() == "li")
                    {
                        self.render_li(li_el);
                    }
                }
                self.list_stack.pop();
            }
            "figure" => self.render_figure(el),
            "figcaption" => {
                let text = collapse_ws(&el.text().collect::<String>());
                if !text.is_empty() {
                    self.push_block(&format!("*{}*", text));
                }
            }
            "hr" => self.push_block("---"),
            "table" => self.render_table(el),
            "iframe" => {
                if let Some(src) = el.value().attr("src") {
                    if src.starts_with("http") {
                        self.push_block(&format!("[Embedded content]({})", src));
                    }
                }
            }
            "img" => {
                if let Some(md) = self.img_markdown(el) {
                    self.push_block(&md);
                }
            }
            _ => {
                let has_block = el.children().any(|c| {
                    ElementRef::wrap(c)
                        .map(|e| BLOCK_TAGS.contains(&e.value().name()))
                        .unwrap_or(false)
                });
                if has_block {
                    self.render_children_block(el);
                } else {
                    let text = self.inline_children(el);
                    self.push_block(&text);
                }
            }
        }
    }

    fn render_figure(&mut self, el: ElementRef) {
        if let Some(img) = el.select(&sel("img")).next() {
            if let Some(md) = self.img_markdown(img) {
                self.push_block(&md);
            }
        }
        if let Some(cap) = el.select(&sel("figcaption")).next() {
            let text = collapse_ws(&cap.text().collect::<String>());
            if !text.is_empty() {
                self.push_block(&format!("*{}*", text));
            }
        }
    }

    fn render_li(&mut self, li: ElementRef) {
        let depth = self.list_stack.len().saturating_sub(1);
        let indent = "  ".repeat(depth);
        let (marker, mut counter) = *self.list_stack.last().unwrap_or(&('-', 1));
        let prefix = if marker == '1' {
            format!("{indent}{counter}. ")
        } else {
            format!("{indent}- ")
        };
        let mut inline = String::new();
        let mut sub_blocks: Vec<String> = Vec::new();
        for child in li.children() {
            match child.value() {
                scraper::node::Node::Text(t) => inline.push_str(&collapse_ws(t)),
                scraper::node::Node::Element(e) => {
                    let n = e.name();
                    if n == "ul" || n == "ol" {
                        self.list_stack.push((if n == "ul" { '-' } else { '1' }, 1));
                        let saved = std::mem::take(&mut self.out);
                        for sub_li in ElementRef::wrap(child).unwrap().children() {
                            if let Some(sub_el) =
                                ElementRef::wrap(sub_li).filter(|l| l.value().name() == "li")
                            {
                                self.render_li(sub_el);
                            }
                        }
                        sub_blocks.push(std::mem::replace(&mut self.out, saved));
                        self.list_stack.pop();
                    } else {
                        inline.push_str(&self.inline_children(ElementRef::wrap(child).unwrap()));
                        inline.push(' ');
                    }
                }
                _ => {}
            }
        }
        let li_text = collapse_ws(&inline);
        self.out.push_str(&prefix);
        self.out.push_str(&li_text);
        self.out.push('\n');
        for b in sub_blocks {
            for line in b.lines() {
                if !line.trim().is_empty() {
                    self.out.push_str(&indent);
                    self.out.push_str(line);
                    self.out.push('\n');
                }
            }
        }
        if marker == '1' {
            counter += 1;
            if let Some(last) = self.list_stack.last_mut() {
                last.1 = counter;
            }
        }
    }

    fn render_table(&mut self, el: ElementRef) {
        let rows: Vec<Vec<String>> = el
            .select(&sel("tr"))
            .map(|tr| {
                tr.select(&sel("th,td"))
                    .map(|cell| collapse_ws(&cell.text().collect::<String>()))
                    .collect()
            })
            .collect();
        if rows.is_empty() {
            return;
        }
        let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if width == 0 {
            return;
        }
        let mut lines: Vec<String> = Vec::new();
        let header: Vec<String> = rows[0].clone();
        lines.push(format!("| {} |", header.join(" | ")));
        lines.push(format!("|{}|", vec![" --- "; width].join("|")));
        for row in rows.iter().skip(1) {
            let mut padded = row.clone();
            padded.resize(width, String::new());
            lines.push(format!("| {} |", padded.join(" | ")));
        }
        self.push_block(&lines.join("\n"));
    }

    fn img_markdown(&self, img: ElementRef) -> Option<String> {
        if !self.opts.include_images {
            return None;
        }
        let src = pick_img_src(img)?;
        let alt = img.value().attr("alt").unwrap_or("").trim().to_string();
        Some(format!("![{}]({})", escape_alt(&alt), src))
    }
}

struct InlineCollector {
    depth: usize,
    opts: MdOptions,
}

impl InlineCollector {
    fn collect(&mut self, el: ElementRef) -> String {
        if self.depth > 24 {
            return String::new();
        }
        let mut out = String::new();
        for child in el.children() {
            match child.value() {
                scraper::node::Node::Text(t) => out.push_str(&collapse_ws(t)),
                scraper::node::Node::Element(e) => {
                    let name = e.name();
                    match name {
                        "script" | "style" | "noscript" | "svg" | "button" => {}
                        "br" => out.push(' '),
                        "strong" | "b" => {
                            self.depth += 1;
                            let inner = self.collect(ElementRef::wrap(child).unwrap());
                            self.depth -= 1;
                            let t = inner.trim();
                            if !t.is_empty() {
                                out.push_str(&format!(" **{}** ", t));
                            }
                        }
                        "em" | "i" => {
                            self.depth += 1;
                            let inner = self.collect(ElementRef::wrap(child).unwrap());
                            self.depth -= 1;
                            let t = inner.trim();
                            if !t.is_empty() {
                                out.push_str(&format!(" *{}* ", t));
                            }
                        }
                        "del" | "s" | "strike" => {
                            self.depth += 1;
                            let inner = self.collect(ElementRef::wrap(child).unwrap());
                            self.depth -= 1;
                            let t = inner.trim();
                            if !t.is_empty() {
                                out.push_str(&format!(" ~~{}~~ ", t));
                            }
                        }
                        "code" => {
                            let code_el = ElementRef::wrap(child).unwrap();
                            let raw = code_el.text().collect::<String>();
                            let t = raw.trim();
                            if !t.is_empty() {
                                out.push_str(&format!(" `{}` ", t));
                            }
                        }
                        "a" => {
                            self.depth += 1;
                            let inner = self.collect(ElementRef::wrap(child).unwrap());
                            self.depth -= 1;
                            let href = e.attr("href").unwrap_or("");
                            if href.starts_with("javascript:") || href.is_empty() {
                                out.push_str(&inner);
                            } else {
                                let t = inner.trim();
                                if t.is_empty() {
                                    if href.starts_with("http") {
                                        out.push_str(&format!(" <{}> ", href));
                                    }
                                } else {
                                    out.push_str(&format!(" [{}]({}) ", t, href));
                                }
                            }
                        }
                        "img" => {
                            if self.opts.include_images {
                                if let Some(src) =
                                    pick_img_src(ElementRef::wrap(child).unwrap())
                                {
                                    let alt = e.attr("alt").unwrap_or("").trim();
                                    out.push_str(&format!(" ![{alt}]({src}) "));
                                }
                            }
                        }
                        _ => {
                            self.depth += 1;
                            let inner = self.collect(ElementRef::wrap(child).unwrap());
                            self.depth -= 1;
                            out.push_str(&inner);
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

/// Best URL from srcset (widest) / data-src / src; skips data URIs and tracking pixels.
pub fn pick_img_src(img: ElementRef) -> Option<String> {
    let v = img.value();
    let skip = |u: &str| u.is_empty() || u.starts_with("data:");
    let srcset_best = |ss: &str| -> Option<String> {
        let mut best: Option<(u64, String)> = None;
        for cand in ss.split(',') {
            let mut parts = cand.split_whitespace();
            let url = parts.next()?;
            let width: u64 = parts
                .next()
                .and_then(|d| d.trim_end_matches('w').parse().ok())
                .unwrap_or(0);
            if best.as_ref().map_or(true, |(bw, _)| width > *bw) {
                best = Some((width, url.to_string()));
            }
        }
        best.map(|(_, u)| u)
    };
    let candidates = [
        v.attr("srcset").and_then(srcset_best),
        v.attr("data-src").map(|s| s.to_string()),
        v.attr("src").map(|s| s.to_string()),
        v.attr("data-echo").map(|s| s.to_string()),
        v.attr("data-original").map(|s| s.to_string()),
    ];
    candidates.into_iter().flatten().find(|u| !skip(u))
}

fn extract_lang(class: &str) -> Option<String> {
    for token in class.split_whitespace() {
        if let Some(rest) = token
            .strip_prefix("language-")
            .or_else(|| token.strip_prefix("lang-"))
        {
            return Some(rest.to_string());
        }
    }
    None
}

fn escape_alt(alt: &str) -> String {
    alt.replace('[', "(").replace(']', ")")
}

fn sel(selector: &str) -> Selector {
    Selector::parse(selector).expect("static selector")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(html: &str) -> String {
        let doc = Html::parse_document(html);
        let article = doc.select(&sel("article")).next().unwrap();
        html_to_markdown(article, MdOptions::default())
    }

    #[test]
    fn converts_basic_article() {
        let html = r#"<article>
            <h1>Hello</h1>
            <p>World <strong>bold</strong> and <em>italic</em>.</p>
            <ul><li>one</li><li>two</li></ul>
            <pre><code class="language-rust">fn main() {}</code></pre>
        </article>"#;
        let md = convert(html);
        assert!(md.contains("# Hello"), "md was: {md}");
        assert!(md.contains("**bold**"));
        assert!(md.contains("*italic*"));
        assert!(md.contains("- one"));
        assert!(md.contains("```rust"));
        assert!(md.contains("fn main() {}"));
    }

    #[test]
    fn blockquote_and_links() {
        let html = r#"<article><blockquote><p>Quote</p></blockquote><p>See <a href="https://x.com">X</a></p></article>"#;
        let md = convert(html);
        assert!(md.contains("> Quote"), "md was: {md}");
        assert!(md.contains("[X](https://x.com)"));
    }

    #[test]
    fn figures_and_captions() {
        let html = r#"<article><figure><img src="https://miro.medium.com/max/1400/x.png" alt="a"><figcaption>Caption</figcaption></figure></article>"#;
        let md = convert(html);
        assert!(
            md.contains("![a](https://miro.medium.com/max/1400/x.png)"),
            "md was: {md}"
        );
        assert!(md.contains("*Caption*"));
    }

    #[test]
    fn excludes_script_and_button() {
        let html = r#"<article><p>Keep</p><script>bad()</script><button>Sign up</button><p>Also keep</p></article>"#;
        let md = convert(html);
        assert!(!md.contains("bad()"));
        assert!(!md.contains("Sign up"));
        assert!(md.contains("Keep"));
    }

    #[test]
    fn nested_lists() {
        let html = r#"<article><ol><li>first</li><li>second<ul><li>inner</li></ul></li></ol></article>"#;
        let md = convert(html);
        assert!(md.contains("1. first"), "md was: {md}");
        assert!(md.contains("2. second"));
        assert!(md.contains("  - inner"));
    }

    #[test]
    fn srcset_picks_widest() {
        let html = r#"<article><p><img srcset="https://a/1.png 640w, https://a/2.png 1600w" src="https://a/1.png"></p></article>"#;
        let md = convert(html);
        assert!(md.contains("https://a/2.png"), "md was: {md}");
    }
}
