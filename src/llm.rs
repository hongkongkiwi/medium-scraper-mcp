use crate::config::OpenAiConfig;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    response_format: ResponseFormat,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
    #[serde(skip_serializing)]
    _marker: std::marker::PhantomData<&'a ()>,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatOutMessage,
}

#[derive(Debug, Deserialize)]
struct ChatOutMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CleanupOut {
    title: Option<String>,
    markdown: Option<String>,
}

/// The two system prompts are ports of crawl4ai-mcp's constraint-heavy filter
/// and generic cleanup pass.
fn system_prompt(instruction: Option<&str>) -> String {
    match instruction.map(str::trim).filter(|s| !s.is_empty()) {
        Some(extra) => format!(
            "You FILTER an extracted Markdown document. \
             Return ONLY a JSON object with keys: title, markdown. \
             CRITICAL: Preserve original content. \
             Output markdown MUST be exact lines copied from CONTENT, in the same order. \
             You may ONLY delete lines/blocks. \
             Do NOT rewrite, paraphrase, translate, or reformat kept lines. \
             Preserve code fences and code lines exactly. \
             Remove: navigation/header/footer boilerplate and sidebars. \
             Remove: ads/marketing/promotions, app download prompts, login prompts. \
             Remove: share/follow/subscribe CTAs. \
             Remove: decorative/unrelated links/images (logos/icons/social buttons). \
             Keep: main article/tutorial text. \
             Keep: in-body citations/references (links that support the text). \
             Keep: meaningful in-body images supporting nearby text. \
             If unsure, KEEP the line. \
             Additional instruction: {extra}"
        ),
        None => "You extract the main readable content from a webpage crawl. \
             Return JSON with keys: title, markdown. \
             markdown must be clean and keep code blocks."
            .to_string(),
    }
}

pub struct CleanupResult {
    pub title: Option<String>,
    pub markdown: String,
}

/// Post-process extracted markdown with an OpenAI-compatible chat model.
/// Returns Err with a message suitable for the tool's llm_error field.
pub async fn cleanup(
    cfg: &OpenAiConfig,
    url: &str,
    title: Option<&str>,
    content: &str,
    instruction: Option<&str>,
) -> Result<CleanupResult> {
    if content.trim().is_empty() {
        return Err(anyhow!("nothing to clean: content is empty"));
    }
    let req = ChatRequest {
        model: &cfg.model,
        messages: vec![
            ChatMessage {
                role: "system",
                content: system_prompt(instruction),
                _marker: std::marker::PhantomData,
            },
            ChatMessage {
                role: "user",
                content: format!(
                    "URL: {}\nTITLE: {}\n\nCONTENT:\n{}\n",
                    url,
                    title.unwrap_or(""),
                    content
                ),
                _marker: std::marker::PhantomData,
            },
        ],
        temperature: 0.0,
        response_format: ResponseFormat { kind: "json_object" },
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let endpoint = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let resp = client
        .post(&endpoint)
        .bearer_auth(&cfg.api_key)
        .json(&req)
        .send()
        .await
        .with_context(|| format!("LLM request to {endpoint} failed"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("LLM endpoint returned HTTP {status}: {}", truncate(&body, 300)));
    }
    let parsed: ChatResponse = resp.json().await.context("LLM response parse")?;
    let content = parsed
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .ok_or_else(|| anyhow!("LLM response had no message content"))?;
    let cleaned: CleanupOut = serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| anyhow!("LLM output was not the expected JSON object"))?;
    Ok(CleanupResult {
        title: cleaned.title,
        markdown: cleaned.markdown.unwrap_or_else(|| content.clone()),
    })
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...", &s[..n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_prompt_includes_instruction() {
        let p = system_prompt(Some("keep only tutorials"));
        assert!(p.contains("keep only tutorials"));
        assert!(p.contains("You may ONLY delete"));
        let generic = system_prompt(None);
        assert!(generic.contains("main readable content"));
    }

    #[test]
    fn chat_request_serializes() {
        let req = ChatRequest {
            model: "m",
            messages: vec![ChatMessage {
                role: "system",
                content: "x".into(),
                _marker: std::marker::PhantomData,
            }],
            temperature: 0.0,
            response_format: ResponseFormat { kind: "json_object" },
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["response_format"]["type"], "json_object");
        assert_eq!(v["messages"][0]["role"], "system");
    }
}
