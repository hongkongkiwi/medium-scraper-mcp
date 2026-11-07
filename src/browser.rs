use crate::config::Config;
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

/// Locate a Chrome/Chromium executable across macOS and Linux.
pub fn find_chrome() -> Option<PathBuf> {
    for name in [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "chrome",
    ] {
        if let Ok(path) = which::which(name) {
            return Some(path);
        }
    }
    let home = dirs::home_dir()?;
    let mac_candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
    ];
    for c in mac_candidates {
        let p = PathBuf::from(&c);
        if p.exists() {
            return Some(p);
        }
    }
    let linux_candidates = ["/usr/bin/google-chrome", "/usr/bin/chromium-browser", "/usr/bin/chromium"];
    for c in linux_candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    let _ = home;
    None
}

pub struct RenderedPage {
    pub html: String,
    pub final_url: String,
}

/// Render a URL in a real headless Chrome (CDP). Used as the escalation path
/// for blocked or JS-only pages, mirroring crawl4ai-mcp's stronger fallback.
pub async fn render(cfg: &Config, url: &str) -> Result<RenderedPage> {
    let executable = find_chrome().ok_or_else(|| {
        anyhow!("no Chrome/Chromium found; install Chrome or set MEDIUM_SCRAPER_NO_BROWSER=0 with a browser present")
    })?;
    let width: u32 = 980 + (rand::random::<u16>() % 500) as u32;
    let height: u32 = 640 + (rand::random::<u16>() % 340) as u32;

    let mut builder = chromiumoxide::browser::BrowserConfig::builder()
        .chrome_executable(executable)
        .window_size(width, height)
        .no_sandbox();
    if cfg.headless {
        builder = builder.arg("--headless=new");
    }
    if let Some(proxy) = &cfg.proxy {
        builder = builder.arg(format!("--proxy-server={proxy}"));
    }
    let config = builder
        .build()
        .map_err(|e| anyhow!("browser config: {e}"))?;

    let (mut browser, mut handler) = chromiumoxide::Browser::launch(config).await?;
    let handler_handle = tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    let render_result: Result<RenderedPage> = async {
        let page = browser.new_page(url).await.context("open page")?;
        page.wait_for_navigation().await.ok();
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        let html = page.content().await.context("page content")?;
        let final_url = page
            .url()
            .await
            .ok()
            .flatten()
            .map(|u| u.to_string())
            .unwrap_or_else(|| url.to_string());
        Ok(RenderedPage { html, final_url })
    }
    .await;

    let _ = browser.close().await;
    let _ = handler_handle.await;
    render_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_detection_runs() {
        // Either finds a browser or cleanly returns None; must not panic.
        let _ = find_chrome();
    }
}
