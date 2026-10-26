//! Claude's usage limits, as Claude Code hands them to a status line
//! command. The hidden Claude Code runs `limitpeeker.exe --claude-statusline`
//! with its session data on standard input; this notes the limits for the
//! read under way, in a file of its own, and prints one line for Claude Code
//! to show. No sign-in is read or kept here, and nothing is asked of
//! Anthropic.

use crate::json::{self, Json};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

/// The most taken from standard input, and how long it is waited for.
const LONGEST: u64 = 1 << 20;
const WAIT: Duration = Duration::from_secs(3);
/// How large HEARD_FILE grows before it is moved aside once.
const LOG_MOST: u64 = 256 * 1024;
/// The windows kept, by Claude Code's names, with the labels they show as.
const WINDOWS: [(&str, &str); 2] = [("five_hour", "5h"), ("seven_day", "7d")];
/// Set in the hidden Claude Code's environment, which its status line
/// command inherits; only then is the run noted in HEARD_FILE, so the lines
/// there all come from the hidden Claude Code.
pub const HEARD_VAR: &str = "LIMITPEEKER_HEARD";
pub const HEARD_FILE: &str = "claude-heard.log";

/// One rate-limit window as Claude Code handed it.
#[derive(Clone, Debug, PartialEq)]
struct Window {
    label: &'static str,
    used: f64,
    resets_at: Option<i64>,
}

/// What one run was handed: the windows, whether `rate_limits` was there at
/// all, and the session id's start.
#[derive(Debug, Default, PartialEq)]
struct Handed {
    windows: Vec<Window>,
    had_limits: bool,
    session: String,
}

/// Takes what Claude Code handed over, notes it when the hidden Claude Code
/// ran this, and prints a line. Nothing else is kept: the test log of each
/// run is gone, as nothing read it.
pub fn capture() {
    let input = read_input();
    let data = std::str::from_utf8(&input).ok().and_then(json::parse);
    let handed = data.as_ref().map(handed).unwrap_or_default();
    if std::env::var_os(HEARD_VAR).is_some()
        && let Some(folder) = crate::settings::data_dir()
    {
        let _ = std::fs::create_dir_all(&folder);
        let local = crate::log::local_time_ms();
        append(&folder.join(HEARD_FILE), &heard_line(&local, &handed));
    }
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", shown(&handed.windows));
    let _ = out.flush();
}

/// A status line command's standard input, up to LONGEST. The CLI closes it
/// after writing; if it does not within WAIT, nothing is taken. Claude
/// Code's and the Antigravity CLI's alike.
pub fn read_input() -> Vec<u8> {
    let (send, got) = mpsc::channel();
    std::thread::spawn(move || {
        let mut input = Vec::new();
        let _ = std::io::stdin()
            .lock()
            .take(LONGEST)
            .read_to_end(&mut input);
        let _ = send.send(input);
    });
    got.recv_timeout(WAIT).unwrap_or_default()
}

fn handed(data: &Json) -> Handed {
    let text = |key: &str| data.get(key).and_then(Json::text).unwrap_or("").to_string();
    let limits = data
        .get("rate_limits")
        .filter(|l| matches!(l, Json::Object(_)));
    let windows = WINDOWS
        .iter()
        .filter_map(|&(key, label)| {
            let window = limits?.get(key)?;
            let used = window
                .get("used_percentage")
                .and_then(Json::number)
                .filter(|n| n.is_finite())?;
            Some(Window {
                label,
                used,
                resets_at: window
                    .get("resets_at")
                    .and_then(Json::number)
                    .filter(|n| n.is_finite())
                    .map(|n| n as i64),
            })
        })
        .collect();
    Handed {
        windows,
        had_limits: limits.is_some(),
        session: text("session_id").chars().take(8).collect(),
    }
}

/// The line Claude Code shows.
fn shown(windows: &[Window]) -> String {
    if windows.is_empty() {
        return "LimitPeeker: no usage in this session yet".into();
    }
    let parts: Vec<String> = windows
        .iter()
        .map(|w| format!("{} {}%", w.label, w.used.round() as i64))
        .collect();
    format!("LimitPeeker {}", parts.join(" | "))
}

/// What the hidden Claude Code's run was handed, as it was heard: when, the
/// session's start, whether there were limits, and each window's share used
/// and reset time.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub local: String,
    pub session: String,
    pub had_limits: bool,
    pub windows: Vec<(String, f64, Option<i64>)>,
}

/// One line of HEARD_FILE, split by tabs: the time, the session's start,
/// `yes` or `no` for limits, then a label, share and reset time for each
/// window.
fn heard_line(local: &str, handed: &Handed) -> String {
    let mut fields = vec![
        local.to_string(),
        handed.session.clone(),
        if handed.had_limits { "yes" } else { "no" }.to_string(),
    ];
    for window in &handed.windows {
        fields.push(window.label.to_string());
        fields.push(window.used.to_string());
        fields.push(
            window
                .resets_at
                .map_or("-".to_string(), |at| at.to_string()),
        );
    }
    fields.join("\t")
}

/// A line of HEARD_FILE read back; None when it is not one.
pub fn heard(line: &str) -> Option<Heard> {
    let fields: Vec<&str> = line.trim_end_matches(['\r', '\n']).split('\t').collect();
    let [local, session, limits, rest @ ..] = fields.as_slice() else {
        return None;
    };
    if rest.len() % 3 != 0 {
        return None;
    }
    let windows = rest
        .chunks(3)
        .map(|w| {
            Some((
                w[0].to_string(),
                w[1].parse().ok()?,
                if w[2] == "-" {
                    None
                } else {
                    Some(w[2].parse().ok()?)
                },
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Heard {
        local: local.to_string(),
        session: session.to_string(),
        had_limits: match *limits {
            "yes" => true,
            "no" => false,
            _ => return None,
        },
        windows,
    })
}

/// Adds a line to the log, moving a full log aside once first.
fn append(path: &std::path::Path, line: &str) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LOG_MOST) {
        let _ = std::fs::rename(path, path.with_extension("1.log"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        // One write, so two runs at once do not mix their lines.
        let _ = file.write_all(format!("{line}\n").as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of the status line's input in Claude Code's documentation.
    const HANDED: &str = r#"{
        "session_id": "abc123de-f456", "cwd": "D:/secret/project",
        "transcript_path": "C:/Users/me/.claude/projects/x.jsonl",
        "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
        "version": "2.1.237",
        "prompt_id": "9f8e7d6c-5b4a",
        "cost": {"total_cost_usd": 0.01234, "total_duration_ms": 45000},
        "context_window": {"total_input_tokens": 15500, "current_usage": {
            "input_tokens": 8500, "output_tokens": 1200,
            "cache_creation_input_tokens": 5000, "cache_read_input_tokens": 2000}},
        "rate_limits": {
            "five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
            "seven_day": {"used_percentage": 41.2, "resets_at": 1738857600},
            "spend_limit": {"used_percentage": 62.8, "resets_at": 1740787200}
        }
    }"#;

    fn read(text: &str) -> Handed {
        handed(&json::parse(text).unwrap())
    }

    #[test]
    fn the_windows_are_kept_and_nothing_else() {
        // Five hours and a week, as Claude Code hands them; the spend limit
        // and everything but the session's start are left.
        let handed = read(HANDED);
        assert!(handed.had_limits);
        let windows: Vec<_> = handed
            .windows
            .iter()
            .map(|w| (w.label, w.used, w.resets_at))
            .collect();
        assert_eq!(
            windows,
            [
                ("5h", 23.5, Some(1738425600)),
                ("7d", 41.2, Some(1738857600))
            ]
        );
        assert_eq!(handed.session, "abc123de");
        assert_eq!(shown(&handed.windows), "LimitPeeker 5h 24% | 7d 41%");
        // The spend limit is not a window kept.
        assert!(handed.windows.iter().all(|w| w.label != "spend_limit"));
    }

    #[test]
    fn no_limits_yet_leaves_the_file_alone() {
        // Before the session's first answer there are no rate limits; a
        // window may be missing, and a value that is not a number is not one.
        let none = read(r#"{"session_id": "s", "model": {}}"#);
        assert!(!none.had_limits && none.windows.is_empty());
        assert_eq!(
            shown(&none.windows),
            "LimitPeeker: no usage in this session yet"
        );
        let one = read(
            r#"{"rate_limits": {"seven_day": {"used_percentage": 100}, "five_hour": {"used_percentage": "5"}}}"#,
        );
        assert!(one.had_limits);
        assert_eq!(one.windows.len(), 1);
        assert_eq!(
            (one.windows[0].label, one.windows[0].resets_at),
            ("7d", None)
        );
        assert_eq!(one.windows[0].used, 100.0);
    }

    #[test]
    fn what_the_hidden_claude_code_was_handed_is_heard_back() {
        // The hidden Claude Code's runs are told apart by session and read
        // back by the hidden console's read.
        let handed = read(HANDED);
        let line = heard_line("2026-09-26 03:00:00.123", &handed);
        assert_eq!(
            line,
            "2026-09-26 03:00:00.123\tabc123de\tyes\t5h\t23.5\t1738425600\t7d\t41.2\t1738857600"
        );
        let back = heard(&format!("{line}\r\n")).unwrap();
        assert_eq!((back.session.as_str(), back.had_limits), ("abc123de", true));
        assert_eq!(
            back.windows,
            [
                ("5h".to_string(), 23.5, Some(1738425600)),
                ("7d".to_string(), 41.2, Some(1738857600))
            ]
        );
        let none = read(r#"{"session_id": "s1234567xyz"}"#);
        let back = heard(&heard_line("t", &none)).unwrap();
        assert_eq!(
            (back.session.as_str(), back.had_limits, back.windows.len()),
            ("s1234567", false, 0)
        );
        let one = read(r#"{"rate_limits": {"seven_day": {"used_percentage": 9}}}"#);
        assert_eq!(
            heard(&heard_line("t", &one)).unwrap().windows,
            [("7d".to_string(), 9.0, None)]
        );
        for wrong in [
            "",
            "t\ts",
            "t\ts\tmaybe",
            "t\ts\tyes\t5h\t1",
            "t\ts\tyes\t5h\tx\t-",
        ] {
            assert_eq!(heard(wrong), None, "{wrong:?}");
        }
    }
}
