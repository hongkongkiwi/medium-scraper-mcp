# Medium Scraper MCP

A Model Context Protocol (MCP) server, written in Rust, for reading and searching Medium, converting posts to clean Markdown with paywall-aware fetching, agent-friendly batch web crawling, and multi-engine web search.

Version 2.0.0 is a comprehensive Rust rewrite of the original TypeScript server, updated with the latest feature sets from two reference projects:

- [mcp-medium-reader](https://github.com/ZhgChgLi/mcp-medium-reader) (ZhgChgLi): full-depth Markdown reading with frontmatter, post download to disk with images, paywall recovery guidance, custom-domain Medium URL detection.
- [crawl4ai-mcp](https://github.com/pazyork/crawl4ai-mcp) (pazyork): `fetch_urls` batch extraction with browser escalation and blocked-page detection, `search_web` multi-engine fallback, anti-bot realism knobs, video transcript extraction, optional LLM cleanup.

## Tools

| Tool | Description |
| --- | --- |
| `read_medium_post` | Read a Medium post by URL as clean Markdown with frontmatter (title, author, publication, published, tags, reading time). Handles `medium.com`, `*.medium.com`, and Medium publications on custom domains (`-a1b2c3d4e5f6` slug suffix detection). Member-only posts fall back to freedium/readmedium/archive; if bypass fails, the response includes a recovery notice. |
| `download_medium_post` | Save a post to disk: `<output_dir>/Output/medium-scraper-mcp/<date>-<slug>.md` plus images under `assets/<post_id>/`. Re-runs are skip-if-unchanged. |
| `get_article_info` | Article metadata as JSON: title, author, publication, published date, tags, reading time, word count, canonical URL, post id, paywall status. |
| `search_medium` | Search Medium posts by query and optional tag. Layered: Medium search HTML, then tag pages, then a DuckDuckGo `site:medium.com` fallback. Returns `{title, url, author, snippet}`. |
| `fetch_urls` | Fetch 1-20 URLs at once. Markdown or HTML output, main-content extraction, link extraction, blocked-page detection, automatic headless-Chrome fallback for blocked or JS-heavy pages, YouTube/Bilibili transcript extraction via yt-dlp, optional LLM cleanup. |
| `search_web` | Web search across 7 engines: google, bing, duckduckgo, baidu, sogou, so360, yandex. `auto` plans by query language (English starts on DuckDuckGo, Chinese starts on Baidu). Deduplicated by URL and fuzzy title (85% threshold). |
| `validate_setup` | Dependency and connectivity status: Medium/freedium reachability, cookies configured, Chrome present, yt-dlp present, LLM config. Optional live `test_url` read. |
| `medium_scraper` | Legacy compatibility tool from v1: one tool with `operation=search|convert|info`. |

## Installation

### Prebuilt binaries

Grab a binary from [Releases](https://github.com/hongkongkiwi/medium-scraper-mcp/releases), unzip, and point your MCP client at it.

### From source

```bash
cargo install --git https://github.com/hongkongkiwi/medium-scraper-mcp
# or clone and: cargo install --path .
```

### MCP client configuration

```json
{
  "mcpServers": {
    "medium-scraper": {
      "command": "/path/to/medium-scraper-mcp"
    }
  }
}
```

Codex (`config.toml`):

```toml
[mcp_servers.medium-scraper]
command = "/path/to/medium-scraper-mcp"
```

## Configuration (environment variables)

| Variable | Purpose | Default |
| --- | --- | --- |
| `MEDIUM_SCRAPER_PROXY` | Upstream proxy: `http://`, `https://`, `socks5://`, `host:port`, or port-only (normalized). `CRAWL4AI_MCP_PROXY` is accepted as an alias. | unset |
| `MEDIUM_SCRAPER_COOKIES` | Raw Cookie header for medium.com requests. Setting your signed-in `sid`/`uid` (plus `cf_clearance`) unlocks member-only posts directly. | unset |
| `MEDIUM_SCRAPER_TIMEOUT_MS` | Per-request HTTP timeout. | `30000` |
| `MEDIUM_SCRAPER_HEADLESS` | Launch the browser fallback headless. | `true` |
| `MEDIUM_SCRAPER_NO_BROWSER` | Set `1` to disable the Chrome fallback entirely. | unset |
| `MEDIUM_SCRAPER_MAX_CONTENT_CHARS` | Default truncation cap for `fetch_urls`. | `200000` |
| `MEDIUM_SCRAPER_USER_AGENT` | Pin a user agent; otherwise requests rotate a pool of modern UAs. | rotation |
| `MEDIUM_SCRAPER_ACCEPT_LANGUAGE` | Accept-Language header. | `en-US,en;q=0.9` |
| `MEDIUM_SCRAPER_YTDLP_COOKIES_FROM_BROWSER` | yt-dlp cookie source for video transcripts, e.g. `chrome`, `firefox:default`. | unset |
| `MEDIUM_SCRAPER_YTDLP_COOKIEFILE` | Netscape cookies.txt path for yt-dlp. | unset |
| `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL` | OpenAI-compatible endpoint for optional `use_llm` cleanup in `fetch_urls`. Off unless all three are set. | unset |
| `MEDIUM_SCRAPER_VERBOSE` | Set `1` for info-level stderr logging. | warnings only |

## How Medium fetching works

1. Direct fetch with browser-realistic headers and optional cookies, parse metadata (JSON-LD, OpenGraph) and the article body, render to Markdown.
2. If the post is member-only (paywall detected) and bypass is enabled: try `freedium.cfd`, then `readmedium.com`, then `archive.ph`, validating each returned body.
3. If every path returns only a preview, the tool returns the preview plus a recovery notice (cookie setup instructions), instead of silently returning truncated text.

The v1 paywall-bypass proxy chain is preserved and the mcp-medium-reader frontmatter shape and paywall-notice pattern are ported. The Ruby-gem cookie cache of mcp-medium-reader is replaced by the `MEDIUM_SCRAPER_COOKIES` variable: the MCP server stays a single static binary with no runtime dependencies.

## One-shot CLI

The same binary doubles as a stateless CLI (a port of crawl4ai-mcp's `crawl4agent`):

```bash
medium-scraper-mcp fetch "https://example.com/article" --format markdown
medium-scraper-mcp search "rust async" --tag rust --limit 5
medium-scraper-mcp search-web "agent framework" --engine auto --max-results 5
medium-scraper-mcp doctor
medium-scraper-mcp serve   # MCP stdio server (default when no subcommand is given)
```

## Development

```bash
cargo test
cargo clippy --all-targets
cargo build --release
```

Project layout:

```
src/
├── main.rs            # CLI: serve (default), fetch, search, search-web, doctor
├── server.rs          # rmcp server: 8 MCP tools
├── config.rs          # env config, proxy normalization
├── http.rs            # fetcher: UA rotation, proxy, cookies, retries
├── md.rs              # HTML -> Markdown renderer
├── extract.rs         # metadata, JSON-LD, main-content selection, blocked detection
├── medium/
│   ├── parse.rs       # Medium URL detection, article parsing, frontmatter
│   ├── fetch.rs       # direct + freedium/readmedium/archive chain
│   ├── search.rs      # Medium search + tag pages + DDG fallback
│   └── download.rs    # disk archive with images, skip-if-unchanged
├── crawl.rs           # fetch_urls engine: concurrency, escalation, cleanup
├── browser.rs         # chromiumoxide (CDP) fallback via system Chrome
├── search_web.rs      # 7-engine search with auto-fallback
├── video.rs           # yt-dlp transcript extraction (vtt/srt/json3/ttml)
└── llm.rs             # OpenAI-compatible cleanup prompts
```

## CI/CD

- `ci.yml`: build, test, clippy on every push/PR (macOS + Ubuntu).
- `release.yml`: on a `v*` tag, builds release binaries (macOS arm64/x86_64, Linux x86_64), generates SHA256SUMS, and publishes a GitHub release.

Release process:

```bash
git tag v2.0.x && git push origin v2.0.x
```

## Acknowledgements

Feature design adapted from [ZhgChgLi/mcp-medium-reader](https://github.com/ZhgChgLi/mcp-medium-reader) (MIT) and [pazyork/crawl4ai-mcp](https://github.com/pazyork/crawl4ai-mcp) (AGPL-3.0). This Rust implementation was written independently; see each project for its own terms. The original v1 TypeScript server by Hong Kong Kiwi is preserved in git history.

## License

MIT. See [LICENSE](./LICENSE).

## Disclaimer

All content fetched or downloaded through this server belongs to its respective owners and is subject to copyright. This tool does not claim ownership of any fetched content and does not condone copyright infringement. Users are solely responsible for ensuring they have the necessary permissions and rights for any content they fetch or download, and for complying with Medium's [Terms of Service](https://policy.medium.com/medium-terms-of-service-9db0094a1e0f).
