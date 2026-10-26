//! `--remove`: takes away what LimitPeeker keeps on this PC, so deleting
//! its folder leaves nothing behind. It runs without a window, so it asks
//! and tells in message boxes. The folder with the EXE is left for the
//! person to delete.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, LPARAM, WPARAM};
use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

/// How long the running product has to end once asked.
const CLOSING: Duration = Duration::from_secs(30);
/// How often the data folder is tried while a file in it is still let go.
const TRIES: u32 = 5;
/// How long the hook of the product just closed has to come to rest in
/// Explorer before it is sent away.
const RESTING_WITHIN: Duration = Duration::from_secs(3);

pub fn run() {
    let data = data_dir();
    let folder = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let shown_data = data
        .as_ref()
        .map_or("(LOCALAPPDATA is not set)".to_string(), |d| {
            d.display().to_string()
        });
    if !ask(&format!(
        "Remove what LimitPeeker keeps on this PC?\n\n\
         \u{2022} Its start at sign-in, if it is on\n\
         \u{2022} Its folder in AppData, with the settings, custom boxes, the services' logos, and the own folders of the hidden Claude Code, Antigravity CLI and Grok CLI:\n    {shown_data}\n\
         \u{2022} The hidden Claude Code's entries in Claude Code's file (.claude.json)\n\n\
         LimitPeeker is closed first if it is running. The folder with limitpeeker.exe is left for you to delete afterwards."
    )) {
        return;
    }
    if let Err(why) = close_running() {
        tell(&why);
        return;
    }
    // The hook rests in the taskbar's Explorer, still subscribed, until
    // Explorer ends; nothing of it should stay once this is gone. Its DLL
    // stays loaded either way, which the last message says.
    if let Some(shell) = crate::shell::taskbar() {
        crate::xaml::release_resting(shell.pid, RESTING_WITHIN);
    }
    let mut removed = Vec::new();
    let mut left = Vec::new();
    match crate::startup::turn_off() {
        Ok(true) => removed.push("The start at sign-in".to_string()),
        Ok(false) => {}
        Err(why) => left.push(format!("The start at sign-in: {why}")),
    }
    if let Some(dir) = &data {
        let home = dir.join("claude-home");
        match (crate::claude_home::config(), home.to_str()) {
            (Some(config), Some(folder)) => {
                match crate::claude_home::untrust(&config, &crate::claude_home::keys(folder)) {
                    crate::claude_home::Trust::Wrote(_) => removed.push(format!(
                        "The hidden Claude Code's entries in {}",
                        config.display()
                    )),
                    crate::claude_home::Trust::Already => {}
                    crate::claude_home::Trust::Failed(why) => {
                        left.push(format!("The entries in {}: {why}", config.display()))
                    }
                }
            }
            _ => left.push("The entries in Claude Code's file: its place is not known".into()),
        }
        if dir.exists() {
            match remove_dir(dir) {
                Ok(()) => removed.push(dir.display().to_string()),
                Err(why) => left.push(format!("{}: {why}", dir.display())),
            }
        }
    }
    let mut text = String::new();
    if removed.is_empty() && left.is_empty() {
        text.push_str("There was nothing to remove.\n\n");
    }
    for (head, items) in [("Removed:", &removed), ("Could not remove:", &left)] {
        if !items.is_empty() {
            text.push_str(head);
            for item in items {
                text.push_str(&format!("\n\u{2022} {item}"));
            }
            text.push_str("\n\n");
        }
    }
    // A sign-in task a person made is theirs to delete.
    let tasks: Vec<_> = std::env::current_exe()
        .map(|exe| crate::logon_task::find(&exe))
        .unwrap_or_default()
        .into_iter()
        .map(|task| task.path)
        .collect();
    if !tasks.is_empty() {
        text.push_str(&format!(
            "Task Scheduler still has the task you made to start LimitPeeker at sign-in: {}. \
             Delete it in Task Scheduler.\n\n",
            tasks.join(", ")
        ));
    }
    let folder = folder.map_or("LimitPeeker's folder".to_string(), |f| {
        f.display().to_string()
    });
    text.push_str(&format!(
        "Now delete {folder}. If Windows says limitpeeker_xaml.dll is in use, File Explorer still has it from when \
         the box was on the taskbar: restart Windows Explorer from Task Manager, or sign out, then delete the folder."
    ));
    tell(&text);
}

/// The data folder, only where it is this program's: LimitPeeker, under an
/// absolute LOCALAPPDATA.
fn data_dir() -> Option<PathBuf> {
    crate::settings::data_dir().filter(|dir| {
        dir.is_absolute() && dir.file_name().is_some_and(|name| name == "LimitPeeker")
    })
}

/// Asks with Yes and No, No first.
fn ask(text: &str) -> bool {
    crate::message::show(
        "LimitPeeker",
        text,
        MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
    ) == IDYES
}

/// Tells what came of it.
fn tell(text: &str) {
    crate::message::show("LimitPeeker", text, MB_OK | MB_ICONINFORMATION);
}

/// Closes the running product, as its Exit does, and waits for it to end;
/// its hidden Claude Code and agy end with its Job.
fn close_running() -> Result<(), String> {
    unsafe {
        if let Ok(window) = FindWindowW(w!("LimitPeekerController"), w!("LimitPeeker")) {
            let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        let start = Instant::now();
        while let Ok(mutex) = OpenMutexW(
            SYNCHRONIZATION_SYNCHRONIZE,
            false,
            w!("Local\\LimitPeekerController"),
        ) {
            let _ = CloseHandle(mutex);
            if start.elapsed() > CLOSING {
                return Err(format!(
                    "LimitPeeker did not close within {} seconds, so nothing was removed. \
                     Exit it from its tray icon, then run --remove again.",
                    CLOSING.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    Ok(())
}

/// Deletes the data folder, trying again while a file the product's
/// helpers held is still being let go.
fn remove_dir(dir: &Path) -> Result<(), String> {
    for attempt in 1..=TRIES {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) if attempt == TRIES => return Err(e.to_string()),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    Ok(())
}
