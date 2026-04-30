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

/// Returns the appropriate system prompt for the given context identifier.
/// Unknown contexts fall back to the platform voice — never error on this.
pub fn system_prompt(context: &str) -> &'static str {
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
