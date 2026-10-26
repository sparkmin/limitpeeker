//! Each CLI's own version update, run by this program apart from its reads:
//! the CLI's update command, in a console without a window, so what it
//! starts takes that console and shows none; every 15 minutes while its
//! service is read, after a read; put off while the person uses that CLI.
//! It runs in no job and writes to a file, so it goes on to its end when
//! this program ends or stops waiting for it: an update cut short could
//! leave the CLI half replaced. The reads keep their own update checks off.

use crate::job::{self, Job};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::PWSTR;

/// How often each CLI's update runs, as the Antigravity CLI checks its own.
pub const EVERY: Duration = Duration::from_secs(15 * 60);
/// How much short of EVERY a last run may be and the update still be due.
/// The last run is kept when the update ends, and the wait to the next read
/// takes in the time the update took, so the read on the 15-minute beat
/// comes a second or two short of EVERY; without this, every other one
/// would pass the update by.
const EARLY: Duration = Duration::from_secs(60);
/// How long an update is waited for: Claude Code and the Antigravity CLI
/// each download hundreds of MB. One still running then is left to end.
const WITHIN: Duration = Duration::from_secs(5 * 60);
/// How long `--version` may take.
const VERSION_WITHIN: Duration = Duration::from_secs(30);
/// How long what the CLI wrote is waited for once it has ended.
const DRAIN_WITHIN: Duration = Duration::from_secs(1);
/// The longest line taken from the CLI.
const LONGEST_LINE: u64 = 1 << 16;
/// The most of a line the log keeps.
const LONGEST_SAID: usize = 160;
/// The most of what an update wrote that is read back for its last line.
const TAIL: u64 = 1 << 16;

/// A CLI's command with `args`, made as its reads run it: in its folder,
/// with its home and environment. None when that cannot be made.
pub type Prepared = fn(&Path, &[&str]) -> Option<Command>;

/// Where the time of `tag`'s last update run is kept: an empty file's time,
/// so a restart does not run it again at once.
fn mark_file(tag: &str) -> Option<PathBuf> {
    crate::settings::data_dir().map(|dir| dir.join(format!("{tag}-updated.timestamp")))
}

/// Where `tag`'s update writes what it says.
fn said_file(tag: &str) -> Option<PathBuf> {
    crate::settings::data_dir().map(|dir| dir.join(format!("{tag}-update.log")))
}

/// The name of the file that keeps what `tag`'s last failed update said,
/// beside `said_file`, until the next failure: each update writes that file
/// anew, so a good one would wipe out why the one before failed.
fn failed_name(tag: &str) -> String {
    format!("{tag}-update-failed.log")
}

/// Why a copy an editor keeps is not updated here, as the log says it.
pub const EDITOR: &str = "is updated with the editor it came with";

/// Why the CLI at `exe` is not updated here, or None when its own update
/// command updates it: for a CLI whose command updates any copy but one an
/// editor keeps.
pub fn kept_by_editor(exe: &Path) -> Option<&'static str> {
    (!updated_here(exe)).then_some(EDITOR)
}

/// Whether the CLI at `exe` is one its own update command updates: not a
/// copy an editor keeps with an extension of its own, as VS Code keeps
/// Claude Code, which the editor updates with the extension and whose
/// command could install another CLI beside it.
pub fn updated_here(exe: &Path) -> bool {
    let parts: Vec<_> = exe
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    !parts
        .windows(2)
        .any(|pair| pair[0].starts_with('.') && pair[1].eq_ignore_ascii_case("extensions"))
}

/// Whether `tag`'s update is due: its last run is EVERY old, or there is
/// none.
pub fn due(tag: &str) -> bool {
    mark_file(tag).is_some_and(|file| due_at(&file, SystemTime::now()))
}

fn due_at(file: &Path, now: SystemTime) -> bool {
    std::fs::metadata(file)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| now.duration_since(at).ok())
        .is_none_or(|age| age + EARLY >= EVERY)
}

/// Keeps now as the time of `tag`'s last update run.
fn mark(tag: &str) {
    if let Some(file) = mark_file(tag) {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .and_then(|opened| opened.set_modified(SystemTime::now()));
    }
}

/// Whether a program runs from `exe` now: the person uses that CLI, which
/// may be updating itself meanwhile. This program's own runs of it are
/// over by the time an update runs, as one thread reads each service in
/// turn. A program started through a link, as through the folder Codex's
/// installer puts on the PATH, is named by the link, so both are taken as
/// the files they lead to; only programs of the same file name are looked
/// into.
pub fn in_use(exe: &Path) -> bool {
    let wanted = plain(exe);
    let name = exe.file_name().map(|name| name.to_ascii_lowercase());
    running().iter().any(|image| {
        let image = Path::new(image);
        image.file_name().map(|name| name.to_ascii_lowercase()) == name && plain(image) == wanted
    })
}

/// A path as a running program's image names it: full, in lower case,
/// without the `\\?\` that canonicalizing puts first.
fn plain(path: &Path) -> String {
    let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = full.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_lowercase()
}

/// The full paths of the programs running now that this one may ask.
fn running() -> Vec<String> {
    let mut ids = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return Vec::new();
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                ids.push(entry.th32ProcessID);
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    ids.into_iter().filter_map(image).collect()
}

/// The full path of the program running as `pid`, when it may be asked.
fn image(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut name = [0u16; 1024];
        let mut length = name.len() as u32;
        let read = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(name.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        read.ok()?;
        Some(String::from_utf16_lossy(&name[..length as usize]))
    }
}

/// Runs the CLI's update once, keeps now as its last run, and says what
/// came of it for the log: its version before and after, or why it failed,
/// with what a failed one said kept aside.
pub fn run(tag: &str, exe: &Path, prepared: Prepared) -> String {
    let began = Instant::now();
    let before = version(exe, prepared);
    let updated = update(tag, exe, prepared);
    let after = version(exe, prepared);
    mark(tag);
    let kept = updated.is_err() && said_file(tag).is_some_and(|said| keep_aside(&said, tag));
    told(
        tag,
        &before,
        &after,
        updated,
        kept,
        began.elapsed().as_secs_f64(),
    )
}

/// Copies what a failed update said beside it, under `failed_name`; whether
/// that was done.
fn keep_aside(said: &Path, tag: &str) -> bool {
    std::fs::copy(said, said.with_file_name(failed_name(tag))).is_ok()
}

/// What came of an update, for the log. A CLI may say it updated when it was
/// already the newest, as Codex's installer does, so a version that did not
/// change is told as such before what it said.
fn told(
    tag: &str,
    before: &str,
    after: &str,
    updated: Result<String, String>,
    kept: bool,
    took: f64,
) -> String {
    match updated {
        Ok(_) if before != after => format!("updated {before} -> {after} in {took:.1} s"),
        Ok(said) if said.is_empty() => {
            format!("update checked in {took:.1} s, version unchanged ({after})")
        }
        Ok(said) => {
            format!("update checked in {took:.1} s, version unchanged ({after}); it said: {said}")
        }
        Err(why) if kept => format!(
            "update failed in {took:.1} s, {after}: {why}; its output is kept in {}",
            failed_name(tag)
        ),
        Err(why) => format!("update failed in {took:.1} s, {after}: {why}"),
    }
}

/// Runs the CLI's update until it ends or WITHIN passes, and gives the last
/// line it wrote; an error when it could not start, ran on or did not end
/// well. One that runs on is left to end on its own.
fn update(tag: &str, exe: &Path, prepared: Prepared) -> Result<String, String> {
    let file = said_file(tag).ok_or("no LOCALAPPDATA")?;
    let written = std::fs::File::create(&file)
        .map_err(|e| format!("could not make {}: {e}", file.display()))?;
    let also = written
        .try_clone()
        .map_err(|e| format!("could not share {}: {e}", file.display()))?;
    let mut command = prepared(exe, &["update"]).ok_or("its folder could not be made")?;
    command
        .stdin(Stdio::null())
        .stdout(written)
        .stderr(also)
        .creation_flags(job::CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start: {e}"))?;
    let status = wait(&mut child, WITHIN);
    let last = last_line(&tail(&file));
    ended(status, last, WITHIN, "; left to end on its own")
}

/// The end of a file, up to TAIL bytes of it, as text.
fn tail(file: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let mut bytes = Vec::new();
    if let Ok(mut opened) = std::fs::File::open(file) {
        let length = opened.metadata().map_or(0, |meta| meta.len());
        let _ = opened.seek(SeekFrom::Start(length.saturating_sub(TAIL)));
        let _ = opened.take(TAIL).read_to_end(&mut bytes);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Waits for `child` to end, until `within` passes; its status, or None
/// when it runs on.
fn wait(child: &mut std::process::Child, within: Duration) -> Option<std::process::ExitStatus> {
    let until = Instant::now() + within;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(100)),
            _ => return None,
        }
    }
}

/// What came of a run: the last line it wrote when it ended well, else why
/// not, with `running_on` told of a run that had not ended.
fn ended(
    status: Option<std::process::ExitStatus>,
    last: String,
    within: Duration,
    running_on: &str,
) -> Result<String, String> {
    match status {
        None => Err(format!(
            "still running after {} s{running_on}",
            within.as_secs()
        )),
        Some(status) if status.success() => Ok(last),
        Some(status) => Err(format!(
            "ended with {}{}",
            status
                .code()
                .map_or("no code".into(), |code| code.to_string()),
            if last.is_empty() {
                String::new()
            } else {
                format!(": {last}")
            }
        )),
    }
}

/// The CLI's version as `--version` says it, or "version not known".
fn version(exe: &Path, prepared: Prepared) -> String {
    asked(exe, prepared, &["--version"], VERSION_WITHIN)
        .ok()
        .filter(|said| !said.is_empty())
        .unwrap_or_else(|| "version not known".into())
}

/// Runs the CLI with `args` in a job until it ends or `within` passes, and
/// gives its last line; an error when it could not start, ran too long or
/// did not end well. What it started ends with it.
fn asked(
    exe: &Path,
    prepared: Prepared,
    args: &[&str],
    within: Duration,
) -> Result<String, String> {
    let mut command = prepared(exe, args).ok_or("its folder could not be made")?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(job::CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start: {e}"))?;
    let held = Job::around(HANDLE(child.as_raw_handle()));
    let outputs: Vec<_> = [
        child.stdout.take().map(|out| job::lines(out, LONGEST_LINE)),
        child.stderr.take().map(|out| job::lines(out, LONGEST_LINE)),
    ]
    .into_iter()
    .flatten()
    .collect();
    let status = wait(&mut child, within);
    // What it started goes with it, and its output closes.
    held.end();
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let mut said = String::new();
    for output in outputs {
        let until = Instant::now() + DRAIN_WITHIN;
        while let Ok(Ok(line)) =
            output.recv_timeout(until.saturating_duration_since(Instant::now()))
        {
            said.push_str(&String::from_utf8_lossy(&line));
        }
    }
    ended(status, last_line(&said), within, "")
}

/// The last line with anything in it, without terminal colours or control
/// characters, and cut short for the log. A line a progress bar redraws
/// with carriage returns counts by its last drawing.
fn last_line(text: &str) -> String {
    let line = text
        .split(['\r', '\n'])
        .map(plain_text)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default();
    line.chars().take(LONGEST_SAID).collect()
}

/// Text without ANSI escape sequences or control characters, trimmed.
fn plain_text(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // A CSI sequence ends at its first letter; anything else is dropped with its next character.
            if chars.next_if_eq(&'[').is_some() {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                chars.next();
            }
        } else if !c.is_control() {
            out.push(c);
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn an_update_is_due_every_15_minutes() {
        let file = std::env::temp_dir().join(format!(
            "limitpeeker-update-{}.timestamp",
            std::process::id()
        ));
        let now = SystemTime::now();
        assert!(due_at(&file, now), "never run");
        std::fs::write(&file, b"").unwrap();
        assert!(!due_at(&file, now + Duration::from_secs(13 * 60)));
        // The read on the 15-minute beat, a little short of it.
        assert!(due_at(&file, now + EVERY - Duration::from_secs(2)));
        assert!(due_at(&file, now + Duration::from_secs(16 * 60)));
        // A time ahead of now, as after the clock was set back, is due.
        assert!(due_at(&file, now - Duration::from_secs(3600)));
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn a_cli_in_use_is_one_running_from_the_same_file() {
        // This test's own program runs, from a path given in any case.
        let me = std::env::current_exe().unwrap();
        assert!(in_use(&me));
        assert!(in_use(Path::new(&me.to_string_lossy().to_uppercase())));
        assert!(!in_use(&me.with_file_name("not-running-here.exe")));
        assert_eq!(
            plain(Path::new(r"C:\No\Such\Grok.EXE")),
            r"c:\no\such\grok.exe"
        );
    }

    /// Makes `link` a junction to the folder `target`, as Codex's installer
    /// makes its folder on the PATH.
    pub(crate) fn junction(link: &Path, target: &Path) {
        let made = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
    }

    #[test]
    fn a_cli_started_through_a_link_is_the_file_it_leads_to() {
        // The link is taken as the file it leads to, which runs.
        let me = std::env::current_exe().unwrap();
        let link = std::env::temp_dir().join(format!("limitpeeker-link-{}", std::process::id()));
        junction(&link, me.parent().unwrap());
        assert!(in_use(&link.join(me.file_name().unwrap())));
        assert!(!in_use(&link.join("not-running-here.exe")));
        // Removing a junction takes away the link, not what it leads to.
        std::fs::remove_dir(&link).unwrap();
        assert!(me.is_file());
    }

    #[test]
    fn what_the_cli_said_last_goes_to_the_log() {
        assert_eq!(
            last_line(
                "Current version: 2.1.283\nSuccessfully updated from 2.1.283 to version 2.1.304\n\n"
            ),
            "Successfully updated from 2.1.283 to version 2.1.304"
        );
        assert_eq!(
            last_line("\u{1b}[32m\u{2713} You are already on the latest version.\u{1b}[0m\r\n"),
            "\u{2713} You are already on the latest version."
        );
        assert_eq!(
            last_line("Downloading 10%\rDownloading 100%\r"),
            "Downloading 100%"
        );
        assert_eq!(last_line(""), "");
        assert_eq!(last_line(&"x".repeat(400)).len(), LONGEST_SAID);
    }

    #[test]
    fn the_log_tells_an_unchanged_version_and_where_a_failure_is_kept() {
        // Codex's installer says it updated when it was already the newest;
        // a failed update's output is kept aside.
        let codex = "codex-cli 0.163.0";
        assert_eq!(
            told(
                "codex",
                codex,
                codex,
                Ok("Update ran successfully! Please restart Codex.".into()),
                false,
                1.74
            ),
            "update checked in 1.7 s, version unchanged (codex-cli 0.163.0); it said: Update ran successfully! Please restart Codex."
        );
        assert_eq!(
            told("codex", codex, codex, Ok(String::new()), false, 2.0),
            "update checked in 2.0 s, version unchanged (codex-cli 0.163.0)"
        );
        assert_eq!(
            told(
                "claude",
                "2.1.307",
                "2.1.308",
                Ok("Successfully updated".into()),
                false,
                6.0
            ),
            "updated 2.1.307 -> 2.1.308 in 6.0 s"
        );
        let failed = Err("ended with 1: Error: failed with status exit code: 1".to_string());
        assert_eq!(
            told("codex", codex, codex, failed.clone(), true, 0.8),
            "update failed in 0.8 s, codex-cli 0.163.0: ended with 1: Error: failed with status exit code: 1; its output is kept in codex-update-failed.log"
        );
        assert_eq!(
            told("codex", codex, codex, failed, false, 0.8),
            "update failed in 0.8 s, codex-cli 0.163.0: ended with 1: Error: failed with status exit code: 1"
        );
        // Kept beside what each update writes anew, until the next failure.
        let dir = std::env::temp_dir().join(format!("limitpeeker-failed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let said = dir.join("codex-update.log");
        std::fs::write(&said, "irm : The remote server returned an error\n").unwrap();
        assert!(keep_aside(&said, "codex"));
        assert_eq!(
            std::fs::read_to_string(dir.join("codex-update-failed.log")).unwrap(),
            "irm : The remote server returned an error\n"
        );
        assert!(!keep_aside(&dir.join("missing.log"), "grok"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_copy_an_editor_keeps_is_not_updated_here() {
        // VS Code updates the Claude Code in its extension; its update
        // command could install another beside it.
        let here = |path: &str| updated_here(Path::new(path));
        assert!(!here(
            r"C:\Users\me\.vscode\extensions\anthropic.claude-code-2.1.258-win32-x64\resources\native-binary\claude.exe"
        ));
        assert!(!here(
            r"C:\Users\me\.vscode-insiders\Extensions\x\claude.exe"
        ));
        assert!(here(r"C:\Users\me\.local\bin\claude.exe"));
        assert!(here(r"C:\Users\me\AppData\Local\agy\bin\agy.exe"));
        assert!(here(r"C:\Users\me\.grok\bin\grok.exe"));
        assert!(here(r"C:\Tools\extensions\grok.exe"));
        assert_eq!(
            kept_by_editor(Path::new(r"C:\Users\me\.vscode\extensions\x\claude.exe")),
            Some(EDITOR)
        );
        assert_eq!(
            kept_by_editor(Path::new(r"C:\Users\me\.local\bin\claude.exe")),
            None
        );
    }
}
