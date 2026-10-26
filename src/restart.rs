//! Restarting the taskbar's Explorer from the popup, while the attach waits
//! for its Snap layouts thread to go: with Windows' Restart Manager, as
//! installers do, and explorer.exe started by hand when the taskbar does not
//! come back. It takes seconds, so it runs on a thread of its own.

use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::System::RestartManager::*;
use windows::Win32::System::Threading::*;
use windows::core::PWSTR;

/// How long the taskbar has to come back before explorer.exe is started.
const BACK_WITHIN: Duration = Duration::from_secs(20);

/// Restarts the Explorer that owns the taskbar, and not one that only shows
/// folders. What happened, for the log, and in a few words for the popup.
pub fn restart() -> (String, &'static str) {
    let began = Instant::now();
    // The Explorer by its id and start, since a new one can be given the
    // old one's id.
    let Some(old) = crate::shell::taskbar() else {
        let (started, ok) = start();
        return (
            format!("no taskbar; {started}"),
            if ok {
                "No taskbar was found; Explorer was started."
            } else {
                "Explorer could not be started."
            },
        );
    };
    let how = match shut_and_restart(old.pid) {
        Ok(()) => "restart manager".to_string(),
        Err(step) => format!("restart manager, which failed at {step}"),
    };
    // Back when another process owns the taskbar.
    loop {
        match crate::shell::taskbar() {
            Some(new) if !new.is(&old) => {
                return (
                    format!(
                        "explorer pid={} -> {} by {how} in {} ms",
                        old.pid,
                        new.pid,
                        began.elapsed().as_millis()
                    ),
                    "Explorer restarted.",
                );
            }
            now if began.elapsed() > BACK_WITHIN => {
                return if now.is_some_and(|now| now.is(&old)) {
                    // Still running: starting another would open a folder.
                    (
                        format!("explorer pid={} still runs after {how}", old.pid),
                        "Explorer did not restart.",
                    )
                } else {
                    let (started, ok) = start();
                    (
                        format!(
                            "explorer pid={} did not come back in {} s after {how}; {started}",
                            old.pid,
                            BACK_WITHIN.as_secs(),
                        ),
                        if ok {
                            "Explorer did not come back; it was started again."
                        } else {
                            "Explorer could not be started."
                        },
                    )
                };
            }
            _ => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

/// Has Restart Manager close the process, forcing it if it does not answer,
/// and start it again; the step that failed, if one did.
fn shut_and_restart(pid: u32) -> Result<(), String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|e| format!("open ({:#x})", e.code().0))?;
        let [mut created, mut exited, mut kernel, mut user] = [FILETIME::default(); 4];
        let times = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user);
        let _ = CloseHandle(process);
        times.map_err(|e| format!("times ({:#x})", e.code().0))?;
        let mut session = 0;
        let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
        check(
            RmStartSession(&mut session, None, PWSTR(key.as_mut_ptr())),
            "start session",
        )?;
        let app = [RM_UNIQUE_PROCESS {
            dwProcessId: pid,
            ProcessStartTime: created,
        }];
        let done = check(
            RmRegisterResources(session, None, Some(&app), None),
            "register",
        )
        .and_then(|()| {
            check(
                RmShutdown(session, RmForceShutdown.0 as u32, None),
                "shutdown",
            )
        })
        .and_then(|()| check(RmRestart(session, None, None), "restart"));
        let _ = RmEndSession(session);
        done
    }
}

fn check(error: WIN32_ERROR, step: &str) -> Result<(), String> {
    if error == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("{step} ({})", error.0))
    }
}

/// Starts explorer.exe, which becomes the shell when there is none; what
/// happened, and whether it started.
fn start() -> (String, bool) {
    let windows = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:\\Windows".into());
    match std::process::Command::new(windows.join("explorer.exe")).spawn() {
        Ok(_) => ("started explorer.exe".into(), true),
        Err(error) => (format!("could not start explorer.exe: {error}"), false),
    }
}
