//! What the worker last saw of the hook, and the log lines it writes when
//! that changes: where the box shows, the hook's stage, the padding, the
//! label and icons it took, its measure, why it hid the box, and its
//! status.

use super::*;
use crate::layout::DisplayMode;
use crate::log::{Log, log};

/// What the worker last saw of the hook, so a line is logged only when it
/// changes.
pub(super) struct Seen {
    pub(super) request: DisplayMode,
    pub(super) status: u32,
    pub(super) shown: u32,
    pub(super) hide: u32,
    pub(super) failure: u32,
    pub(super) stage: u32,
    pub(super) reserve: u32,
    pub(super) settling: u32,
    pub(super) taken: (u32, u32, u32, u32, u32),
    pub(super) icons_taken: u32,
    /// The taskbar repeater's margins: the left end's state and width, and
    /// the right one's width.
    pub(super) margins: [u32; 3],
    /// The last drop's sequence and the drags lost.
    pub(super) drop_seq: u32,
    pub(super) drags_lost: u32,
    /// The last drop not yet applied: which box, its place's word, its order
    /// and the box it is beside.
    pub(super) dropped: Option<(u32, u32, u32, u32)>,
    /// Each box: where it was asked for, whether it shows, why not, and its
    /// width.
    pub(super) boxes: [[u32; 4]; MAX_BOXES],
    /// The apps' two ends last logged, and when.
    pub(super) apps: (u32, u32),
    pub(super) apps_at: Option<std::time::Instant>,
    /// The Widgets button's two ends last logged, and when.
    pub(super) widgets: [u32; 4],
    pub(super) widgets_at: Option<std::time::Instant>,
    /// How often the hook's SetSite reached the session, and why it last
    /// turned it down.
    pub(super) site: (u32, u32),
}

impl Seen {
    /// Whether a margin went in, as last seen.
    pub(super) fn margins_used(&self) -> bool {
        self.margins != [0; 3]
    }
    pub(super) fn new() -> Self {
        Self {
            request: DisplayMode::Full,
            status: u32::MAX,
            shown: u32::MAX,
            hide: u32::MAX,
            failure: u32::MAX,
            stage: u32::MAX,
            reserve: u32::MAX,
            settling: 0,
            taken: (u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX),
            icons_taken: u32::MAX,
            margins: [0; 3],
            drop_seq: u32::MAX,
            drags_lost: u32::MAX,
            dropped: None,
            boxes: [[0; 4]; MAX_BOXES],
            apps: (0, 0),
            apps_at: None,
            widgets: [0; 4],
            widgets_at: None,
            site: (0, 0),
        }
    }

    /// A session is let go and another is to come: its status and its
    /// hook's SetSite are logged afresh.
    pub(super) fn new_session(&mut self) {
        self.status = u32::MAX;
        self.site = (0, 0);
    }

    /// Logs what changed in the hook's words since the last turn: where it
    /// shows the box, its stage, the padding, the label it took and drew, the
    /// icons, its measure, why it hid the box, and its status. The hook's own
    /// lines go to `hook_log`, which is the log only with --diagnostics. The
    /// status read here.
    pub(super) fn report(&mut self, c: &Channel, shown: u32, file: &Log, hook_log: &Log) -> u32 {
        if shown != self.shown {
            log(
                file,
                format!(
                    "shown mode={shown} hook_items_right={} hook_tray_left={}",
                    c.get(MEASURE_ITEMS) as i32,
                    c.get(MEASURE_TRAY) as i32
                ),
            );
            self.shown = shown;
        }
        let stage = c.get(ATTACH_STAGE);
        if stage != self.stage {
            log(hook_log, format!("hook stage={}", stage_name(stage)));
            self.stage = stage;
        }
        // Each time a hook reached the session, and whether it took it: a
        // refusal is rare and says why the box did not come.
        let site = (c.get(SITE_CALLS), c.get(SITE_REFUSAL));
        if site != self.site && site.0 != 0 {
            log(
                file,
                format!(
                    "hook site_calls={} refusal={}",
                    site.0,
                    refusal_name(site.1)
                ),
            );
        }
        self.site = site;
        // Whether the padding went in, and whether the box ever
        // waited for the apps to clear the strip.
        let reserve = c.get(RESERVE_STATE);
        let settling = c.get(SETTLING_TICKS);
        if reserve != self.reserve || (settling > 0) != (self.settling > 0) {
            // On a refusal the width word carries its reason.
            let width = c.get(RESERVE_WIDTH);
            let detail = if reserve == 2 {
                format!(
                    "stop_children={} columns={} grid_children={} depth={}",
                    width & 0xff,
                    (width >> 8) & 0xff,
                    (width >> 16) & 0xff,
                    width >> 24
                )
            } else {
                format!("box_px={width}")
            };
            log(
                hook_log,
                format!(
                    "hook reserve={} {detail} settling_ticks={settling} hook_items_right={} hook_tray_left={}",
                    reserve_name(reserve),
                    c.get(MEASURE_ITEMS) as i32,
                    c.get(MEASURE_TRAY) as i32
                ),
            );
            self.reserve = reserve;
            self.settling = settling;
        }
        // Which label the hook took and which it drew, the
        // width of the box drawn and of the strip reserved,
        // and whether a text outgrew its cell.
        let taken = (
            c.get(TEXT_APPLIED),
            c.get(TEXT_DRAWN),
            c.get(FULL_WIDTH),
            c.get(RESERVE_WIDTH),
            c.get(TEXT_CLIPPED),
        );
        if taken != self.taken {
            log(
                hook_log,
                format!(
                    "hook text taken_seq={} drawn_seq={} sent_seq={} box_dip={} strip_px={} text_clipped={}",
                    taken.0,
                    taken.1,
                    c.get(TEXT_SEQ),
                    taken.2,
                    taken.3,
                    taken.4
                ),
            );
            self.taken = taken;
        }
        let icons_taken = c.get(ICON_TAKEN);
        if icons_taken != self.icons_taken {
            log(
                hook_log,
                format!(
                    "hook icons taken_seq={icons_taken} sent_seq={}",
                    c.get(ICON_SEQ)
                ),
            );
            self.icons_taken = icons_taken;
        }
        let failure = c.get(MEASURE_FAILURE);
        if failure != self.failure {
            log(hook_log, format!("hook measure={}", failure_name(failure)));
            self.failure = failure;
        }
        let hide = c.get(HIDE_REASON);
        if hide != self.hide {
            log(
                hook_log,
                format!(
                    "hook hide={} label_measures={} full_dip={}",
                    hide_name(hide),
                    c.get(LABEL_MEASURES),
                    c.get(FULL_WIDTH)
                ),
            );
            self.hide = hide;
        }
        // What became of the taskbar repeater's margins, once one went in:
        // the left end's and the one after the apps.
        let margins = [LEFT_STATE, LEFT_WIDTH, RIGHT_WIDTH].map(|word| c.get(word));
        if margins != self.margins {
            log(file, margin_line(margin_words(c)));
            self.margins = margins;
        }
        // Where each box went, once it was asked for.
        for (index, seen) in self.boxes.iter_mut().enumerate() {
            let words = [BOX_PLACES, BOX_SHOWN, BOX_HIDE, BOX_WIDTH].map(|w| c.get(w + index));
            if words != *seen {
                if words[0] != 0 || seen[0] != 0 {
                    log(file, box_line(index as u32, words));
                }
                *seen = words;
            }
        }
        // Each box dragged on the taskbar: where it was dropped, or that the
        // drag was lost.
        let drop_seq = c.get(DROP_SEQ);
        if drop_seq != self.drop_seq {
            if self.drop_seq != u32::MAX {
                let dropped = (
                    c.get(DROP_BOX),
                    c.get(DROP_PLACE),
                    c.get(DROP_ORDER),
                    c.get(DROP_TARGET),
                );
                log(
                    file,
                    format!(
                        "{} drags={}",
                        drop_line(dropped.0, dropped.1, dropped.2, dropped.3),
                        c.get(DRAGS)
                    ),
                );
                self.dropped = Some(dropped);
            }
            self.drop_seq = drop_seq;
        }
        let drags_lost = c.get(DRAGS_LOST);
        if drags_lost != self.drags_lost {
            if self.drags_lost != u32::MAX {
                log(
                    file,
                    format!("drag lost: drags={} lost={drags_lost}", c.get(DRAGS)),
                );
            }
            self.drags_lost = drags_lost;
        }
        // Where the apps start and end, at most once a second, so a run of
        // the fill tool shows in the log.
        let apps = (c.get(LEFT_ITEMS_START), c.get(MEASURE_ITEMS));
        if apps != self.apps && self.apps_at.is_none_or(|at| at.elapsed().as_secs() >= 1) {
            log(
                hook_log,
                format!("apps start_px={} end_px={}", apps.0 as i32, apps.1 as i32),
            );
            self.apps = apps;
            self.apps_at = Some(std::time::Instant::now());
        }
        // Where the Widgets button is, at most once a second: the places
        // before the Start button and after the last app go by it.
        let widgets = [WIDGETS_LEFT, WIDGETS_RIGHT, SEEN_LEFT, SEEN_RIGHT].map(|word| c.get(word));
        if widgets != self.widgets && self.widgets_at.is_none_or(|at| at.elapsed().as_secs() >= 1) {
            log(file, widgets_line(widgets));
            self.widgets = widgets;
            self.widgets_at = Some(std::time::Instant::now());
        }
        let status = c.get(HOOK_STATUS);
        if status != self.status {
            log(
                file,
                format!(
                    "hook status={status} error={:#x} stage={} advise={}/{} visibility_changes={} hide={} label_measures={} attach_root_width={} full_dip={}",
                    c.get(HOOK_ERROR_CODE),
                    stage_name(c.get(ATTACH_STAGE)),
                    advise_tries(c.get(ADVISE_TRIES)).0,
                    advise_tries(c.get(ADVISE_TRIES)).1,
                    c.get(VISIBILITY_CHANGES),
                    hide_name(c.get(HIDE_REASON)),
                    c.get(LABEL_MEASURES),
                    c.get(ATTACH_ROOT_WIDTH),
                    c.get(FULL_WIDTH)
                ),
            );
            self.status = status;
        }
        status
    }
}
