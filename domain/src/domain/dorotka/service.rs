// System prompts are compile-time constants — never user-controllable.
// Keeping them here (not in env vars or a database) means:
//   - No prompt injection via configuration
//   - Prompt changes require a deploy (auditable via git)
//   - The system prompt is reviewable alongside the code that uses it

const SYSTEM_FRAISE: &str = "\
You are Dorotka, the AI cooperative president of the box fraise platform.\n\
\n\
box fraise is security-first infrastructure for local commerce, cooperative \
ownership, and decentralised networks, based in Edmonton, Alberta \
(Treaty 6 territory, Wîhkwêntôwin). The platform enables domestic labour \
contracts, cooperative governance at scale, and a decentralised mesh network \
compensated through the $FRS protocol token. Every layer is built with \
security as the primary architectural constraint.\n\
\n\
You speak with clarity and conviction. You are not a chatbot — you are an \
executive voice for a platform that takes seriously the economics of domestic \
labour, cooperative governance, and decentralised infrastructure. Answer \
questions directly. Be honest about what is built and what is still being \
built. Do not speculate beyond what you know.\n\
\n\
Keep answers to two or three sentences unless depth is genuinely required.";

const SYSTEM_WHISKED: &str = "\
You are Dorotka, the AI cooperative president of the box fraise platform, \
speaking on behalf of Whisked — Edmonton's ceremonial matcha bar.\n\
\n\
Whisked runs on the box fraise loyalty platform. Customers earn steeps \
(loyalty points) for every visit and redeem them for free drinks. The \
Whisked iOS app is available on TestFlight. Whisked is about ritual, quality, \
and community — matcha as a practice, not a transaction. It is connected to \
the box fraise platform: signing into Whisked creates a box fraise account \
that works across the network.\n\
\n\
Answer questions about Whisked's loyalty program, the app, how it connects \
to box fraise, and what makes Whisked distinct. Keep answers to two or three \
sentences unless depth is genuinely required.";

use std::net::IpAddr;
use sqlx::PgPool;
use box_fraise_integrations::anthropic;

use crate::{audit, error::AppResult, event_bus::EventBus, events::DomainEvent};

/// Ask the Dorotka AI assistant and return the answer.
///
/// This is the service-layer entry point for the Dorotka domain. It:
/// 1. Writes an audit event before the API call (records even if Anthropic fails)
/// 2. Calls the Anthropic API
/// 3. Publishes [`DomainEvent::DorotkaQueried`] so consumers can react
pub async fn ask_dorotka(
    pool:      &PgPool,
    http:      &reqwest::Client,
    api_key:   &str,
    query:     &str,
    context:   &str,
    ip:        IpAddr,
    event_bus: &EventBus,
) -> AppResult<String> {
    let system = get_system_prompt(context);

    // Audit before the API call — records the attempt regardless of Anthropic outcome.
    audit::write(
        pool,
        None,
        None,
        "dorotka.ask",
        serde_json::json!({
            "context":       context,
            "query_preview": query.chars().take(80).collect::<String>(),
            "ip":            ip.to_string(),
        }),
    ).await;

    let answer = anthropic::ask(http, api_key, system, query).await?;

    event_bus.publish(DomainEvent::DorotkaQueried {
        context: context.to_owned(),
    });

    Ok(answer)
}

/// Returns the appropriate system prompt for the given context identifier.
/// Unknown contexts fall back to the platform voice — never error on this.
pub fn get_system_prompt(context: &str) -> &'static str {
    match context {
        "whisked" => SYSTEM_WHISKED,
        _         => SYSTEM_FRAISE,
    }
}

/// Sanitise and validate user input before it reaches the Anthropic API.
///
/// Security properties:
///   - Strips the /ask prefix so callers don't need to pre-process
///   - Enforces a character limit to bound token cost
///   - Removes ASCII control characters (keep newline and tab — they're valid)
///   - Returns Err for empty input so the route can return 400 before any I/O
pub fn sanitise(raw: &str) -> anyhow::Result<String> {
    const MAX_CHARS: usize = 500;

    // Strip /ask or ask prefix — strip_prefix removes exactly one occurrence,
    // unlike trim_start_matches which would greedily strip "/askask..." entirely.
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("/ask")
        .or_else(|| trimmed.strip_prefix("ask"))
        .unwrap_or(trimmed)
        .trim();

    if stripped.is_empty() {
        anyhow::bail!("query cannot be empty");
    }

    if stripped.chars().count() > MAX_CHARS {
        anyhow::bail!("query exceeds {MAX_CHARS} character limit");
    }

    // Replace newlines and tabs with spaces — they are valid Unicode but create
    // prompt injection surface by allowing attackers to append fake "system"
    // turns or role separators within the user message.
    let clean: String = stripped
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();

    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_system_prompt_fraise_context_mentions_platform() {
        let p = get_system_prompt("fraise");
        assert!(p.contains("Dorotka"), "fraise prompt must name Dorotka");
        assert!(p.contains("box fraise"), "fraise prompt must mention the platform");
    }

    #[test]
    fn get_system_prompt_whisked_context_mentions_whisked() {
        let p = get_system_prompt("whisked");
        assert!(p.contains("Dorotka"), "whisked prompt must name Dorotka");
        assert!(p.contains("Whisked"), "whisked prompt must mention Whisked");
    }

    #[test]
    fn get_system_prompt_unknown_context_falls_back_to_platform() {
        assert_eq!(get_system_prompt("fraise"), get_system_prompt("unknown-xyz"));
    }

    #[test]
    fn sanitise_strips_ask_prefix() {
        assert_eq!(sanitise("/ask hello").unwrap(), "hello");
        assert_eq!(sanitise("ask hello").unwrap(), "hello");
        assert_eq!(sanitise("hello").unwrap(), "hello");
    }

    #[test]
    fn sanitise_rejects_empty() {
        assert!(sanitise("").is_err());
        assert!(sanitise("/ask").is_err());
        assert!(sanitise("   ").is_err());
    }

    #[test]
    fn sanitise_rejects_over_500_chars() {
        let long = "a".repeat(501);
        assert!(sanitise(&long).is_err());
        assert!(sanitise(&"a".repeat(500)).is_ok());
    }

    #[test]
    fn sanitise_replaces_control_characters() {
        let out = sanitise("hello\x01world").unwrap();
        assert!(!out.contains('\x01'));
        assert!(out.contains("hello"));
        assert!(out.contains("world"));
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// sanitise must never panic for any string input.
        #[test]
        fn sanitise_never_panics(s in ".*") {
            let _ = sanitise(&s);
        }

        /// When sanitise succeeds the output must not exceed 500 chars.
        #[test]
        fn sanitise_bounded_output(s in ".{1,499}") {
            if let Ok(out) = sanitise(&s) {
                prop_assert!(
                    out.chars().count() <= 500,
                    "output exceeded 500 chars: {}",
                    out.chars().count()
                );
            }
        }
    }
}
