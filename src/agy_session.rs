//! Gemini's usage, read once: a hidden `agy` started in a home folder of
//! this program's own, its status line's first run with a quota heard, and
//! ended. Nothing stays running between reads, and the person's own `agy`
//! settings are not touched.

use crate::agy::{self, Bucket, HEARD_FILE};
use crate::conpty::Console;
use crate::job::Cost;
use crate::usage::Found;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long `agy` has to sign in and hand its status line a quota. It took
/// 3.5 s in the probe.
const QUOTA_WITHIN: Duration = Duration::from_secs(30);
/// How long `agy` has to end after each Ctrl+C, before its job is ended.
const QUIT_WITHIN: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(100);
/// The console's columns and rows.
const SIZE: (i16, i16) = (120, 40);
/// Taken out of `agy`'s environment: with a key, it might use the key
/// instead of the Google sign-in, and hand no quota.
const REMOVED: [&str; 2] = ["GEMINI_API_KEY", "GOOGLE_API_KEY"];
/// This program's home folder for `agy`, in its data folder.
const HOME: &str = "agy-home";
/// Where `agy` keeps when it last checked for updates, in its home; it checks
/// again only once this is 15 minutes old.
const LAST_CHECK: [&str; 3] = [".gemini", "antigravity-cli", "last_check.timestamp"];

/// Reads Gemini's usage once with the `agy` at `exe`, and tells what it
/// cost: how long it took, and the CPU and most memory `agy` and all it
/// started used.
pub fn read(exe: &Path) -> (Result<Found, String>, Cost) {
    let (result, cost, _) = read_seen(exe);
    (result, cost)
}

/// A read, with the end of what `agy` wrote to its console, which the probe
/// keeps.
fn read_seen(exe: &Path) -> (Result<Found, String>, Cost, Vec<u8>) {
    let began = Instant::now();
    let home = match prepare() {
        Ok(home) => home,
        Err(why) => return (Err(why), Cost::default(), Vec::new()),
    };
    let Some(heard) = home.parent().map(|data| data.join(HEARD_FILE)) else {
        return (Err("no data folder".into()), Cost::default(), Vec::new());
    };
    let mut listener = Listener::new(heard);
    let folder = home.to_string_lossy().into_owned();
    // Only the home folder changes, so its settings are this program's and
    // the sign-in, which `agy` keeps elsewhere, is the person's.
    let added = [
        ("USERPROFILE", folder.as_str()),
        ("HOME", folder.as_str()),
        (crate::claude::HEARD_VAR, "1"),
    ];
    hold_update_check(&home);
    let console = match Console::start(exe, &[], &home, &REMOVED, &added, SIZE) {
        Ok(console) => console,
        Err(why) => return (Err(why), Cost::default(), Vec::new()),
    };
    let heard = listener.wait_for_quota(&console);
    let ended = quit(&console);
    let seen = console.kept();
    let cost = Cost {
        wall_ms: began.elapsed().as_secs_f64() * 1000.0,
        ..console.end()
    };
    clear_crashes(&home);
    let result = heard.and_then(|(buckets, runs)| {
        agy::found(&buckets).map(|mut found| {
            found
                .told
                .push_str(&format!(" after {runs} status line runs; {ended}"));
            found
        })
    });
    (result, cost, seen)
}

/// Has `agy` take its last update check as just now, so it does not check:
/// the check runs apart from the hidden console and showed a terminal
/// window. Updating is left to the person's own `agy`.
fn hold_update_check(home: &Path) {
    let file = LAST_CHECK
        .iter()
        .fold(home.to_path_buf(), |path, part| path.join(part));
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .and_then(|opened| opened.set_modified(std::time::SystemTime::now()));
}

/// `agy` at `exe` with `args`, in this program's home for it as the reads
/// run it, so its update keeps its state there and not in the person's.
/// For its own update, which the reads hold off.
pub fn command(exe: &Path, args: &[&str]) -> Option<std::process::Command> {
    let home = crate::settings::data_dir()?.join(HOME);
    std::fs::create_dir_all(&home).ok()?;
    Some(command_in(&home, exe, args))
}

fn command_in(home: &Path, exe: &Path, args: &[&str]) -> std::process::Command {
    let mut command = std::process::Command::new(exe);
    command
        .args(args)
        .current_dir(home)
        .env("USERPROFILE", home)
        .env("HOME", home);
    for name in REMOVED {
        command.env_remove(name);
    }
    command
}

/// The home folder, made if it is not there, with the settings for this
/// EXE written when they differ.
fn prepare() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let home = crate::settings::data_dir()
        .ok_or("no LOCALAPPDATA")?
        .join(HOME);
    let folder = home.join(".gemini").join("antigravity-cli");
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("could not make {}: {e}", folder.display()))?;
    let file = folder.join("settings.json");
    let text = agy::settings(&agy::bare_path(&exe)?, &home);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&file, text)
            .map_err(|e| format!("could not write {}: {e}", file.display()))?;
    }
    Ok(home)
}

/// Ctrl+C, and again if `agy` is still there after QUIT_WITHIN; what ended
/// it, for the log. Its job ends whatever is left.
fn quit(console: &Console) -> &'static str {
    for _ in 0..2 {
        if console.type_keys(b"\x03").is_err() {
            break;
        }
        let until = Instant::now() + QUIT_WITHIN;
        while Instant::now() < until {
            if !console.running() {
                return "agy ended on Ctrl+C";
            }
            std::thread::sleep(POLL);
        }
    }
    if console.running() {
        "agy ended by its job"
    } else {
        "agy ended on Ctrl+C"
    }
}

/// Takes away the empty crash notes an `agy` ended by its job leaves in the
/// home, which is this program's own; one with something in it stays.
fn clear_crashes(home: &Path) {
    let crashes = home.join(".gemini").join("antigravity-cli").join("crashes");
    if let Ok(entries) = std::fs::read_dir(crashes) {
        for entry in entries.flatten() {
            if entry.metadata().is_ok_and(|m| m.is_file() && m.len() == 0) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// The lines the status line wrote to HEARD_FILE, read as they come. The
/// file is emptied first; only the hidden `agy` writes to it.
struct Listener(crate::log::Tail);
impl Listener {
    fn new(path: PathBuf) -> Self {
        Self(crate::log::Tail::new(path))
    }
    /// The runs heard since last asked: each with its buckets, or None
    /// when it had no quota.
    fn lines(&mut self) -> Vec<Option<Vec<Bucket>>> {
        self.0
            .lines()
            .iter()
            .filter_map(|line| agy::heard(line))
            .collect()
    }
    /// The first run with a quota, and how many runs it took, or why none
    /// came in time.
    fn wait_for_quota(&mut self, console: &Console) -> Result<(Vec<Bucket>, usize), String> {
        let until = Instant::now() + QUOTA_WITHIN;
        let mut runs = 0;
        loop {
            for buckets in self.lines() {
                runs += 1;
                if let Some(buckets) = buckets {
                    return Ok((buckets, runs));
                }
            }
            if !console.running() {
                return Err(format!(
                    "agy ended after {runs} status line runs, none with a quota"
                ));
            }
            if Instant::now() >= until {
                return Err(format!(
                    "no quota in {} s ({runs} status line runs)",
                    QUOTA_WITHIN.as_secs()
                ));
            }
            std::thread::sleep(POLL);
        }
    }
}

/// `--agy-probe`: one read, as the product makes it, with what it cost and
/// how `agy` ended written beside the EXE. Alone, while the product is not
/// running, so its reads do not mix in.
pub fn probe() {
    let mut seen = Vec::new();
    let report = match agy::find() {
        None => "agy not found on the PATH or in %LOCALAPPDATA%\\agy\\bin".to_string(),
        Some(exe) => {
            let (read, cost, screen) = read_seen(&exe);
            seen = screen;
            let spent = format!(
                "{:.0} ms, cpu {:.0} ms, peak {:.0} MB",
                cost.wall_ms,
                cost.cpu_ms(),
                cost.peak_mb
            );
            match read {
                Ok(found) => format!("agy: {}\nread {} in {spent}", exe.display(), found.told),
                Err(why) => format!("agy: {}\nread failed after {spent}: {why}", exe.display()),
            }
        }
    };
    if let Some(folder) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        let name = format!(
            "agy-probe-{}",
            crate::log::local_time().replace([' ', ':'], "-")
        );
        let _ = std::fs::write(folder.join(format!("{name}.txt")), format!("{report}\n"));
        // The end of what `agy` drew, as it came, to see where it stopped.
        let _ = std::fs::write(folder.join(format!("{name}-screen.txt")), seen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_update_runs_in_this_programs_home_for_it() {
        // Its update keeps its state in that home, as the reads do, and it
        // is given no API key. Where it installs is by LOCALAPPDATA, which
        // stays the person's.
        let home = Path::new(r"C:\Users\me\AppData\Local\LimitPeeker\agy-home");
        let command = command_in(
            home,
            Path::new(r"C:\Users\me\AppData\Local\agy\bin\agy.exe"),
            &["update"],
        );
        assert_eq!(command.get_current_dir(), Some(home));
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["update"]);
        let envs: Vec<_> = command.get_envs().collect();
        for name in ["USERPROFILE", "HOME"] {
            assert!(envs.contains(&(std::ffi::OsStr::new(name), Some(home.as_os_str()))));
        }
        for name in REMOVED {
            assert!(envs.contains(&(std::ffi::OsStr::new(name), None)), "{name}");
        }
        assert!(envs.iter().all(|(name, _)| *name != "LOCALAPPDATA"));
    }

    #[test]
    fn agy_takes_its_last_update_check_as_just_now() {
        // It checks for updates once its last check is 15 minutes old, and
        // that check showed a terminal window; the empty file it keeps the
        // time in is made, or set, to now.
        let home =
            std::env::temp_dir().join(format!("limitpeeker-agy-check-{}", std::process::id()));
        let folder = home.join(".gemini").join("antigravity-cli");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("last_check.timestamp");
        let age = |file: &Path| {
            std::fs::metadata(file)
                .unwrap()
                .modified()
                .unwrap()
                .elapsed()
                .unwrap_or_default()
        };
        hold_update_check(&home);
        assert!(age(&file) < Duration::from_secs(60));
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options()
            .append(true)
            .open(&file)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert!(age(&file) > Duration::from_secs(3000));
        hold_update_check(&home);
        assert!(age(&file) < Duration::from_secs(60));
        assert_eq!(std::fs::metadata(&file).unwrap().len(), 0);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn runs_are_heard_once_whole_and_in_order() {
        let path =
            std::env::temp_dir().join(format!("limitpeeker-agy-heard-{}.log", std::process::id()));
        std::fs::write(&path, "left from before\n").unwrap();
        let mut listener = Listener::new(path.clone());
        assert!(listener.lines().is_empty());
        std::fs::write(&path, "none\nquota\tgemini-weekly\t0.5\t-\nquota\tgemini").unwrap();
        let heard = listener.lines();
        assert_eq!(heard.len(), 2);
        assert_eq!(heard[0], None);
        assert_eq!(heard[1].as_ref().unwrap()[0].left, 0.5);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        std::io::Write::write_all(&mut file, b"-5h\t1\t-\n").unwrap();
        drop(file);
        let heard = listener.lines();
        assert_eq!(heard[0].as_ref().unwrap()[0].name, "gemini-5h");
        assert!(listener.lines().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn only_empty_crash_notes_are_taken_away() {
        let home =
            std::env::temp_dir().join(format!("limitpeeker-agy-home-{}", std::process::id()));
        let crashes = home.join(".gemini").join("antigravity-cli").join("crashes");
        std::fs::create_dir_all(&crashes).unwrap();
        std::fs::write(crashes.join("crash_1_a.log"), b"").unwrap();
        std::fs::write(crashes.join("crash_2_b.log"), b"stack").unwrap();
        clear_crashes(&home);
        let left: Vec<_> = std::fs::read_dir(&crashes)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["crash_2_b.log"]);
        let _ = std::fs::remove_dir_all(&home);
    }
}
