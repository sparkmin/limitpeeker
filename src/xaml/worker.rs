//! The worker: attaches, sends the label, scene and icons, watches the hook,
//! and says why the box is off. Each turn is a few steps of `Worker`.

use super::*;
use crate::choice::{BoxPlace, Choice, Tile};
use crate::layout::DisplayMode;
use crate::log::{Log, log};
use crate::retry::{Retry, START_WAIT, Start, StartWatch};
use crate::scene::Scene;
use crate::shell::Shell;
use crate::style::{Fill, Look};
use crate::usage::{self, CAPACITY, Label, Service};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Win32::Foundation::*;

use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::*;

/// With --sample-boxes, four boxes of one service each, one to a place, in
/// the look chosen: Claude beside the tray, GPT at the left end, Gemini
/// before the Start button, and PC's CPU and RAM after the last app; a
/// service not read shows as it would in a box.
fn samples(services: &[Service], look: &Look, fill: Fill, warn: bool) -> Vec<Drawn> {
    [
        ("claude", BoxPlace::Tray),
        ("gpt", BoxPlace::Left),
        ("gemini", BoxPlace::BeforeStart),
        (crate::resources::ID, BoxPlace::AfterApps),
    ]
    .into_iter()
    .map(|(id, place)| {
        let mut picks = usage::Picks::default();
        picks.set_services(vec![id.to_string()]);
        picks.set_limits(crate::resources::ID, vec!["CPU".into(), "RAM".into()]);
        Drawn {
            label: usage::label(services, look, fill, &picks, warn),
            scene: None,
            custom: false,
            place: place.word(),
        }
    })
    .collect()
}

/// Why there is nothing to send the hook, or None when there is: a label
/// with no cells, or a label or scene the hook would refuse. The hook would
/// keep the last one on screen as if it were current.
pub(super) fn unsendable(
    label: &Label,
    scene: Option<&Scene>,
    custom: bool,
) -> Option<&'static str> {
    match scene {
        Some(scene) => (!scene.fits()).then_some(if custom { BOX_UNFIT } else { UNFIT }),
        None if custom => Some(NO_BLOCKS),
        None if label.cells.is_empty() => Some(NO_LIMITS),
        None => (!label.fits()).then_some(UNFIT),
    }
}

/// Puts a reason, or none, where the popup reads it, and logs a change.
pub(super) fn tell(reason: &Mutex<Option<Reason>>, file: &Log, next: Option<Reason>) {
    if let Ok(mut held) = reason.lock()
        && *held != next
    {
        log(file, format!("reason: {}", next.map_or("none", |r| r.text)));
        *held = next;
    }
}

/// Whether the worker runs: started, and not ended, however it ended.
pub(super) fn running(thread: &Option<std::thread::JoinHandle<()>>) -> bool {
    thread.as_ref().is_some_and(|t| !t.is_finished())
}

/// The tray icon's text: what each service the boxes show reads, every
/// limit included, in the boxes' order. This computer's meters are left out,
/// as the tray icon leaves them out: they are not limits, and would change
/// the text, sent to Explorer each time, every second. With nothing to show,
/// it says so.
pub(super) fn told(services: &[Service], shown: &[&'static str], fill: Fill) -> String {
    let limit = |s: &&Service| s.meaning == usage::Meaning::Limit;
    let told: Vec<String> = shown
        .iter()
        .filter_map(|id| services.iter().find(|s| s.id == *id))
        .filter(limit)
        // A service with no limits at all has no cell either.
        .filter(|s| s.limits.as_ref().is_none_or(|l| !l.is_empty()))
        .map(|s| usage::summary_of(s, fill))
        .collect();
    if !told.is_empty() {
        told.join("  |  ")
    } else if services.iter().any(|s| limit(&s)) {
        "No limit shown".into()
    } else {
        "No service connected".into()
    }
}

/// Why the box is off, from the hook's status and what could not be sent:
/// Some(None) once it shows, None when this turn does not say. Before
/// anything is read, nothing to send is no reason. `hide` and `failure` are
/// the hook's words for why it hid the box and why its measure failed.
pub(super) fn off_reason(
    status: u32,
    unsent: Option<&'static str>,
    read_any: bool,
    hide: u32,
    failure: u32,
) -> Option<Option<Reason>> {
    let off = |text| Some(Reason::new(text, true));
    match status {
        4 | 6 | RESTING => Some(off(STALLED)),
        5 => Some(off(HOOK_ERROR)),
        _ if unsent.is_some() => Some(
            unsent
                .filter(|_| read_any)
                .map(|text| Reason::new(text, false)),
        ),
        2 => Some(None),
        3 => {
            let placed = hide_name(hide) == "no-measure"
                && matches!(failure_name(failure), "not-primary" | "not-bottom");
            Some(placed.then(|| Reason::new(NOT_BOTTOM, false)))
        }
        _ => None,
    }
}

pub(super) fn worker(
    stop: Arc<AtomicBool>,
    presentation: Arc<AtomicU32>,
    shown_text: Arc<Mutex<String>>,
    figures: Arc<Figures>,
    choice: Arc<Choice>,
    menu_window: u32,
    snap_wait: Arc<AtomicBool>,
    reason: Arc<Mutex<Option<Reason>>>,
    file: Log,
    dll: String,
    verbose: bool,
) {
    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            log(&file, "COM init failed");
            return;
        }
        // A panic in here is logged and told, rather than ending the thread
        // unseen.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> windows::core::Result<()> {
                let mut worker = Worker::new(
                    presentation.clone(),
                    shown_text,
                    figures,
                    choice,
                    menu_window,
                    snap_wait,
                    reason.clone(),
                    file.clone(),
                    dll,
                    verbose,
                );
                while !stop.load(Ordering::SeqCst) {
                    if worker.turn() {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                }
                worker.stop();
                Ok(())
            },
        ));
        let failed = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(format!("ERROR {error}")),
            Err(_) => Some("ERROR the worker panicked".to_string()),
        };
        if let Some(failed) = failed {
            log(&file, failed);
            tell(&reason, &file, Some(Reason::new(INTERNAL, false)));
        }
        presentation.store(0, Ordering::SeqCst);
        CoUninitialize();
    }
}

/// What a turn draws: the services read, the first box's label and scene
/// made of them, whether a custom box chose them, and its place; and the
/// other boxes.
struct Frame {
    services: Vec<Service>,
    label: Label,
    scene: Option<Scene>,
    custom: bool,
    place: u32,
    /// The boxes after the first, in the list's order.
    others: Vec<Drawn>,
    /// With --sample-boxes, the four sample boxes in place of the boxes
    /// chosen.
    samples: Option<Vec<Drawn>>,
    /// Every box hidden, so nothing is sent.
    all_hidden: bool,
}

/// A box after the first as drawn: its label, or its custom box's scene,
/// and where it goes, as the hook's word.
struct Drawn {
    label: Label,
    scene: Option<Scene>,
    custom: bool,
    place: u32,
}

/// The worker's state from turn to turn, with what it shares with the
/// controller.
struct Worker {
    presentation: Arc<AtomicU32>,
    shown_text: Arc<Mutex<String>>,
    figures: Arc<Figures>,
    choice: Arc<Choice>,
    menu_window: u32,
    snap_wait: Arc<AtomicBool>,
    reason: Arc<Mutex<Option<Reason>>>,
    file: Log,
    dll: String,
    /// The hook's own lines only with --diagnostics.
    hook_log: Log,
    /// The services' logos' folder.
    icon_dir: PathBuf,
    began: u64,
    channel: Option<Channel>,
    /// The taskbar's Explorer last seen.
    explorer: Shell,
    retry: Retry,
    /// Whether the hook of the last attach took its session.
    start: StartWatch,
    snap: SnapWait,
    seen: Seen,
    /// The first box's label and scene last logged, with the meters'
    /// readings taken out.
    label: Option<Label>,
    scene: Option<Scene>,
    /// The other boxes' labels and scenes last logged.
    others_logged: [(Option<Label>, Option<Scene>); crate::choice::MAX_TILES - 1],
}

impl Worker {
    fn new(
        presentation: Arc<AtomicU32>,
        shown_text: Arc<Mutex<String>>,
        figures: Arc<Figures>,
        choice: Arc<Choice>,
        menu_window: u32,
        snap_wait: Arc<AtomicBool>,
        reason: Arc<Mutex<Option<Reason>>>,
        file: Log,
        dll: String,
        verbose: bool,
    ) -> Self {
        let hook_log: Log = if verbose {
            file.clone()
        } else {
            Arc::new(Mutex::new(None))
        };
        // Beside the settings; without LOCALAPPDATA, where no settings are
        // kept either, beside the DLL as before.
        let icon_dir = crate::icons::dir()
            .unwrap_or_else(|| std::path::Path::new(&dll).with_file_name("icons"));
        Self {
            presentation,
            shown_text,
            figures,
            choice,
            menu_window,
            snap_wait,
            reason,
            file,
            dll,
            hook_log,
            icon_dir,
            began: shared_now(),
            channel: None,
            explorer: Shell::default(),
            retry: Retry::default(),
            start: StartWatch::default(),
            snap: SnapWait::default(),
            seen: Seen::new(),
            label: None,
            scene: None,
            others_logged: Default::default(),
        }
    }

    /// One turn: what to draw, the taskbar's Explorer, an attach when it is
    /// time, and the hook driven. Whether to rest before the next: not
    /// after an attach that did not go through, whose next step is taken
    /// at once.
    fn turn(&mut self) -> bool {
        let frame = self.frame();
        match crate::shell::taskbar() {
            Some(shell) => {
                self.follow(shell);
                if !self.try_attach(shell, &frame) {
                    return false;
                }
                if self.drive(&frame) {
                    self.let_go_of_resume();
                }
                self.check_start();
            }
            None => {
                if let Some(c) = &self.channel {
                    c.request(DisplayMode::TrayOnly);
                    self.presentation.store(0, Ordering::SeqCst);
                }
            }
        }
        true
    }

    /// What the providers last read, or the mock figures, as the label or
    /// scene the hook is to draw; each logged when it changes.
    fn frame(&mut self) -> Frame {
        // What the providers last read, or the mock figures; the
        // hook draws whatever label it is sent.
        let services = self.figures.services(shared_now() - self.began);
        // The look and the fill mode, as last chosen on the command line,
        // in the popup or in the settings window.
        // The first box here; the others after it; a hidden box is not on
        // the taskbar, so the hook's boxes are the shown ones.
        let tiles = self.choice.shown_tiles();
        let first = tiles.first().cloned().unwrap_or_default();
        let look = first.look();
        let fill = self.choice.fill();
        // Whether low values warn in color, and the time now, for how long
        // until a limit resets.
        let warn = self.choice.warn();
        let now = usage::unix_now();
        let background = first.background;
        self.choice.offer(&services);
        // The box's own color on its look, dark content on a light one;
        // the taskbar's theme is read each frame, so a box the taskbar
        // shows through follows it.
        let light = crate::popup::light_taskbar();
        let look = &look.colored(first.color, light);
        // One box, or a card per service, whatever the look.
        let drawn = Look {
            style: background.apply(look.style),
            ..*look
        };
        // A custom box: its blocks choose the services and limits
        // the tooltip and the log tell of, and it is drawn as a
        // scene.
        let custom = self.choice.custom_of(&first);
        let picks = match &custom {
            Some(custom) => custom.picks(&services),
            None => first.picks.clone(),
        };
        let label = usage::label(&services, &drawn, fill, &picks, warn);
        // This computer's meters move each second: the log tells of the
        // label and the scene when anything else changes.
        let steady = usage::steady(&services);
        let steady_label = usage::label(&steady, &drawn, fill, &picks, warn);
        if self.label.as_ref() != Some(&steady_label) {
            log(
                &self.file,
                format!(
                    "box1 label place={} cells=\"{}\" sizing=\"{}\" fills={:?} look={} fill={} background={} fits={} capacity={CAPACITY}",
                    first.place.name(),
                    label.summary(),
                    label
                        .sizing()
                        .replace(usage::CELL_BREAK, "  |  ")
                        .replace(usage::GAUGE_BREAK, " ")
                        .replace(usage::LABEL_BREAK, " "),
                    label
                        .cells
                        .iter()
                        .map(|c| c.gauges.iter().map(|g| g.fill).collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    look.name,
                    fill.name(),
                    background.name(),
                    label.fits()
                ),
            );
            self.label = Some(steady_label.clone());
        }
        // What the tray icon's tooltip says.
        if let Ok(mut text) = self.shown_text.lock() {
            let shown = self.choice.all_services(&self.choice.offers());
            *text = told(&services, &shown, fill);
        }
        // A custom box's scene in place of the label.
        // A custom box is drawn as one box in its color.
        let plain = crate::style::DEFAULT.colored(first.color, light);
        let scene_of = |services: &[Service]| {
            custom.as_ref().and_then(|custom| {
                custom.scene(
                    services,
                    &plain,
                    fill,
                    warn,
                    now,
                    &mut crate::boxes::text_width,
                )
            })
        };
        let scene = scene_of(&services);
        let steady_scene = scene_of(&steady);
        if steady_scene.is_some() && self.scene != steady_scene {
            if let Some(scene) = &scene {
                log(
                    &self.file,
                    format!(
                        "box1 scene place={}{} {} icons={:?} fits={}",
                        first.place.name(),
                        custom
                            .as_ref()
                            .map_or(String::new(), |c| format!(" box=\"{}\"", c.name)),
                        scene.summary(),
                        scene.icons.iter().map(|i| i.0).collect::<Vec<_>>(),
                        scene.fits()
                    ),
                );
            }
            self.scene = steady_scene;
        }
        let others = tiles
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, tile)| self.other(index, tile, &services, &steady, fill, warn, now))
            .collect();
        let samples = self
            .choice
            .sample_boxes()
            .then(|| samples(&services, &drawn, fill, warn));
        Frame {
            services,
            label,
            scene,
            custom: custom.is_some(),
            place: first.place.word(),
            others,
            samples,
            all_hidden: tiles.is_empty(),
        }
    }

    /// A box after the first: its look's label of the services and limits
    /// picked for it, or its custom box's scene, each logged as the first
    /// box's is when something but a meter's value changes.
    fn other(
        &mut self,
        index: usize,
        tile: &Tile,
        services: &[Service],
        steady: &[Service],
        fill: Fill,
        warn: bool,
        now: i64,
    ) -> Drawn {
        let custom = self.choice.custom_of(tile);
        let light = crate::popup::light_taskbar();
        let look = &tile.look().colored(tile.color, light);
        let drawn = Look {
            style: tile.background.apply(look.style),
            ..*look
        };
        let picks = match &custom {
            Some(custom) => custom.picks(services),
            None => tile.picks.clone(),
        };
        let label = usage::label(services, &drawn, fill, &picks, warn);
        let steady_label = usage::label(steady, &drawn, fill, &picks, warn);
        let name = box_name(index as u32);
        let place = tile.place.name();
        let logged = &mut self.others_logged[index - 1];
        if custom.is_none() && logged.0.as_ref() != Some(&steady_label) {
            log(
                &self.file,
                format!(
                    "{name} label place={place} cells=\"{}\" look={} fits={}",
                    label.summary(),
                    look.name,
                    label.fits()
                ),
            );
            logged.0 = Some(steady_label);
        }
        let plain = crate::style::DEFAULT.colored(tile.color, light);
        let scene_of = |services: &[Service]| {
            custom.as_ref().and_then(|c| {
                c.scene(
                    services,
                    &plain,
                    fill,
                    warn,
                    now,
                    &mut crate::boxes::text_width,
                )
            })
        };
        let scene = scene_of(services);
        let steady_scene = scene_of(steady);
        if steady_scene.is_some() && logged.1 != steady_scene {
            if let (Some(scene), Some(custom)) = (&scene, &custom) {
                log(
                    &self.file,
                    format!(
                        "{name} scene place={place} box=\"{}\" {} fits={}",
                        custom.name,
                        scene.summary(),
                        scene.fits()
                    ),
                );
            }
            logged.1 = steady_scene;
        }
        Drawn {
            label,
            scene,
            custom: custom.is_some(),
            place: tile.place.word(),
        }
    }

    /// Explorer by its id and start: a new one can have the id of one that
    /// ended. What held the box back held another Explorer, and one the box
    /// was in that is gone is let go.
    fn follow(&mut self, shell: Shell) {
        let pid = shell.pid;
        if !self.explorer.is(&shell) {
            tell(&self.reason, &self.file, None);
            self.explorer = shell;
        }
        if let Some(gone) = self
            .channel
            .as_ref()
            .map(|c| c.shell)
            .filter(|s| !s.is(&shell))
        {
            log(
                &self.file,
                format!(
                    "explorer pid={} is gone; the taskbar's is pid={pid}",
                    gone.pid
                ),
            );
            self.channel = None;
            self.presentation.store(0, Ordering::SeqCst);
            self.seen.new_session();
        }
    }

    /// Attaches to the taskbar's Explorer when it is time: a hook resting
    /// from the last run is claimed, or the hook is attached anew once the
    /// Snap layouts thread allows. False when this turn ends here and the
    /// next starts at once.
    fn try_attach(&mut self, shell: Shell, frame: &Frame) -> bool {
        let pid = shell.pid;
        let was_waiting = self.snap.blocked && self.snap.shell.is(&shell);
        // A hook resting from the last run is taken back with
        // no attach, so the Snap layouts thread is not waited
        // on.
        let ready = self.channel.is_none() && self.retry.ready(shell, shared_now());
        let resuming = ready && resting(pid);
        if resuming && self.snap.blocked {
            self.snap.blocked = false;
            self.snap_wait.store(false, Ordering::SeqCst);
        }
        if !(ready
            && (resuming
                || !self
                    .snap
                    .blocks(shell, shared_now(), &self.file, &self.snap_wait)))
        {
            return true;
        }
        if was_waiting {
            // The wait may have outlasted the attach's
            // 60 s, which start now.
            log(
                &self.file,
                "attach window starts over after the Snap layouts wait",
            );
            self.retry.restart(shared_now());
        }
        // An Explorer that cannot be read is left, and the
        // thread goes on to the next one.
        let is_elevated = unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).and_then(|process| {
                let is_elevated = elevated(process);
                let _ = CloseHandle(process);
                is_elevated
            })
        };
        let is_elevated = match is_elevated {
            Ok(is_elevated) => is_elevated,
            Err(error) => {
                log(
                    &self.file,
                    format!("attach skipped: explorer pid={pid} could not be read: {error}"),
                );
                self.retry.complete(false, shared_now());
                tell(
                    &self.reason,
                    &self.file,
                    Some(Reason::new(NOT_ATTACHED, true)),
                );
                return false;
            }
        };
        if is_elevated {
            log(
                &self.file,
                format!("attach skipped: explorer pid={pid} runs as administrator"),
            );
            self.retry.complete(false, shared_now());
            tell(
                &self.reason,
                &self.file,
                Some(Reason::new(EXPLORER_ELEVATED, false)),
            );
            return false;
        }
        let mut next = match Channel::new(shell) {
            Ok(c) => c,
            Err(error) => {
                // A session that is merely busy can free up.
                let retryable = error.code() == SESSION_BUSY;
                log(&self.file, format!("tray-only: {error}"));
                self.retry.complete(retryable, shared_now());
                if self.retry.finished() {
                    let text = if retryable { EARLIER_BOX } else { NOT_ATTACHED };
                    tell(&self.reason, &self.file, Some(Reason::new(text, true)));
                }
                return false;
            }
        };
        log(
            &self.file,
            format!(
                "explorer pid={pid} elevated={is_elevated} reused={} resumed={}",
                next.reused, next.resumed
            ),
        );
        if resuming && !next.resumed {
            // Its rest ran out between the look and the
            // claim: attach anew, after the Snap layouts
            // look.
            log(
                &self.file,
                "the resting hook left before it was claimed; attaching anew",
            );
            drop(next);
            self.retry.restart(shared_now());
            return false;
        }
        // Where a right click on the box asks for the
        // popup.
        next.set(MENU_WINDOW, self.menu_window);
        // The attach this session is for, as its hook is to be told; a
        // hook an earlier attach loaded does not take it.
        let token = (shared_now() as u32).max(1);
        if !next.resumed {
            next.set(ATTACH_TOKEN, token);
        }
        // The hook's first tick finds a label waiting.
        match &frame.scene {
            Some(scene) => {
                next.show_scene_icons(Panel::MAIN, scene, &self.icon_dir, &self.file);
                next.show_scene(Panel::MAIN, scene);
            }
            None => {
                next.show_icons(Panel::MAIN, &frame.label, &self.icon_dir, &self.file);
                next.show(Panel::MAIN, &frame.label);
            }
        }
        if next.resumed {
            log(
                &self.file,
                "resumed: the hook the last run left resting takes the box back; nothing is attached, so the Snap layouts thread does not matter",
            );
            self.retry.complete(false, shared_now());
            self.channel = Some(next);
            tell(&self.reason, &self.file, None);
        } else {
            match attach(pid, &self.dll, token, &self.file) {
                Ok(()) => {
                    self.retry.complete(false, shared_now());
                    self.start.attached(shell, shared_now());
                    self.channel = Some(next);
                    tell(&self.reason, &self.file, None);
                }
                Err(error) => {
                    // Only a missing diagnostics endpoint with no TAP activity
                    // can retry. Never retry a possibly attached/erroring hook.
                    let retryable = error.code()
                        == windows::core::HRESULT::from_win32(ERROR_NOT_FOUND.0)
                        && next.get(HOOK_STATUS) == 0;
                    self.retry.complete(retryable, shared_now());
                    log(
                        &self.file,
                        format!(
                            "attach rejected: {error}; transient={retryable}; retry_window=60s"
                        ),
                    );
                    if self.retry.finished() {
                        tell(
                            &self.reason,
                            &self.file,
                            Some(Reason::new(NOT_ATTACHED, true)),
                        );
                    }
                }
            }
        }
        true
    }

    /// Sends the hook what to draw and asks it to show the box, or not when
    /// there is nothing to send; logs what changed in the hook and tells why
    /// the box is off. Whether a claimed hook left without showing the box.
    fn drive(&mut self, frame: &Frame) -> bool {
        let Some(c) = &mut self.channel else {
            return false;
        };
        // The hook places the boxes from the live tree; this side only asks
        // for them and records what happened. Each box's place and rank:
        // the boxes in the order chosen, or with --sample-boxes the four
        // samples, one to a place.
        let mut places = [0u32; MAX_BOXES];
        let mut ranks = [0u32; MAX_BOXES];
        let unsent = match &frame.samples {
            Some(samples) => {
                for (at, sample) in samples.iter().enumerate().take(MAX_BOXES) {
                    c.show_icons(Panel(at), &sample.label, &self.icon_dir, &self.file);
                    c.show(Panel(at), &sample.label);
                    places[at] = sample.place;
                }
                unsendable(&samples[0].label, None, false)
            }
            // Every box hidden: none is sent, and the popup says why.
            None if frame.all_hidden => Some(ALL_HIDDEN),
            None => {
                match &frame.scene {
                    Some(scene) => {
                        c.show_scene_icons(Panel::MAIN, scene, &self.icon_dir, &self.file);
                        c.show_scene(Panel::MAIN, scene);
                    }
                    // A custom box with nothing to draw sends
                    // nothing, and the box goes.
                    None if frame.custom => {}
                    None => {
                        c.show_icons(Panel::MAIN, &frame.label, &self.icon_dir, &self.file);
                        c.show(Panel::MAIN, &frame.label);
                    }
                }
                // The other boxes: each one's label or scene in its words,
                // and where it goes; nowhere with nothing to send, so an old
                // one is not left on screen. They stand in a place in the
                // list's order.
                for (at, other) in frame.others.iter().enumerate().take(MAX_BOXES - 1) {
                    let panel = Panel(at + 1);
                    if unsendable(&other.label, other.scene.as_ref(), other.custom).is_some() {
                        continue;
                    }
                    match &other.scene {
                        Some(scene) => {
                            c.show_scene_icons(panel, scene, &self.icon_dir, &self.file);
                            c.show_scene(panel, scene);
                        }
                        None => {
                            c.show_icons(panel, &other.label, &self.icon_dir, &self.file);
                            c.show(panel, &other.label);
                        }
                    }
                    places[at + 1] = other.place;
                }
                places[0] = frame.place;
                for (at, rank) in ranks.iter_mut().enumerate() {
                    *rank = at as u32;
                }
                unsendable(&frame.label, frame.scene.as_ref(), frame.custom)
            }
        };
        // Nothing to send hides the box.
        let request = if unsent.is_some() {
            DisplayMode::TrayOnly
        } else {
            DisplayMode::Full
        };
        c.request(request);
        for at in 0..MAX_BOXES {
            c.set(BOX_PLACES + at, places[at]);
            c.set(BOX_RANKS + at, ranks[at]);
        }
        c.set(DRAG_ALLOWED, u32::from(self.choice.drag()));
        // What the hook costs Explorer, each hour.
        if let Some(line) = c.cost(shared_now(), false) {
            log(&self.file, line);
        }
        if request != self.seen.request {
            log(
                &self.file,
                format!(
                    "request={request:?}{}",
                    unsent.map_or(String::new(), |why| format!(" because {why}"))
                ),
            );
            self.seen.request = request;
        }
        let shown = DisplayMode::from_word(c.get(SHOWN)) as u32;
        self.presentation.store(
            if c.get(HOOK_STATUS) == 2 { shown } else { 0 },
            Ordering::SeqCst,
        );
        let status = self.seen.report(c, shown, &self.file, &self.hook_log);
        // This run stopped answering long enough for the lease to run out,
        // and the hook took the box off and rests: moving again, the run
        // takes it back as a run started again would.
        if status == RESTING && c.reclaim() {
            log(
                &self.file,
                "taken back: the hook took the box off when this run stopped answering; claiming it again",
            );
        }
        // A box dropped on the taskbar moves there, as the popup moves it,
        // before or after the box it was dropped beside; the tray thread
        // saves it. The sample boxes do not move.
        if let Some((dropped, place, order, target)) = self.seen.dropped.take() {
            if frame.samples.is_some() {
                log(&self.file, "drop only logged: the sample boxes do not move");
            } else if !self.choice.apply_drop(dropped, place, order, target) {
                log(&self.file, "drop not understood; nothing moved");
            }
        }
        // A claimed hook that left without showing the box
        // (its tray frame went while it rested) is let go,
        // and the box is attached anew.
        c.live |= status == 2 || status == 3;
        let resume_failed = c.resumed && !c.live && status == 4;
        // Why the box is off, or nothing once it shows.
        if let Some(next) = off_reason(
            status,
            unsent,
            !frame.services.is_empty(),
            c.get(HIDE_REASON),
            c.get(MEASURE_FAILURE),
        ) {
            tell(&self.reason, &self.file, next);
        }
        resume_failed
    }

    /// Lets go of a claimed hook that left without showing the box, to
    /// attach anew.
    fn let_go_of_resume(&mut self) {
        log(
            &self.file,
            "resume failed: the resting hook left without showing the box; attaching anew",
        );
        self.channel = None;
        self.presentation.store(0, Ordering::SeqCst);
        self.seen.new_session();
        self.retry.restart(shared_now());
    }

    /// Attaches again when the hook of an attach that went through has not
    /// taken its session in time, as often as one Explorer allows; after
    /// that the box stays off, and the popup says so. The session is kept,
    /// so a hook that takes it late still shows the box. What the hook and
    /// Explorer show of it goes in the log.
    fn check_start(&mut self) {
        let Some(c) = &self.channel else {
            return;
        };
        let check = self.start.check(c.get(HOOK_STATUS), shared_now());
        let (Start::Again(attach) | Start::GiveUp(attach)) = check else {
            return;
        };
        let loaded = match loaded(c.shell.pid, &self.dll) {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        };
        let line = format!(
            "hook did not start in {} s after attach {attach}: dll_loaded={loaded} site_calls={} refusal={}",
            START_WAIT / 1000,
            c.get(SITE_CALLS),
            refusal_name(c.get(SITE_REFUSAL))
        );
        if check == Start::Again(attach) {
            log(&self.file, format!("{line}; attaching again"));
            self.channel = None;
            self.presentation.store(0, Ordering::SeqCst);
            self.seen.new_session();
            self.retry.restart(shared_now());
        } else {
            log(&self.file, format!("{line}; staying tray-only"));
            tell(
                &self.reason,
                &self.file,
                Some(Reason::new(NOT_ATTACHED, true)),
            );
        }
    }

    /// Takes the box off as the run ends, and logs what the hook came to.
    fn stop(self) {
        if let Some(mut c) = self.channel {
            if let Some(line) = c.cost(shared_now(), true) {
                log(&self.file, line);
            }
            log(
                &self.file,
                format!(
                    "layout_revision={} visibility_changes={}",
                    c.get(LAYOUT_REVISION),
                    c.get(VISIBILITY_CHANGES)
                ),
            );
            c.set(REQUEST, 0);
            c.set(CONTROLLER_PID, 0);
            for _ in 0..20 {
                if matches!(c.get(HOOK_STATUS), 4 | RESTING) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // The taskbar's margins, given back.
            if self.seen.margins_used() {
                log(
                    &self.file,
                    format!("stop: {}", margin_line(margin_words(&c))),
                );
            }
            log(
                &self.file,
                format!(
                    "stop: hook status={} (4=UI detached, DLL remains loaded; {RESTING}=UI detached, resting until a restart or until Explorer ends) reserve={} settling_ticks={} text_taken={} text_drawn={} text_clipped={} menu_requests={}",
                    c.get(HOOK_STATUS),
                    reserve_name(c.get(RESERVE_STATE)),
                    c.get(SETTLING_TICKS),
                    c.get(TEXT_APPLIED),
                    c.get(TEXT_DRAWN),
                    c.get(TEXT_CLIPPED),
                    c.get(MENU_REQUESTS)
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tray_icons_text_tells_the_limits_the_boxes_show() {
        // Each service the boxes show, in their order, every limit included;
        // this computer's meters never, as they change each second; and what
        // is missing said.
        let services = usage::mock(0);
        assert_eq!(
            told(&services, &["pc", "claude", "gpt"], Fill::Left),
            "Claude 5h 58% 7d 10% Opus 7d 40%  |  GPT 5h 72% 7d 58%"
        );
        assert_eq!(told(&services, &["pc"], Fill::Left), "No limit shown");
        assert_eq!(told(&services, &[], Fill::Left), "No limit shown");
        let pc: Vec<Service> = services.into_iter().filter(|s| s.id == "pc").collect();
        assert_eq!(told(&pc, &["pc"], Fill::Left), "No service connected");
        assert_eq!(told(&[], &[], Fill::Left), "No service connected");
    }

    #[test]
    fn why_the_box_is_off_follows_the_hooks_status() {
        // Off after a stall or an error, with Restart Explorer; what could
        // not be sent, once something was read; nothing once it shows; and
        // a taskbar it cannot measure as one not at the bottom.
        let stalled = Some(Some(Reason::new(STALLED, true)));
        for status in [4, 6, RESTING] {
            assert_eq!(off_reason(status, None, true, 0, 0), stalled);
        }
        assert_eq!(
            off_reason(5, Some(NO_LIMITS), true, 0, 0),
            Some(Some(Reason::new(HOOK_ERROR, true)))
        );
        assert_eq!(
            off_reason(2, Some(NO_LIMITS), true, 0, 0),
            Some(Some(Reason::new(NO_LIMITS, false)))
        );
        assert_eq!(off_reason(2, Some(NO_LIMITS), false, 0, 0), Some(None));
        assert_eq!(off_reason(2, None, true, 0, 0), Some(None));
        // The hook's words: hide 1 is no-measure, failures 2 and 3 are
        // not-primary and not-bottom.
        for failure in [2, 3] {
            assert_eq!(
                off_reason(3, None, true, 1, failure),
                Some(Some(Reason::new(NOT_BOTTOM, false)))
            );
        }
        assert_eq!(off_reason(3, None, true, 1, 0), Some(None));
        assert_eq!(off_reason(3, None, true, 0, 3), Some(None));
        for status in [0, 1, 8] {
            assert_eq!(off_reason(status, None, true, 0, 0), None);
        }
    }
}
