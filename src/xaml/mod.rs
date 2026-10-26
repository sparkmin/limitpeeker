//! The controller of the box in Explorer's taskbar: it starts the worker,
//! holds what the tray and popup read, and ends the run. The shared
//! memory is in `channel`, attaching in `attach`, the loop in `worker`, and
//! what it logs of the hook in `report`.

mod attach;
mod channel;
mod report;
mod worker;

use attach::*;
use channel::*;
use report::*;
use worker::*;

pub(crate) use attach::release_resting;

use crate::choice::Choice;
use crate::log::{Log, local_time, log, rotate};
use crate::providers::Providers;
use crate::usage::{self, Service};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;

use windows::Win32::System::Threading::*;

/// Where the figures shown come from: the providers, or the mock figures,
/// held or walked through.
pub enum Figures {
    Real(Providers),
    Mock { cycle: bool },
}
impl Figures {
    /// The services as they stand `elapsed` ms after the start.
    fn services(&self, elapsed: u64) -> Vec<Service> {
        match self {
            Figures::Real(providers) => providers.services(),
            Figures::Mock { cycle } => usage::mock(if *cycle {
                elapsed / usage::CYCLE_STEP_MS
            } else {
                0
            }),
        }
    }
}

/// Why the box is not on the taskbar, for the popup, and whether restarting
/// Explorer is what brings it back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reason {
    pub text: &'static str,
    pub restart: bool,
}
impl Reason {
    pub const fn new(text: &'static str, restart: bool) -> Self {
        Self { text, restart }
    }
}
/// The reasons, short enough for one line of the popup.
const ADMIN: &str = "Run without administrator rights to show the box.";
const NO_DLL: &str = "limitpeeker_xaml.dll is missing beside the EXE.";
const EXPLORER_ELEVATED: &str = "Explorer runs as administrator; the box stays off.";
const NOT_ATTACHED: &str = "Could not attach to Explorer.";
const EARLIER_BOX: &str = "Explorer still has a box from an earlier run.";
const STALLED: &str = "The box was taken off because it stopped responding.";
const HOOK_ERROR: &str = "The box stopped because of an error.";
const NOT_BOTTOM: &str = "The box needs a bottom taskbar on the main display.";
const INTERNAL: &str = "The box hit an internal error. Restart LimitPeeker.";
const NO_LIMITS: &str = "The services in the box reported no limits.";
const UNFIT: &str = "The chosen limits do not fit in the box.";
/// A custom box with no block left to draw, or one the hook would refuse.
const NO_BLOCKS: &str = "No block of this box has a service to show.";
const BOX_UNFIT: &str = "This box does not fit; check its file.";
/// Every box hidden: nothing goes on the taskbar.
const ALL_HIDDEN: &str = "Every box is hidden. Pick a place in the popup.";

pub struct Controller {
    stop: Arc<AtomicBool>,
    pub presentation: Arc<AtomicU32>,
    /// The tray icon's text: the limits the boxes show.
    pub label: Arc<Mutex<String>>,
    /// Whether the attach waits for Explorer's Snap layouts thread to go,
    /// for the tray icon.
    pub snap_wait: Arc<AtomicBool>,
    /// Why the box is not on the taskbar, for the popup.
    reason: Arc<Mutex<Option<Reason>>>,
    /// Where the figures come from, and when this run began, for the tray
    /// icon when no worker runs.
    figures: Arc<Figures>,
    began: u64,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Whether this run attaches at all, for the log's last line.
    attaching: bool,
    /// The log, and whether Explorer is being restarted from the popup.
    file: Log,
    restarting: Arc<AtomicBool>,
    /// What the last restart of Explorer came to, and when.
    restarted: Arc<Mutex<Option<(Instant, &'static str)>>>,
}
impl Controller {
    /// `menu_window` is the tray window, which opens the popup when the
    /// box is right-clicked too.
    /// `mock` shows the mock figures, walked through with `cycle`, instead
    /// of what the providers read.
    pub fn start(
        enabled: bool,
        diagnostics: bool,
        mock: bool,
        cycle: bool,
        choice: Arc<Choice>,
        menu_window: HWND,
    ) -> Self {
        // A window handle uses only its low 32 bits.
        let menu_window = menu_window.0 as usize as u32;
        let stop = Arc::new(AtomicBool::new(false));
        let presentation = Arc::new(AtomicU32::new(0));
        let snap_wait = Arc::new(AtomicBool::new(false));
        let label = Arc::new(Mutex::new(String::new()));
        let directory = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_owned()));
        // Every run keeps a log of what it attached to, read and failed at;
        // --diagnostics adds the hook's own lines. It is beside the EXE, or in
        // the folder LIMITPEEKER_LOG_DIR names, for an EXE whose folder cannot
        // be written.
        let file = {
            let path = std::env::var_os("LIMITPEEKER_LOG_DIR")
                .filter(|folder| std::path::Path::new(folder).is_dir())
                .map(std::path::PathBuf::from)
                .or_else(|| directory.clone())
                .map(|folder| folder.join("limitpeeker.log"));
            // The runs before are kept beside it: three runs in a row used to
            // lose the first.
            path.and_then(|p| {
                rotate(&p);
                std::fs::File::create(p).ok()
            })
        };
        let file = Arc::new(Mutex::new(file));
        let args: Vec<String> = std::env::args().skip(1).collect();
        log(
            &file,
            format!("run started {} local, args={args:?}", local_time()),
        );
        let figures = Arc::new(if mock || cycle {
            Figures::Mock { cycle }
        } else {
            Figures::Real(Providers::start(file.clone(), choice.clone()))
        });
        let began = shared_now();
        let dll = directory
            .map(|p| p.join("limitpeeker_xaml.dll"))
            .filter(|p| p.is_file());
        // What holds the box back before any worker runs.
        let reason = Arc::new(Mutex::new(None));
        let admin = enabled && elevated(unsafe { GetCurrentProcess() }).unwrap_or(false);
        if admin {
            log(&file, "attach skipped: this process runs as administrator");
            tell(&reason, &file, Some(Reason::new(ADMIN, false)));
        } else if enabled && dll.is_none() {
            tell(&reason, &file, Some(Reason::new(NO_DLL, false)));
        }
        let thread = if enabled && !admin {
            dll.map(|dll| {
                let (s, p, l, w, r, f, figures, choice) = (
                    stop.clone(),
                    presentation.clone(),
                    label.clone(),
                    snap_wait.clone(),
                    reason.clone(),
                    file.clone(),
                    figures.clone(),
                    choice.clone(),
                );
                std::thread::spawn(move || {
                    worker(
                        s,
                        p,
                        l,
                        figures,
                        choice,
                        menu_window,
                        w,
                        r,
                        f,
                        dll.to_string_lossy().into_owned(),
                        diagnostics,
                    )
                })
            })
        } else {
            None
        };
        let controller = Self {
            stop,
            presentation,
            label,
            snap_wait,
            reason,
            figures,
            began,
            thread,
            attaching: enabled,
            file,
            restarting: Arc::new(AtomicBool::new(false)),
            restarted: Arc::new(Mutex::new(None)),
        };
        controller.refresh(&choice);
        controller
    }
    /// With no worker, which otherwise does it on each tick, puts what the
    /// providers read in the tray icon's text and the popup.
    /// A worker that has ended, however it ended, counts as none.
    pub fn refresh(&self, choice: &Choice) {
        if running(&self.thread) {
            return;
        }
        let services = self.figures.services(shared_now() - self.began);
        choice.offer(&services);
        let shown = choice.all_services(&choice.offers());
        if let Ok(mut text) = self.label.lock() {
            *text = told(&services, &shown, choice.fill());
        }
    }
    /// Has every service read at once, unless the last such request was
    /// under ten seconds ago or the figures are the mock ones; whether it
    /// was taken.
    pub fn read_now(&self) -> bool {
        match &*self.figures {
            Figures::Real(providers) => providers.read_now(),
            Figures::Mock { .. } => false,
        }
    }
    /// When reading at once is taken again; never with the mock figures.
    pub fn read_now_ready_at(&self) -> Option<Instant> {
        match &*self.figures {
            Figures::Real(providers) => Some(providers.ready_at()),
            Figures::Mock { .. } => None,
        }
    }
    /// Writes a line to the run's log, for the tray thread.
    pub fn note(&self, text: impl AsRef<str>) {
        log(&self.file, text);
    }
    /// The log itself, for the line written when Windows ends the session.
    pub fn log_file(&self) -> crate::log::Log {
        self.file.clone()
    }
    /// What the last restart of Explorer came to, for a minute after it,
    /// for the popup.
    pub fn notice(&self) -> Option<&'static str> {
        let held = self.restarted.lock().ok()?;
        held.filter(|(at, _)| at.elapsed() < Duration::from_secs(60))
            .map(|(_, brief)| brief)
    }
    /// When, in Unix milliseconds, reading at once is taken again, 0 before
    /// the first request; never with the mock figures.
    pub fn read_now_at(&self) -> Option<u64> {
        match &*self.figures {
            Figures::Real(providers) => Some(providers.read_now_at()),
            Figures::Mock { .. } => None,
        }
    }
    /// Restarts the taskbar's Explorer on a thread of its own, unless a
    /// restart is under way. The worker attaches to the new Explorer as to
    /// any.
    pub fn restart_explorer(&self) {
        if self.restarting.swap(true, Ordering::SeqCst) {
            return;
        }
        log(&self.file, "restart explorer: asked from the popup");
        let (file, restarting, restarted) = (
            self.file.clone(),
            self.restarting.clone(),
            self.restarted.clone(),
        );
        std::thread::spawn(move || {
            let (told, brief) =
                std::panic::catch_unwind(crate::restart::restart).unwrap_or_else(|_| {
                    (
                        "the restart panicked".into(),
                        "Explorer could not be restarted.",
                    )
                });
            log(&file, format!("restart explorer: {told}"));
            if let Ok(mut held) = restarted.lock() {
                *held = Some((Instant::now(), brief));
            }
            restarting.store(false, Ordering::SeqCst);
        });
    }
    /// Why the box is not on the taskbar, if it is known.
    pub fn reason(&self) -> Option<Reason> {
        self.reason.lock().ok().and_then(|r| *r)
    }
    /// Whether Explorer is being restarted.
    pub fn restarting(&self) -> bool {
        self.restarting.load(Ordering::SeqCst)
    }
    /// Whether the figures are the mock ones, which the tray says.
    pub fn mock(&self) -> bool {
        matches!(*self.figures, Figures::Mock { .. })
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let mut ended = true;
        if let Some(thread) = self.thread.take() {
            ended = false;
            for _ in 0..30 {
                if thread.is_finished() {
                    let _ = thread.join();
                    ended = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // Do not hang exit on UIA. The native lease expires without a publisher.
        }
        let why = self
            .reason
            .lock()
            .ok()
            .and_then(|r| r.as_ref().map(|r| r.text));
        log(
            &self.file,
            run_ended(
                ended,
                self.attaching,
                self.snap_wait.load(Ordering::SeqCst),
                why,
            ),
        );
    }
}

/// The last line when Windows ends the session, at sign-out, restart or
/// shutdown: the process may be ended before the controller is put away.
pub const SESSION_ENDED: &str =
    "run ended: Windows ended the session (sign-out, restart or shutdown)";

/// The last line of every run, after the hook's `stop:` when it was
/// attached, and with why it was not when it was not: a log that stops
/// short of it is a run ended from outside.
fn run_ended(thread_ended: bool, attaching: bool, snap_wait: bool, why: Option<&str>) -> String {
    let end = if !thread_ended {
        ": the hook's thread did not end within 3 s".to_string()
    } else if !attaching {
        ": tray only, as asked".to_string()
    } else if snap_wait {
        ": not attached, waiting for Explorer's Snap layouts thread to go".to_string()
    } else if let Some(why) = why {
        format!(": the box was off: {why}")
    } else {
        String::new()
    };
    format!("run ended{end}")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::log::Log;

    use crate::shell::Shell;

    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use windows::Win32::System::Memory::*;

    use windows::core::PCWSTR;

    #[test]
    fn every_run_ends_its_log_with_one_line() {
        // Attached, or not and why, so a log without it was ended from
        // outside.
        assert_eq!(run_ended(true, true, false, None), "run ended");
        assert_eq!(
            run_ended(true, false, false, None),
            "run ended: tray only, as asked"
        );
        assert_eq!(
            run_ended(true, true, true, None),
            "run ended: not attached, waiting for Explorer's Snap layouts thread to go"
        );
        assert_eq!(
            run_ended(true, true, false, Some(ADMIN)),
            format!("run ended: the box was off: {ADMIN}")
        );
        // The session's end is a run ended too.
        assert!(SESSION_ENDED.starts_with("run ended: "));
        // A hook's thread held up is said before anything else.
        assert_eq!(
            run_ended(false, true, true, Some(NO_DLL)),
            "run ended: the hook's thread did not end within 3 s"
        );
    }

    #[test]
    fn a_resting_hook_is_claimed_and_only_it_skips_the_attach() {
        // The hook's side of a session in an Explorer that does not exist:
        // the mapping it keeps open, and the words it leaves.
        let pid = 4_000_000_000 + std::process::id() % 1_000_000;
        let shell = Shell { pid, started: 7 };
        let name = wide(&format!("Local\\LimitPeekerXaml_v1_{pid}"));
        unsafe {
            let hook = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                (WORDS * 4) as u32,
                PCWSTR(name.as_ptr()),
            )
            .unwrap();
            let view = MapViewOfFile(hook, FILE_MAP_ALL_ACCESS, 0, 0, WORDS * 4);
            assert!(!view.Value.is_null());
            let word = |index: usize| &*view.Value.cast::<AtomicU32>().add(index);
            // Status, version: claimed, reused, or refused as busy.
            for (status, version, resumed, busy) in [
                (RESTING, VERSION, true, false),
                (4, VERSION, false, false),
                (2, VERSION, false, true),
                (RESTING, VERSION - 1, false, true),
            ] {
                word(0).store(MAGIC, Ordering::SeqCst);
                word(1).store(version, Ordering::SeqCst);
                word(CONTROLLER_PID).store(0, Ordering::SeqCst);
                word(HOOK_STATUS).store(status, Ordering::SeqCst);
                word(TEXT_SEQ).store(9, Ordering::SeqCst);
                word(ADVISE_TRIES).store(1, Ordering::SeqCst);
                word(ATTACH_ROOT_WIDTH).store(1920, Ordering::SeqCst);
                assert_eq!(resting(pid), resumed, "{status} {version}");
                match Channel::new(shell) {
                    Ok(channel) => {
                        assert!(!busy, "{status} {version}");
                        assert!(channel.reused);
                        assert_eq!(channel.resumed, resumed);
                        // A claimed hook keeps its status to start again by,
                        // and the record of the subscription it still holds;
                        // the rest is new, as for any session.
                        let expected = if resumed { CLAIMED } else { 0 };
                        assert_eq!(word(HOOK_STATUS).load(Ordering::SeqCst), expected);
                        let (tries, width) = if resumed { (1, 1920) } else { (0, 0) };
                        assert_eq!(word(ADVISE_TRIES).load(Ordering::SeqCst), tries);
                        assert_eq!(word(ATTACH_ROOT_WIDTH).load(Ordering::SeqCst), width);
                        assert_eq!(word(TEXT_SEQ).load(Ordering::SeqCst), 0);
                        assert_eq!(word(1).load(Ordering::SeqCst), VERSION);
                        assert_eq!(
                            word(CONTROLLER_PID).load(Ordering::SeqCst),
                            std::process::id()
                        );
                    }
                    Err(error) => {
                        assert!(busy, "{status} {version}: {error}");
                        assert_eq!(error.code(), SESSION_BUSY);
                    }
                }
            }
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(hook);
        }
        // No mapping at all: nothing rests.
        assert!(!resting(pid + 1));
        // Both sides spell the rest the same, and the hook rests with no
        // time limit.
        let native = include_str!("../../native/taskbar_tap.cpp");
        assert!(native.contains(&format!(
            "constexpr LONG Resting={RESTING}, Claimed={CLAIMED};"
        )));
        assert!(native.contains("constexpr DWORD ClaimMs=10000;"));
        assert!(!native.contains("RestMs"));
        assert!(native.contains("} while(rest(state));"));
    }

    #[test]
    fn a_run_that_stalled_takes_its_resting_hook_back() {
        // A session of this run in an Explorer that does not exist. Its hook
        // took the box off when the lease ran out, and rests: the run claims
        // it, 7 to 8, as a run started again would, and a hook that then
        // leaves is let go like a failed resume. Any other status is left.
        let pid = 4_200_000_000 + std::process::id() % 1_000_000;
        let mut channel = Channel::new(Shell { pid, started: 7 }).unwrap();
        channel.live = true;
        for status in [0, 1, 2, 3, 4, 5, 6, CLAIMED] {
            channel.set(HOOK_STATUS, status);
            assert!(!channel.reclaim(), "{status}");
            assert_eq!(channel.get(HOOK_STATUS), status);
            assert!(!channel.resumed && channel.live);
        }
        channel.set(HOOK_STATUS, RESTING);
        assert!(channel.reclaim());
        assert_eq!(channel.get(HOOK_STATUS), CLAIMED);
        assert!(channel.resumed && !channel.live);
        // The session's words stay as the run wrote them: the hook's new box
        // reads them from the start.
        assert_eq!(channel.get(1), VERSION);
        assert_eq!(channel.get(CONTROLLER_PID), std::process::id());
    }

    #[test]
    fn remove_sends_only_a_resting_hook_of_this_version_away() {
        // The hook's side, as in the test above. Resting, it is sent away
        // (7 to 4); one still taking its box off is waited for; anything
        // else is left as it is.
        let pid = 4_100_000_000 + std::process::id() % 1_000_000;
        let name = wide(&format!("Local\\LimitPeekerXaml_v1_{pid}"));
        assert!(!release_resting(pid, Duration::ZERO));
        unsafe {
            let hook = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                (WORDS * 4) as u32,
                PCWSTR(name.as_ptr()),
            )
            .unwrap();
            let view = MapViewOfFile(hook, FILE_MAP_ALL_ACCESS, 0, 0, WORDS * 4);
            assert!(!view.Value.is_null());
            let word = |index: usize| &*view.Value.cast::<AtomicU32>().add(index);
            word(0).store(MAGIC, Ordering::SeqCst);
            // Status, version: sent away, or left with its status.
            for (status, version, released) in [
                (RESTING, VERSION, true),
                (RESTING, VERSION - 1, false),
                (4, VERSION, false),
                (5, VERSION, false),
                (CLAIMED, VERSION, false),
                (2, VERSION, false),
            ] {
                word(1).store(version, Ordering::SeqCst);
                word(HOOK_STATUS).store(status, Ordering::SeqCst);
                assert_eq!(
                    release_resting(pid, Duration::ZERO),
                    released,
                    "{status} {version}"
                );
                let after = if released { 4 } else { status };
                assert_eq!(word(HOOK_STATUS).load(Ordering::SeqCst), after);
            }
            // Taken off just now, it rests a moment later.
            word(1).store(VERSION, Ordering::SeqCst);
            word(HOOK_STATUS).store(6, Ordering::SeqCst);
            let address = view.Value as usize;
            let hook_side = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                (*(address as *const AtomicU32).add(HOOK_STATUS)).store(RESTING, Ordering::SeqCst);
            });
            assert!(release_resting(pid, Duration::from_secs(3)));
            hook_side.join().unwrap();
            assert_eq!(word(HOOK_STATUS).load(Ordering::SeqCst), 4);
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(hook);
        }
    }

    #[test]
    fn whether_a_dll_is_loaded_is_read_from_the_modules() {
        // Of a hook that never took its session, this tells whether its DLL
        // came at all; the name is matched without its folder or case.
        let pid = std::process::id();
        assert_eq!(
            loaded(pid, "C:\\Windows\\System32\\KERNEL32.DLL"),
            Some(true)
        );
        assert_eq!(
            loaded(pid, "C:\\Tools\\LimitPeeker\\limitpeeker_xaml.dll"),
            Some(false)
        );
        assert_eq!(loaded(u32::MAX, "kernel32.dll"), None);
    }

    #[test]
    fn the_hooks_refusals_are_logged_by_name() {
        // As `enum Refusal` runs in native/taskbar_tap.cpp.
        let names: Vec<_> = (0..9).map(refusal_name).collect();
        assert_eq!(
            names,
            [
                "none",
                "site-again",
                "elevated",
                "not-taskbar",
                "lease-gone",
                "other-attach",
                "taken",
                "setup-failed",
                "unknown"
            ]
        );
    }

    #[test]
    fn a_process_without_snap_layouts_is_not_held_back() {
        // Threads are read by name; this process and one that does not exist
        // have no Snap layouts thread, so nothing waits.
        assert_eq!(snap_flyout(std::process::id()), Some(false));
        assert_eq!(snap_flyout(u32::MAX), Some(false));
        let file: Log = Arc::new(Mutex::new(None));
        let waiting = AtomicBool::new(true);
        let mut snap = SnapWait::default();
        assert!(!snap.blocks(Shell::of(std::process::id()), 0, &file, &waiting));
        assert!(!waiting.load(Ordering::SeqCst));
        // The settings window has no maximize button, which would start
        // the thread when hovered.
        let frame = include_str!("../../settings/Frame.cs");
        let window = include_str!("../../settings/MainWindow.xaml.cs");
        assert!(frame.contains("GetWindowLongPtr(window, Style) & ~MaximizeBox"));
        assert!(window.contains("Frame.WithoutMaximize(self);"));
    }

    #[test]
    fn the_log_names_each_box_by_its_number_and_the_margins_apart() {
        // Each box: where it was asked for, whether it showed and why not,
        // its width. The repeater's margins on a line of their own; the
        // right one only while it is in.
        assert_eq!(
            box_line(0, [1, 0, 4, 118]),
            "box1 place=left shown=0 hide=settling box_dip=118"
        );
        assert_eq!(
            box_line(1, [2, 1, 0, 118]),
            "box2 place=tray shown=1 hide=shown box_dip=118"
        );
        assert_eq!(
            box_line(2, [4, 1, 0, 120]),
            "box3 place=before-start shown=1 hide=shown box_dip=120"
        );
        assert_eq!(
            box_line(3, [0, 0, 2, 0]).split(' ').nth(1),
            Some("place=none")
        );
        assert_eq!(
            margin_line([1, 142, 3, 162, 0, 0]),
            "margins left=reserved left_px=142 left_settling_ticks=3 items_start=162"
        );
        assert_eq!(
            margin_line([2, 2, 0, 0, 0, 0]),
            "margins left=refused: no ItemsRepeater under it"
        );
        assert_eq!(
            margin_line([1, 0, 0, 600, 142, 2]),
            "margins left=reserved left_px=0 left_settling_ticks=0 items_start=600 right_px=142 right_settling_ticks=2"
        );
        assert_eq!(
            margin_line([0, 0, 0, 0, 142, 2]),
            "margins left=none right_px=142 right_settling_ticks=2"
        );
        // Where the Widgets button is, which the places beside the apps go
        // by, and the button inside it as it is seen, which the left end
        // goes by.
        assert_eq!(
            widgets_line([0, 216, 6, 158]),
            "widgets button px=0..216 seen=6..158"
        );
        assert_eq!(widgets_line([539, 691, 0, 0]), "widgets button px=539..691");
        assert_eq!(widgets_line([0, 0, 0, 0]), "widgets button: none");
        assert_eq!(
            drop_line(1, 5, 2, 0),
            "drop: box=box2 place=after-apps order=after box1"
        );
        assert_eq!(
            drop_line(3, 1, 0, u32::MAX),
            "drop: box=box4 place=left order=alone"
        );
    }

    #[test]
    fn the_hooks_cost_is_told_per_tick() {
        // How many ticks and how fast, and each one's time, walk and nodes
        // on average; counters that wrapped still count forward.
        assert_eq!(
            cost_line(
                [10, 1000, 500, 100],
                [4010, 2_001_000, 1_000_500, 800_100],
                1_000_000
            ),
            "hook cost over 1000 s: ticks=4000 (4.0/s) tick_us=500 walk_us=250 visited=200"
        );
        assert!(cost_line([u32::MAX, 0, 0, 0], [9, 10, 0, 0], 1000).contains("ticks=10 "));
        assert!(cost_line([0; 4], [0; 4], 0).contains("ticks=0 (0.0/s) tick_us=0"));
    }

    #[test]
    fn a_worker_that_ended_counts_as_none_and_reasons_change_once() {
        // The tray fills in the figures once the worker has ended, however
        // it ended; what the popup is told changes only when it differs.
        assert!(!running(&None));
        let (hold, wait) = std::sync::mpsc::channel::<()>();
        let worker = Some(std::thread::spawn(move || {
            let _ = wait.recv();
        }));
        assert!(running(&worker));
        drop(hold);
        for _ in 0..200 {
            if !running(&worker) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!running(&worker));
        let file: Log = Arc::new(Mutex::new(None));
        let reason = Mutex::new(None);
        tell(&reason, &file, Some(Reason::new(STALLED, true)));
        assert_eq!(*reason.lock().unwrap(), Some(Reason::new(STALLED, true)));
        tell(&reason, &file, None);
        assert_eq!(*reason.lock().unwrap(), None);
        // Each fits one line of the popup at its caption size.
        for text in [
            ADMIN,
            NO_DLL,
            EXPLORER_ELEVATED,
            NOT_ATTACHED,
            EARLIER_BOX,
            STALLED,
            HOOK_ERROR,
            NOT_BOTTOM,
            INTERNAL,
            NO_LIMITS,
            UNFIT,
            NO_BLOCKS,
            BOX_UNFIT,
            ALL_HIDDEN,
        ] {
            assert!(text.chars().count() <= 52, "{text}");
        }
    }

    #[test]
    fn nothing_to_send_is_told_rather_than_left_on_screen() {
        // A label with no cells, or one the hook would refuse, is not sent,
        // so the box is asked to go; a label or scene that fits is sent.
        use crate::style::{Fill, find};
        use crate::usage::{Picks, label, mock};
        let look = find("slim").unwrap();
        let picks = Picks::default();
        let good = label(&mock(0), look, Fill::Used, &picks, true);
        assert_eq!(unsendable(&good, None, false), None);
        let empty = label(&[], look, Fill::Used, &picks, true);
        assert_eq!(unsendable(&empty, None, false), Some(NO_LIMITS));
        let mut none_left = mock(0);
        for service in &mut none_left {
            service.limits = Some(Vec::new());
        }
        let none_left = label(&none_left, look, Fill::Used, &picks, true);
        assert_eq!(unsendable(&none_left, None, false), Some(NO_LIMITS));
        let mut outsized = good.clone();
        outsized.style.font = 9999;
        assert_eq!(unsendable(&outsized, None, false), Some(UNFIT));
        let scene = crate::scene::sample(&good, look).unwrap();
        assert_eq!(unsendable(&good, Some(&scene), false), None);
        let mut too_wide = scene.clone();
        too_wide.width = 0;
        assert_eq!(unsendable(&good, Some(&too_wide), false), Some(UNFIT));
        // A custom box: nothing left to draw, or a scene too wide, is told
        // as the box's.
        assert_eq!(unsendable(&good, None, true), Some(NO_BLOCKS));
        assert_eq!(unsendable(&good, Some(&too_wide), true), Some(BOX_UNFIT));
        assert_eq!(unsendable(&good, Some(&scene), true), None);
    }
}
