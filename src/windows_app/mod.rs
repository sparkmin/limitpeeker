//! Tray/message-loop only. UI Automation never runs on this thread.
//!
//! The tray icon is in `tray`, the settings window in `settings_window`.
use crate::{layout::DisplayMode, popup, settings, settings_link, style, xaml::Controller};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

mod settings_window;
mod tray;
mod watch;

use settings_window::*;
use tray::*;

const CALLBACK: u32 = WM_APP + 1;
const RESET_TRAY: u32 = WM_APP + 3;
/// Opening the popup, from the tray icon or the box.
const OPEN_MENU: u32 = WM_APP + 4;
/// Opening the settings window, and the settings window's commands waiting
/// for the message loop.
const OPEN_SETTINGS: u32 = WM_APP + 5;
const SETTINGS_INBOX: u32 = WM_APP + 6;
/// The least time between two notifications, so Windows shows each rather
/// than the last over the one before.
const NOTIFY_GAP: std::time::Duration = std::time::Duration::from_secs(6);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
thread_local! {
    /// Each command with the window that sent it, taken from WM_COPYDATA
    /// and applied on the message loop, where the choice is.
    static INBOX: RefCell<VecDeque<(usize, String)>> = const { RefCell::new(VecDeque::new()) };
    /// The log, for the last line when Windows ends the session, written
    /// once.
    static ENDING: RefCell<Option<crate::log::Log>> = const { RefCell::new(None) };
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // A message handled here, even one sent while the loop waits in
    // GetMessage, is a step of its own for the watch.
    let _held = watch::hold(watch::Step::Window, msg);
    unsafe {
        if msg == TASKBAR_CREATED.load(Ordering::Relaxed) && msg != 0 {
            let _ = PostMessageW(Some(hwnd), RESET_TRAY, WPARAM(0), LPARAM(0));
            return LRESULT(0);
        }
        match msg {
            CALLBACK => {
                // Shell callbacks may be sent directly, bypassing GetMessage.
                // Defer the popup to the outer loop to avoid reentrancy.
                // A notification selected opens it too.
                if matches!(
                    lparam.0 as u32,
                    WM_LBUTTONUP | WM_RBUTTONUP | NIN_BALLOONUSERCLICK
                ) {
                    let _ = PostMessageW(Some(hwnd), OPEN_MENU, WPARAM(0), LPARAM(0));
                }
                LRESULT(0)
            }
            WM_COPYDATA => {
                // A command from the settings window, kept for the message
                // loop like the menu, out of this call.
                if let Some(data) = (lparam.0 as *const COPYDATASTRUCT).as_ref()
                    && data.dwData == settings_link::COMMAND
                    && data.cbData % 2 == 0
                    && data.cbData as usize / 2 <= settings_link::LONGEST
                    && !data.lpData.is_null()
                    && let Ok(text) = String::from_utf16(std::slice::from_raw_parts(
                        data.lpData as *const u16,
                        data.cbData as usize / 2,
                    ))
                {
                    INBOX.with(|inbox| inbox.borrow_mut().push_back((wparam.0, text)));
                    let _ = PostMessageW(Some(hwnd), SETTINGS_INBOX, WPARAM(0), LPARAM(0));
                    return LRESULT(1);
                }
                LRESULT(0)
            }
            // Windows may end the process any time after this returns, before
            // the loop stops and the controller is put away, so the log's last
            // line is written here.
            WM_ENDSESSION => {
                if wparam.0 != 0 {
                    ENDING.with(|ending| {
                        if let Some(file) = ending.borrow_mut().take() {
                            crate::log::log(&file, crate::xaml::SESSION_ENDED);
                        }
                    });
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
/// Whether this run attaches to Explorer: always, unless `--tray-only` is
/// given.
fn attaches(args: &[String]) -> bool {
    !args.iter().any(|a| a == "--tray-only")
}
pub fn run() -> windows::core::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    // Claude Code's status line runs this often and briefly, while the
    // product may be running: it only takes what it is handed and ends.
    if args == ["--claude-statusline"] {
        crate::claude::capture();
        return Ok(());
    }
    // The same for the hidden Antigravity CLI's status line.
    if args == ["--agy-statusline"] {
        crate::agy::capture();
        return Ok(());
    }
    // Takes away what the product keeps, before its folder is deleted. It
    // closes a running product itself, so it does not wait for its place.
    if args.iter().any(|a| a == "--remove") {
        crate::remove::run();
        return Ok(());
    }
    if let Some(unknown) = args.iter().find(|a| {
        ![
            "--tray-only",
            "--diagnostics",
            "--mock",
            // One Gemini read, alone, to check it.
            "--agy-probe",
            "--cycle-mock",
            // Four sample boxes, one to a place, to check them.
            "--sample-boxes",
            // Claude read the hidden console's way only, to check it.
            "--claude-fallback",
        ]
        .contains(&a.as_str())
            && !a
                .strip_prefix("--look=")
                .is_some_and(|name| style::find(name).is_some())
            && !a
                .strip_prefix("--fill=")
                .is_some_and(|name| style::Fill::find(name).is_some())
            && !a
                .strip_prefix("--background=")
                .is_some_and(|name| style::Background::find(name).is_some())
            // The first box's place.
            && !a
                .strip_prefix("--place=")
                .is_some_and(|name| crate::choice::BoxPlace::find(name).is_some())
    }) {
        const SUPPORTED: &str = "Supported: --tray-only --remove --diagnostics --mock --agy-probe --cycle-mock --sample-boxes --claude-fallback --place=tray|left|before-start|after-apps --look=slim|inline|stack|pie|rings --fill=used|left --background=box|cards";
        // Started by a double click, it has no console to say it in.
        crate::message::show(
            "LimitPeeker",
            &format!("LimitPeeker does not know {unknown}.\n\n{SUPPORTED}"),
            MB_OK | MB_ICONINFORMATION,
        );
        return Err(windows::core::Error::new(E_INVALIDARG, SUPPORTED));
    }
    let enabled = attaches(&args);
    unsafe {
        let singleton = CreateMutexW(None, false, w!("Local\\LimitPeekerController"))?;
        struct MutexHandle(HANDLE);
        impl Drop for MutexHandle {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseHandle(self.0);
                }
            }
        }
        let _singleton = MutexHandle(singleton);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return Ok(());
        }
        // One Gemini read alone: the product is not running, so its reads
        // do not mix in.
        if args.iter().any(|a| a == "--agy-probe") {
            crate::agy_session::probe();
            return Ok(());
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = w!("LimitPeekerController");
        if RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        }) == 0
        {
            return Err(windows::core::Error::from_thread());
        }
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("LimitPeeker"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )?;
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(w!("TaskbarCreated")),
            Ordering::Relaxed,
        );
        let flag = |name: &str| args.iter().any(|a| a == name);
        if flag("--claude-fallback") {
            crate::claude_usage::FALLBACK_ONLY.store(true, Ordering::Relaxed);
        }
        // The figures are what the providers read; --mock shows the mock
        // ones instead, and --cycle-mock walks them to check on screen that
        // a value changes in place and only a new format moves the
        // reservation.
        // --look= holds one look, --fill= colors the share used or what is
        // left, in every look, and --background= draws one box or a card per
        // service. Each comes before what the last run saved.
        let look = args
            .iter()
            .find_map(|a| a.strip_prefix("--look="))
            .and_then(style::find);
        let fill = args
            .iter()
            .find_map(|a| a.strip_prefix("--fill="))
            .and_then(style::Fill::find);
        let background = args
            .iter()
            .find_map(|a| a.strip_prefix("--background="))
            .and_then(style::Background::find);
        let choice = Arc::new(settings::starting(settings::load(), look, fill, background));
        // The custom boxes: the samples, and the files beside the settings.
        let boxes_dir = settings::data_dir().map(|dir| dir.join("boxes"));
        let (boxes, mut box_problems) = crate::boxes::load(boxes_dir.as_deref());
        choice.set_boxes(boxes);
        // The logos 0.1.0 read beside the program go beside the settings,
        // before anything draws them; what was done is noted once the log
        // is open.
        let old_icons = match (std::env::current_exe(), crate::icons::dir()) {
            (Ok(exe), Some(new)) => exe
                .parent()
                .map(|dir| crate::icons::move_old(&dir.join("icons"), &new))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        // The first box's place, from the command line before what the last
        // run saved, as --look does.
        if let Some(place) = args
            .iter()
            .find_map(|a| a.strip_prefix("--place="))
            .and_then(crate::choice::BoxPlace::find)
        {
            choice.set_box_place(place);
        }
        choice.set_sample_boxes(flag("--sample-boxes"));
        let mut window = SettingsWindow {
            saved: settings::text(&choice),
            ..Default::default()
        };
        let controller = Controller::start(
            enabled,
            flag("--diagnostics"),
            flag("--mock"),
            flag("--cycle-mock"),
            choice.clone(),
            hwnd,
        );
        for problem in &box_problems {
            controller.note(format!("box file left out: {problem}"));
        }
        for line in &old_icons {
            controller.note(format!("old icons folder: {line}"));
        }
        ENDING.with(|ending| *ending.borrow_mut() = Some(controller.log_file()));
        // The job this run was started in: a packaged app's update ends it
        // with no word.
        controller.note(crate::job::around());
        // A start at sign-in left on a folder moved away comes to this EXE;
        // the mock figures, held or walked, are a check, not the product.
        if !controller.mock()
            && let Ok(exe) = std::env::current_exe()
            && let Some(note) = crate::startup::follow(&exe)
        {
            controller.note(note);
        }
        // How it starts at sign-in, by Windows' Startup apps or a Task
        // Scheduler task a person made.
        if let Ok(exe) = std::env::current_exe() {
            controller.note(crate::startup::told(
                crate::startup::state(),
                &crate::logon_task::find(&exe),
            ));
        }
        let label = || {
            let text = controller
                .label
                .lock()
                .map(|l| l.clone())
                .unwrap_or_default();
            // Why no box shows: the attach waits for Explorer's Snap layouts
            // thread to go.
            if controller.snap_wait.load(Ordering::SeqCst) {
                format!("Restart Explorer to show the box. {text}")
            } else {
                text
            }
        };
        let mut current = DisplayMode::TrayOnly;
        let mut shown = label();
        let mock = controller.mock();
        // The tray icon is drawn with WIC on this thread.
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let mut drawn = TrayIcon::default();
        destroy(drawn.update(tray_look(&choice, &controller, current), &controller));
        let first = add_icon(&tray(hwnd, current, &shown, mock, drawn.handle()));
        if first == Added::There {
            controller.note(first.note());
        }
        let mut added = first != Added::Missing;
        let mut icon_missing = false;
        // What the limits read call for, the notifications waiting their
        // turn, when the last went, and its picture, which Windows may still
        // be showing.
        let mut notifier = crate::notify::Notifier::default();
        let mut waiting: VecDeque<crate::notify::Alert> = VecDeque::new();
        let mut last_told: Option<std::time::Instant> = None;
        let mut picture: Option<HICON> = None;
        // Whether the popup's box settings are unfolded: folded each time
        // it opens, unfolded by Edit or a tab.
        let mut unfolded = false;
        SetTimer(Some(hwnd), 1, 1000, None);
        // A stop of this thread is logged with the step it stopped at; the
        // 1 s timer marks it even while nothing else happens.
        watch::start(controller.log_file());
        let mut message = MSG::default();
        loop {
            watch::mark(watch::Step::Waiting);
            let result = GetMessageW(&mut message, None, 0, 0).0;
            if result <= 0 {
                break;
            }
            if message.hwnd == hwnd {
                match message.message {
                    WM_TIMER | RESET_TRAY => {
                        if message.message == RESET_TRAY {
                            added = false;
                        }
                        watch::mark(watch::Step::Refresh);
                        let next = mode(controller.presentation.load(Ordering::SeqCst));
                        // The popup closes itself when it loses focus.
                        choice.set_popup_open(popup::is_open());
                        controller.refresh(&choice);
                        // The mock figures are a check, not the limits.
                        watch::mark(watch::Step::Notify);
                        if !mock {
                            for alert in notifier.update(
                                &choice.read(),
                                &choice.read_services(),
                                choice.alerts(),
                                choice.fill(),
                                crate::usage::unix_now(),
                                crate::notify::local_offset(),
                            ) {
                                controller
                                    .note(format!("notify: {} · {}", alert.title, alert.body));
                                waiting.push_back(alert);
                            }
                        }
                        if added
                            && last_told.is_none_or(|at| at.elapsed() >= NOTIFY_GAP)
                            && let Some(alert) = waiting.pop_front()
                        {
                            let look =
                                crate::tray_icon::alert_look(alert.face, popup::light_taskbar());
                            let large = crate::tray_icon::icon(look).ok();
                            if !Shell_NotifyIconW(NIM_MODIFY, &balloon(hwnd, &alert, large))
                                .as_bool()
                            {
                                controller.note(format!("notify: not taken: {}", alert.title));
                            }
                            destroy(Some(std::mem::replace(&mut picture, large)));
                            last_told = Some(std::time::Instant::now());
                        }
                        let text = label();
                        watch::mark(watch::Step::TrayDraw);
                        let replaced =
                            drawn.update(tray_look(&choice, &controller, next), &controller);
                        watch::mark(watch::Step::TrayIcon);
                        if !added {
                            let how = add_icon(&tray(hwnd, next, &text, mock, drawn.handle()));
                            added = how != Added::Missing;
                            // Once each way, not each second, and an icon
                            // found there already each time.
                            if added == icon_missing || how == Added::There {
                                controller.note(how.note());
                                icon_missing = !added;
                            }
                        } else if next != current || text != shown || replaced.is_some() {
                            let _ = Shell_NotifyIconW(
                                NIM_MODIFY,
                                &tray(hwnd, next, &text, mock, drawn.handle()),
                            );
                        }
                        destroy(replaced);
                        current = next;
                        shown = text;
                        // A limit that came or went reaches the window, and
                        // new values and the countdown an open popup.
                        watch::mark(watch::Step::SettingsSync);
                        window.sync(hwnd, &choice, &controller);
                        watch::mark(watch::Step::PopupUpdate);
                        popup::update(popup_view(
                            &choice,
                            &controller,
                            window.unsaved,
                            current,
                            mock,
                            unfolded,
                        ));
                        continue;
                    }

                    OPEN_MENU => {
                        watch::mark(watch::Step::PopupOpen);
                        // The popup opens on the box right-clicked, numbered
                        // from 1 among the boxes shown, and from the tray
                        // icon, which sends 0, on the first.
                        if message.wParam.0 > 0 {
                            controller.note(format!("menu asked from box {}", message.wParam.0));
                            choice.select_shown(message.wParam.0 - 1);
                        } else {
                            choice.select(0);
                        }
                        // Folded however it opens; Edit unfolds the box
                        // chosen, the one right-clicked.
                        unfolded = false;
                        // Box files changed since are read again, and what is
                        // wrong with them told once.
                        let (boxes, problems) = crate::boxes::load(boxes_dir.as_deref());
                        choice.set_boxes(boxes);
                        if problems != box_problems {
                            for problem in &problems {
                                controller.note(format!("box file left out: {problem}"));
                            }
                            box_problems = problems;
                        }
                        // Where the tray icon or the box was clicked; a click
                        // while it is open closes it.
                        let mut point = POINT::default();
                        if GetCursorPos(&mut point).is_ok() {
                            let view = popup_view(
                                &choice,
                                &controller,
                                window.unsaved,
                                current,
                                mock,
                                unfolded,
                            );
                            if let Err(error) = popup::toggle(hwnd, point, view) {
                                controller.note(format!("popup could not open: {error}"));
                            }
                        }
                        // Its summary's meters are measured while it is open.
                        choice.set_popup_open(popup::is_open());
                        continue;
                    }
                    popup::ACTION => {
                        watch::mark(watch::Step::PopupAction);
                        // A chip is the settings window's command, taken by
                        // the same rules; the popup stays open to show it.
                        for action in popup::take() {
                            match action {
                                popup::Action::Apply(command) => {
                                    settings_link::apply(&choice, &command);
                                }
                                popup::Action::Refresh => {
                                    controller.read_now();
                                }
                                popup::Action::Settings => window.open(hwnd, &choice),
                                popup::Action::RestartExplorer => controller.restart_explorer(),
                                popup::Action::Customize => window.customize(hwnd, &choice),
                                // A folded tab chooses its box as it unfolds.
                                popup::Action::Unfold(command) => {
                                    if let Some(command) = command {
                                        settings_link::apply(&choice, &command);
                                    }
                                    unfolded = true;
                                }
                                popup::Action::Fold => unfolded = false,
                                popup::Action::Exit => {
                                    let _ =
                                        PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                                }
                                // The popup picks the color itself and
                                // sends it as a command.
                                popup::Action::PickColor | popup::Action::Eyedrop => {}
                            }
                        }
                        watch::mark(watch::Step::SettingsSync);
                        window.sync(hwnd, &choice, &controller);
                        watch::mark(watch::Step::PopupUpdate);
                        popup::update(popup_view(
                            &choice,
                            &controller,
                            window.unsaved,
                            current,
                            mock,
                            unfolded,
                        ));
                        continue;
                    }
                    OPEN_SETTINGS => {
                        watch::mark(watch::Step::SettingsCommand);
                        window.open(hwnd, &choice);
                        continue;
                    }
                    SETTINGS_INBOX => {
                        watch::mark(watch::Step::SettingsCommand);
                        let commands: Vec<_> =
                            INBOX.with(|inbox| inbox.borrow_mut().drain(..).collect());
                        for (sender, command) in commands {
                            if command == settings_link::HELLO {
                                window.hello(HWND(sender as *mut _));
                            } else if command == settings_link::REFRESH {
                                controller.read_now();
                            } else if command == settings_link::STARTUP_CHECK {
                                window.look_for_tasks();
                            } else if command == settings_link::NOTIFY_TEST {
                                // Sent first, at the next second.
                                let sample = crate::notify::sample();
                                controller.note(format!("notify: {} (test)", sample.title));
                                waiting.push_front(sample);
                                last_told = None;
                            } else if let Some(on) = settings_link::startup(&command) {
                                // Kept in the registry, not the settings file.
                                let done = if on {
                                    std::env::current_exe()
                                        .map_err(|e| e.to_string())
                                        .and_then(|exe| crate::startup::turn_on(&exe))
                                } else {
                                    crate::startup::turn_off().map(|_| ())
                                };
                                controller.note(match done {
                                    Ok(()) => format!("startup: {}", if on { "on" } else { "off" }),
                                    Err(why) => format!("startup: {why}"),
                                });
                            } else if !editor(
                                &command,
                                HWND(sender as *mut _),
                                hwnd,
                                &choice,
                                &controller,
                                boxes_dir.as_deref(),
                            ) {
                                settings_link::apply(&choice, &command);
                            }
                        }
                        watch::mark(watch::Step::SettingsSync);
                        window.sync(hwnd, &choice, &controller);
                        watch::mark(watch::Step::PopupUpdate);
                        popup::update(popup_view(
                            &choice,
                            &controller,
                            window.unsaved,
                            current,
                            mock,
                            unfolded,
                        ));
                        continue;
                    }
                    _ => {}
                }
            }
            watch::mark_message(watch::Step::Dispatch, message.message);
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        watch::mark(watch::Step::Ending);
        popup::shut();
        let _ = KillTimer(Some(hwnd), 1);
        let _ = Shell_NotifyIconW(
            NIM_DELETE,
            &tray(hwnd, current, &shown, mock, drawn.handle()),
        );
        drop(controller);
        drop(drawn);
        destroy(Some(picture));
        if com {
            CoUninitialize();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tooltip_is_cut_after_a_service() {
        // At most 127 units, whole services, then an ellipsis.
        let short = "LimitPeeker · On the taskbar\nCodex 7d 48%";
        assert_eq!(String::from_utf16(&tip(short, 127)).unwrap(), short);
        let long = format!(
            "LimitPeeker · Tray only\n{}",
            ["Codex 5h 28% 7d 42%"; 8].join("  |  ")
        );
        let cut = String::from_utf16(&tip(&long, 127)).unwrap();
        assert!(cut.encode_utf16().count() <= 127);
        assert!(cut.ends_with("7d 42%\u{2026}"), "{cut}");
        let one = "x".repeat(200);
        assert_eq!(tip(&one, 127).len(), 127);
    }

    #[test]
    fn a_plain_start_attaches_and_tray_only_does_not() {
        // Attaching is the default; `--tray-only` turns it off.
        let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert!(attaches(&args(&[])));
        assert!(attaches(&args(&["--diagnostics", "--mock"])));
        assert!(!attaches(&args(&["--tray-only"])));
        assert!(!attaches(&args(&["--diagnostics", "--tray-only"])));
    }
}
