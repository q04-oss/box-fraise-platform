/// Anthropic Messages API client.
///
/// API key is typed as `&str` (caller holds the SecretString and calls
/// `.expose_secret()` at the call site — the key never lives in this module).
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const API_URL:     &str = "https://api.anthropic.com/v1/messages";
const MODEL:       &str = "claude-sonnet-4-6";
const MAX_TOKENS:  u32  = 512;
const API_VERSION: &str = "2023-06-01";

#[derive(Serialize)]
struct Message<'a> {
    role:    &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct Request<'a> {
    model:      &'a str,
    max_tokens: u32,
    system:     &'a str,
    messages:   Vec<Message<'a>>,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

/// Send a single-turn message to the Anthropic API and return the text response.
/// Failures are mapped to AppError::Internal — callers must never surface raw
/// Anthropic error details to end users.
pub async fn ask(
    http:    &reqwest::Client,
    api_key: &str,
    system:  &str,
    query:   &str,
) -> AppResult<String> {
    let body = Request {
        model:      MODEL,
        max_tokens: MAX_TOKENS,
        system,
        messages: vec![Message { role: "user", content: query }],
    };

    let resp = http
        .post(API_URL)
        .header("x-api-key",        api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type",      "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Anthropic request: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        tracing::warn!(status = %status, "Anthropic API returned non-200");
        return Err(AppError::Internal(anyhow::anyhow!("Anthropic API error")));
    }

    let parsed: ApiResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Anthropic decode: {e}")))?;

    parsed
        .content
        .into_iter()
        .find(|b| b.kind == "text")
        .and_then(|b| b.text)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Anthropic returned no text block")))
}
