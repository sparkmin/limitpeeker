//! Starting with Windows: a value in the person's own Run key, which the
//! settings window turns on and off and Windows' Startup apps may turn off
//! on its side. No administrator is needed.

use crate::logon_task::{LogonTask, Problem};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::*;
use windows::core::{PCWSTR, w};

/// The person's own programs started at sign-in.
const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
/// Where Windows' Startup apps marks a program it turned off.
const APPROVED: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
/// This program's value in both.
const NAME: PCWSTR = w!("LimitPeeker");

/// Whether this program starts at sign-in: not at all, yes, or registered
/// but turned off in Windows' Startup apps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Startup {
    Off,
    On,
    Blocked,
}

impl Startup {
    /// The word the settings window is sent.
    pub fn word(self) -> &'static str {
        match self {
            Startup::Off => "off",
            Startup::On => "on",
            Startup::Blocked => "blocked",
        }
    }
}

/// The log's line on how this program starts at sign-in: by Windows'
/// Startup apps, by Task Scheduler tasks a person made, both or neither.
pub fn told(startup: Startup, tasks: &[LogonTask]) -> String {
    let apps = match startup {
        Startup::Off => "off",
        Startup::On => "on",
        Startup::Blocked => "turned off in Windows",
    };
    let tasks = if tasks.is_empty() {
        "none".to_string()
    } else {
        tasks
            .iter()
            .map(|task| {
                let mut notes = vec![if task.enabled { "on" } else { "off" }];
                if !task.this_exe {
                    notes.push("another copy");
                }
                if task.problem != Problem::None {
                    notes.push(task.problem.word());
                }
                format!("{} ({})", task.path, notes.join(", "))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("startup: Startup apps {apps}; Task Scheduler at sign-in {tasks}")
}

/// Whether this program starts at sign-in, as the registry has it now.
pub fn state() -> Startup {
    match registered() {
        None => Startup::Off,
        Some(_) if marked_off(&approved().unwrap_or_default()) => Startup::Blocked,
        Some(_) => Startup::On,
    }
}

/// Registers `exe` to start at sign-in, and takes away a mark Windows'
/// Startup apps left turning it off: turned on here is on.
pub fn turn_on(exe: &Path) -> Result<(), String> {
    write(&command(exe))?;
    delete(APPROVED)?;
    Ok(())
}

/// Takes the registration away, with Windows' mark. Whether there was one.
pub fn turn_off() -> Result<bool, String> {
    let was = registered().is_some();
    delete(RUN)?;
    delete(APPROVED)?;
    Ok(was)
}

/// When the EXE registered is gone, as when its folder was moved, registers
/// `exe` in its place. One still there is left alone, so another copy
/// started does not take it over. What was done, for the log.
pub fn follow(exe: &Path) -> Option<String> {
    let was = program(&registered()?)?;
    if was.is_file() || was == exe {
        return None;
    }
    Some(match write(&command(exe)) {
        Ok(()) => format!(
            "startup: {} is gone; signing in starts {} now",
            was.display(),
            exe.display()
        ),
        Err(why) => format!(
            "startup: {} is gone, and could not be replaced: {why}",
            was.display()
        ),
    })
}

/// The command registered for an EXE: its path in quotes, and nothing more,
/// so it starts as a double click does.
fn command(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// The EXE a registered command starts: inside its quotes, or the whole of
/// it when it has none.
fn program(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let path = match command.strip_prefix('"') {
        Some(rest) => &rest[..rest.find('"')?],
        None => command,
    };
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Whether Windows' mark says turned off: its first byte is odd.
fn marked_off(mark: &[u8]) -> bool {
    mark.first().is_some_and(|b| b & 1 == 1)
}

/// The command registered for this program, if any.
fn registered() -> Option<String> {
    let mut size = 0u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN,
            NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    let mut text = vec![0u16; size as usize / 2 + 1];
    let mut size = (text.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN,
            NAME,
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    Some(String::from_utf16_lossy(&text[..end]))
}

/// Windows' mark for this program, if it left one.
fn approved() -> Option<Vec<u8>> {
    let mut mark = [0u8; 64];
    let mut size = mark.len() as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            APPROVED,
            NAME,
            RRF_RT_REG_BINARY,
            None,
            Some(mark.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    Some(mark[..size as usize].to_vec())
}

fn write(command: &str) -> Result<(), String> {
    let text = crate::message::wide(command);
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN,
            NAME,
            REG_SZ.0,
            Some(text.as_ptr() as *const c_void),
            (text.len() * 2) as u32,
        )
    }
    .ok()
    .map_err(|e| format!("could not register it to start at sign-in: {}", e.message()))
}

/// Deletes this program's value under `key`; one not there is fine.
fn delete(key: PCWSTR) -> Result<(), String> {
    match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key, NAME) } {
        error if error.is_ok() || error == ERROR_FILE_NOT_FOUND => Ok(()),
        error => Err(format!(
            "could not take away its start at sign-in: {}",
            error.to_hresult().message()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_is_the_exe_in_quotes_and_reads_back() {
        let exe = Path::new(r"D:\My Tools\LimitPeeker\limitpeeker.exe");
        assert_eq!(command(exe), r#""D:\My Tools\LimitPeeker\limitpeeker.exe""#);
        assert_eq!(program(&command(exe)).as_deref(), Some(exe));
        // Changed by hand: arguments after the quotes, or no quotes.
        assert_eq!(
            program(r#" "C:\a b\x.exe" --tray-only"#).as_deref(),
            Some(Path::new(r"C:\a b\x.exe"))
        );
        assert_eq!(
            program(r"C:\x.exe").as_deref(),
            Some(Path::new(r"C:\x.exe"))
        );
        assert_eq!(program(r#""C:\x.exe"#), None);
        assert_eq!(program("  "), None);
    }

    #[test]
    fn windows_marks_off_with_an_odd_first_byte() {
        // What Windows' Startup apps writes: 02 or 06 on, 03 or 07 off,
        // then when.
        assert!(!marked_off(&[]));
        assert!(!marked_off(&[2, 0, 0, 0]));
        assert!(!marked_off(&[6, 0, 0, 0]));
        assert!(marked_off(&[3, 0, 0, 0, 1, 2]));
        assert!(marked_off(&[7]));
        assert_eq!(
            [Startup::Off, Startup::On, Startup::Blocked].map(Startup::word),
            ["off", "on", "blocked"]
        );
    }

    #[test]
    fn the_log_says_how_it_starts_at_sign_in() {
        // Startup apps and the tasks a person made, each with what is off
        // about it.
        assert_eq!(
            told(Startup::Off, &[]),
            "startup: Startup apps off; Task Scheduler at sign-in none"
        );
        let task = |path: &str, enabled, this_exe, problem| LogonTask {
            path: path.into(),
            program: r"D:\x\limitpeeker.exe".into(),
            enabled,
            this_exe,
            problem,
        };
        assert_eq!(
            told(
                Startup::Blocked,
                &[
                    task(r"\LimitPeeker", true, true, Problem::None),
                    task(r"\Old\LimitPeeker", false, false, Problem::Admin),
                ]
            ),
            r"startup: Startup apps turned off in Windows; Task Scheduler at sign-in \LimitPeeker (on), \Old\LimitPeeker (off, another copy, admin)"
        );
    }
}
