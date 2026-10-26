//! The run's log: one line at a time, the runs before kept beside it, and
//! the local time its first line starts with. Every part of the program
//! writes to it.

use std::io::Write;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};

pub(crate) type Log = Arc<Mutex<Option<std::fs::File>>>;

/// One line, after the milliseconds since the PC started. This clock counts
/// sleep, which suits a log read beside the event log; nothing compares
/// against it.
pub(crate) fn log(file: &Log, text: impl AsRef<str>) {
    if let Ok(mut file) = file.lock() {
        if let Some(file) = file.as_mut() {
            let _ = writeln!(file, "{} {}", unsafe { GetTickCount64() }, text.as_ref());
            let _ = file.flush();
        }
    }
}

/// How many runs' logs are kept: this run's, and the nine before it, the
/// newest first.
const KEPT_LOGS: usize = 10;
/// Where the log of the run `age` runs back is: this run's at 0, the one
/// before at `name.1.log`, and so on.
fn aged(path: &std::path::Path, age: usize) -> std::path::PathBuf {
    if age == 0 {
        path.to_path_buf()
    } else {
        path.with_extension(format!("{age}.log"))
    }
}
/// Moves each kept log one run back, and drops the oldest, before this run
/// starts its own.
pub(crate) fn rotate(path: &std::path::Path) {
    let _ = std::fs::remove_file(aged(path, KEPT_LOGS - 1));
    for age in (0..KEPT_LOGS - 1).rev() {
        let _ = std::fs::rename(aged(path, age), aged(path, age + 1));
    }
}
/// The local wall-clock time, for a log's first line, which the event log
/// and crash dumps can be matched to.
pub(crate) fn local_time() -> String {
    written(&unsafe { GetLocalTime() })
}

/// The same to the millisecond, so that lines a second apart are told
/// apart in order.
pub(crate) fn local_time_ms() -> String {
    let t = unsafe { GetLocalTime() };
    format!("{}.{:03}", written(&t), t.wMilliseconds)
}

fn written(t: &SYSTEMTIME) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// The whole lines added to a file since last asked, read as they come:
/// what a hidden CLI's status line notes in its heard file. The file is
/// emptied first; one moved aside when full is read again from its start.
pub(crate) struct Tail {
    path: std::path::PathBuf,
    at: u64,
    partial: String,
}
impl Tail {
    pub(crate) fn new(path: std::path::PathBuf) -> Self {
        let _ = std::fs::remove_file(&path);
        Self {
            path,
            at: 0,
            partial: String::new(),
        }
    }
    pub(crate) fn lines(&mut self) -> Vec<String> {
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut file) = std::fs::File::open(&self.path) else {
            return Vec::new();
        };
        let length = file.metadata().map_or(0, |m| m.len());
        if length < self.at {
            self.at = 0;
            self.partial.clear();
        }
        let mut read = Vec::new();
        if file.seek(SeekFrom::Start(self.at)).is_err() || file.read_to_end(&mut read).is_err() {
            return Vec::new();
        }
        self.at += read.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&read));
        let mut lines = Vec::new();
        while let Some(end) = self.partial.find('\n') {
            lines.push(self.partial.drain(..=end).collect());
        }
        lines
    }
}

/// Text for the log, cut short and on one line.
pub(crate) fn short(text: &str) -> String {
    text.chars()
        .take(200)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_runs_of_logs_are_kept() {
        // Each run moves the ones before a place back and drops the
        // eleventh; the run before is `name.1.log`.
        let dir = std::env::temp_dir().join(format!("limitpeeker-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("limitpeeker.log");
        for run in 1..=12 {
            rotate(&path);
            std::fs::write(&path, format!("run {run}")).unwrap();
        }
        let kept: Vec<String> = (0..KEPT_LOGS + 1)
            .map(|age| std::fs::read_to_string(aged(&path, age)).unwrap_or_default())
            .collect();
        assert_eq!(kept[0], "run 12");
        assert_eq!(kept[1], "run 11");
        assert_eq!(kept[KEPT_LOGS - 1], "run 3");
        assert_eq!(kept[KEPT_LOGS], "");
        assert_eq!(aged(&path, 1).file_name().unwrap(), "limitpeeker.1.log");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), KEPT_LOGS);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(local_time().len(), "2026-09-25 02:47:45".len());
        assert_eq!(short("a\nb"), "a b");
        assert_eq!(short(&"x".repeat(300)).len(), 200);
    }
}
