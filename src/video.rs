use crate::config::Config;
use crate::http::HttpFetcher;
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use std::sync::OnceLock;
use tokio::process::Command;

/// Video hosts whose pages are handled via yt-dlp subtitle extraction,
/// mirroring crawl4ai-mcp's video path.
pub fn is_supported_video_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_default();
    host.contains("youtube.com")
        || host == "youtu.be"
        || host.contains("bilibili.com")
        || host == "b23.tv"
        || (lower.contains("youtube.com/watch") || lower.contains("youtu.be/"))
}

pub struct VideoTranscript {
    pub title: Option<String>,
    pub uploader: Option<String>,
    pub duration_secs: Option<u64>,
    pub transcript_markdown: String,
    pub track_lang: Option<String>,
}

pub async fn transcript(cfg: &Config, http: &HttpFetcher, url: &str) -> Result<VideoTranscript> {
    let ytdlp = which::which("yt-dlp")
        .map_err(|_| anyhow!("yt-dlp is not installed; install it (pip install yt-dlp) for video transcript extraction"))?;
    let mut cmd = Command::new(&ytdlp);
    cmd.arg("--dump-json")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg(url)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(cb) = &cfg.ytdlp_cookies_from_browser {
        cmd.arg(format!("--cookies-from-browser={cb}"));
    }
    if let Some(cf) = &cfg.ytdlp_cookiefile {
        cmd.arg(format!("--cookies={cf}"));
    }
    if let Some(proxy) = &cfg.proxy {
        cmd.arg(format!("--proxy={proxy}"));
    }
    let out = cmd
        .output()
        .await
        .context("failed to run yt-dlp")?;
    if !out.status.success() {
        return Err(anyhow!(
            "yt-dlp failed (exit {:?}) for {url}",
            out.status.code()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first_line = stdout.lines().next().unwrap_or("{}");
    let info: serde_json::Value = serde_json::from_str(first_line).context("yt-dlp JSON parse")?;

    let title = info
        .get("title")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let uploader = info
        .get("uploader")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let duration = info.get("duration").and_then(|d| d.as_u64());

    // pick best track: manual subtitles over auto, en/zh first, vtt/srt first
    let mut best: Option<(i32, &str, &str, &serde_json::Value)> = None;
    for (source_name, source_rank) in [("subtitles", 0), ("automatic_captions", 1)] {
        let Some(tracks) = info.get(source_name).and_then(|t| t.as_object()) else {
            continue;
        };
        for (lang, variants) in tracks {
            let rank_lang = lang_rank(lang);
            for variant in variants.as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                let Some(ext) = variant.get("ext").and_then(|e| e.as_str()) else {
                    continue;
                };
                let Some(track_url) = variant.get("url").and_then(|u| u.as_str()) else {
                    continue;
                };
                if !matches!(ext, "vtt" | "srt" | "json3" | "ttml" | "srv1") {
                    continue;
                }
                let score = source_rank * 1000 + rank_lang * 10 + ext_rank(ext);
                if best.as_ref().map_or(true, |(bs, _, _, _)| score < *bs) {
                    best = Some((score, lang, track_url, variant));
                }
            }
        }
    }

    let Some((_, lang, track_url, _)) = best else {
        return Err(anyhow!("no subtitle track found for {url}"));
    };
    let payload = http.get_bytes(track_url).await?;
    let text = String::from_utf8_lossy(&payload).to_string();
    let transcript = normalize_subtitle(&text, track_url);

    let md = format!(
        "# {}\n\n*Video transcript*\n- Source: {}\n- Uploader: {}\n- Duration: {}s\n\n{}",
        title.as_deref().unwrap_or("Video"),
        url,
        uploader.as_deref().unwrap_or("unknown"),
        duration.unwrap_or(0),
        transcript
    );

    Ok(VideoTranscript {
        title,
        uploader,
        duration_secs: duration,
        transcript_markdown: md,
        track_lang: Some(lang.to_string()),
    })
}

fn lang_rank(lang: &str) -> i32 {
    let l = lang.to_ascii_lowercase();
    if l == "en" || l.starts_with("en-") {
        0
    } else if l.starts_with("zh") {
        1
    } else if l.starts_with("ja") {
        2
    } else if l.starts_with("es") || l.starts_with("de") || l.starts_with("fr") {
        3
    } else {
        9
    }
}

fn ext_rank(ext: &str) -> i32 {
    match ext {
        "vtt" => 0,
        "srt" => 1,
        "json3" => 2,
        "ttml" => 3,
        _ => 9,
    }
}

/// Parse vtt/srt/json3/ttml payloads into deduped plain lines.
pub fn normalize_subtitle(payload: &str, track_url: &str) -> String {
    let ext = track_url.split('?').next().unwrap_or(track_url);
    let text = if ext.ends_with(".json3") || payload.trim_start().starts_with('{') {
        parse_json3(payload)
    } else if ext.ends_with(".ttml") || payload.trim_start().starts_with("<tt") {
        parse_ttml(payload)
    } else {
        parse_vtt_or_srt(payload)
    };
    dedupe_lines(&text)
}

fn parse_vtt_or_srt(payload: &str) -> String {
    let ts_re = timestamp_re();
    let mut out = String::new();
    for line in payload.lines() {
        let line = line.trim();
        if line.is_empty()
            || line == "WEBVTT"
            || line.starts_with("NOTE")
            || line.starts_with("Kind:")
            || line.starts_with("Language:")
            || ts_re.is_match(line)
            || line.chars().all(|c| c.is_ascii_digit() || c == ':')
                && line.len() < 12
        {
            continue;
        }
        let cleaned = strip_tags(line);
        if !cleaned.trim().is_empty() {
            out.push_str(cleaned.trim());
            out.push('\n');
        }
    }
    out
}

fn parse_json3(payload: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
        return String::new();
    };
    let mut out = String::new();
    if let Some(events) = v.get("events").and_then(|e| e.as_array()) {
        for ev in events {
            if let Some(segs) = ev.get("segs").and_then(|s| s.as_array()) {
                for seg in segs {
                    if let Some(t) = seg.get("utf8").and_then(|t| t.as_str()) {
                        if t.trim() != "\n" {
                            out.push_str(t);
                        }
                    }
                }
                out.push('\n');
            }
        }
    }
    out
}

fn parse_ttml(payload: &str) -> String {
    let br_re = br_re();
    let with_breaks = br_re.replace_all(payload, "\n");
    strip_tags(with_breaks.as_ref())
}

fn strip_tags(s: &str) -> String {
    let tag_re = tag_re();
    let style_re = style_re();
    let no_styles = style_re.replace_all(s, "");
    tag_re.replace_all(&no_styles, "").to_string()
}

fn dedupe_lines(text: &str) -> String {
    let mut out_lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if out_lines.last().map(|l: &String| l == line).unwrap_or(false) {
            continue;
        }
        out_lines.push(line.to_string());
    }
    out_lines.join("\n")
}

fn timestamp_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{1,2}:\d{2}(:\d{2})?[.,]?\d{0,3}\s*(-->|$)").unwrap())
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").unwrap())
}

fn style_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\\[^}]*\}").unwrap())
}

fn br_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<br\s*/?>").unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_urls() {
        assert!(is_supported_video_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_supported_video_url("https://youtu.be/abc"));
        assert!(is_supported_video_url("https://www.bilibili.com/video/BV123"));
        assert!(is_supported_video_url("https://b23.tv/abc"));
        assert!(!is_supported_video_url("https://example.com/video"));
    }

    #[test]
    fn vtt_parsing() {
        let vtt = "WEBVTT\nKind: captions\n\n00:00:01.000 --> 00:00:03.000\nHello <b>world</b>\n\n00:00:03.000 --> 00:00:05.000\nHello <b>world</b>\n\n00:00:05.000 --> 00:00:07.000\nSecond line\n";
        let out = normalize_subtitle(vtt, "https://example.com/cap.vtt");
        assert!(out.contains("Hello world"));
        assert!(!out.contains("-->"));
        // consecutive duplicate removed
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn json3_parsing() {
        let j3 = r#"{"events":[{"segs":[{"utf8":"Hi "},{"utf8":"there"}]},{"segs":[{"utf8":"Next"}]}]}"#;
        let out = normalize_subtitle(j3, "https://example.com/cap.json3");
        assert!(out.contains("Hi there"));
        assert!(out.contains("Next"));
    }

    #[test]
    fn lang_ranking_prefers_english_manual() {
        assert!(lang_rank("en") < lang_rank("zh-Hans"));
        assert!(lang_rank("zh") < lang_rank("fr"));
        assert!(ext_rank("vtt") < ext_rank("json3"));
    }
}
