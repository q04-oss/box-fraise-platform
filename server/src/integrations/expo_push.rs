/// Expo push notification client.
use serde::Serialize;

use crate::error::{AppError, AppResult};

const EXPO_PUSH_URL: &str = "https://exp.host/--/api/v2/push/send";

#[derive(Debug, Serialize)]
pub struct PushMessage<'a> {
    pub to:    &'a str,
    pub title: Option<&'a str>,
    pub body:  &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data:  Option<serde_json::Value>,
    pub sound: &'a str,
}

impl<'a> Default for PushMessage<'a> {
    fn default() -> Self {
        Self { to: "", title: None, body: "", data: None, sound: "default" }
    }
}

/// Send a push notification. Logs warnings on non-fatal failures rather than
/// propagating — a failed push should never roll back a successful transaction.
pub async fn send(
    http: &reqwest::Client,
    msg:  PushMessage<'_>,
) -> AppResult<()> {
    if !is_expo_token(msg.to) {
        tracing::warn!(token = msg.to, "send_push: not an Expo token, skipping");
        return Ok(());
    }

    let resp = http
        .post(EXPO_PUSH_URL)
        .json(&msg)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("push request failed: {e}")))?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "push notification failed");
    }

    Ok(())
}

fn is_expo_token(token: &str) -> bool {
    token.starts_with("ExponentPushToken[") || token.starts_with("ExpoPushToken[")
}
