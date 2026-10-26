//! Gemini's usage limits, as the Antigravity CLI (`agy`) hands them to a
//! status line command. A hidden `agy`, started in a home folder of this
//! program's own, runs `limitpeeker.exe --agy-statusline` with its state on
//! standard input; this takes the quota out of it, notes it for the read
//! under way and prints one line nobody sees. No sign-in is read or kept, and
//! the person's own `agy` settings are not touched.

use crate::json::{self, Json};
use crate::usage::{Found, Limit, Reading};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where the hidden `agy`'s status line notes what it was handed. Only runs
/// with crate::claude::HEARD_VAR set write it.
pub const HEARD_FILE: &str = "agy-heard.log";
/// The Gemini buckets kept, by `agy`'s names, with the labels they show as,
/// in the order they show. Others, such as `3p-weekly` for the Claude and
/// GPT-OSS models inside Antigravity, are not Gemini's.
const BUCKETS: [(&str, &str); 3] = [
    ("gemini-5h", "5h"),
    ("gemini-daily", "1d"),
    ("gemini-weekly", "7d"),
];
/// Where the official installer puts `agy`, under LOCALAPPDATA.
const INSTALLED: &str = "agy\\bin\\agy.exe";

/// One quota bucket as `agy` handed it: its name, the share of it left from
/// 0 to 1, and when it starts over, in Unix seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Bucket {
    pub name: String,
    pub left: f64,
    pub resets: Option<i64>,
}

/// Takes what `agy` handed over, notes its quota when the hidden `agy` ran
/// this, and prints a line.
pub fn capture() {
    let input = crate::claude::read_input();
    let data = std::str::from_utf8(&input).ok().and_then(json::parse);
    // Input that is not JSON is taken as a run without a quota.
    let buckets = data
        .as_ref()
        .and_then(|data| buckets(data, crate::usage::unix_now()));
    if std::env::var_os(crate::claude::HEARD_VAR).is_some()
        && let Some(folder) = crate::settings::data_dir()
    {
        let _ = std::fs::create_dir_all(&folder);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(folder.join(HEARD_FILE))
        {
            // One write, so two runs at once do not mix their lines.
            let _ = file.write_all(format!("{}\n", heard_line(buckets.as_deref())).as_bytes());
        }
    }
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", shown(buckets.as_deref()));
    let _ = out.flush();
}

/// The buckets in `quota`, or None when there is no `quota` yet, as in the
/// first runs after `agy` starts. A bucket whose name could not be written
/// on one line, or whose share is not a number, is left out.
pub fn buckets(data: &Json, now: i64) -> Option<Vec<Bucket>> {
    let Some(Json::Object(fields)) = data.get("quota") else {
        return None;
    };
    Some(
        fields
            .iter()
            .filter(|(name, _)| {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            })
            .filter_map(|(name, item)| {
                let left = item
                    .get("remaining_fraction")
                    .and_then(Json::number)
                    .filter(|n| n.is_finite())?
                    .clamp(0.0, 1.0);
                let resets = item
                    .get("reset_time")
                    .and_then(Json::text)
                    .and_then(crate::claude_usage::unix_seconds)
                    .or_else(|| {
                        item.get("reset_in_seconds")
                            .and_then(Json::number)
                            .filter(|n| n.is_finite() && *n >= 0.0)
                            .map(|n| now + n as i64)
                    });
                Some(Bucket {
                    name: name.clone(),
                    left,
                    resets,
                })
            })
            .collect(),
    )
}

/// One line of HEARD_FILE, split by tabs: `quota` or `none`, then a name,
/// share left and reset time for each bucket.
fn heard_line(buckets: Option<&[Bucket]>) -> String {
    let Some(buckets) = buckets else {
        return "none".into();
    };
    let mut fields = vec!["quota".to_string()];
    for bucket in buckets {
        fields.push(bucket.name.clone());
        fields.push(bucket.left.to_string());
        fields.push(bucket.resets.map_or("-".to_string(), |at| at.to_string()));
    }
    fields.join("\t")
}

/// A line of HEARD_FILE read back: Some(None) for a run without `quota`,
/// None when it is not a line of it.
pub fn heard(line: &str) -> Option<Option<Vec<Bucket>>> {
    let fields: Vec<&str> = line.trim_end_matches(['\r', '\n']).split('\t').collect();
    match fields.as_slice() {
        ["none"] => Some(None),
        ["quota", rest @ ..] if rest.len() % 3 == 0 => rest
            .chunks(3)
            .map(|b| {
                Some(Bucket {
                    name: b[0].to_string(),
                    left: b[1].parse().ok()?,
                    resets: if b[2] == "-" {
                        None
                    } else {
                        Some(b[2].parse().ok()?)
                    },
                })
            })
            .collect::<Option<Vec<_>>>()
            .map(Some),
        _ => None,
    }
}

/// The line the hidden `agy` shows.
fn shown(buckets: Option<&[Bucket]>) -> String {
    match buckets {
        None => "LimitPeeker: no quota yet".into(),
        Some(buckets) => {
            let parts: Vec<String> = buckets
                .iter()
                .map(|b| format!("{} {}%", b.name, (b.left * 100.0).round() as i64))
                .collect();
            format!("LimitPeeker {}", parts.join(" | "))
        }
    }
}

/// Gemini's limits in what was heard: each bucket of BUCKETS that came, its
/// share used rounded to a whole percent, as Codex's are; the other
/// buckets are told but not shown. Without a Gemini bucket the read failed.
pub fn found(buckets: &[Bucket]) -> Result<Found, String> {
    let mut limits = Vec::new();
    let mut told = Vec::new();
    for (name, label) in BUCKETS {
        if let Some(bucket) = buckets.iter().find(|b| b.name == name) {
            let used = (100.0 - bucket.left * 100.0).round().clamp(0.0, 100.0) as u8;
            let resets = bucket.resets.map_or("unknown".into(), |at| at.to_string());
            told.push(format!("{label}={used}% resets_at={resets}"));
            limits.push(Limit {
                label: label.into(),
                reading: Reading::Read(used),
                resets: bucket.resets,
                amount: None,
            });
        }
    }
    let others: Vec<String> = buckets
        .iter()
        .filter(|b| !BUCKETS.iter().any(|(name, _)| *name == b.name))
        .map(|b| format!("{}={}%left", b.name, (b.left * 100.0).round() as i64))
        .collect();
    if limits.is_empty() {
        return Err(if others.is_empty() {
            "the quota had no bucket".into()
        } else {
            format!("no Gemini bucket came, only {}", others.join(" "))
        });
    }
    if !others.is_empty() {
        told.push(format!("(not shown: {})", others.join(" ")));
    }
    Ok(Found {
        limits,
        told: told.join(" "),
    })
}

/// Where `agy` is: `agy.exe` in a folder on the PATH, or where the official
/// installer puts it.
pub fn find() -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("agy.exe"))
        .find(|exe| exe.is_file())
        .or_else(|| {
            Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(INSTALLED))
                .filter(|exe| exe.is_absolute() && exe.is_file())
        })
}

/// The EXE as the status line command names it. `agy` hands the command to
/// cmd with its quotes escaped, and cmd then takes them as part of the name,
/// so the path goes bare, with `/`; one with spaces goes by its short name.
pub fn bare_path(exe: &Path) -> Result<String, String> {
    let long = exe.to_string_lossy().replace('\\', "/");
    if !long.contains(' ') {
        return Ok(long);
    }
    short_name(exe)
        .map(|short| short.replace('\\', "/"))
        .filter(|short| !short.contains(' '))
        .ok_or_else(|| {
            format!(
                "the EXE's path has spaces and no short name, which the Antigravity CLI cannot run: {}",
                exe.display()
            )
        })
}

/// The 8.3 name of a path, where the volume keeps them.
fn short_name(path: &Path) -> Option<String> {
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let long = windows::core::PCWSTR(wide.as_ptr());
    unsafe {
        let needed = windows::Win32::Storage::FileSystem::GetShortPathNameW(long, None);
        if needed == 0 {
            return None;
        }
        let mut short = vec![0u16; needed as usize];
        let length =
            windows::Win32::Storage::FileSystem::GetShortPathNameW(long, Some(&mut short)) as usize;
        (length > 0 && length < short.len()).then(|| String::from_utf16_lossy(&short[..length]))
    }
}

/// The hidden `agy`'s settings in its home: the status line runs this EXE
/// by `bare_path`; its colors are set and the home itself is trusted, so it
/// asks nothing on its first start.
pub fn settings(exe: &str, home: &Path) -> String {
    let command = format!("{exe} --agy-statusline");
    format!(
        "{{\"colorScheme\":\"dark\",\"trustedWorkspaces\":[\"{}\"],\"statusLine\":{{\"type\":\"command\",\"command\":\"{}\"}}}}\n",
        json::escaped(&home.to_string_lossy()),
        json::escaped(&command)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape `agy` 1.2.14 handed the probe on a free account, with what
    /// identifies the person in it.
    const HANDED: &str = r#"{
        "cwd": "C:\\Users\\me", "session_id": "s-1", "conversation_id": "c-1",
        "email": "me@example.com",
        "model": {"id": "Gemini 3.8 Flash (High)", "display_name": "Gemini 3.8 Flash (High)"},
        "version": "1.2.14", "product": "antigravity",
        "quota": {
            "3p-weekly": {"remaining_fraction": 1, "reset_time": "2026-10-08T04:52:40Z", "reset_in_seconds": 604788},
            "gemini-weekly": {"remaining_fraction": 0.9914176, "reset_time": "2026-10-08T04:48:54Z", "reset_in_seconds": 604562}
        },
        "agent_state": "idle", "plan_tier": "Antigravity Starter Quota"
    }"#;

    fn read(text: &str) -> Option<Vec<Bucket>> {
        buckets(&json::parse(text).unwrap(), 1_000)
    }

    #[test]
    fn the_quota_is_taken_and_nothing_else() {
        // The buckets, with when each starts over; the e-mail, the folder
        // and the conversation are not kept anywhere.
        let buckets = read(HANDED).unwrap();
        assert_eq!(
            buckets,
            [
                Bucket {
                    name: "3p-weekly".into(),
                    left: 1.0,
                    resets: Some(1_791_435_160)
                },
                Bucket {
                    name: "gemini-weekly".into(),
                    left: 0.9914176,
                    resets: Some(1_791_434_934)
                },
            ]
        );
        let line = heard_line(Some(&buckets));
        assert_eq!(
            line,
            "quota\t3p-weekly\t1\t1791435160\tgemini-weekly\t0.9914176\t1791434934"
        );
        assert!(!line.contains("me") && !line.contains("c-1"));
        assert_eq!(heard(&format!("{line}\r\n")), Some(Some(buckets.clone())));
        assert_eq!(
            shown(Some(&buckets)),
            "LimitPeeker 3p-weekly 100% | gemini-weekly 99%"
        );
    }

    #[test]
    fn a_run_before_the_quota_is_told_apart() {
        // The first runs after `agy` starts have no quota; a share that is
        // not a number, or a name that would break the line, is left out,
        // and with no reset time the seconds left are counted from now.
        assert_eq!(read(r#"{"agent_state": "authenticating"}"#), None);
        assert_eq!(heard_line(None), "none");
        assert_eq!(heard("none"), Some(None));
        assert_eq!(shown(None), "LimitPeeker: no quota yet");
        let odd = read(
            r#"{"quota": {"gemini-5h": {"remaining_fraction": "x"}, "bad name\t": {"remaining_fraction": 1},
                "gemini-weekly": {"remaining_fraction": 1.5, "reset_in_seconds": 60}}}"#,
        )
        .unwrap();
        assert_eq!(
            odd,
            [Bucket {
                name: "gemini-weekly".into(),
                left: 1.0,
                resets: Some(1_060)
            }]
        );
        for wrong in ["", "quota\ta\t1", "quota\ta\tx\t-", "maybe"] {
            assert_eq!(heard(wrong), None, "{wrong:?}");
        }
    }

    #[test]
    fn gemini_buckets_become_limits_and_others_are_only_told() {
        // 5h, 1d, 7d in that order, used rounded to a whole percent; the
        // third-party models' bucket is not Gemini's.
        let bucket = |name: &str, left: f64, resets: Option<i64>| Bucket {
            name: name.into(),
            left,
            resets,
        };
        let found = found(&[
            bucket("3p-weekly", 1.0, None),
            bucket("gemini-weekly", 0.9914176, Some(1_791_434_934)),
            bucket("gemini-5h", 0.4, None),
        ])
        .unwrap();
        let limits: Vec<_> = found
            .limits
            .iter()
            .map(|l| (l.label.as_str(), l.reading, l.resets))
            .collect();
        assert_eq!(
            limits,
            [
                ("5h", Reading::Read(60), None),
                ("7d", Reading::Read(1), Some(1_791_434_934))
            ]
        );
        assert_eq!(
            found.told,
            "5h=60% resets_at=unknown 7d=1% resets_at=1791434934 (not shown: 3p-weekly=100%left)"
        );
        assert_eq!(
            super::found(&[bucket("3p-weekly", 1.0, None)])
                .err()
                .unwrap(),
            "no Gemini bucket came, only 3p-weekly=100%left"
        );
        assert_eq!(super::found(&[]).err().unwrap(), "the quota had no bucket");
    }

    #[test]
    fn the_hidden_agy_runs_this_exe_and_trusts_only_its_home() {
        let text = settings(
            "D:/Tools/LimitPeeker/limitpeeker.exe",
            Path::new(r"C:\Users\me\AppData\Local\LimitPeeker\agy-home"),
        );
        let data = json::parse(&text).unwrap();
        assert_eq!(
            data.get("statusLine")
                .and_then(|s| s.get("command"))
                .and_then(Json::text),
            Some("D:/Tools/LimitPeeker/limitpeeker.exe --agy-statusline")
        );
        assert_eq!(
            data.get("trustedWorkspaces"),
            Some(&Json::List(vec![Json::Text(
                r"C:\Users\me\AppData\Local\LimitPeeker\agy-home".into()
            )]))
        );
        assert_eq!(data.get("colorScheme").and_then(Json::text), Some("dark"));
    }

    #[test]
    fn the_command_names_the_exe_without_quotes() {
        // `agy` escapes quotes on their way to cmd, which then cannot find
        // the program: a path goes bare, and one with spaces by its short
        // name, or not at all.
        assert_eq!(
            bare_path(Path::new(r"C:\Tools\LimitPeeker\limitpeeker.exe")),
            Ok("C:/Tools/LimitPeeker/limitpeeker.exe".to_string())
        );
        assert!(bare_path(Path::new(r"C:\No Such Folder\limitpeeker.exe")).is_err());
        // A folder with spaces that is there has a short name where the
        // volume keeps them; either way the command never holds a space.
        let program_files = std::env::var("ProgramFiles").unwrap_or_default();
        if !program_files.is_empty()
            && let Ok(path) = bare_path(Path::new(&program_files))
        {
            assert!(!path.contains(' ') && !path.contains('\\'), "{path}");
        }
    }
}
