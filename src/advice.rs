//! What a failed read means to the person: what went wrong, in a few words,
//! and what they can do about it when there is something. Told from the
//! read's own words, which stay in the log as they were.

/// A failed read as the popup says it.
#[derive(Clone, Debug, PartialEq)]
pub struct Advice {
    /// What went wrong, short enough for one line beside the next try.
    pub what: String,
    /// What the person can do, when there is something.
    pub todo: Option<String>,
}

/// A service's tool: its name, the company whose service it asks, and how
/// one signs in to it.
struct Tool {
    id: &'static str,
    name: &'static str,
    company: &'static str,
    sign_in: &'static str,
}

const TOOLS: [Tool; 4] = [
    Tool {
        id: "gpt",
        name: "Codex",
        company: "OpenAI",
        sign_in: "Run codex login, or sign in to the Codex app",
    },
    Tool {
        id: "claude",
        name: "Claude Code",
        company: "Anthropic",
        sign_in: "Run claude, then /login",
    },
    Tool {
        id: "gemini",
        name: "Antigravity",
        company: "Google",
        sign_in: "Run agy and sign in",
    },
    Tool {
        id: "grok",
        name: "Grok",
        company: "xAI",
        sign_in: "Run grok and sign in",
    },
];

/// What the tools say when no one is signed in, as seen, and as they are
/// likely to say it.
const SIGNED_OUT: [&str; 6] = [
    "authentication required",
    "not signed in",
    "not logged in",
    "unauthorized",
    "please log in",
    "login required",
];
/// What a service that is down or busy answers through the tool.
const SERVICE: [&str; 6] = [
    "service unavailable",
    "bad gateway",
    "gateway timeout",
    "too many requests",
    "failed to fetch",
    "error sending request",
];
/// Our own deadlines passing.
const SLOW: [&str; 3] = ["in time", "nothing heard in", "no quota in"];
/// An answer not of the shape this version knows.
const SHAPE: [&str; 14] = [
    "not json",
    "no rate limits",
    "no usedpercent",
    "no rate limit windows",
    "neither five_hour",
    "is not a list",
    "is not an object",
    "has no utilization",
    "is not known",
    "no bucket",
    "no gemini bucket",
    "no current period",
    "no kind and no span",
    "answered nothing",
];

/// What the read of service `id` (named `name`) that failed with `why`
/// means to the person. The first kind that fits, in order: Claude's sign-in
/// with no plan, Claude's plan with no limits, no sign-in, the service down,
/// no answer in time, the tool not starting, an answer of an unknown shape,
/// the tool ending early, and anything else.
pub fn advise(id: &str, name: &str, why: &str) -> Advice {
    let tool = TOOLS.iter().find(|tool| tool.id == id);
    let name = tool.map_or(name, |tool| tool.name);
    let lower = why.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| lower.contains(word));
    let advice = |what: String, todo: Option<&str>| Advice {
        what,
        todo: todo.map(Into::into),
    };
    let status = status(why);
    if lower.contains(crate::claude_usage::NOT_ON_A_PLAN) {
        return advice(
            "Not signed in to a Claude plan".into(),
            Some("Run claude, then /login with a Claude plan"),
        );
    }
    if lower.contains(crate::claude_usage::NO_PLAN_LIMITS) {
        return advice("No plan limits for this sign-in".into(), None);
    }
    if has(&SIGNED_OUT) || status == Some(401) {
        return advice(
            format!("Not signed in to {name}"),
            tool.map(|tool| tool.sign_in),
        );
    }
    if has(&SERVICE) || status.is_some_and(|code| code == 429 || code >= 500) {
        let company = tool.map_or(name, |tool| tool.company);
        return advice(
            match status {
                Some(code) => format!("{company} did not answer ({code})"),
                None => format!("{company} did not answer"),
            },
            None,
        );
    }
    if has(&SLOW) {
        return advice(format!("{name} did not answer in time"), None);
    }
    if lower.starts_with("could not start") {
        let said = why.split_once(": ").map_or("", |(_, said)| said.trim());
        return Advice {
            what: format!("{name} could not be started"),
            todo: (!said.is_empty()).then(|| format!("Windows said: {said}")),
        };
    }
    if has(&SHAPE) {
        return Advice {
            what: format!("Unknown answer from {name}"),
            todo: Some(format!("Update {name} or LimitPeeker")),
        };
    }
    if lower.contains("ended") {
        return advice(
            format!("{name} stopped early"),
            (id == "gemini").then_some("Run agy once to check it signs in"),
        );
    }
    advice("Read failed".into(), Some(why))
}

/// The HTTP status a tool passed on, as in `503 Service Unavailable`: three
/// digits from 400 to 599 on their own, then a word that starts with a
/// capital and goes on in small letters, so a number in an answer's body or
/// a size such as `503 MB` is not taken for one.
fn status(why: &str) -> Option<u16> {
    let bytes = why.as_bytes();
    (0..bytes.len().saturating_sub(5)).find_map(|at| {
        let digits = &bytes[at..at + 3];
        let before = at.checked_sub(1).map(|b| bytes[b]);
        let fits = digits.iter().all(u8::is_ascii_digit)
            && matches!(digits[0], b'4' | b'5')
            && !before.is_some_and(|b| b.is_ascii_alphanumeric())
            && bytes[at + 3] == b' '
            && bytes[at + 4].is_ascii_uppercase()
            && bytes[at + 5].is_ascii_lowercase();
        fits.then(|| std::str::from_utf8(digits).ok()?.parse().ok())
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(id: &str, why: &str) -> (String, Option<String>) {
        let advice = advise(id, "Service", why);
        (advice.what, advice.todo)
    }

    #[test]
    fn what_the_tools_said_becomes_what_to_do() {
        // The words seen from the tools, and those this program writes,
        // each told as the person can act on it.
        let cases: [(&str, &str, &str, Option<&str>); 13] = [
            // Codex 0.157.0 with no sign-in, 2026-10-19.
            (
                "gpt",
                "Codex refused: codex account authentication required to read rate limits",
                "Not signed in to Codex",
                Some("Run codex login, or sign in to the Codex app"),
            ),
            // Codex on 2026-10-12, OpenAI's service down.
            (
                "gpt",
                r#"Codex refused: failed to fetch codex rate limits: GET https://chatgpt.com/backend-api/wham/usage failed: 503 Service Unavailable; content-type=application/json; body={"detail":"Unavailable 1234"}"#,
                "OpenAI did not answer (503)",
                None,
            ),
            // Claude Code with no sign-in answers get_usage with no plan.
            (
                "claude",
                "not signed in to a Claude plan (no subscription); /usage not tried",
                "Not signed in to a Claude plan",
                Some("Run claude, then /login with a Claude plan"),
            ),
            (
                "claude",
                "plan rate limits do not apply to this sign-in; /usage not tried",
                "No plan limits for this sign-in",
                None,
            ),
            (
                "grok",
                "x.ai/billing was refused: 401 Unauthorized",
                "Not signed in to Grok",
                Some("Run grok and sign in"),
            ),
            (
                "gpt",
                "no answer in time",
                "Codex did not answer in time",
                None,
            ),
            (
                "claude",
                "get_usage failed: no answer to usage in time; /usage failed: nothing heard in 20 s, 2 times",
                "Claude Code did not answer in time",
                None,
            ),
            (
                "gemini",
                "no quota in 30 s (4 status line runs)",
                "Antigravity did not answer in time",
                None,
            ),
            (
                "gpt",
                "could not start: Access is denied. (os error 5)",
                "Codex could not be started",
                Some("Windows said: Access is denied. (os error 5)"),
            ),
            (
                "grok",
                "_x.ai/billing is not known",
                "Unknown answer from Grok",
                Some("Update Grok or LimitPeeker"),
            ),
            (
                "gemini",
                "agy ended after 3 status line runs, none with a quota",
                "Antigravity stopped early",
                Some("Run agy once to check it signs in"),
            ),
            ("gpt", "Codex ended", "Codex stopped early", None),
            (
                "gpt",
                "could not write: The pipe is being closed.",
                "Read failed",
                Some("could not write: The pipe is being closed."),
            ),
        ];
        for (id, why, what, todo) in cases {
            assert_eq!(
                said(id, why),
                (what.to_string(), todo.map(str::to_string)),
                "{why}"
            );
        }
        // A service this table does not know keeps its own name.
        assert_eq!(said("pc", "Codex ended").0, "Service stopped early");
    }

    #[test]
    fn an_http_status_is_read_only_before_its_name() {
        assert_eq!(
            status("GET x failed: 503 Service Unavailable; body"),
            Some(503)
        );
        assert_eq!(status("429 Too Many Requests"), Some(429));
        assert_eq!(status("refused: 401 Unauthorized"), Some(401));
        assert_eq!(status(r#"body={"detail":"Unavailable 5034 x"}"#), None);
        assert_eq!(status("peak 503 MB"), None);
        assert_eq!(status("v1503 Service"), None);
        assert_eq!(status("200 OK"), None);
        assert_eq!(status(""), None);
    }
}
