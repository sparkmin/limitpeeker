//! The memory shared with the hook in Explorer: its words, which must match
//! native/taskbar_tap.cpp, their names for the log, and the channel that
//! writes them and keeps the heartbeat.

use crate::icons::{self, Icon};
use crate::layout::DisplayMode;
use crate::log::{Log, log};
use crate::scene::{
    SCENE_COUNT, SCENE_ITEMS, SCENE_MODE, SCENE_POOL, SCENE_POOL_LENGTH, SCENE_WIDTH, Scene,
};
use crate::shell::Shell;
use crate::usage::{Label, MAX_CELLS, MAX_GAUGES};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use windows::Win32::Foundation::*;

use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::*;
use windows::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTime;

use windows::core::{GUID, PCWSTR};

pub(super) const CLSID: GUID = GUID::from_u128(0x3f4bddb3_3bd6_4279_96bd_6cc03075a181);
pub(super) const WORDS: usize = 70102;
/// The box's words, from TEXT_SEQ to the gauges' ends; each other box has a
/// copy of them, BOX_WORDS further on than the one before. Each gauge's
/// colored part sits from the start (0) or against the end (1), in
/// GAUGE_ENDS.
pub(super) const BOX_WORDS: usize = 17505;
pub(super) const MAX_BOXES: usize = 4;
pub(super) const GAUGE_ENDS: usize = 17541;
/// What the controller asks to show, what the hook shows, and the label's
/// width in DIP as the hook measured it.
pub(super) const REQUEST: usize = 5;
pub(super) const SHOWN: usize = 22;
pub(super) const FULL_WIDTH: usize = 43;
/// The tray window, as the low 32 bits of its handle, which the hook asks for
/// the popup when the box is right-clicked, and how often it asked.
pub(super) const MENU_WINDOW: usize = 3;
pub(super) const MENU_REQUESTS: usize = 6;
/// The label, under one sequence (odd while being written): the two string
/// lengths, the sequence the hook took, how often a text was wider than its
/// widest form, and the sequence of the label the hook drew, which lags the
/// one it took while a wider label waits for room.
pub(super) const TEXT_SEQ: usize = 48;
pub(super) const TEXT_LENGTH: usize = 49;
pub(super) const SIZING_LENGTH: usize = 50;
pub(super) const TEXT_APPLIED: usize = 51;
pub(super) const TEXT_CLIPPED: usize = 52;
pub(super) const TEXT_DRAWN: usize = 53;
/// The look, in `style::Style::words()` order.
pub(super) const STYLE_BOX: usize = 54;
/// Each gauge's fill in permille (all ones for none) and its value's and
/// fill's colors, in order across the cells; then the text and its widest
/// form as UTF-16, two units a word.
pub(super) const GAUGE_FILLS: usize = 83;
pub(super) const GAUGE_TEXT_COLORS: usize = 95;
pub(super) const GAUGE_FILL_COLORS: usize = 107;
pub(super) const TEXT_CHARS: usize = 119;
pub(super) const SIZING_CHARS: usize = 215;
/// The icons, under their own sequence: the sequence the hook took, each
/// cell's side in pixels, and a block of premultiplied BGRA per cell.
pub(super) const ICON_SEQ: usize = 311;
pub(super) const ICON_TAKEN: usize = 312;
pub(super) const ICON_SIZES: usize = 313;
pub(super) const ICON_PIXELS: usize = 321;
pub(super) const ICON_BLOCK: usize = 4096;
/// The hook's measurement in screen pixels, logged here.
pub(super) const MEASURE_ITEMS: usize = 34;
pub(super) const MEASURE_TRAY: usize = 35;
/// The taskbar's DPI as the hook measured it; icons are drawn for it.
pub(super) const MEASURE_DPI: usize = 36;
/// Why the hook hid the box (0 when shown), and the width of the tree it
/// adopted at attach.
pub(super) const HIDE_REASON: usize = 38;
pub(super) const ATTACH_ROOT_WIDTH: usize = 39;
/// How often the hook measured the label width.
pub(super) const LABEL_MEASURES: usize = 20;
/// Why the hook's last measurement failed (0 when it succeeded). Tells a taskbar
/// that is not at the bottom apart from anchors a Windows update renamed.
pub(super) const MEASURE_FAILURE: usize = 21;
/// Names for the hook's measurement failures, in `enum Failure` order.
pub(super) fn failure_name(code: u32) -> &'static str {
    const NAMES: [&str; 10] = [
        "measured",
        "not-shell-window",
        "not-primary",
        "not-bottom",
        "no-root",
        "bad-scale",
        "root-size-mismatch",
        "walk-failed",
        "anchors-missing",
        "no-rects",
    ];
    NAMES.get(code as usize).copied().unwrap_or("unknown")
}
/// How far the hook got attaching its session, in `enum Stage` order. The call
/// that failed is the one after the recorded stage.
pub(super) const ATTACH_STAGE: usize = 30;
pub(super) fn stage_name(code: u32) -> &'static str {
    const NAMES: [&str; 9] = [
        "none",
        "site",
        "thread",
        "diagnostics",
        "tree-service",
        "advised",
        "frame-seen",
        "dispatched",
        "started",
    ];
    NAMES.get(code as usize).copied().unwrap_or("unknown")
}
/// Subscribe calls the hook made (low byte) and cleanup unsubscribes that
/// succeeded (next byte).
pub(super) const ADVISE_TRIES: usize = 31;
pub(super) fn advise_tries(word: u32) -> (u32, u32) {
    (word & 0xff, (word >> 8) & 0xff)
}
/// The attach made last, which the hook's initialization data must name; how
/// often the hook's SetSite reached the session; and why it last turned the
/// session down, in `enum Refusal` order. An attach returns before its hook
/// starts, so one the hook never takes must leave a trace.
pub(super) const ATTACH_TOKEN: usize = 7;
pub(super) const SITE_CALLS: usize = 8;
pub(super) const SITE_REFUSAL: usize = 9;
pub(super) fn refusal_name(code: u32) -> &'static str {
    const NAMES: [&str; 8] = [
        "none",
        "site-again",
        "elevated",
        "not-taskbar",
        "lease-gone",
        "other-attach",
        "taken",
        "setup-failed",
    ];
    NAMES.get(code as usize).copied().unwrap_or("unknown")
}
/// The reservation: what happened to it, the box's width in px (or a refusal's
/// reason), and ticks the box waited for the apps to clear the strip.
pub(super) const RESERVE_STATE: usize = 45;
pub(super) const RESERVE_WIDTH: usize = 46;
pub(super) const SETTLING_TICKS: usize = 47;
/// What became of the taskbar repeater's margins, the left one's width in px
/// (or, refused, 1 for no TaskbarFrame and 2 for no repeater), the ticks an
/// item reached into the left strip, and where the items start in px.
pub(super) const LEFT_STATE: usize = 11;
pub(super) const LEFT_WIDTH: usize = 12;
pub(super) const LEFT_TICKS: usize = 13;
pub(super) const LEFT_ITEMS_START: usize = 14;
/// After the boxes' words, per box (MAX_BOXES each): where the controller
/// asks for it (0 nowhere, 1 the left end, 2 beside the tray, 4 before the
/// Start button, 5 after the last app; the first box's 0 is beside the
/// tray), its rank there (the lower, the further left), whether it shows,
/// why not, and its width in DIP. Then whether a box may be dragged; where
/// one was dropped, under its sequence: which box, its place, its order
/// there (0 alone, 1 before a box, 2 after it) and that box (-1 for none);
/// how often a drag began and how often one was lost; and after the last
/// app, the repeater's right margin in px and the ticks the row there
/// reached the tray's side. The hook only reports a drop.
pub(super) const BOX_PLACES: usize = 70068;
pub(super) const BOX_RANKS: usize = 70072;
pub(super) const BOX_SHOWN: usize = 70076;
pub(super) const BOX_HIDE: usize = 70080;
pub(super) const BOX_WIDTH: usize = 70084;
pub(super) const DRAG_ALLOWED: usize = 70088;
pub(super) const DROP_SEQ: usize = 70089;
pub(super) const DROP_BOX: usize = 70090;
pub(super) const DROP_PLACE: usize = 70091;
pub(super) const DROP_ORDER: usize = 70092;
pub(super) const DROP_TARGET: usize = 70093;
pub(super) const DRAGS: usize = 70094;
pub(super) const DRAGS_LOST: usize = 70095;
pub(super) const RIGHT_WIDTH: usize = 70096;
pub(super) const RIGHT_TICKS: usize = 70097;
/// Where the Widgets button is, its left and right ends in px, while the
/// margins are in; 0 and 0 when there is none. Then the same for the button
/// inside it as it is seen.
pub(super) const WIDGETS_LEFT: usize = 70098;
pub(super) const WIDGETS_RIGHT: usize = 70099;
pub(super) const SEEN_LEFT: usize = 70100;
pub(super) const SEEN_RIGHT: usize = 70101;
/// A box as the log names it: by its number in the list, from 1.
pub(super) fn box_name(index: u32) -> &'static str {
    const NAMES: [&str; MAX_BOXES] = ["box1", "box2", "box3", "box4"];
    NAMES.get(index as usize).copied().unwrap_or("none")
}
/// A drop as the log names it, from DROP_BOX, DROP_PLACE, DROP_ORDER and
/// DROP_TARGET.
pub(super) fn drop_line(dropped: u32, place: u32, order: u32, target: u32) -> String {
    const ORDERS: [&str; 3] = ["alone", "before", "after"];
    let beside = match ORDERS.get(order as usize) {
        Some(&"alone") | None => String::new(),
        Some(_) => format!(" {}", box_name(target)),
    };
    format!(
        "drop: box={} place={} order={}{beside}",
        box_name(dropped),
        place_name(place),
        ORDERS.get(order as usize).unwrap_or(&"unknown")
    )
}
/// A box as the hook placed it, for the log: where it was asked for,
/// whether it shows, why not, and its width.
pub(super) fn box_line(index: u32, [place, shown, hide, width]: [u32; 4]) -> String {
    format!(
        "{} place={} shown={shown} hide={} box_dip={width}",
        box_name(index),
        place_name(place),
        hide_name(hide)
    )
}
/// The hook's enum Place, as the log names it.
pub(super) fn place_name(word: u32) -> &'static str {
    const PLACES: [&str; 6] = [
        "none",
        "left",
        "tray",
        "unknown",
        "before-start",
        "after-apps",
    ];
    PLACES.get(word as usize).copied().unwrap_or("unknown")
}
/// The room on both sides of the taskbar repeater as the hook left it, for
/// the log, still named margins: the left side's (its state, width and
/// ticks, and where the apps then start), and the one after the apps while
/// it is in (its width and ticks). A refusal's width says what was missing.
pub(super) fn margin_line(words: [u32; 6]) -> String {
    let [left, left_px, ticks, items, right_px, right_ticks] = words;
    let left = match (left_name(left), left_px) {
        ("refused", 1) => "left=refused: no Taskbar.TaskbarFrame".into(),
        ("refused", _) => "left=refused: no ItemsRepeater under it".into(),
        ("none", _) => "left=none".into(),
        (name, px) => format!(
            "left={name} left_px={px} left_settling_ticks={ticks} items_start={}",
            items as i32
        ),
    };
    let right = if right_px > 0 {
        format!(" right_px={right_px} right_settling_ticks={right_ticks}")
    } else {
        String::new()
    };
    format!("margins {left}{right}")
}
/// Where the Widgets button is, and the button inside it as it is seen, for
/// the log: WIDGETS_LEFT to SEEN_RIGHT.
pub(super) fn widgets_line([left, right, seen_left, seen_right]: [u32; 4]) -> String {
    if right == 0 {
        return "widgets button: none".into();
    }
    let seen = if seen_right > 0 {
        format!(" seen={}..{}", seen_left as i32, seen_right as i32)
    } else {
        String::new()
    };
    format!("widgets button px={}..{}{seen}", left as i32, right as i32)
}
/// The margins' words as the log reads them.
pub(super) fn margin_words(c: &Channel) -> [u32; 6] {
    [
        LEFT_STATE,
        LEFT_WIDTH,
        LEFT_TICKS,
        LEFT_ITEMS_START,
        RIGHT_WIDTH,
        RIGHT_TICKS,
    ]
    .map(|word| c.get(word))
}
/// Names for the left end's outcome, in `enum Left` order.
pub(super) fn left_name(code: u32) -> &'static str {
    const NAMES: [&str; 6] = [
        "none",
        "reserved",
        "refused",
        "restored",
        "abandoned",
        "restore-mismatch",
    ];
    NAMES.get(code as usize).copied().unwrap_or("unknown")
}
/// Names for the reservation outcome, in `enum Reserve` order.
pub(super) fn reserve_name(code: u32) -> &'static str {
    const NAMES: [&str; 6] = [
        "none",
        "reserved",
        "target-refused",
        "restored",
        "structure-changed",
        "restore-mismatch",
    ];
    NAMES.get(code as usize).copied().unwrap_or("unknown")
}
/// Names for why the box is hidden, in `enum Hide` order.
pub(super) fn hide_name(code: u32) -> &'static str {
    const NAMES: [&str; 6] = [
        "shown",
        "no-measure",
        "not-requested",
        "no-reservation",
        "settling",
        "no-text",
    ];
    NAMES.get(code as usize).copied().unwrap_or("unknown")
}
/// Liveness only: proves the controller process still runs.
pub(super) const HEARTBEAT: usize = 23;
/// The session's own words: the controller's process id (0 once it stops),
/// when it last observed (the hook no longer reads it), the hook's status
/// and error, how often the tree's layout changed and the box was shown or
/// hidden, and what the hook's ticks cost. Must match the ...Word names in
/// native/taskbar_tap.cpp.
pub(super) const CONTROLLER_PID: usize = 2;
pub(super) const OBSERVED: usize = 4;
pub(super) const HOOK_STATUS: usize = 15;
pub(super) const HOOK_ERROR_CODE: usize = 16;
pub(super) const LAYOUT_REVISION: usize = 17;
pub(super) const VISIBILITY_CHANGES: usize = 18;
pub(super) const TICKS: usize = 24;
pub(super) const TICK_US: usize = 25;
pub(super) const WALK_US: usize = 26;
pub(super) const VISITED: usize = 27;
/// How long the hook waits for a heartbeat, which must match LeaseMs, and
/// how often one is written: five and more a lease.
pub(super) const LEASE_MS: u64 = 1500;
pub(super) const HEARTBEAT_MS: u64 = 200;
const _: () = assert!(HEARTBEAT_MS * 5 <= LEASE_MS);
/// How often the hook's cost is logged while attached.
pub(super) const COST_EVERY: u64 = 3_600_000;

/// What the hook's ticks cost between two readings `ms` apart: how many,
/// and on average their time, the tree walk's time and the nodes walked.
pub(super) fn cost_line(from: [u32; 4], to: [u32; 4], ms: u64) -> String {
    let [ticks, tick_us, walk_us, visited]: [u32; 4] =
        std::array::from_fn(|i| to[i].wrapping_sub(from[i]));
    let each = |total: u32| if ticks == 0 { 0 } else { total / ticks };
    format!(
        "hook cost over {} s: ticks={ticks} ({:.1}/s) tick_us={} walk_us={} visited={}",
        ms / 1000,
        f64::from(ticks) * 1000.0 / ms.max(1) as f64,
        each(tick_us),
        each(walk_us),
        each(visited)
    )
}
/// E_PENDING: the hook's session is not free to reuse yet, so retry.
pub(super) const SESSION_BUSY: windows::core::HRESULT =
    windows::core::HRESULT(0x8000_000A_u32 as i32);
pub(super) const MAGIC: u32 = 0x4c504b31;
/// Shared-layout version. Three places compare it: the controller writes it, the
/// reuse check reads it, and the hook refuses a mismatch. Two of them once
/// disagreed silently, which broke session reuse. One constant now, so a bump
/// cannot leave one of them behind. 32: a hook takes the session only for the
/// attach that loaded it, and says why it turned one down.
pub(super) const VERSION: u32 = 32;
/// A hook whose box came off cleanly rests, still subscribed, until Explorer
/// ends; a controller started again claims it, and it starts the box again
/// without a new subscription. Must match Resting and Claimed in
/// native/taskbar_tap.cpp.
pub(super) const RESTING: u32 = 7;
pub(super) const CLAIMED: u32 = 8;
/// What a claim keeps: the status the hook starts again by, and the record of
/// the subscription it still holds, which it writes only when it subscribes.
pub(super) const KEPT_ON_CLAIM: [usize; 3] = [HOOK_STATUS, ADVISE_TRIES, ATTACH_ROOT_WIDTH];
/// Every time shared with the hook, and every time compared against one, in ms.
/// Stops during sleep: `GetTickCount64` did not, so resuming aged the heartbeat
/// by the whole sleep and the hook could remove a live controller's widget.
/// Must match shared_now() in native/taskbar_tap.cpp.
pub(super) fn shared_now() -> u64 {
    let mut ticks = 0u64;
    // Cannot fail on any supported Windows; a zero would read as a stale sample.
    let _ = unsafe { QueryUnbiasedInterruptTime(&mut ticks) };
    ticks / 10_000
}
pub(super) use crate::message::wide;

/// Which box's words, by its place in the list of boxes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Panel(pub(super) usize);

impl Panel {
    /// The first box.
    pub(super) const MAIN: Panel = Panel(0);
    pub(super) fn base(&self) -> usize {
        self.0 * BOX_WORDS
    }
}

/// What a box was last sent: its label or scene, so an unchanged one is not
/// sent again, and the side, the (service, color) and the files of its
/// icons, so they are drawn again only when one of them changes.
#[derive(Default)]
pub(super) struct Sent {
    pub(super) label: Option<Label>,
    pub(super) scene: Option<Scene>,
    pub(super) icons: Option<(u32, Vec<(&'static str, u32)>, Vec<u64>)>,
}

pub(super) struct Channel {
    pub(super) mapping: HANDLE,
    pub(super) view: MEMORY_MAPPED_VIEW_ADDRESS,
    /// The Explorer the session is in.
    pub(super) shell: Shell,
    /// True when this session took over an abandoned mapping rather than
    /// creating one. A stale attach only ever appeared here.
    pub(super) reused: bool,
    /// True when it claimed a resting hook, which starts the box again with
    /// no attach; and whether the hook has since shown or hidden the box.
    pub(super) resumed: bool,
    pub(super) live: bool,
    pub(super) stop: Arc<AtomicBool>,
    pub(super) beat: Option<std::thread::JoinHandle<()>>,
    /// What each box was last sent.
    pub(super) sent: [Sent; MAX_BOXES],
    /// When the hook's cost was last read, and what it was.
    pub(super) cost_mark: Option<(u64, [u32; 4])>,
}
impl Channel {
    pub(super) fn new(shell: Shell) -> windows::core::Result<Self> {
        let pid = shell.pid;
        unsafe {
            let name = wide(&format!("Local\\LimitPeekerXaml_v1_{pid}"));
            let mapping = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                (WORDS * 4) as u32,
                PCWSTR(name.as_ptr()),
            )?;
            let existing = GetLastError() == ERROR_ALREADY_EXISTS;
            let view = MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, WORDS * 4);
            if view.Value.is_null() {
                let _ = CloseHandle(mapping);
                return Err(windows::core::Error::from_thread());
            }
            let mut resumed = false;
            if existing {
                // Reuse an abandoned session instead of staying tray-only until
                // Explorer restarts. Only once the hook detached its UI (4) or
                // never started one (0), and never a session a live controller
                // owns: the session mutex makes any other recorded controller
                // stale. Read before constructing, so a refusal cannot run Drop
                // against a session this process does not own.
                let word = |index: usize| {
                    (*view.Value.cast::<AtomicU32>().add(index)).load(Ordering::SeqCst)
                };
                let ours = word(0) == MAGIC
                    && word(1) == VERSION
                    && word(CONTROLLER_PID) != GetCurrentProcessId();
                // A resting hook is claimed, with one compare-exchange as its
                // leaving is, so only one of them wins.
                resumed = ours
                    && (*view.Value.cast::<AtomicU32>().add(HOOK_STATUS))
                        .compare_exchange(RESTING, CLAIMED, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok();
                let abandoned = ours && (word(HOOK_STATUS) == 4 || word(HOOK_STATUS) == 0);
                if !resumed && !abandoned {
                    let _ = UnmapViewOfFile(view);
                    let _ = CloseHandle(mapping);
                    return Err(windows::core::Error::new(
                        SESSION_BUSY,
                        "XAML session still in use; retrying before tray-only",
                    ));
                }
            }
            let mut channel = Self {
                mapping,
                view,
                shell,
                reused: existing,
                resumed,
                live: false,
                stop: Arc::new(AtomicBool::new(false)),
                beat: None,
                sent: Default::default(),
                cost_mark: None,
            };
            for index in 0..WORDS {
                // A claimed hook reads its status to start again, and its
                // subscription stays on record.
                if !(resumed && KEPT_ON_CLAIM.contains(&index)) {
                    channel.set(index, 0);
                }
            }
            channel.set(0, MAGIC);
            channel.set(1, VERSION);
            channel.set(CONTROLLER_PID, GetCurrentProcessId());

            channel.set(OBSERVED, shared_now() as u32);
            channel.set(HEARTBEAT, shared_now() as u32);
            // The lease must depend on nothing but this process still running.
            // Driving it from the observation time made a UI Automation call
            // blocked by a busy shell look like a dead controller, and the hook
            // then removed the widget for good.
            let stop = channel.stop.clone();
            let address = view.Value as usize;
            channel.beat = Some(std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    (*(address as *const AtomicU32).add(HEARTBEAT))
                        .store(shared_now() as u32, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(HEARTBEAT_MS));
                }
            }));
            Ok(channel)
        }
    }
    pub(super) fn word(&self, index: usize) -> &AtomicU32 {
        assert!(index < WORDS);
        unsafe { &*(self.view.Value.cast::<AtomicU32>().add(index)) }
    }
    pub(super) fn set(&self, index: usize, value: u32) {
        self.word(index).store(value, Ordering::SeqCst);
    }
    pub(super) fn get(&self, index: usize) -> u32 {
        self.word(index).load(Ordering::SeqCst)
    }
    /// The hook's cost since the last reading, once COST_EVERY has passed or
    /// when `now` is the end; the first call only marks.
    pub(super) fn cost(&mut self, now: u64, end: bool) -> Option<String> {
        let words = [TICKS, TICK_US, WALK_US, VISITED].map(|w| self.get(w));
        let Some((at, from)) = self.cost_mark else {
            self.cost_mark = Some((now, words));
            return None;
        };
        if !end && now.saturating_sub(at) < COST_EVERY {
            return None;
        }
        self.cost_mark = Some((now, words));
        Some(cost_line(from, words, now.saturating_sub(at)))
    }
    /// Takes back this session's hook that took the box off and rests, as a
    /// run started again claims one: the same compare-exchange, 7 to 8. A
    /// session taken back counts as one claimed, so a hook that then leaves
    /// without showing the box is let go and attached anew. Whether it was
    /// taken.
    pub(super) fn reclaim(&mut self) -> bool {
        let taken = self
            .word(HOOK_STATUS)
            .compare_exchange(RESTING, CLAIMED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();
        if taken {
            self.resumed = true;
            self.live = false;
        }
        taken
    }
    /// What the controller wants on screen; the hook decides where.
    pub(super) fn request(&self, mode: DisplayMode) {
        self.set(REQUEST, mode as u32);
    }
    /// Draws the label's icons for the taskbar's DPI and sends them to a
    /// box's words under their own odd/even sequence, before the label that
    /// shows them. Only when the side, a service, a color or an icon's file
    /// changed.
    pub(super) fn show_icons(
        &mut self,
        panel: Panel,
        label: &Label,
        dir: &std::path::Path,
        file: &Log,
    ) {
        let services = label.cells.iter().map(|c| (c.id, c.color)).collect();
        self.send_icons(panel, label.style.icon, services, dir, file);
    }
    /// The same for a scene: a slot's service, at its tallest icon block.
    pub(super) fn show_scene_icons(
        &mut self,
        panel: Panel,
        scene: &Scene,
        dir: &std::path::Path,
        file: &Log,
    ) {
        self.send_icons(panel, scene.icon_size(), scene.icons.clone(), dir, file);
    }
    pub(super) fn send_icons(
        &mut self,
        panel: Panel,
        size: u32,
        services: Vec<(&'static str, u32)>,
        dir: &std::path::Path,
        file: &Log,
    ) {
        if size == 0 {
            return;
        }
        let dpi = match self.get(MEASURE_DPI) {
            dpi @ 96..=480 => dpi,
            _ => 96,
        };
        let side = icons::side(size, dpi);
        let files = services
            .iter()
            .map(|&(id, _)| icons::stamp(dir, id))
            .collect();
        let key = (side, services, files);
        if self.sent[panel.0].icons.as_ref() == Some(&key) {
            return;
        }
        let (drawn, sources): (Vec<Icon>, Vec<String>) = key
            .1
            .iter()
            .map(|&(id, color)| {
                let (icon, source) = icons::load(dir, id, side, color);
                (icon, format!("{id}={source}"))
            })
            .unzip();
        let base = panel.base();
        let seq = self.get(base + ICON_SEQ);
        self.set(base + ICON_SEQ, seq.wrapping_add(1));
        for index in 0..MAX_CELLS {
            let icon = drawn.get(index);
            self.set(base + ICON_SIZES + index, icon.map_or(0, |i| i.size));
            for (at, &pixel) in icon.iter().flat_map(|i| i.pixels.iter()).enumerate() {
                self.set(base + ICON_PIXELS + index * ICON_BLOCK + at, pixel);
            }
        }
        self.set(base + ICON_SEQ, seq.wrapping_add(2));
        log(
            file,
            format!(
                "{} icons px={side} dpi={dpi} {}",
                box_name(panel.0 as u32),
                sources.join(" ")
            ),
        );
        self.sent[panel.0].icons = Some(key);
    }
    /// Sends the label to a box's words under an odd/even sequence so the
    /// hook never takes a half-written one. One that does not fit is not
    /// sent, and the hook keeps what it has.
    pub(super) fn show(&mut self, panel: Panel, label: &Label) {
        let sent = &self.sent[panel.0];
        if sent.label.as_ref() == Some(label) || !label.fits() {
            return;
        }
        let base = panel.base();
        let pack = |at: usize, text: &str| {
            let units: Vec<u16> = text.encode_utf16().collect();
            for (index, pair) in units.chunks(2).enumerate() {
                let high = pair.get(1).copied().unwrap_or(0);
                self.set(
                    base + at + index,
                    u32::from(pair[0]) | u32::from(high) << 16,
                );
            }
            units.len() as u32
        };
        let seq = self.get(base + TEXT_SEQ);
        self.set(base + TEXT_SEQ, seq.wrapping_add(1));
        self.set(base + SCENE_MODE, 0);
        self.set(base + TEXT_LENGTH, pack(TEXT_CHARS, &label.text()));
        self.set(base + SIZING_LENGTH, pack(SIZING_CHARS, &label.sizing()));
        let gauges: Vec<_> = label.cells.iter().flat_map(|c| &c.gauges).collect();
        for index in 0..MAX_GAUGES {
            let gauge = gauges.get(index);
            let fill = gauge.and_then(|g| g.fill);
            self.set(base + GAUGE_FILLS + index, fill.map_or(u32::MAX, u32::from));
            self.set(
                base + GAUGE_TEXT_COLORS + index,
                gauge.map_or(0, |g| g.text_color),
            );
            self.set(
                base + GAUGE_FILL_COLORS + index,
                gauge.map_or(0, |g| g.fill_color),
            );
            self.set(
                base + GAUGE_ENDS + index,
                gauge.map_or(0, |g| u32::from(g.end)),
            );
        }
        for (index, value) in label.style.words().into_iter().enumerate() {
            self.set(base + STYLE_BOX + index, value);
        }
        self.set(base + TEXT_SEQ, seq.wrapping_add(2));
        let sent = &mut self.sent[panel.0];
        sent.scene = None;
        sent.label = Some(label.clone());
    }
    /// Sends a scene in place of the label, under the label's sequence, with
    /// the look the box takes its color, size and motion from. One that does
    /// not fit is not sent, and the hook keeps what it has.
    pub(super) fn show_scene(&mut self, panel: Panel, scene: &Scene) {
        let sent = &self.sent[panel.0];
        if sent.scene.as_ref() == Some(scene) || !scene.fits() {
            return;
        }
        let base = panel.base();
        let (words, pool) = scene.words();
        let seq = self.get(base + TEXT_SEQ);
        self.set(base + TEXT_SEQ, seq.wrapping_add(1));
        self.set(base + SCENE_MODE, 1);
        self.set(base + SCENE_WIDTH, words[0]);
        self.set(base + SCENE_COUNT, words[1]);
        self.set(base + SCENE_POOL_LENGTH, words[2]);
        for (index, &value) in words[3..].iter().enumerate() {
            self.set(base + SCENE_ITEMS + index, value);
        }
        for (index, &value) in pool.iter().enumerate() {
            self.set(base + SCENE_POOL + index, value);
        }
        for (index, value) in scene.style.words().into_iter().enumerate() {
            self.set(base + STYLE_BOX + index, value);
        }
        self.set(base + TEXT_SEQ, seq.wrapping_add(2));
        let sent = &mut self.sent[panel.0];
        sent.label = None;
        sent.scene = Some(scene.clone());
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        // Stop the heartbeat before the view is unmapped.
        self.stop.store(true, Ordering::SeqCst);
        if let Some(beat) = self.beat.take() {
            let _ = beat.join();
        }
        self.set(REQUEST, 0);
        self.set(CONTROLLER_PID, 0); // Permanent session stop, not just placement uncertainty.
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.mapping);
        }
    }
}
