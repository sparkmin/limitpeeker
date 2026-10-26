use crate::shell::Shell;

/// A shell that is still starting refuses the diagnostics endpoint for a while.
/// Bound the retries by elapsed time, not by a count: with backoff a count says
/// little about how long the product actually keeps trying. A measured failure
/// exhausted five attempts in 11.8s and stayed tray-only for that Explorer.
const WINDOW: u64 = 60_000;

#[derive(Default)]
pub struct Retry {
    shell: Shell,
    attempts: u32,
    started: u64,
    next: u64,
    finished: bool,
}
impl Retry {
    /// Whether to try `shell` now. Another Explorer, even with the same id,
    /// starts over.
    pub fn ready(&mut self, shell: Shell, now: u64) -> bool {
        if shell.pid == 0 {
            return false;
        }
        if !self.shell.is(&shell) {
            *self = Self {
                shell,
                started: now,
                next: now.saturating_add(1000),
                ..Self::default()
            };
        }
        !self.finished && now >= self.next
    }
    /// Whether it has stopped trying this Explorer: attached, refused, or
    /// out of time.
    pub fn finished(&self) -> bool {
        self.finished
    }
    /// Starts the window over for the same Explorer, as when the wait for
    /// its Snap layouts thread outlasted it.
    pub fn restart(&mut self, now: u64) {
        self.attempts = 0;
        self.started = now;
        self.next = now;
        self.finished = false;
    }
    pub fn complete(&mut self, retryable: bool, now: u64) {
        self.attempts += 1;
        self.finished = !retryable || now.saturating_sub(self.started) >= WINDOW;
        self.next = now.saturating_add(1000 << (self.attempts - 1).min(2));
    }
}

/// How long the hook of an attach that went through has to take its session.
/// The call returns before the hook starts, and the hook takes the session
/// within half a second as a rule; a PC busy with a sign-in gets twenty times
/// that.
pub const START_WAIT: u64 = 10_000;
/// How many attaches one Explorer gets whose hook does not take the session.
/// After the last the box stays off until Explorer restarts, or until that
/// hook takes it after all.
pub const ATTACHES: u32 = 3;

/// What to do about an attach whose hook has not taken its session: wait,
/// attach again, or give up; the last two with the attach's number.
#[derive(Debug, PartialEq)]
pub enum Start {
    Wait,
    Again(u32),
    GiveUp(u32),
}

/// The attaches to one Explorer, and when the last went through while its
/// hook has not yet taken the session.
#[derive(Default)]
pub struct StartWatch {
    shell: Shell,
    attaches: u32,
    since: Option<u64>,
}
impl StartWatch {
    /// An attach to `shell` went through at `now`. Another Explorer, even
    /// with the same id, starts the count over.
    pub fn attached(&mut self, shell: Shell, now: u64) {
        if !self.shell.is(&shell) {
            *self = Self {
                shell,
                ..Self::default()
            };
        }
        self.attaches += 1;
        self.since = Some(now);
    }
    /// The hook's status at `now`. A hook that took the session, whatever
    /// became of it after, is not attached to again. Each attach is answered
    /// once.
    pub fn check(&mut self, status: u32, now: u64) -> Start {
        let Some(since) = self.since else {
            return Start::Wait;
        };
        if status != 0 {
            self.since = None;
            return Start::Wait;
        }
        if now.saturating_sub(since) < START_WAIT {
            return Start::Wait;
        }
        self.since = None;
        if self.attaches < ATTACHES {
            Start::Again(self.attaches)
        } else {
            Start::GiveUp(self.attaches)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const A: Shell = Shell {
        pid: 12,
        started: 1,
    };
    const B: Shell = Shell {
        pid: 13,
        started: 2,
    };
    #[test]
    fn backs_off_and_keeps_trying_for_the_whole_window() {
        let mut r = Retry::default();
        assert!(!r.ready(A, 0));
        assert!(!r.ready(A, 999));
        for now in [1000, 2000, 4000, 8000, 12000, 16000, 20000] {
            assert!(r.ready(A, now));
            r.complete(true, now);
            assert!(!r.ready(A, now));
        }
        // The old five-attempt limit would have given up by now.
        assert!(r.ready(A, 24000));
        r.complete(true, 24000);
        // The window ends it, not the count.
        assert!(r.ready(A, 60000));
        r.complete(true, 60000);
        assert!(!r.ready(A, 999999));
    }
    #[test]
    fn success_or_permanent_failure_stops_until_new_pid() {
        let mut r = Retry::default();
        assert!(!r.ready(Shell::default(), 0));
        assert!(!r.ready(A, 0));
        assert!(r.ready(A, 1000));
        r.complete(false, 1000);
        assert!(!r.ready(A, 999999));
        assert!(!r.ready(B, 1000000));
        assert!(r.ready(B, 1001000));
    }
    #[test]
    fn the_same_id_started_again_is_tried_and_a_long_wait_starts_over() {
        // Windows can give a new Explorer the id of one that ended; its
        // start tells them apart. A wait that outlasted the window starts
        // it again.
        let mut r = Retry::default();
        assert!(!r.ready(A, 0));
        assert!(r.ready(A, 1000));
        r.complete(false, 1000);
        assert!(!r.ready(A, 999_999));
        assert!(!r.ready(Shell { started: 0, ..A }, 999_999));
        let again = Shell { started: 7, ..A };
        assert!(!r.ready(again, 999_999));
        assert!(r.ready(again, 1_000_999));
        let mut r = Retry::default();
        assert!(!r.ready(B, 0));
        r.restart(100_000);
        assert!(r.ready(B, 100_000));
        r.complete(true, 100_000);
        assert!(r.ready(B, 101_000));
    }
    #[test]
    fn a_hook_that_does_not_start_is_attached_again_twice_then_left() {
        // The hook gets ten seconds to take the session after each attach,
        // and one Explorer three attaches.
        let mut w = StartWatch::default();
        assert_eq!(w.check(0, 999_999), Start::Wait);
        w.attached(A, 1000);
        assert_eq!(w.check(0, 1000 + START_WAIT - 1), Start::Wait);
        assert_eq!(w.check(0, 1000 + START_WAIT), Start::Again(1));
        // Answered once: nothing more until the next attach.
        assert_eq!(w.check(0, 999_999), Start::Wait);
        w.attached(A, 20_000);
        assert_eq!(w.check(0, 20_000 + START_WAIT), Start::Again(2));
        w.attached(A, 40_000);
        assert_eq!(w.check(0, 40_000 + START_WAIT), Start::GiveUp(ATTACHES));
        assert_eq!(w.check(0, 999_999), Start::Wait);
        // A new Explorer, even with the same id, counts from one.
        w.attached(Shell { started: 9, ..A }, 60_000);
        assert_eq!(w.check(0, 60_000 + START_WAIT), Start::Again(1));
        w.attached(B, 80_000);
        assert_eq!(w.check(0, 80_000 + START_WAIT), Start::Again(1));
    }
    #[test]
    fn a_hook_that_took_the_session_is_not_attached_to_again() {
        // Shown, hidden, stalled or failed: once the status leaves 0 the
        // hook has the session, and a second one would fight it over the
        // taskbar.
        for status in [1, 2, 3, 4, 5, 6] {
            let mut w = StartWatch::default();
            w.attached(A, 1000);
            assert_eq!(w.check(status, 1500), Start::Wait);
            assert_eq!(w.check(0, 1000 + START_WAIT), Start::Wait, "{status}");
        }
    }
}
