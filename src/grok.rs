//! Grok's credit usage, from the Grok CLI it signs in itself: `grok agent
//! stdio` started for each read, greeted over the Agent Client Protocol and
//! asked its billing extension, then ended. No session is made and nothing
//! is prompted, so no model is called; the CLI's sign-in is never read. A
//! free account is told its period but no usage, which then shows as `--`;
//! a paid one leaves its usage out while it is 0, which shows as 0%. The
//! CLI runs with LimitPeeker's own home, so its logs and caches stay out of
//! the user's ~/.grok, and signs in with the user's sign-in file.

use crate::job::{self, Cost, Job};
use crate::json::{self, Json};
use crate::usage::{Found, Limit, Reading};
use std::ffi::OsString;
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HANDLE;

/// How long each answer is waited for.
const TIMEOUT: Duration = Duration::from_secs(30);
/// The longest line taken from the CLI.
const LONGEST_LINE: u64 = 1 << 20;
/// JSON-RPC: the method is not known.
const UNKNOWN_METHOD: f64 = -32601.0;
/// The billing extension, by the name the CLI answers to, then the bare
/// name the source spells.
const BILLING: [&str; 2] = ["_x.ai/billing", "x.ai/billing"];
/// Where the official installer puts the CLI, under USERPROFILE.
const INSTALLED: &str = ".grok\\bin\\grok.exe";
/// JSON-RPC from the CLI: it is not signed in.
const AUTH_REQUIRED: f64 = -32000.0;
/// LimitPeeker's own home for the CLI, in its data folder beside
/// Antigravity's.
const HOME: &str = "grok-home";
/// The most the CLI's logs in that home may take before they go, as each
/// read adds about 10 KB.
const LOGS_MOST: u64 = 1 << 20;
/// Set once the CLI in its own home was not signed in although the user's
/// sign-in file is there, as when a later CLI no longer takes the file's
/// path; the rest of this run reads in the user's home.
static OWN_HOME_REFUSED: AtomicBool = AtomicBool::new(false);

/// Where the Grok CLI is: `grok.exe` in a folder on the PATH, or where the
/// official installer puts it.
pub fn find() -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("grok.exe"))
        .find(|exe| exe.is_file())
        .or_else(|| {
            Some(PathBuf::from(std::env::var_os("USERPROFILE")?).join(INSTALLED))
                .filter(|exe| exe.is_absolute() && exe.is_file())
        })
}

/// Why a billing question got no answer: the CLI was not signed in, or
/// anything else.
enum Unanswered {
    SignIn(String),
    Other(String),
}

impl Unanswered {
    fn why(self) -> String {
        match self {
            Self::SignIn(why) | Self::Other(why) => why,
        }
    }
}

/// The Grok CLI at `exe` with `args` as the person's own runs: in its
/// folder and the person's environment, for its own update, which the
/// reads leave off. Not in the reads' own home: Grok installs an update
/// into the home it runs in, so there it would land beside this program's
/// state and never reach the CLI that is read.
pub fn command(exe: &Path, args: &[&str]) -> Option<Command> {
    let mut command = Command::new(exe);
    command.args(args).current_dir(exe.parent()?);
    Some(command)
}

/// Asks the Grok CLI at `exe` for its billing once, and tells what it cost.
/// It runs in LimitPeeker's own home with the user's sign-in file; when it
/// is not signed in there though the file is there, it asks again in the
/// user's home, says so once, and asks there for the rest of this run.
pub fn read(exe: &Path) -> (Result<Found, String>, Cost) {
    let own = (!OWN_HOME_REFUSED.load(Ordering::Relaxed))
        .then(|| {
            let home = crate::settings::data_dir()?.join(HOME);
            std::fs::create_dir_all(&home).ok()?;
            trim_logs(&home);
            own_environment(&home, |name| std::env::var_os(name))
        })
        .flatten();
    read_in(exe, own)
}

/// Asks in `own` home when there is one, else in the user's home.
fn read_in(exe: &Path, own: Option<OwnHome>) -> (Result<Found, String>, Cost) {
    let Some(environment) = own else {
        let (result, cost) = read_with(exe, &[]);
        return (result.map_err(Unanswered::why), cost);
    };
    match read_with(exe, &environment.pairs) {
        (Err(Unanswered::SignIn(_)), first) if environment.auth.is_file() => {
            OWN_HOME_REFUSED.store(true, Ordering::Relaxed);
            let (result, again) = read_with(exe, &[]);
            let cost = Cost {
                wall_ms: first.wall_ms + again.wall_ms,
                user_ms: first.user_ms + again.user_ms,
                kernel_ms: first.kernel_ms + again.kernel_ms,
                peak_mb: first.peak_mb.max(again.peak_mb),
            };
            let said = "its own home is not signed in; read in the user's home";
            let result = result.map(|found| Found {
                told: format!("{said}; {}", found.told),
                ..found
            });
            (result.map_err(|why| format!("{said}; {}", why.why())), cost)
        }
        (result, cost) => (result.map_err(Unanswered::why), cost),
    }
}

/// Asks the CLI started with `environment` added to this program's.
fn read_with(exe: &Path, environment: &[(&str, OsString)]) -> (Result<Found, Unanswered>, Cost) {
    let began = Instant::now();
    let mut agent = match Agent::spawn(exe, environment) {
        Ok(agent) => agent,
        Err(why) => return (Err(Unanswered::Other(why)), Cost::default()),
    };
    let result = agent
        .billing()
        .and_then(|answer| found(&answer).map_err(Unanswered::Other));
    let cost = Cost {
        wall_ms: began.elapsed().as_secs_f64() * 1000.0,
        ..agent.end()
    };
    (result, cost)
}

/// What keeps the CLI to its own home, and the user's sign-in file it is
/// given.
struct OwnHome {
    pairs: Vec<(&'static str, OsString)>,
    auth: PathBuf,
}

/// The environment that keeps the CLI to `home`: its home there, the
/// user's sign-in file by the path the user's Grok takes it from (the
/// user's own GROK_AUTH_PATH, else the auth.json in the user's GROK_HOME,
/// else in ~/.grok), and no self-update, which this program runs apart
/// from the reads. None when no such path can be named.
fn own_environment(home: &Path, user: impl Fn(&str) -> Option<OsString>) -> Option<OwnHome> {
    let set = |name: &str| user(name).filter(|value| !value.is_empty());
    let auth = set("GROK_AUTH_PATH")
        .map(PathBuf::from)
        .or_else(|| set("GROK_HOME").map(|dir| PathBuf::from(dir).join("auth.json")))
        .or_else(|| {
            set("USERPROFILE").map(|dir| PathBuf::from(dir).join(".grok").join("auth.json"))
        })?;
    Some(OwnHome {
        pairs: vec![
            ("GROK_HOME", home.as_os_str().to_owned()),
            ("GROK_AUTH_PATH", auth.as_os_str().to_owned()),
            ("GROK_DISABLE_AUTOUPDATER", "1".into()),
        ],
        auth,
    })
}

/// Takes away the CLI's logs in its own home once they pass LOGS_MOST.
fn trim_logs(home: &Path) {
    let logs = home.join("logs");
    if size_of(&logs) > LOGS_MOST {
        let _ = std::fs::remove_dir_all(&logs);
    }
}

/// The bytes of the files in a folder and the folders in it.
fn size_of(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| match entry.file_type() {
                    Ok(kind) if kind.is_dir() => size_of(&entry.path()),
                    Ok(_) => entry.metadata().map_or(0, |m| m.len()),
                    Err(_) => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

/// A running `grok agent stdio`, one JSON-RPC message per line each way, in
/// a job that ends it and what it started.
struct Agent {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    lines: mpsc::Receiver<Result<Vec<u8>, String>>,
    next: u32,
    job: Job,
}

impl Agent {
    fn spawn(exe: &Path, environment: &[(&str, OsString)]) -> Result<Self, String> {
        let mut child = Command::new(exe)
            .args(["agent", "stdio"])
            .envs(environment.iter().map(|(name, value)| (name, value)))
            .current_dir(exe.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(job::CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("could not start: {e}"))?;
        let job = Job::around(HANDLE(child.as_raw_handle()));
        let input = child.stdin.take();
        let lines = match child.stdout.take() {
            Some(output) => job::lines(output, LONGEST_LINE),
            None => mpsc::channel().1,
        };
        Ok(Self {
            child,
            input,
            lines,
            next: 1,
            job,
        })
    }

    fn send(&mut self, message: &str) -> Result<(), String> {
        let input = self.input.as_mut().ok_or("no input")?;
        input
            .write_all(format!("{message}\n").as_bytes())
            .and_then(|_| input.flush())
            .map_err(|e| format!("could not write: {e}"))
    }

    /// Asks `method` and waits for its answer, result or error. What the
    /// CLI asks in turn is refused, since this offers no client features.
    fn request(&mut self, method: &str, params: &str) -> Result<Json, String> {
        let id = self.next;
        self.next += 1;
        self.send(&format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"{method}","params":{params}}}"#
        ))?;
        let until = Instant::now() + TIMEOUT;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(left) {
                Ok(Ok(line)) => line,
                Ok(Err(why)) => return Err(why),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(format!("no answer to {method} in {} s", TIMEOUT.as_secs()));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(format!("the CLI ended before answering {method}"));
                }
            };
            let Some(message) = std::str::from_utf8(&line).ok().and_then(json::parse) else {
                continue;
            };
            if message.get("method").is_some() {
                if let Some(asked) = message.get("id") {
                    let asked = match asked {
                        Json::Number(n) => n.to_string(),
                        Json::Text(t) => format!("\"{}\"", json::escaped(t)),
                        _ => continue,
                    };
                    self.send(&format!(
                        r#"{{"jsonrpc":"2.0","id":{asked},"error":{{"code":-32601,"message":"not supported"}}}}"#
                    ))?;
                }
                continue;
            }
            if message.get("id").and_then(Json::number) == Some(f64::from(id)) {
                return Ok(message);
            }
        }
    }

    /// Greets the CLI, then asks for its billing by the first name it
    /// knows; the answer's result. Not signed in, it is refused, and it is
    /// never asked to sign in, which would open a browser.
    fn billing(&mut self) -> Result<Json, Unanswered> {
        let hello = self
            .request(
                "initialize",
                r#"{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false}}"#,
            )
            .map_err(Unanswered::Other)?;
        if let Some(error) = hello.get("error") {
            return Err(Unanswered::Other(format!(
                "initialize was refused: {}",
                said(error)
            )));
        }
        let mut why = String::new();
        for method in BILLING {
            let answer = self.request(method, "{}").map_err(Unanswered::Other)?;
            match answer.get("error") {
                None => {
                    return answer
                        .get("result")
                        .cloned()
                        .ok_or_else(|| Unanswered::Other(format!("{method} answered nothing")));
                }
                Some(error) if error.get("code").and_then(Json::number) == Some(UNKNOWN_METHOD) => {
                    why = format!("{method} is not known");
                }
                Some(error) if error.get("code").and_then(Json::number) == Some(AUTH_REQUIRED) => {
                    return Err(Unanswered::SignIn(format!(
                        "{method} was refused: {}",
                        said(error)
                    )));
                }
                Some(error) => {
                    return Err(Unanswered::Other(format!(
                        "{method} was refused: {}",
                        said(error)
                    )));
                }
            }
        }
        Err(Unanswered::Other(why))
    }

    /// Ends the CLI, and tells what it used in all.
    fn end(mut self) -> Cost {
        self.input = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.job.cost()
    }
}

/// An error's code and message, for the log.
fn said(error: &Json) -> String {
    format!(
        "{} {}",
        error.get("code").and_then(Json::number).unwrap_or(0.0),
        error.get("message").and_then(Json::text).unwrap_or("")
    )
}

/// The one limit in a billing answer: the current period, with the share
/// of credit used when it came. A paid tier leaves the share out while it
/// is 0, so there it is 0%; a free or unknown tier's is not read, so it
/// shows `--`. It starts over when the period ends. The tier is told, not
/// shown.
pub fn found(answer: &Json) -> Result<Found, String> {
    let config = answer.get("config");
    let period = config
        .and_then(|c| c.get("currentPeriod"))
        .ok_or("the answer has no current period")?;
    let at = |key: &str| {
        period
            .get(key)
            .and_then(Json::text)
            .and_then(crate::claude_usage::unix_seconds)
    };
    let (start, end) = (at("start"), at("end"));
    let label = match period.get("type").and_then(Json::text) {
        Some("USAGE_PERIOD_TYPE_WEEKLY") => "7d".to_string(),
        Some("USAGE_PERIOD_TYPE_MONTHLY") => "1mo".to_string(),
        _ => match (start, end) {
            (Some(start), Some(end)) if end > start => {
                format!("{}d", ((end - start) as f64 / 86_400.0).round() as i64)
            }
            _ => return Err("the period has no kind and no span".into()),
        },
    };
    let given = config
        .and_then(|c| c.get("creditUsagePercent"))
        .and_then(Json::number)
        .filter(|n| n.is_finite())
        .map(|n| n.round().clamp(0.0, 100.0) as u8);
    let tier = answer
        .get("subscription_tier")
        .or_else(|| answer.get("subscriptionTier"))
        .and_then(Json::text)
        .filter(|tier| !tier.trim().is_empty());
    let paid = tier.is_some_and(|tier| !tier.trim().eq_ignore_ascii_case("free"));
    let used = given.or(paid.then_some(0));
    let tier = tier.unwrap_or("unknown");
    let resets = end.map_or("unknown".to_string(), |at| at.to_string());
    let told = match (given, used) {
        (Some(used), _) => format!("{label}={used}% resets_at={resets} tier={tier}"),
        (None, Some(used)) => format!("{label}={used}% (left out) resets_at={resets} tier={tier}"),
        (None, None) => format!("{label}=no usage given resets_at={resets} tier={tier}"),
    };
    Ok(Found {
        limits: vec![Limit {
            label,
            reading: used.map_or(Reading::Pending, Reading::Read),
            resets: end,
            amount: None,
        }],
        told,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<Found, String> {
        found(&json::parse(text).unwrap())
    }

    #[test]
    fn a_free_account_has_its_period_and_no_usage() {
        // What Grok CLI 1.0.46 answered a free account: the week, and no
        // usage, which shows as `--` with the week's end.
        let found = read(
            r#"{"config": {"currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-10-01T00:00:00+00:00", "end": "2026-10-08T00:00:00+00:00"},
                "onDemandCap": {"val": 0}, "onDemandUsed": {"val": 0}, "prepaidBalance": {"val": 0},
                "isUnifiedBillingUser": true},
              "subscription_tier": "Free"}"#,
        )
        .unwrap();
        let limits: Vec<_> = found
            .limits
            .iter()
            .map(|l| (l.label.as_str(), l.reading, l.resets))
            .collect();
        assert_eq!(limits, [("7d", Reading::Pending, Some(1_791_417_600))]);
        assert_eq!(
            found.told,
            "7d=no usage given resets_at=1791417600 tier=Free"
        );
    }

    #[test]
    fn a_paid_account_shows_its_credit_used() {
        // As the source spells a paid account's answer; a month, or a
        // period of another kind by its days.
        let found = read(
            r#"{"config": {"creditUsagePercent": 37.6, "currentPeriod": {"type": "USAGE_PERIOD_TYPE_MONTHLY",
                "start": "2026-10-01T00:00:00Z", "end": "2026-11-01T00:00:00Z"}},
              "subscriptionTier": "SuperGrok"}"#,
        )
        .unwrap();
        assert_eq!(
            (found.limits[0].label.as_str(), found.limits[0].reading),
            ("1mo", Reading::Read(38))
        );
        assert!(found.told.ends_with("tier=SuperGrok"));
        let other = read(
            r#"{"config": {"creditUsagePercent": 140, "currentPeriod": {"type": "USAGE_PERIOD_TYPE_DAILY",
                "start": "2026-10-01T00:00:00Z", "end": "2026-10-02T00:00:00Z"}}}"#,
        )
        .unwrap();
        assert_eq!(
            (other.limits[0].label.as_str(), other.limits[0].reading),
            ("1d", Reading::Read(100))
        );
        assert!(other.told.ends_with("tier=unknown"));
    }

    #[test]
    fn a_paid_account_that_has_used_nothing_is_at_0() {
        // What Grok CLI 1.0.62 answered a SuperGrok Lite account just after
        // it was bought: the week from then, and no share, which a paid tier
        // leaves out at 0. A tier not known keeps `--`.
        let lite = read(
            r#"{"config": {"currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-10-20T06:38:52.066+00:00", "end": "2026-10-27T06:38:52.066+00:00"},
                "prepaidBalance": {"val": 0}, "isUnifiedBillingUser": true,
                "billingPeriodStart": "2026-10-20T06:38:52.066+00:00",
                "billingPeriodEnd": "2026-10-27T06:38:52.066+00:00"},
              "subscription_tier": "SuperGrok Lite"}"#,
        )
        .unwrap();
        assert_eq!(
            (
                lite.limits[0].label.as_str(),
                lite.limits[0].reading,
                lite.limits[0].resets
            ),
            ("7d", Reading::Read(0), Some(1_793_083_132))
        );
        assert_eq!(
            lite.told,
            "7d=0% (left out) resets_at=1793083132 tier=SuperGrok Lite"
        );
        for tier in [
            r#""subscription_tier": "free""#,
            r#""subscription_tier": " ""#,
            r#""other": 1"#,
        ] {
            let unknown = read(&format!(
                r#"{{"config": {{"currentPeriod": {{"type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-10-20T00:00:00Z", "end": "2026-10-27T00:00:00Z"}}}}, {tier}}}"#
            ))
            .unwrap();
            assert_eq!(unknown.limits[0].reading, Reading::Pending, "{tier}");
        }
    }

    #[test]
    #[ignore = "needs the Grok CLI installed and signed in"]
    fn the_installed_cli_answers_its_billing() {
        // Run by hand: cargo test the_installed_cli -- --ignored --nocapture
        let exe = find().expect("the Grok CLI is not installed");
        let (read, cost) = super::read(&exe);
        println!(
            "{exe:?}: {:?} in {:.0} ms, cpu {:.0} ms, peak {:.0} MB",
            read.as_ref().map(|found| &found.told),
            cost.wall_ms,
            cost.cpu_ms(),
            cost.peak_mb
        );
        assert!(read.is_ok());
    }

    #[test]
    #[ignore = "needs the Grok CLI installed and signed in"]
    fn not_signed_in_in_its_own_home_it_asks_in_the_users() {
        // Run by hand: cargo test not_signed_in_in_its_own_home -- --ignored --nocapture
        // The CLI in its own home is given a sign-in file that is not
        // there, as a later CLI that no longer takes the path would be,
        // while the user's own file is; it is never asked to sign in, and
        // asks again in the user's home.
        let exe = find().expect("the Grok CLI is not installed");
        let home =
            std::env::temp_dir().join(format!("limitpeeker-grok-own-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let mut own = own_environment(&home, |name| std::env::var_os(name)).unwrap();
        assert!(own.auth.is_file(), "the user's Grok is not signed in");
        own.pairs[1].1 = home.join("nowhere.json").into();
        let (read, cost) = read_in(&exe, Some(own));
        println!(
            "{:?} in {:.0} ms, cpu {:.0} ms",
            read.as_ref().map(|found| &found.told),
            cost.wall_ms,
            cost.cpu_ms()
        );
        assert!(
            read.unwrap()
                .told
                .starts_with("its own home is not signed in; read in the user's home; 7d=")
        );
        assert!(OWN_HOME_REFUSED.load(Ordering::Relaxed));
        assert!(!home.join("nowhere.json").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn without_a_period_the_read_failed() {
        for wrong in [
            r#"{}"#,
            r#"{"config": {}}"#,
            r#"{"config": {"currentPeriod": {"type": "USAGE_PERIOD_TYPE_DAILY"}}}"#,
        ] {
            assert!(read(wrong).is_err(), "{wrong}");
        }
    }

    /// An environment of the given names and values only.
    fn user(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |name| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn the_cli_keeps_to_its_own_home_and_signs_in_with_the_users_file() {
        // The sign-in file is where the user's Grok takes it from: the
        // user's own GROK_AUTH_PATH, else the user's GROK_HOME, else
        // ~/.grok; with none of them, nothing is moved.
        let home = Path::new(r"C:\Users\me\AppData\Local\LimitPeeker\grok-home");
        let auth = |pairs| own_environment(home, user(pairs)).map(|own| own.auth);
        const ME: (&str, &str) = ("USERPROFILE", r"C:\Users\me");
        assert_eq!(
            auth(&[ME]),
            Some(PathBuf::from(r"C:\Users\me\.grok\auth.json"))
        );
        assert_eq!(
            auth(&[ME, ("GROK_HOME", r"D:\grok")]),
            Some(PathBuf::from(r"D:\grok\auth.json"))
        );
        assert_eq!(
            auth(&[
                ME,
                ("GROK_HOME", r"D:\grok"),
                ("GROK_AUTH_PATH", r"E:\keys\grok.json")
            ]),
            Some(PathBuf::from(r"E:\keys\grok.json"))
        );
        assert_eq!(
            auth(&[ME, ("GROK_HOME", ""), ("GROK_AUTH_PATH", "")]),
            Some(PathBuf::from(r"C:\Users\me\.grok\auth.json"))
        );
        assert_eq!(auth(&[("GROK_HOME", "")]), None);
        let own = own_environment(home, user(&[ME])).unwrap();
        let pairs: Vec<_> = own
            .pairs
            .iter()
            .map(|(name, value)| (*name, value.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("GROK_HOME", home.display().to_string()),
                ("GROK_AUTH_PATH", r"C:\Users\me\.grok\auth.json".to_string()),
                ("GROK_DISABLE_AUTOUPDATER", "1".to_string()),
            ]
        );
    }

    #[test]
    fn its_update_runs_as_the_persons_own_would() {
        // In its folder and the person's environment, with no home of this
        // program's: Grok installs an update into the home it runs in, and
        // in the reads' own home it installed beside this program's state.
        let exe = Path::new(r"C:\Users\me\.grok\bin\grok.exe");
        let update = command(exe, &["update"]).unwrap();
        assert_eq!(update.get_current_dir(), exe.parent());
        assert_eq!(update.get_args().collect::<Vec<_>>(), ["update"]);
        assert_eq!(update.get_envs().count(), 0);
    }

    #[test]
    fn its_own_logs_go_once_past_a_megabyte() {
        // Each read adds about 10 KB to the CLI's log in its own home; all
        // its logs go together once they pass LOGS_MOST.
        let home =
            std::env::temp_dir().join(format!("limitpeeker-grok-home-{}", std::process::id()));
        let logs = home.join("logs");
        std::fs::create_dir_all(logs.join("mcp")).unwrap();
        std::fs::write(logs.join("unified.jsonl"), vec![b'x'; 600_000]).unwrap();
        std::fs::write(logs.join("mcp").join("a.log"), vec![b'x'; 400_000]).unwrap();
        trim_logs(&home);
        assert!(logs.join("unified.jsonl").is_file());
        std::fs::write(logs.join("mcp").join("b.log"), vec![b'x'; 100_000]).unwrap();
        trim_logs(&home);
        assert!(!logs.exists());
        trim_logs(&home);
        std::fs::remove_dir_all(&home).unwrap();
    }
}
