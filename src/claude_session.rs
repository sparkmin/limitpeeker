//! Claude's usage, read once: a hidden Claude Code started in its own
//! folder, shown `/usage` and closed, its status line's run heard, and
//! ended. Nothing stays running between reads.

use crate::claude::{self, HEARD_FILE, Heard};
use crate::claude_home::{self, Trust};
use crate::conpty::Console;
use crate::job::Cost;
use crate::usage::{Found, Limit, Reading};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long Claude Code has to show its prompt, which its status line's
/// first run tells.
pub const READY_WITHIN: Duration = Duration::from_secs(30);
/// How long the status line has to run after Esc.
pub const HEARD_WITHIN: Duration = Duration::from_secs(8);
/// Between typing `/usage` and Enter, so the command list keeps up.
pub const TYPED_TO_ENTER: Duration = Duration::from_millis(300);
/// The console's columns and rows.
pub const SIZE: (i16, i16) = (120, 40);
/// Between the prompt showing and `/usage`. The first measurements waited
/// 3 s.
const SETTLE: Duration = Duration::from_secs(1);
/// How long the usage screen is open: 1 s was enough when it was measured.
const OPEN_FOR: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(100);
/// How often the usage screen is opened in one read. The status line runs
/// once after each Esc; when the screen closed before its figures came, that
/// run has no limits, and only opening it again brings them.
const CYCLES: u32 = 2;

/// Reads Claude's usage once with the Claude Code at `claude`, and tells
/// what it cost: how long it took, and the CPU and most memory Claude Code
/// and all it started used.
pub fn read(claude: &Path) -> (Result<Found, String>, Cost) {
    let began = Instant::now();
    let (home, settings, trusted) = match prepare() {
        Ok(prepared) => prepared,
        Err(why) => return (Err(why), Cost::default()),
    };
    let id = match claude_home::session_id() {
        Ok(id) => id,
        Err(why) => return (Err(why), Cost::default()),
    };
    let short: String = id.chars().take(8).collect();
    let Some(heard) = home.parent().map(|data| data.join(HEARD_FILE)) else {
        return (Err("no data folder".into()), Cost::default());
    };
    let mut listener = Listener::new(heard);
    let console = match Console::start(
        claude,
        &claude_home::arguments(&settings, &id, true),
        &home,
        &claude_home::REMOVED,
        &claude_home::ADDED,
        SIZE,
    ) {
        Ok(console) => console,
        Err(why) => return (Err(why), Cost::default()),
    };
    let result = usage(&console, &mut listener, &short).map(|mut found| {
        if trusted {
            found.told.push_str(" (trusted its folder)");
        }
        found
    });
    let cost = Cost {
        wall_ms: began.elapsed().as_secs_f64() * 1000.0,
        ..console.end()
    };
    (result, cost)
}

/// The folder, the settings file for this EXE, and the folder trusted:
/// whether that was written now.
fn prepare() -> Result<(PathBuf, PathBuf, bool), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let home = claude_home::home()?;
    let settings = claude_home::write_settings(&exe)?;
    let folder = home.to_str().ok_or("the folder's path is not text")?;
    let config = claude_home::config().ok_or("no USERPROFILE")?;
    match claude_home::trust(&config, &claude_home::keys(folder)) {
        Trust::Already => Ok((home, settings, false)),
        Trust::Wrote(_) => Ok((home, settings, true)),
        Trust::Failed(why) => Err(format!("could not trust its folder: {why}")),
    }
}

/// Waits for the prompt, then `/usage`, Enter, the screen left open, Esc,
/// and the status line's run, again when that run had no limits. No key is
/// sent before the prompt shows, and none after a key went unanswered.
fn usage(console: &Console, listener: &mut Listener, short: &str) -> Result<Found, String> {
    listener
        .wait(short, READY_WITHIN, console)
        .map_err(|why| format!("the prompt did not show ({why}); no key was sent"))?;
    until_limits(|| {
        std::thread::sleep(SETTLE);
        let _ = listener.lines();
        console.type_keys(b"/usage")?;
        std::thread::sleep(TYPED_TO_ENTER);
        console.type_keys(b"\r")?;
        std::thread::sleep(OPEN_FOR);
        console.type_keys(b"\x1b")?;
        listener
            .wait(short, HEARD_WITHIN, console)
            .map_err(|why| format!("nothing came after Esc ({why})"))
    })
}

/// Opens the usage screen with `cycle` until the status line's run after
/// Esc has limits, CYCLES times at most. A cycle that fails, as when nothing
/// came after Esc, ends it at once.
fn until_limits(mut cycle: impl FnMut() -> Result<Heard, String>) -> Result<Found, String> {
    let mut why = String::new();
    for _ in 0..CYCLES {
        match found(&cycle()?) {
            Ok(found) => return Ok(found),
            Err(no_limits) => why = no_limits,
        }
    }
    Err(format!("{why}, {CYCLES} times"))
}

/// The limits in what the status line was handed, each window's share
/// used rounded to a whole percent, as Codex's are.
fn found(heard: &Heard) -> Result<Found, String> {
    if !heard.had_limits || heard.windows.is_empty() {
        return Err("no rate limits came".into());
    }
    let mut limits = Vec::new();
    let mut told = Vec::new();
    for (label, used, resets) in &heard.windows {
        let used = used.round().clamp(0.0, 255.0) as u8;
        let told_resets = resets.map_or("unknown".to_string(), |at| at.to_string());
        told.push(format!("{label}={used}% resets_at={told_resets}"));
        limits.push(Limit {
            label: label.clone(),
            reading: Reading::Read(used),
            resets: *resets,
            amount: None,
        });
    }
    Ok(Found {
        limits,
        told: told.join(" "),
    })
}

/// The lines the status line wrote to HEARD_FILE, read as they come. The
/// file is emptied first; only the hidden Claude Code writes to it.
pub struct Listener(crate::log::Tail);
impl Listener {
    pub fn new(path: PathBuf) -> Self {
        Self(crate::log::Tail::new(path))
    }
    pub fn lines(&mut self) -> Vec<Heard> {
        self.0
            .lines()
            .iter()
            .filter_map(|line| claude::heard(line))
            .collect()
    }
    /// The next line from `session`, or why none came in time.
    pub fn wait(
        &mut self,
        session: &str,
        within: Duration,
        console: &Console,
    ) -> Result<Heard, String> {
        let until = Instant::now() + within;
        loop {
            if let Some(line) = self.lines().into_iter().find(|h| h.session == session) {
                return Ok(line);
            }
            if !console.running() {
                return Err("Claude Code ended".into());
            }
            if Instant::now() >= until {
                return Err(format!("nothing heard in {} s", within.as_secs()));
            }
            std::thread::sleep(POLL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(line: &str) -> Heard {
        claude::heard(line).unwrap()
    }

    #[test]
    fn the_windows_heard_become_limits() {
        // Rounded to whole percents, as Codex's; no limits is a failed
        // read, not a read of nothing.
        let found = found(&heard("t\tabc12345\tyes\t5h\t45.5\t1790412600\t7d\t88\t-")).unwrap();
        let limits: Vec<_> = found
            .limits
            .iter()
            .map(|l| (l.label.as_str(), l.reading, l.resets))
            .collect();
        // When each starts over, for the popup.
        assert_eq!(
            limits,
            [
                ("5h", Reading::Read(46), Some(1_790_412_600)),
                ("7d", Reading::Read(88), None)
            ]
        );
        assert_eq!(
            found.told,
            "5h=46% resets_at=1790412600 7d=88% resets_at=unknown"
        );
        assert!(found_fails("t\tabc12345\tno"));
        assert!(found_fails("t\tabc12345\tyes"));
    }

    fn found_fails(line: &str) -> bool {
        super::found(&heard(line)).is_err()
    }

    #[test]
    fn the_usage_screen_is_opened_again_when_its_figures_came_late() {
        // One run of the status line follows each Esc; one without limits
        // means the screen closed too soon, so it is opened once more. A
        // cycle that heard nothing ends the read at once.
        let run = |lines: Vec<Result<&str, &str>>| {
            let mut lines = lines.into_iter();
            let mut cycles = 0;
            let result = until_limits(|| {
                cycles += 1;
                lines.next().unwrap().map(heard).map_err(String::from)
            });
            (result.map(|found| found.told), cycles)
        };
        let late = "t\tabc12345\tno";
        let good = "t\tabc12345\tyes\t5h\t45\t-\t7d\t88\t-";
        let told = "5h=45% resets_at=unknown 7d=88% resets_at=unknown";
        assert_eq!(run(vec![Ok(good)]), (Ok(told.into()), 1));
        assert_eq!(run(vec![Ok(late), Ok(good)]), (Ok(told.into()), 2));
        assert_eq!(
            run(vec![Ok(late), Ok(late)]),
            (Err("no rate limits came, 2 times".into()), 2)
        );
        assert_eq!(
            run(vec![Err("nothing came after Esc"), Ok(good)]),
            (Err("nothing came after Esc".into()), 1)
        );
    }

    #[test]
    fn lines_are_heard_once_and_whole() {
        let path =
            std::env::temp_dir().join(format!("limitpeeker-heard-{}.log", std::process::id()));
        std::fs::write(&path, "left from before\n").unwrap();
        let mut listener = Listener::new(path.clone());
        assert!(listener.lines().is_empty());
        std::fs::write(&path, "t\ta\tno\nt\tb\tyes\t5h\t1\t-\nt\tc\t").unwrap();
        let sessions: Vec<_> = listener.lines().into_iter().map(|h| h.session).collect();
        assert_eq!(sessions, ["a", "b"]);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        std::io::Write::write_all(&mut file, b"no\n").unwrap();
        drop(file);
        let sessions: Vec<_> = listener.lines().into_iter().map(|h| h.session).collect();
        assert_eq!(sessions, ["c"]);
        assert!(listener.lines().is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
