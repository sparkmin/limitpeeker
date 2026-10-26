//! Whether the tray thread stops taking its messages. The thread marks the
//! time and its step as it goes; a thread of its own looks each second and
//! logs a stop of over 2 s, and when the thread goes on. Nothing else would
//! leave a trace of it: the log tells only of what happens, and Windows kept
//! no dump of a hang.

use crate::log::{Log, log};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use windows::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTime;

/// What the tray thread does: each step that may wait on another process or
/// on a device, and where a message goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Step {
    /// In GetMessage: waiting, or a message sent to a window but the tray's.
    Waiting = 1,
    Refresh,
    TrayDraw,
    TrayIcon,
    SettingsSync,
    PopupUpdate,
    PopupOpen,
    PopupAction,
    SettingsCommand,
    LogonTask,
    /// DispatchMessage, to the popup or the tray window.
    Dispatch,
    /// The tray window's own handling of a message.
    Window,
    /// Deciding and sending notifications.
    Notify,
    /// The loop is over and the run ends: nothing more is looked at.
    Ending,
}
impl Step {
    const ALL: [Step; 14] = [
        Step::Waiting,
        Step::Refresh,
        Step::TrayDraw,
        Step::TrayIcon,
        Step::SettingsSync,
        Step::PopupUpdate,
        Step::PopupOpen,
        Step::PopupAction,
        Step::SettingsCommand,
        Step::LogonTask,
        Step::Dispatch,
        Step::Window,
        Step::Notify,
        Step::Ending,
    ];
    fn told(self, message: u32) -> String {
        match self {
            Step::Waiting => "GetMessage".into(),
            Step::Refresh => "the refresh".into(),
            Step::TrayDraw => "drawing the tray icon".into(),
            Step::TrayIcon => "Shell_NotifyIcon".into(),
            Step::SettingsSync => "the settings window's sync".into(),
            Step::PopupUpdate => "the popup's update".into(),
            Step::PopupOpen => "opening the popup".into(),
            Step::PopupAction => "a popup action".into(),
            Step::SettingsCommand => "a settings window command".into(),
            Step::LogonTask => "looking for sign-in tasks".into(),
            Step::Dispatch => format!("dispatching message 0x{message:04x}"),
            Step::Window => format!("the tray window's message 0x{message:04x}"),
            Step::Notify => "a notification".into(),
            Step::Ending => "ending".into(),
        }
    }
}

/// The time of the last mark in ms, its step and its message, in one word,
/// so the watch never reads a time with another mark's step: the time takes
/// the top 40 bits, the step the next 8, the message the last 16.
static MARK: AtomicU64 = AtomicU64::new(0);
/// A stop longer than this is logged.
const STUCK_MS: u64 = 2_000;
/// How often the watch looks.
const LOOK_MS: u64 = 1_000;

/// Milliseconds that stop while the computer sleeps, so waking is no stop.
fn now() -> u64 {
    let mut ticks = 0u64;
    let _ = unsafe { QueryUnbiasedInterruptTime(&mut ticks) };
    ticks / 10_000
}
fn pack(at: u64, step: Step, message: u32) -> u64 {
    (at << 24) | ((step as u64) << 16) | u64::from(message & 0xFFFF)
}
fn unpack(word: u64) -> (u64, Option<Step>, u32) {
    let step = ((word >> 16) & 0xFF) as usize;
    (
        word >> 24,
        Step::ALL.get(step.wrapping_sub(1)).copied(),
        (word & 0xFFFF) as u32,
    )
}

/// The tray thread is at `step` now.
pub(super) fn mark(step: Step) {
    mark_message(step, 0);
}
/// The tray thread is at `step` now, for `message`.
pub(super) fn mark_message(step: Step, message: u32) {
    MARK.store(pack(now(), step, message), Ordering::Release);
}
/// A step inside another, such as a message the tray window handles while
/// the thread waits in GetMessage: the one before is marked again, at the
/// time it ends, when this is dropped.
pub(super) struct Held(u64);
pub(super) fn hold(step: Step, message: u32) -> Held {
    let before = MARK.load(Ordering::Acquire);
    mark_message(step, message);
    Held(before)
}
impl Drop for Held {
    fn drop(&mut self) {
        let (_, step, message) = unpack(self.0);
        if let Some(step) = step {
            mark_message(step, message);
        }
    }
}

/// Starts the watch, which logs to `file` and lasts until the run ends.
pub(super) fn start(file: Log) {
    mark(Step::Waiting);
    let _ = std::thread::Builder::new()
        .name("tray watch".into())
        .spawn(move || {
            let mut watch = Watch::default();
            loop {
                std::thread::sleep(Duration::from_millis(LOOK_MS));
                match watch.look(now(), MARK.load(Ordering::Acquire)) {
                    Look::Fine => {}
                    Look::Tell(line) => log(&file, line),
                    Look::Ended => break,
                }
            }
        });
}

/// What the watch makes of one look.
#[derive(Debug, PartialEq)]
enum Look {
    Fine,
    Tell(String),
    Ended,
}

/// What the watch keeps between looks.
#[derive(Default)]
struct Watch {
    /// When it last looked, to tell when it woke late itself.
    looked: u64,
    /// Not before this is a stop counted: a late wake means this whole
    /// process did not run, which is no stop of the tray thread.
    floor: u64,
    /// The mark found stuck and told.
    stuck: Option<u64>,
}
impl Watch {
    fn look(&mut self, now: u64, word: u64) -> Look {
        let (at, step, message) = unpack(word);
        if self.looked != 0 && now.saturating_sub(self.looked) > LOOK_MS + STUCK_MS {
            self.floor = now;
        }
        self.looked = now;
        let Some(step) = step.filter(|&s| s != Step::Ending) else {
            return if step == Some(Step::Ending) {
                Look::Ended
            } else {
                Look::Fine
            };
        };
        if let Some(was) = self.stuck {
            if word == was {
                return Look::Fine;
            }
            self.stuck = None;
            let (was_at, was_step, was_message) = unpack(was);
            let told = was_step.map_or("?".into(), |s| s.told(was_message));
            return Look::Tell(format!(
                "ui thread: moving again after {:.1} s at {told}",
                at.saturating_sub(was_at) as f64 / 1000.0
            ));
        }
        let stopped = now.saturating_sub(at.max(self.floor));
        if stopped > STUCK_MS {
            self.stuck = Some(word);
            return Look::Tell(format!(
                "ui thread: stuck {:.1} s at {}",
                stopped as f64 / 1000.0,
                step.told(message)
            ));
        }
        Look::Fine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_keeps_its_time_step_and_message_together() {
        let word = pack(123_456_789, Step::Dispatch, 0x000F);
        assert_eq!(unpack(word), (123_456_789, Some(Step::Dispatch), 0x000F));
        for step in Step::ALL {
            assert_eq!(unpack(pack(1, step, 0)).1, Some(step));
        }
        assert_eq!(unpack(0).1, None);
    }

    #[test]
    fn a_stop_of_over_two_seconds_is_told_once_and_its_end_too() {
        // Fine within 2 s; told once past it, with the step; told again when
        // the thread marks anew, with how long it stood.
        let mut watch = Watch::default();
        let stuck = pack(10_000, Step::TrayIcon, 0);
        assert_eq!(watch.look(11_000, stuck), Look::Fine);
        assert_eq!(watch.look(12_000, stuck), Look::Fine);
        assert_eq!(
            watch.look(13_000, stuck),
            Look::Tell("ui thread: stuck 3.0 s at Shell_NotifyIcon".into())
        );
        assert_eq!(watch.look(14_000, stuck), Look::Fine);
        assert_eq!(watch.look(20_000, stuck), Look::Fine);
        assert_eq!(
            watch.look(21_000, pack(20_500, Step::Waiting, 0)),
            Look::Tell("ui thread: moving again after 10.5 s at Shell_NotifyIcon".into())
        );
        // A message names itself.
        let sent = pack(21_900, Step::Window, 0x004A);
        assert_eq!(watch.look(22_000, sent), Look::Fine);
        assert_eq!(watch.look(23_000, sent), Look::Fine);
        assert_eq!(
            watch.look(24_000, sent),
            Look::Tell("ui thread: stuck 2.1 s at the tray window's message 0x004a".into())
        );
    }

    #[test]
    fn a_late_look_of_its_own_and_the_runs_end_are_no_stop() {
        // The watch itself ran 5 s late: the whole process stood, so the
        // stop counts from then. Past the loop's end it stops looking.
        let mut watch = Watch::default();
        let mark = pack(10_000, Step::Refresh, 0);
        assert_eq!(watch.look(10_500, mark), Look::Fine);
        assert_eq!(watch.look(15_500, mark), Look::Fine);
        assert_eq!(watch.look(16_500, mark), Look::Fine);
        assert_eq!(
            watch.look(18_000, mark),
            Look::Tell("ui thread: stuck 2.5 s at the refresh".into())
        );
        let mut watch = Watch::default();
        assert_eq!(
            watch.look(10_000, pack(1_000, Step::Ending, 0)),
            Look::Ended
        );
        assert_eq!(watch.look(10_000, 0), Look::Fine);
    }
}
