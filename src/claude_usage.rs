//! Claude's usage, read once by Claude Code's own `get_usage` request:
//! Claude Code started in print mode in its own folder, speaking JSON lines
//! on its standard input and output, asked for the structured `/usage` data
//! and ended. The answer has each model's weekly limit too, which the status
//! line never had. When it fails, the hidden console's `/usage` is read
//! instead, unless the answer says the sign-in has no plan limits, which
//! `/usage` would only say again. No sign-in is read here, and no model is
//! asked.

use crate::claude::HEARD_VAR;
use crate::claude_home;
use crate::claude_session;
use crate::job::{self, Cost, Job};
use crate::json::{self, Json};
use crate::log::short;
use crate::usage::{Found, Limit, Reading};
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HANDLE;

/// How long each answer may take.
const ANSWER_WITHIN: Duration = Duration::from_secs(20);
/// How long Claude Code has to end once its input is closed.
const END_WITHIN: Duration = Duration::from_secs(5);
/// The longest line taken from Claude Code.
const LONGEST_LINE: u64 = 1 << 20;
/// Print mode with JSON lines both ways, no session kept, no settings but
/// those passed (none), and no MCP servers.
const ARGS: [&str; 9] = [
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--no-session-persistence",
    "--restricted",
    "--strict-mcp-config",
];
/// The windows `/usage` shows by name, with their labels, in its order; each
/// model's weekly window follows them.
const WINDOWS: [(&str, &str); 3] = [
    ("five_hour", "5h"),
    ("seven_day", "7d"),
    ("seven_day_sonnet", "Sonnet 7d"),
];
/// The most characters of a model's name kept in its label, so the box
/// still takes it.
const LONGEST_NAME: usize = 16;

/// Set by `--claude-fallback`: every read takes the hidden console's
/// `/usage`, so that way can be checked against the Claude Code installed
/// now.
pub static FALLBACK_ONLY: AtomicBool = AtomicBool::new(false);

/// How a read with no Claude plan signed in is told, in small letters, as
/// the popup's advice looks for it.
pub const NOT_ON_A_PLAN: &str = "not signed in to a claude plan";
/// And one with a plan that has no limits.
pub const NO_PLAN_LIMITS: &str = "plan rate limits do not apply";

/// Why `get_usage` gave no limits: no Claude plan is signed in (Claude Code
/// signed out, or on an API key), or the plan has no limits, both of which
/// `/usage` would only say again; or anything else, after which `/usage` is
/// read instead.
#[derive(Debug, PartialEq)]
enum Failure {
    NotOnAPlan,
    NoPlanLimits,
    Other(String),
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::NotOnAPlan => write!(f, "{NOT_ON_A_PLAN} (no subscription)"),
            Failure::NoPlanLimits => write!(f, "{NO_PLAN_LIMITS} to this sign-in"),
            Failure::Other(why) => f.write_str(why),
        }
    }
}
impl From<String> for Failure {
    fn from(why: String) -> Self {
        Failure::Other(why)
    }
}

/// Reads Claude's usage once with the Claude Code at `claude`: by
/// `get_usage`, or when that fails, by the hidden console's `/usage`, which
/// has no model's limits. What it cost covers both.
pub fn read(claude: &Path) -> (Result<Found, String>, Cost) {
    if FALLBACK_ONLY.load(Ordering::Relaxed) {
        let skipped = |_: &Path| {
            (
                Err(Failure::Other("not asked (--claude-fallback)".into())),
                Cost::default(),
            )
        };
        return read_with(claude, skipped, claude_session::read);
    }
    read_with(claude, ask, claude_session::read)
}

/// `read`, with how `get_usage` is asked and how `/usage` is read passed
/// in, so that when each is tried can be tested.
fn read_with(
    claude: &Path,
    ask: impl FnOnce(&Path) -> (Result<Found, Failure>, Cost),
    instead: impl FnOnce(&Path) -> (Result<Found, String>, Cost),
) -> (Result<Found, String>, Cost) {
    let began = Instant::now();
    let (asked, cost) = ask(claude);
    let (read, cost) = match asked {
        Ok(found) => (Ok(found), cost),
        // `/usage` would say the same of this sign-in.
        Err(no_plan @ (Failure::NotOnAPlan | Failure::NoPlanLimits)) => {
            (Err(format!("{no_plan}; /usage not tried")), cost)
        }
        Err(Failure::Other(why)) => {
            let (read, instead) = instead(claude);
            let read = match read {
                Ok(mut found) => {
                    found
                        .told
                        .push_str(&format!(" via /usage, as get_usage failed: {why}"));
                    Ok(found)
                }
                Err(also) => Err(format!("get_usage failed: {why}; /usage failed: {also}")),
            };
            let both = Cost {
                wall_ms: 0.0,
                user_ms: cost.user_ms + instead.user_ms,
                kernel_ms: cost.kernel_ms + instead.kernel_ms,
                peak_mb: cost.peak_mb.max(instead.peak_mb),
            };
            (read, both)
        }
    };
    let wall_ms = began.elapsed().as_secs_f64() * 1000.0;
    (read, Cost { wall_ms, ..cost })
}

/// Starts Claude Code, greets it, asks for its usage and ends it, with
/// what that cost.
fn ask(claude: &Path) -> (Result<Found, Failure>, Cost) {
    let home = match claude_home::home() {
        Ok(home) => home,
        Err(why) => return (Err(why.into()), Cost::default()),
    };
    let mut command = Command::new(claude);
    command
        .args(ARGS)
        .current_dir(&home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(job::CREATE_NO_WINDOW);
    for name in claude_home::REMOVED {
        command.env_remove(name);
    }
    command.envs(added());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return (Err(format!("could not start: {e}").into()), Cost::default()),
    };
    let job = Job::around(HANDLE(child.as_raw_handle()));
    let replies = match child.stdout.take() {
        Some(output) => job::lines(output, LONGEST_LINE),
        None => mpsc::channel().1,
    };
    let found = match child.stdin.take() {
        Some(mut input) => {
            let found = converse(&mut input, &replies);
            // Closing its input ends it.
            drop(input);
            found
        }
        None => Err(Failure::Other("no input".into())),
    };
    end(&mut child);
    job.end();
    // Without a job, what Claude Code started may outlive it.
    let found = if job.held() {
        found
    } else {
        match found {
            Ok(mut found) => {
                found.told.push_str(" (not in a job)");
                Ok(found)
            }
            Err(Failure::Other(why)) => Err(Failure::Other(format!("{why} (not in a job)"))),
            other => other,
        }
    };
    (found, job.cost())
}

/// Added to Claude Code's environment: as for the hidden console, but not
/// the status line's mark, as none runs.
fn added() -> impl Iterator<Item = (&'static str, &'static str)> {
    claude_home::ADDED
        .into_iter()
        .filter(|(name, _)| *name != HEARD_VAR)
}

/// `initialize`, then `get_usage` without the scan of past sessions, each
/// answered in turn.
fn converse(
    input: &mut ChildStdin,
    replies: &mpsc::Receiver<Result<Vec<u8>, String>>,
) -> Result<Found, Failure> {
    send(input, "initialize", r#"{"subtype":"initialize"}"#)?;
    answer(replies, "initialize", Instant::now() + ANSWER_WITHIN)?;
    send(
        input,
        "usage",
        r#"{"subtype":"get_usage","skip_behaviors":true}"#,
    )?;
    limits(&answer(replies, "usage", Instant::now() + ANSWER_WITHIN)?)
}

fn send(input: &mut impl Write, id: &str, request: &str) -> Result<(), String> {
    let line = format!(r#"{{"type":"control_request","request_id":"{id}","request":{request}}}"#);
    input
        .write_all(format!("{line}\n").as_bytes())
        .and_then(|_| input.flush())
        .map_err(|e| format!("could not write: {e}"))
}

/// Waits END_WITHIN for Claude Code to end, then ends it.
fn end(child: &mut Child) {
    let until = Instant::now() + END_WITHIN;
    while Instant::now() < until && matches!(child.try_wait(), Ok(None)) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// What the request `id` was answered: its response when it succeeded, and
/// Claude Code's reason when not. Whatever else it says, and any line that
/// is not JSON, is passed over.
fn answer(
    replies: &mpsc::Receiver<Result<Vec<u8>, String>>,
    id: &str,
    deadline: Instant,
) -> Result<Json, String> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = match replies.recv_timeout(left) {
            Ok(line) => line?,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(format!("no answer to {id} in time"));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err("Claude Code ended".into()),
        };
        let Some(message) = std::str::from_utf8(&line).ok().and_then(json::parse) else {
            continue;
        };
        let Some(response) = message
            .get("response")
            .filter(|_| message.get("type").and_then(Json::text) == Some("control_response"))
            .filter(|r| r.get("request_id").and_then(Json::text) == Some(id))
        else {
            continue;
        };
        return match response.get("subtype").and_then(Json::text) {
            Some("success") => Ok(response
                .get("response")
                .cloned()
                .unwrap_or(Json::Object(Vec::new()))),
            _ => Err(format!(
                "Claude Code refused {id}: {}",
                short(
                    response
                        .get("error")
                        .and_then(Json::text)
                        .unwrap_or("no reason")
                )
            )),
        };
    }
}

/// The limits in a `get_usage` answer, in `/usage`'s order: five hours, the
/// week, Sonnet's week, then each model's week, labeled `<name> 7d`. One
/// with no share used does not apply; a label is taken once.
/// Only an answer that says the sign-in has no plan limits is told apart.
fn limits(answer: &Json) -> Result<Found, Failure> {
    let Some(windows) = answer
        .get("rate_limits")
        .filter(|l| matches!(l, Json::Object(_)))
    else {
        // With no subscription, no plan is signed in: Claude Code answers
        // so when signed out, as seen, and with an API key.
        let subscribed = answer
            .get("subscription_type")
            .and_then(Json::text)
            .is_some_and(|kind| !kind.is_empty());
        return Err(
            match answer.get("rate_limits_available") == Some(&Json::Bool(false)) {
                true if subscribed => Failure::NoPlanLimits,
                true => Failure::NotOnAPlan,
                false => Failure::Other("no rate limits".into()),
            },
        );
    };
    let mut limits: Vec<Limit> = Vec::new();
    for (key, label) in WINDOWS {
        if let Some(limit) =
            window(windows.get(key), label).map_err(|why| format!("{key} {why}"))?
        {
            take(&mut limits, limit);
        }
    }
    if !limits.iter().any(|l| l.label == "5h" || l.label == "7d") {
        return Err(Failure::Other("neither five_hour nor seven_day".into()));
    }
    match windows.get("model_scoped") {
        None | Some(Json::Null) => {}
        Some(Json::List(models)) => {
            for model in models {
                let Some(name) = model
                    .get("display_name")
                    .and_then(Json::text)
                    .and_then(model_name)
                else {
                    continue;
                };
                let label = format!("{name} 7d");
                if let Some(limit) =
                    window(Some(model), &label).map_err(|why| format!("{label} {why}"))?
                {
                    take(&mut limits, limit);
                }
            }
        }
        Some(_) => return Err(Failure::Other("model_scoped is not a list".into())),
    }
    let mut told: Vec<String> = limits
        .iter()
        .map(|l| {
            let used = match l.reading {
                Reading::Read(used) => used,
                _ => 0,
            };
            let resets = l.resets.map_or("unknown".to_string(), |at| at.to_string());
            format!("{}={used}% resets_at={resets}", l.label)
        })
        .collect();
    if let Some(plan) = answer.get("subscription_type").and_then(Json::text) {
        told.push(format!("plan={}", short(plan)));
    }
    told.push("via get_usage".into());
    Ok(Found {
        limits,
        told: told.join(" "),
    })
}

/// One window: None when it is not there or has no share used; an error
/// when it is not as `get_usage` gives it.
fn window(value: Option<&Json>, label: &str) -> Result<Option<Limit>, String> {
    let window = match value {
        None | Some(Json::Null) => return Ok(None),
        Some(window @ Json::Object(_)) => window,
        Some(_) => return Err("is not an object".into()),
    };
    let used = match window.get("utilization") {
        None | Some(Json::Null) => return Ok(None),
        Some(Json::Number(used)) if used.is_finite() => *used,
        Some(_) => return Err("has no utilization".into()),
    };
    Ok(Some(Limit {
        label: label.into(),
        reading: Reading::Read(used.round().clamp(0.0, 255.0) as u8),
        resets: window
            .get("resets_at")
            .and_then(Json::text)
            .and_then(unix_seconds),
        amount: None,
    }))
}

/// Adds a limit unless one of its label, in any case, is there.
fn take(limits: &mut Vec<Limit>, limit: Limit) {
    if !limits
        .iter()
        .any(|l| l.label.eq_ignore_ascii_case(&limit.label))
    {
        limits.push(limit);
    }
}

/// A model's name as the server gave it, cut to LONGEST_NAME; None when it
/// is empty or has a control character, which a label cannot hold.
fn model_name(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().any(char::is_control) {
        return None;
    }
    Some(
        name.chars()
            .take(LONGEST_NAME)
            .collect::<String>()
            .trim_end()
            .to_string(),
    )
}

/// Unix seconds from an ISO 8601 time as `get_usage` gives it, such as
/// `2026-09-28T16:30:00.186342+00:00` or one ending in `Z`; parts of a
/// second are dropped.
pub(crate) fn unix_seconds(text: &str) -> Option<i64> {
    let number = |from: usize, len: usize| -> Option<i64> {
        let part = text.get(from..from + len)?;
        if part.bytes().all(|b| b.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(5, 2)?, number(8, 2)?);
    let (hour, minute, second) = (number(11, 2)?, number(14, 2)?, number(17, 2)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut at = 19;
    if bytes[at] == b'.' {
        let digits = bytes[at + 1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits == 0 {
            return None;
        }
        at += 1 + digits;
    }
    let offset = match &text[at..] {
        "Z" | "z" => 0,
        zone if zone.len() == 6 && zone.as_bytes()[3] == b':' => {
            let sign = match zone.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let (hours, minutes) = (number(at + 1, 2)?, number(at + 4, 2)?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        }
        _ => return None,
    };
    Some(days(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// Days from 1970-01-01 to a day of the Gregorian calendar.
pub fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_cycle - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_limits(text: &str) -> Result<Found, String> {
        limits(&json::parse(text).unwrap()).map_err(|why| why.to_string())
    }

    fn shown(found: &Found) -> Vec<(&str, Reading, Option<i64>)> {
        found
            .limits
            .iter()
            .map(|l| (l.label.as_str(), l.reading, l.resets))
            .collect()
    }

    #[test]
    fn a_usage_answer_becomes_limits_with_each_model_week() {
        // As Claude Code 2.1.283 answered on 2026-09-29, cut short: five
        // hours, the week and Fable's week, and many windows left null.
        let found = read_limits(
            r#"{"session": {"total_cost_usd": 0, "model_usage": {}},
               "subscription_type": "pro", "rate_limits_available": true,
               "rate_limits": {
                 "five_hour": {"utilization": 13.0, "resets_at": "2026-09-28T16:30:00.186342+00:00"},
                 "seven_day": {"utilization": 3.0, "resets_at": "2026-09-28T22:00:00.186367+00:00"},
                 "seven_day_oauth_apps": null, "seven_day_opus": null, "seven_day_sonnet": null,
                 "nimbus_quill": {"utilization": 0.0, "resets_at": null},
                 "extra_usage": {"is_enabled": true, "utilization": 68.9},
                 "limits": [{"kind": "session", "percent": 13}],
                 "model_scoped": [{"display_name": "Fable", "utilization": 0,
                                   "resets_at": "2026-09-28T22:00:00+00:00"}]},
               "behaviors": null}"#,
        )
        .unwrap();
        assert_eq!(
            shown(&found),
            [
                ("5h", Reading::Read(13), Some(1_790_613_000)),
                ("7d", Reading::Read(3), Some(1_790_632_800)),
                ("Fable 7d", Reading::Read(0), Some(1_790_632_800)),
            ]
        );
        assert_eq!(
            found.told,
            "5h=13% resets_at=1790613000 7d=3% resets_at=1790632800 \
             Fable 7d=0% resets_at=1790632800 plan=pro via get_usage"
        );
    }

    #[test]
    fn null_windows_are_no_limits_and_names_are_kept_short_and_once() {
        let found = read_limits(
            r#"{"rate_limits": {
                 "five_hour": {"utilization": null, "resets_at": null},
                 "seven_day": {"utilization": 130.4, "resets_at": "not a time"},
                 "seven_day_sonnet": {"utilization": -3, "resets_at": "2026-10-01T00:00:00Z"},
                 "model_scoped": [
                   {"display_name": "sonnet", "utilization": 50, "resets_at": null},
                   {"display_name": " Fable ", "utilization": 7.5, "resets_at": null},
                   {"display_name": "tab\tname", "utilization": 1, "resets_at": null},
                   {"display_name": "", "utilization": 1, "resets_at": null},
                   {"display_name": "Opus", "utilization": null, "resets_at": null},
                   {"display_name": "A model with a long name", "utilization": 2, "resets_at": null}
                 ]}}"#,
        )
        .unwrap();
        assert_eq!(
            shown(&found),
            [
                ("7d", Reading::Read(130), None),
                ("Sonnet 7d", Reading::Read(0), Some(1_790_812_800)),
                ("Fable 7d", Reading::Read(8), None),
                ("A model with a l 7d", Reading::Read(2), None),
            ]
        );
    }

    #[test]
    fn an_answer_not_as_asked_fails() {
        // A failed read falls back to the console's `/usage`.
        for (wrong, why) in [
            (r#"{}"#, "no rate limits"),
            (
                r#"{"subscription_type": "max", "rate_limits_available": false, "rate_limits": null}"#,
                "plan rate limits do not apply to this sign-in",
            ),
            (
                r#"{"rate_limits_available": false, "rate_limits": null}"#,
                "not signed in to a claude plan (no subscription)",
            ),
            (
                r#"{"rate_limits": {"five_hour": null, "seven_day": null}}"#,
                "neither five_hour nor seven_day",
            ),
            (
                r#"{"rate_limits": {"five_hour": {"utilization": "13"}}}"#,
                "five_hour has no utilization",
            ),
            (
                r#"{"rate_limits": {"seven_day": [3]}}"#,
                "seven_day is not an object",
            ),
            (
                r#"{"rate_limits": {"seven_day": {"utilization": 3}, "model_scoped": {}}}"#,
                "model_scoped is not a list",
            ),
            (
                r#"{"rate_limits": {"seven_day": {"utilization": 3},
                    "model_scoped": [{"display_name": "Fable", "utilization": true}]}}"#,
                "Fable 7d has no utilization",
            ),
        ] {
            assert_eq!(read_limits(wrong).err().as_deref(), Some(why), "{wrong}");
        }
    }

    #[test]
    fn usage_is_not_read_for_a_sign_in_without_plan_limits() {
        // Only an answer that says the sign-in has no plan limits skips the
        // console's `/usage`, which would say the same. Anything else, as
        // when a new Claude Code refuses or reshapes `get_usage`, reads it.
        // With no subscription, as Claude Code answered signed out on
        // 2026-10-19, no plan is signed in.
        let parsed = |text: &str| limits(&json::parse(text).unwrap());
        assert_eq!(
            parsed(r#"{"subscription_type": "pro", "rate_limits_available": false, "rate_limits": null}"#)
                .err(),
            Some(Failure::NoPlanLimits)
        );
        for signed_out in [
            r#"{"session": {"total_cost_usd": 0, "model_usage": {}}, "subscription_type": null,
                "rate_limits_available": false, "rate_limits": null, "behaviors": null}"#,
            r#"{"rate_limits_available": false, "rate_limits": null}"#,
            r#"{"subscription_type": "", "rate_limits_available": false}"#,
        ] {
            assert_eq!(
                parsed(signed_out).err(),
                Some(Failure::NotOnAPlan),
                "{signed_out}"
            );
        }
        assert_eq!(
            Failure::NotOnAPlan.to_string(),
            "not signed in to a claude plan (no subscription)"
        );
        for other in [
            r#"{}"#,
            r#"{"rate_limits_available": true, "rate_limits": null}"#,
            r#"{"rate_limits": {"five_hour": null, "seven_day": null}}"#,
            r#"{"rate_limits": {"seven_day": [3]}}"#,
        ] {
            assert!(matches!(parsed(other), Err(Failure::Other(_))), "{other}");
        }
        let tried = std::cell::Cell::new(0);
        let instead = |_: &Path| {
            tried.set(tried.get() + 1);
            let found = Found {
                limits: Vec::new(),
                told: "5h=1% resets_at=unknown".into(),
            };
            (Ok(found), Cost::default())
        };
        let claude = Path::new("claude.exe");
        let (read, _) = read_with(
            claude,
            |_| (Err(Failure::NoPlanLimits), Cost::default()),
            instead,
        );
        assert_eq!(
            read.err().as_deref(),
            Some("plan rate limits do not apply to this sign-in; /usage not tried")
        );
        assert_eq!(tried.get(), 0);
        let (read, _) = read_with(
            claude,
            |_| (Err(Failure::NotOnAPlan), Cost::default()),
            instead,
        );
        assert_eq!(
            read.err().as_deref(),
            Some("not signed in to a claude plan (no subscription); /usage not tried")
        );
        assert_eq!(tried.get(), 0);
        let refused = "Claude Code refused usage: unknown subtype";
        let (read, _) = read_with(
            claude,
            |_| (Err(Failure::Other(refused.into())), Cost::default()),
            instead,
        );
        assert!(
            read.unwrap()
                .told
                .ends_with(&format!("via /usage, as get_usage failed: {refused}"))
        );
        assert_eq!(tried.get(), 1);
    }

    #[test]
    fn iso_times_become_unix_seconds() {
        for (text, unix) in [
            ("2026-09-28T16:30:00.186342+00:00", Some(1_790_613_000)),
            ("2026-09-28T22:00:00+00:00", Some(1_790_632_800)),
            ("2026-09-29T07:00:00+09:00", Some(1_790_632_800)),
            ("2026-09-28T17:00:00-05:00", Some(1_790_632_800)),
            ("2026-09-28T22:00:00Z", Some(1_790_632_800)),
            ("1970-01-01T00:00:00Z", Some(0)),
            ("2024-02-29T12:00:00.5Z", Some(1_709_208_000)),
            ("2026-09-28 22:00:00Z", None),
            ("2026-09-28T22:00:00", None),
            ("2026-09-28T22:00:00.Z", None),
            ("2026-13-28T22:00:00Z", None),
            ("2026-09-28T22:00:00+0000", None),
            ("2026-09-28T2a:00:00Z", None),
            ("", None),
        ] {
            assert_eq!(unix_seconds(text), unix, "{text}");
        }
    }

    #[test]
    fn answers_are_matched_to_their_request() {
        // Only the answer to the request counts; other messages and lines
        // that are not JSON are passed over, and a refusal is told apart.
        let (send, replies) = mpsc::channel();
        for line in [
            r#"{"type":"system","subtype":"init"}"#,
            "not json",
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"other","response":{}}}"#,
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"rate_limits":{}}}}"#,
            r#"{"type":"control_response","response":{"subtype":"error","request_id":"initialize","error":"not signed in"}}"#,
        ] {
            send.send(Ok(line.as_bytes().to_vec())).unwrap();
        }
        let soon = Instant::now() + Duration::from_millis(200);
        assert!(
            answer(&replies, "usage", soon)
                .unwrap()
                .get("rate_limits")
                .is_some()
        );
        assert_eq!(
            answer(&replies, "initialize", soon).unwrap_err(),
            "Claude Code refused initialize: not signed in"
        );
        assert_eq!(
            answer(&replies, "usage", soon).unwrap_err(),
            "no answer to usage in time"
        );
        drop(send);
        assert_eq!(
            answer(&replies, "usage", soon).unwrap_err(),
            "Claude Code ended"
        );
    }

    #[test]
    fn claude_code_keeps_no_session_and_no_status_line_mark() {
        // Print mode keeps no session, reads no settings of the person's,
        // starts no MCP server, and runs no status line.
        for flag in [
            "-p",
            "--no-session-persistence",
            "--restricted",
            "--strict-mcp-config",
        ] {
            assert!(ARGS.contains(&flag), "{flag}");
        }
        let added: Vec<_> = added().map(|(name, _)| name).collect();
        assert_eq!(
            added,
            ["CLAUDE_CODE_SKIP_PROMPT_HISTORY", "DISABLE_AUTOUPDATER"]
        );
    }
}
