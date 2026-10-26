//! The settings window: starting it, the state sent to it, and the box
//! editor's commands.

use super::*;
use crate::{choice::Choice, settings, settings_link, xaml::Controller};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;

/// The editor's commands, answered to the window they came from: a box to
/// open, a new one's start in a style, from a box picked or the style's
/// sample, a draft to preview, one to save or delete. False for any other command. A save or delete reads the boxes
/// again. A save of the box the taskbar shows keeps it shown under the name
/// it was saved with; any other save leaves the taskbar as it is.
pub(super) fn editor(
    command: &str,
    to: HWND,
    from: HWND,
    choice: &Choice,
    controller: &Controller,
    dir: Option<&std::path::Path>,
) -> bool {
    let reply = |kind, text: &str| unsafe { send(to, from, kind, text) };
    let reload = || choice.set_boxes(crate::boxes::load(dir).0);
    if let Some(name) = command.strip_prefix("edit\t") {
        if let Some(text) = crate::boxes::opened(&choice.boxes(), name) {
            reply(settings_link::BOX, &text);
        }
    } else if let Some(rest) = command.strip_prefix("new\t") {
        // The style, and the box to start from when one was picked.
        let (style, from) = rest.split_once('\t').unwrap_or((rest, ""));
        if let Some(text) = crate::boxes::started(&choice.boxes(), style, from) {
            reply(settings_link::BOX, &text);
        }
    } else if let Some(text) = command.strip_prefix("draft\t") {
        // On the chosen box's color, as the taskbar draws it.
        let look = crate::style::DEFAULT.colored(choice.color(), crate::popup::light_taskbar());
        let preview = crate::boxes::preview(
            text,
            &choice.read(),
            &look,
            choice.fill(),
            choice.warn(),
            &mut crate::boxes::text_width,
        );
        reply(settings_link::PREVIEW, &preview);
    } else if let Some((was, text)) = command
        .strip_prefix("save\t")
        .and_then(|rest| rest.split_once('\t'))
    {
        let saved = match dir {
            Some(dir) => crate::boxes::save(dir, &choice.boxes(), was, text),
            None => Err("there is no folder for the settings".into()),
        };
        match saved {
            Ok(name) => {
                reload();
                // Every box that showed it shows it under its new name.
                if !was.is_empty() {
                    choice.rename_custom(was, Some(name.clone()));
                }
                controller.note(format!("box saved: {name}"));
                reply(settings_link::RESULT, &format!("saved\t{name}"));
            }
            Err(why) => {
                reply(settings_link::RESULT, &format!("failed\t{why}"));
            }
        }
    } else if let Some(name) = command.strip_prefix("delete\t") {
        match crate::boxes::delete(&choice.boxes(), name) {
            Ok(()) => {
                reload();
                choice.rename_custom(name, None);
                controller.note(format!("box deleted: {name}"));
                reply(settings_link::RESULT, &format!("deleted\t{name}"));
            }
            Err(why) => {
                reply(settings_link::RESULT, &format!("failed\t{why}"));
            }
        }
    } else {
        return false;
    }
    true
}
/// The settings window, if it was built beside this program.
pub(super) fn settings_exe() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join("LimitPeeker.Settings.exe"))
        .filter(|exe| exe.is_file())
}

/// What the .NET host ends with, its own dialog turned off, when the .NET
/// Desktop Runtime the settings window needs is missing, or .NET is not
/// there at all.
const NO_FRAMEWORK: u32 = 0x8000_8096;
const NO_DOTNET: u32 = 0x8000_8083;
/// Where the runtime is downloaded, as the README says.
const RUNTIME_PAGE: &str = "https://dotnet.microsoft.com/download/dotnet/10.0";
/// What this program says in place of .NET's dialog.
const RUNTIME_MISSING: &str = "The settings window needs the .NET Desktop Runtime 10 (x64), which is not installed. The boxes and the popup work without it.\n\nInstall it from Microsoft, or run:\nwinget install Microsoft.DotNet.DesktopRuntime.10\n\nOpen the download page now?";
/// Whether that question shows, so a second press does not stack another.
static ASKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The settings window's command: this window's handle, the editor alone
/// when asked, the language picked for it, and .NET's own dialog turned off,
/// so that a missing runtime ends it at once with a code this program tells
/// of in its own words.
fn command(
    exe: &std::path::Path,
    from: HWND,
    editor: bool,
    language: &str,
) -> std::process::Command {
    let mut command = std::process::Command::new(exe);
    command.arg(format!("--controller={}", from.0 as usize));
    if editor {
        command.arg("--editor");
    }
    if !language.is_empty() {
        command.arg(format!("--language={language}"));
    }
    command.env("DOTNET_DISABLE_GUI_ERRORS", "1");
    command
}

/// Asks, on a thread of its own so the tray goes on, whether to open the
/// runtime's download page, and opens it in the browser on Yes.
fn ask_for_runtime() {
    if ASKING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        let answer = crate::message::show(
            "LimitPeeker",
            RUNTIME_MISSING,
            MB_YESNO | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST,
        );
        if answer == IDYES {
            // The shell may hand the page to an extension, which wants COM.
            let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
            let page = crate::message::wide(RUNTIME_PAGE);
            unsafe {
                ShellExecuteW(
                    None,
                    w!("open"),
                    windows::core::PCWSTR(page.as_ptr()),
                    None,
                    None,
                    SW_SHOWNORMAL,
                );
            }
            if com {
                unsafe { CoUninitialize() };
            }
        }
        ASKING.store(false, Ordering::SeqCst);
    });
}
/// Sends a window one message of text, giving up after a second rather than
/// hang the tray on a window that does not answer.
pub(super) unsafe fn send(to: HWND, from: HWND, kind: usize, text: &str) -> bool {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    let data = COPYDATASTRUCT {
        dwData: kind,
        cbData: (units.len() * 2) as u32,
        lpData: if units.is_empty() {
            std::ptr::null_mut()
        } else {
            units.as_mut_ptr().cast()
        },
    };
    let mut result = 0;
    unsafe {
        SendMessageTimeoutW(
            to,
            WM_COPYDATA,
            WPARAM(from.0 as usize),
            LPARAM(&data as *const COPYDATASTRUCT as isize),
            SMTO_ABORTIFHUNG,
            1000,
            Some(&mut result),
        )
    }
    .0 != 0
}
/// The settings window once it has said hello, the one started and not yet
/// heard from, and the state last sent to it and the settings last saved, so
/// each goes again only when it changes.
#[derive(Default)]
pub(super) struct SettingsWindow {
    pub(super) handle: Option<HWND>,
    pub(super) started: Option<std::process::Child>,
    pub(super) sent: String,
    pub(super) saved: String,
    /// Whether the last save failed.
    pub(super) unsaved: bool,
    /// The box the editor is to open once the window is there.
    pub(super) editing: Option<String>,
    /// The Task Scheduler tasks that start it at sign-in, looked for when
    /// the window opens or comes to the front, not each second.
    pub(super) tasks: Vec<crate::logon_task::LogonTask>,
}
impl SettingsWindow {
    pub(super) fn hello(&mut self, handle: HWND) {
        if unsafe { IsWindow(Some(handle)) }.as_bool() {
            self.handle = Some(handle);
            self.sent.clear();
            // Heard from, so no longer one starting.
            self.started = None;
            self.look_for_tasks();
        }
    }
    pub(super) fn look_for_tasks(&mut self) {
        // Task Scheduler answers through COM, with no time limit, on this
        // thread.
        let _held = super::watch::hold(super::watch::Step::LogonTask, 0);
        if let Ok(exe) = std::env::current_exe() {
            self.tasks = crate::logon_task::find(&exe);
        }
    }
    /// Saves the choice when it changed, and sends the window the state
    /// when that changed, with when Refresh now is taken again.
    /// A save that fails is logged once and tried again each second.
    pub(super) fn sync(&mut self, from: HWND, choice: &Choice, controller: &Controller) {
        self.check_started(controller);
        let saved = settings::text(choice);
        if saved != self.saved {
            match settings::save(&saved) {
                Ok(()) => {
                    if self.unsaved {
                        controller.note("settings saved");
                    }
                    self.saved = saved;
                    self.unsaved = false;
                }
                Err(error) => {
                    if !self.unsaved {
                        controller.note(format!("settings could not be saved: {error}"));
                    }
                    self.unsaved = true;
                }
            }
        }
        let read_now_at = controller.read_now_at();
        let Some(handle) = self.handle else {
            return;
        };
        if !unsafe { IsWindow(Some(handle)) }.as_bool() {
            self.handle = None;
            return;
        }
        // Read each time, so one turned off in Windows' Startup apps shows.
        let state = settings_link::state(choice, read_now_at, crate::startup::state(), &self.tasks);
        if state != self.sent && unsafe { send(handle, from, settings_link::STATE, &state) } {
            self.sent = state;
        }
        self.send_editing(from, choice);
    }
    /// A window started that ended before it said hello: why, in the log,
    /// and when the .NET runtime is missing, the question about it.
    fn check_started(&mut self, controller: &Controller) {
        let Some(Ok(Some(status))) = self.started.as_mut().map(|child| child.try_wait()) else {
            return;
        };
        self.started = None;
        let code = status.code().unwrap_or_default() as u32;
        if matches!(code, NO_FRAMEWORK | NO_DOTNET) {
            controller.note(format!(
                "settings window: the .NET Desktop Runtime 10 is not installed ({code:#010X})"
            ));
            ask_for_runtime();
        } else {
            controller.note(format!(
                "settings window: ended before it answered ({code:#010X})"
            ));
        }
    }
    /// Opens the box editor on the box shown now or the first sample, which
    /// is sent once the window is there. A settings window not running is
    /// started for the editor alone; one running brings its editor to the
    /// front.
    pub(super) fn customize(&mut self, from: HWND, choice: &Choice) {
        self.editing = choice
            .custom()
            .map(|c| c.name.clone())
            .or_else(|| choice.boxes().first().map(|b| b.name.clone()));
        match self
            .handle
            .filter(|h| unsafe { IsWindow(Some(*h)) }.as_bool())
        {
            Some(handle) => {
                let mut pid = 0;
                unsafe {
                    GetWindowThreadProcessId(handle, Some(&mut pid));
                    let _ = AllowSetForegroundWindow(pid);
                }
                self.send_editing(from, choice);
            }
            None => self.start(from, true, &choice.language()),
        }
    }
    pub(super) fn send_editing(&mut self, from: HWND, choice: &Choice) {
        let Some(handle) = self
            .handle
            .filter(|h| unsafe { IsWindow(Some(*h)) }.as_bool())
        else {
            return;
        };
        if let Some(name) = self.editing.take()
            && let Some(text) = crate::boxes::opened(&choice.boxes(), &name)
        {
            unsafe { send(handle, from, settings_link::BOX, &text) };
        }
    }
    /// Brings the window to the front, as the shell does for a tray menu, or
    /// starts it with this window's handle. One starting and not heard from
    /// yet is not started twice.
    pub(super) fn open(&mut self, from: HWND, choice: &Choice) {
        if let Some(handle) = self.handle
            && unsafe { IsWindow(Some(handle)) }.as_bool()
        {
            let mut pid = 0;
            unsafe {
                GetWindowThreadProcessId(handle, Some(&mut pid));
                let _ = AllowSetForegroundWindow(pid);
                send(handle, from, settings_link::SHOW, "");
            }
            return;
        }
        if self
            .started
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
        {
            return;
        }
        self.start(from, false, &choice.language());
    }
    /// Starts the settings window with this window's handle, or its editor
    /// alone, in the language picked for it: the window draws its words
    /// before the state comes. One starting and not heard from yet is not
    /// started twice.
    pub(super) fn start(&mut self, from: HWND, editor: bool, language: &str) {
        if self
            .started
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
        {
            return;
        }
        if let Some(exe) = settings_exe() {
            self.started = command(&exe, from, editor, language).spawn().ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_window_starts_with_dotnets_dialog_off() {
        // A missing runtime ends it at once with a code, and this program
        // says so in its own words, where the README sends people.
        let exe = std::path::Path::new(r"C:\Tools\LimitPeeker\LimitPeeker.Settings.exe");
        let made = command(exe, HWND(0x1234 as *mut _), true, "ko");
        let args: Vec<_> = made
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--controller=4660", "--editor", "--language=ko"]);
        let envs: Vec<_> = made
            .get_envs()
            .map(|(k, v)| (k.to_os_string(), v.map(|v| v.to_os_string())))
            .collect();
        assert_eq!(
            envs,
            [("DOTNET_DISABLE_GUI_ERRORS".into(), Some("1".into()))]
        );
        assert_eq!(
            format!("{NO_FRAMEWORK:#010X} {NO_DOTNET:#010X}"),
            "0x80008096 0x80008083"
        );
        assert!(RUNTIME_MISSING.contains("\nwinget install Microsoft.DotNet.DesktopRuntime.10\n"));
        assert!(include_str!("../../README.md").contains(&format!("]({RUNTIME_PAGE})")));
    }
}
