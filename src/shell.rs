//! The Explorer that owns the taskbar, known by its process id and when it
//! started: Windows can give a new Explorer the id of one that has ended.

use windows::Win32::Foundation::{CloseHandle, FILETIME};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};
use windows::core::{PCWSTR, w};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shell {
    pub pid: u32,
    /// When the process started, in 100 ns since 1601; 0 when it could not
    /// be read.
    pub started: u64,
}

impl Shell {
    /// The process `pid` as it is now.
    pub fn of(pid: u32) -> Self {
        Self {
            pid,
            started: started(pid).unwrap_or(0),
        }
    }
    /// Whether this is the same Explorer as `other`: the same id, and the
    /// same start where both starts were read. One that could not be read
    /// does not make it another, as before this was known.
    pub fn is(&self, other: &Shell) -> bool {
        self.pid == other.pid
            && (self.started == 0 || other.started == 0 || self.started == other.started)
    }
}

/// The Explorer that owns the taskbar, if there is a taskbar.
pub fn taskbar() -> Option<Shell> {
    unsafe {
        let window = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        (pid != 0).then(|| Shell::of(pid))
    }
}

fn started(pid: u32) -> Option<u64> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let [mut created, mut exited, mut kernel, mut user] = [FILETIME::default(); 4];
        let times = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user);
        let _ = CloseHandle(process);
        times.ok()?;
        Some(u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explorer_is_its_id_and_when_it_started() {
        // The same id started again is another; a start not read is not.
        let this = Shell::of(std::process::id());
        assert_ne!(this.started, 0);
        assert!(this.is(&Shell::of(std::process::id())));
        assert!(!this.is(&Shell {
            started: this.started + 1,
            ..this
        }));
        assert!(this.is(&Shell { started: 0, ..this }));
        assert!(!this.is(&Shell {
            pid: this.pid + 1,
            ..this
        }));
        assert_eq!(Shell::of(u32::MAX).started, 0);
    }
}
