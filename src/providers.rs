//! Where the real figures come from. A thread per provider reads its service
//! now and then, or at once when asked, and keeps what it last read; the
//! controller takes a copy on each tick, so a slow provider never holds up
//! the taskbar.

use crate::choice::Choice;
use crate::job::Cost;
use crate::log::{Log, log};
use crate::usage::{Limit, Meaning, Reading, Service, Trouble, unix_now};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long after a good read the next one is, for most providers.
const EVERY: Duration = Duration::from_secs(5 * 60);
/// How long after a used-up limit starts over it is read.
const AFTER_RESET: Duration = Duration::from_secs(30);
/// Grok's: each read starts the CLI, which logs and unpacks its own files
/// in the person's folder, and its credit moves by the week or month.
const GROK_EVERY: Duration = Duration::from_secs(15 * 60);
/// How long after a failed read the next try is: longer each time, then the
/// last one on.
const RETRIES: [Duration; 5] = [
    Duration::from_secs(60),
    Duration::from_secs(2 * 60),
    Duration::from_secs(4 * 60),
    Duration::from_secs(8 * 60),
    Duration::from_secs(15 * 60),
];
/// How long before looking again for a provider not found.
const LOOK_AGAIN: Duration = Duration::from_secs(15 * 60);
/// How often a provider read only while its service shows looks whether it
/// does, and how soon before the controller has offered its service, as
/// when it has only just started.
const LOOK_SHOWN: Duration = Duration::from_secs(1);
const LOOK_OFFERED: Duration = Duration::from_millis(250);
/// How long after one request to read at once the next is taken.
const COOLDOWN: Duration = Duration::from_secs(10);
/// How long after the person signs in Windows stays too busy for a read to
/// come in time: the first reads wait until then.
const SETTLE_AFTER_SIGN_IN: Duration = Duration::from_secs(90);

/// The services read so far, in the providers' order. A service whose
/// provider is not on this computer is not in it.
pub struct Providers {
    services: Arc<Mutex<Vec<Service>>>,
    // One for each provider's thread: a message has it read at once, and
    // dropped they stop it; a read under way ends on its own.
    orders: Vec<mpsc::Sender<()>>,
    /// Dropped, it stops the thread measuring this computer.
    _meters: mpsc::Sender<()>,
    asked: Mutex<Cooldown>,
    /// When the providers started, when reading at once is first taken.
    began: Instant,
}

impl Providers {
    pub fn start(file: Log, choice: Arc<Choice>) -> Self {
        let services = Arc::new(Mutex::new(Vec::new()));
        let mut orders = Vec::new();
        // Just after sign-in the first reads would not come in time, so
        // they wait for Windows to settle.
        let settle = since_sign_in().and_then(|since| {
            let wait = settle_wait(since)?;
            log(
                &file,
                format!(
                    "reads wait {} s: signed in {} s ago, and Windows is still busy then",
                    wait.as_secs(),
                    since.as_secs()
                ),
            );
            Some(Instant::now() + wait)
        });
        for &provider in &PROVIDERS {
            let (order, taken) = mpsc::channel();
            orders.push(order);
            let (services, file, choice) = (services.clone(), file.clone(), choice.clone());
            std::thread::spawn(move || run(provider, &services, &taken, &file, &choice, settle));
        }
        // This computer's meters, listed last as the service PC; they need
        // no wait after sign-in.
        let (meters, stop) = mpsc::channel();
        {
            let (services, file, choice) = (services.clone(), file.clone(), choice.clone());
            std::thread::spawn(move || {
                crate::resources::run(&stop, &file, &choice, |pc| set(&services, pc.id, Some(pc)))
            });
        }
        Self {
            services,
            orders,
            _meters: meters,
            asked: Mutex::new(Cooldown::default()),
            began: Instant::now(),
        }
    }
    pub fn services(&self) -> Vec<Service> {
        self.services.lock().map(|s| s.clone()).unwrap_or_default()
    }
    /// When a request to read at once is taken again: COOLDOWN after the
    /// last, or when the providers started. It stays put, so the popup's
    /// view changes only when it does.
    pub fn ready_at(&self) -> Instant {
        self.asked
            .lock()
            .ok()
            .and_then(|asked| asked.last)
            .map_or(self.began, |last| last + COOLDOWN)
    }
    /// When, in Unix milliseconds, a request to read at once is taken
    /// again; 0 before the first. The settings window counts down to it.
    pub fn read_now_at(&self) -> u64 {
        self.asked.lock().map_or(0, |asked| asked.next_ms)
    }
    /// Has every provider read at once, unless the last such request was
    /// under COOLDOWN ago; whether it was taken.
    pub fn read_now(&self) -> bool {
        let wall_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let taken = self
            .asked
            .lock()
            .is_ok_and(|mut asked| asked.take(Instant::now(), wall_ms));
        if taken {
            for order in &self.orders {
                let _ = order.send(());
            }
        }
        taken
    }
}

/// Whether reading at once can be asked for: now, after a wait, or not at
/// all, as with the mock figures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReadNow {
    Ready,
    After(Duration),
    Never,
}
impl ReadNow {
    /// The popup's item: with the seconds left, rounded up, while it
    /// waits, so a greyed item is not taken for a fault.
    pub fn title(&self) -> String {
        match self {
            ReadNow::After(left) => {
                format!("Refresh now (in {} s)", left.as_millis().div_ceil(1000))
            }
            ReadNow::Ready | ReadNow::Never => "Refresh now".into(),
        }
    }
}

/// When a request to read at once was last taken, and when the next will
/// be by the wall clock, in Unix milliseconds, for the settings window.
#[derive(Default)]
struct Cooldown {
    last: Option<Instant>,
    next_ms: u64,
}
impl Cooldown {
    /// How long until a request is taken again; None when it would be now.
    fn left(&self, now: Instant) -> Option<Duration> {
        let waited = now.saturating_duration_since(self.last?);
        COOLDOWN.checked_sub(waited).filter(|left| !left.is_zero())
    }
    fn open(&self, now: Instant) -> bool {
        self.left(now).is_none()
    }
    fn take(&mut self, now: Instant, wall_ms: u64) -> bool {
        let open = self.open(now);
        if open {
            self.last = Some(now);
            self.next_ms = wall_ms + COOLDOWN.as_millis() as u64;
        }
        open
    }
}

/// A provider: its service, and how it is found and read. Each is read only
/// while its service shows, as each read starts a program.
struct Provider {
    id: &'static str,
    name: &'static str,
    color: u32,
    /// What the log calls it.
    tag: &'static str,
    find: fn() -> Option<PathBuf>,
    /// What the log says when it is not found.
    missing: &'static str,
    read: fn(&Path) -> (Result<(Vec<Limit>, String), String>, Cost),
    /// How long after a good read the next one is.
    every: Duration,
    /// The CLI's own update command as its reads run it, run apart from
    /// them every 15 minutes; none for a CLI that does not update itself.
    update: Option<crate::cli_update::Prepared>,
    /// Why a copy of the CLI is not updated here, as the log says it, or
    /// None when its update command updates it.
    kept_by: fn(&Path) -> Option<&'static str>,
}

/// The providers, in the order their services are listed until the person
/// picks one, whichever is found first.
static PROVIDERS: [&Provider; 4] = [&CODEX, &CLAUDE, &GEMINI, &GROK];

/// Codex, as the GPT service: its icon and saved picks stay.
static CODEX: Provider = Provider {
    id: "gpt",
    name: "Codex",
    color: 0xFFF2_F2F2,
    tag: "codex",
    find: crate::codex::find,
    missing: "not found on the PATH (codex.exe or npm's codex.cmd), in the Codex app's, its installer's or npm's folder, or in VS Code's extensions",
    read: |exe| {
        let (read, cost) = crate::codex::read(exe);
        (read.map(|found| (found.limits, found.told)), cost)
    },
    every: EVERY,
    update: Some(crate::codex::command),
    kept_by: crate::codex::kept_by,
};

/// Claude, from a Claude Code started for each read and asked `get_usage`,
/// or its hidden console's `/usage` when that fails: read only while it
/// shows, since a read costs seconds of CPU.
static CLAUDE: Provider = Provider {
    id: "claude",
    name: "Claude",
    color: 0xFFF4_A27A,
    tag: "claude",
    find: crate::claude_home::find,
    missing: "Claude Code was not found on the PATH (claude.exe or npm's claude.cmd), in ~\\.local\\bin, in npm's folder or in VS Code's extensions",
    read: |exe| {
        let (read, cost) = crate::claude_usage::read(exe);
        (read.map(|found| (found.limits, found.told)), cost)
    },
    every: EVERY,
    update: Some(crate::claude_home::command),
    kept_by: crate::cli_update::kept_by_editor,
};

/// Gemini, from a hidden Antigravity CLI started in a home folder of this
/// program's own for each read, its status line's quota heard: read only
/// while it shows, since a read costs seconds and a process. Named for the
/// tool that gives the figures, as Codex is.
static GEMINI: Provider = Provider {
    id: "gemini",
    name: "Antigravity",
    color: 0xFF7A_A7FF,
    tag: "agy",
    find: crate::agy::find,
    missing: "the Antigravity CLI (agy) was not found on the PATH or in %LOCALAPPDATA%\\agy\\bin",
    read: |exe| {
        let (read, cost) = crate::agy_session::read(exe);
        (read.map(|found| (found.limits, found.told)), cost)
    },
    every: EVERY,
    update: Some(crate::agy_session::command),
    kept_by: crate::cli_update::kept_by_editor,
};

/// Grok, from the Grok CLI started for each read, in a home of its own, and
/// asked its billing: a free account is told its period but no usage, which
/// shows as `--`. Read only while it shows, and less often.
static GROK: Provider = Provider {
    id: "grok",
    name: "Grok",
    color: 0xFFCF_CFCF,
    tag: "grok",
    find: crate::grok::find,
    missing: "the Grok CLI was not found on the PATH or in ~\\.grok\\bin",
    read: |exe| {
        let (read, cost) = crate::grok::read(exe);
        (read.map(|found| (found.limits, found.told)), cost)
    },
    every: GROK_EVERY,
    update: Some(crate::grok::command),
    kept_by: crate::cli_update::kept_by_editor,
};

/// Reads a provider's service until the providers stop: after EVERY, after
/// RETRIES when it failed, and at once when asked. The first read waits for
/// `settle`, unless asked at once.
fn run(
    provider: &Provider,
    services: &Mutex<Vec<Service>>,
    orders: &mpsc::Receiver<()>,
    file: &Log,
    choice: &Choice,
    mut settle: Option<Instant>,
) {
    let (id, name, color, tag) = (provider.id, provider.name, provider.color, provider.tag);
    let mut kept: Option<Vec<Limit>> = None;
    let mut failures = 0;
    let mut place: Option<PathBuf> = None;
    let mut was_shown: Option<bool> = None;
    let mut looked = Instant::now();
    let mut held_back: Option<String> = None;
    loop {
        // A place found is kept while it is there, for LOOK_AGAIN, so a
        // slow PATH is not walked each second.
        let found = match &place {
            Some(exe) if exe.is_file() && looked.elapsed() < LOOK_AGAIN => place.clone(),
            _ => {
                looked = Instant::now();
                (provider.find)()
            }
        };
        if found != place {
            match &found {
                Some(exe) => log(file, format!("{tag}: found {}", exe.display())),
                None => log(
                    file,
                    format!("{tag}: {}; looking again in 15 min", provider.missing),
                ),
            }
            place = found.clone();
        }
        let wait = match found {
            None => {
                set(services, id, None);
                LOOK_AGAIN
            }
            Some(exe) => {
                // Until the first read, the cell shows it is waiting.
                if kept.is_none() && failures == 0 {
                    set(services, id, Some(service(id, name, color, None)));
                }
                let shown = shows(choice, id);
                if let Some(now) = shown
                    && was_shown != shown
                {
                    log(
                        file,
                        format!(
                            "{tag}: {}",
                            if now {
                                "shown; reading"
                            } else {
                                "not shown; not read until it is"
                            }
                        ),
                    );
                }
                if shown.is_some() {
                    was_shown = shown;
                }
                if shown.is_none() {
                    LOOK_OFFERED
                } else if shown == Some(false) {
                    LOOK_SHOWN
                } else if let Some(wait) = settle
                    .take()
                    .and_then(|until| until.checked_duration_since(Instant::now()))
                {
                    wait
                } else {
                    let (read, cost) = guarded(|| (provider.read)(&exe));
                    // What each read cost, so the interval can be judged.
                    let spent = format!(
                        "{:.0} ms, cpu {:.0} ms, peak {:.0} MB",
                        cost.wall_ms,
                        cost.cpu_ms(),
                        cost.peak_mb
                    );
                    let next = match read {
                        Ok((limits, told)) => {
                            log(file, format!("{tag}: read {told} in {spent}"));
                            set(
                                services,
                                id,
                                Some(service(id, name, color, Some(limits.clone()))),
                            );
                            let next = next_read(&limits, provider.every, unix_now());
                            kept = Some(limits);
                            failures = 0;
                            next
                        }
                        Err(why) => {
                            let retry = RETRIES[failures.min(RETRIES.len() - 1)];
                            failures += 1;
                            log(
                                file,
                                format!(
                                    "{tag}: read failed after {spent}: {why}; next try in {} s",
                                    retry.as_secs()
                                ),
                            );
                            // Why, and when it is tried again, for the popup.
                            let mut failing =
                                service(id, name, color, Some(failed(kept.as_deref())));
                            failing.trouble = Some(Trouble {
                                why,
                                retry_at: unix_now() + retry.as_secs() as i64,
                            });
                            set(services, id, Some(failing));
                            retry
                        }
                    };
                    // After the read, so a first value does not wait for
                    // a download; the wait to the next read takes it in.
                    let began = Instant::now();
                    update(provider, &exe, choice.updates(), file, &mut held_back);
                    next.saturating_sub(began.elapsed())
                }
            }
        };
        if !rest(orders, wait) {
            return;
        }
    }
}

/// How long the first reads wait when the product starts `since_sign_in`
/// after the person signed in: until SETTLE_AFTER_SIGN_IN, or not at all.
fn settle_wait(since_sign_in: Duration) -> Option<Duration> {
    SETTLE_AFTER_SIGN_IN
        .checked_sub(since_sign_in)
        .filter(|wait| !wait.is_zero())
}

/// How long ago the person signed in to this Windows session; None when
/// Windows cannot say.
fn since_sign_in() -> Option<Duration> {
    use windows::Win32::System::RemoteDesktop::*;
    unsafe {
        let mut buffer = windows::core::PWSTR::null();
        let mut bytes = 0u32;
        WTSQuerySessionInformationW(
            None,
            WTS_CURRENT_SESSION,
            WTSSessionInfo,
            &mut buffer,
            &mut bytes,
        )
        .ok()?;
        let info = (bytes as usize >= size_of::<WTSINFOW>())
            .then(|| std::ptr::read_unaligned(buffer.0 as *const WTSINFOW));
        WTSFreeMemory(buffer.0 as _);
        let info = info?;
        // Both are FILETIMEs, in 100 ns.
        let since = info.CurrentTime.checked_sub(info.LogonTime)?;
        (info.LogonTime > 0 && since >= 0).then(|| Duration::from_nanos(since as u64 * 100))
    }
}

/// Whether the service shows in a box now; None until the controller has
/// offered it, when that is not known yet.
fn shows(choice: &Choice, id: &str) -> Option<bool> {
    let offers = choice.offers();
    // In any box, or in the popup's summary alone.
    offers.iter().any(|o| o.id == id).then(|| {
        choice.all_services(&offers).contains(&id) || choice.summary(&offers).contains(&id)
    })
}

/// When to read again after a good read: after `every`, or sooner, a little
/// after a used-up limit starts over, so that its being free again is told
/// then.
fn next_read(limits: &[Limit], every: Duration, now: i64) -> Duration {
    limits
        .iter()
        .filter(|limit| matches!(limit.reading, Reading::Read(used) if used >= 100))
        .filter_map(|limit| limit.resets)
        .filter(|&at| at > now)
        .map(|at| Duration::from_secs((at - now) as u64) + AFTER_RESET)
        .fold(every, Duration::min)
}

/// Runs the CLI's own update at `exe` when it is due, unless the updates
/// are turned off, the person uses that CLI now or something else updates
/// that copy, such as an editor. Why one is not run is told once, kept in
/// `held_back`.
fn update(provider: &Provider, exe: &Path, on: bool, file: &Log, held_back: &mut Option<String>) {
    use crate::cli_update;
    let tag = provider.tag;
    let Some(prepared) = provider.update else {
        return;
    };
    if on && !cli_update::due(tag) {
        return;
    }
    let note = if !on {
        Some(format!(
            "{tag}: not updated here; tool updates are turned off in the settings"
        ))
    } else if let Some(why) = (provider.kept_by)(exe) {
        Some(format!("{tag}: not updated here; {} {why}", exe.display()))
    } else if cli_update::in_use(exe) {
        Some(format!(
            "{tag}: update put off while {} runs outside this program",
            exe.display()
        ))
    } else {
        // A panic in it is taken as a failed update, so that the thread
        // goes on reading.
        let said = std::panic::catch_unwind(|| cli_update::run(tag, exe, prepared))
            .unwrap_or_else(|_| "update panicked".into());
        log(file, format!("{tag}: {said}"));
        None
    };
    if *held_back != note {
        if let Some(note) = &note {
            log(file, note.clone());
        }
        *held_back = note;
    }
}

/// A read, with a panic in it taken as a failed read, so that the thread
/// goes on reading.
fn guarded<T>(read: impl FnOnce() -> (Result<T, String>, Cost)) -> (Result<T, String>, Cost) {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(read))
        .unwrap_or_else(|_| (Err("the read panicked".into()), Cost::default()))
}

/// Waits `span`, or less when asked to read at once; false once the
/// providers have stopped. What was asked during the read just made is
/// taken as answered by it.
fn rest(orders: &mpsc::Receiver<()>, span: Duration) -> bool {
    loop {
        match orders.try_recv() {
            Ok(()) => continue,
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => return false,
        }
    }
    !matches!(
        orders.recv_timeout(span),
        Err(mpsc::RecvTimeoutError::Disconnected)
    )
}

fn service(id: &'static str, name: &str, color: u32, limits: Option<Vec<Limit>>) -> Service {
    Service {
        id,
        name: name.into(),
        color,
        limits,
        trouble: None,
        meaning: Meaning::Limit,
    }
}

/// The limits after a failed read: each one read before, with its value and
/// when it starts over kept and shown faded, or, with none read yet, one
/// limit known to have failed.
fn failed(kept: Option<&[Limit]>) -> Vec<Limit> {
    match kept {
        Some(limits) => limits
            .iter()
            .map(|l| Limit {
                label: l.label.clone(),
                reading: match l.reading {
                    Reading::Read(used) | Reading::Failed(Some(used)) => {
                        Reading::Failed(Some(used))
                    }
                    Reading::Pending | Reading::Failed(None) => Reading::Failed(None),
                },
                resets: l.resets,
                amount: None,
            })
            .collect(),
        None => vec![Limit {
            label: String::new(),
            reading: Reading::Failed(None),
            resets: None,
            amount: None,
        }],
    }
}

/// Puts a service in the list, in its provider's place, or takes it out.
fn set(services: &Mutex<Vec<Service>>, id: &str, service: Option<Service>) {
    if let Ok(mut services) = services.lock() {
        match (services.iter().position(|s| s.id == id), service) {
            (Some(at), Some(service)) => services[at] = service,
            (Some(at), None) => {
                services.remove(at);
            }
            (None, Some(service)) => {
                let rank = |id: &str| {
                    PROVIDERS
                        .iter()
                        .position(|p| p.id == id)
                        .unwrap_or(usize::MAX)
                };
                let at = services
                    .iter()
                    .position(|s| rank(s.id) > rank(service.id))
                    .unwrap_or(services.len());
                services.insert(at, service);
            }
            (None, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_used_up_limit_is_read_again_just_after_it_starts_over() {
        // Sooner than the usual read only for a used-up limit whose reset
        // is known and ahead.
        let limit = |used: u8, resets: Option<i64>| Limit {
            label: "5h".into(),
            reading: Reading::Read(used),
            resets,
            amount: None,
        };
        let now = 1_000_000;
        assert_eq!(next_read(&[limit(40, Some(now + 60))], EVERY, now), EVERY);
        assert_eq!(
            next_read(&[limit(40, None), limit(100, Some(now + 60))], EVERY, now),
            Duration::from_secs(90)
        );
        assert_eq!(
            next_read(&[limit(100, Some(now + 3600))], EVERY, now),
            EVERY
        );
        assert_eq!(next_read(&[limit(100, Some(now - 5))], EVERY, now), EVERY);
        assert_eq!(next_read(&[limit(100, None)], EVERY, now), EVERY);
    }

    #[test]
    fn a_failed_read_keeps_what_was_read_faded() {
        // The values read before stay, faded, with when each starts over,
        // and a service never read shows one failed limit.
        let kept = [
            Limit {
                label: "5h".into(),
                reading: Reading::Read(28),
                resets: Some(1_790_000_000),
                amount: None,
            },
            Limit {
                label: "7d".into(),
                reading: Reading::Failed(Some(42)),
                resets: None,
                amount: None,
            },
        ];
        let after: Vec<_> = failed(Some(&kept))
            .into_iter()
            .map(|l| (l.label, l.reading, l.resets))
            .collect();
        assert_eq!(
            after,
            [
                (
                    "5h".to_string(),
                    Reading::Failed(Some(28)),
                    Some(1_790_000_000)
                ),
                ("7d".to_string(), Reading::Failed(Some(42)), None)
            ]
        );
        let never = failed(None);
        assert_eq!(
            (never[0].label.as_str(), never[0].reading),
            ("", Reading::Failed(None))
        );
        // Retries grow and then hold.
        assert!(RETRIES.windows(2).all(|w| w[0] < w[1]));
        assert!(EVERY < RETRIES[RETRIES.len() - 1]);
    }

    #[test]
    fn whether_a_service_shows_is_not_known_until_it_is_offered() {
        // Just started, the controller has offered nothing yet: Claude is
        // looked at again soon, not taken as hidden.
        use crate::style::{self, Background, Fill};
        let choice = Choice::new(style::DEFAULT, Fill::Left, Background::Box);
        assert_eq!(shows(&choice, "claude"), None);
        choice.offer(&[
            service("gpt", "Codex", 0, None),
            service("claude", "Claude", 0, None),
        ]);
        assert_eq!(shows(&choice, "claude"), Some(true));
        let offers = choice.offers();
        choice.toggle_service(&offers, "claude");
        assert_eq!(shows(&choice, "claude"), Some(false));
        // In the popup's summary alone, it is read too.
        let offers = choice.offers();
        choice.toggle_summary_service(&offers, "claude");
        assert_eq!(choice.services(&offers), ["gpt"]);
        assert_eq!(shows(&choice, "claude"), Some(true));
        // Turned on again, it is read within a second.
        assert!(LOOK_OFFERED < LOOK_SHOWN && LOOK_SHOWN <= Duration::from_secs(1));
    }

    #[test]
    fn a_service_comes_and_goes_in_its_place() {
        let list = Mutex::new(Vec::new());
        let named = |id: &'static str| service(id, id, 0, None);
        set(&list, "gpt", Some(named("gpt")));
        set(&list, "claude", Some(named("claude")));
        set(&list, "gpt", Some(service("gpt", "Codex", 0, Some(vec![]))));
        let ids = |list: &Mutex<Vec<Service>>| {
            list.lock()
                .unwrap()
                .iter()
                .map(|s| (s.id, s.name.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(&list),
            [
                ("gpt", "Codex".to_string()),
                ("claude", "claude".to_string())
            ]
        );
        set(&list, "gpt", None);
        set(&list, "copilot", None);
        assert_eq!(ids(&list), [("claude", "claude".to_string())]);
        // Whichever is found first, the providers' order holds, Codex,
        // Claude, Gemini, then Grok, and one no provider has goes last.
        set(&list, "copilot", Some(named("copilot")));
        set(&list, "grok", Some(named("grok")));
        set(&list, "gemini", Some(named("gemini")));
        set(&list, "gpt", Some(named("gpt")));
        let order: Vec<_> = ids(&list).into_iter().map(|(id, _)| id).collect();
        assert_eq!(order, ["gpt", "claude", "gemini", "grok", "copilot"]);
    }

    #[test]
    fn reading_at_once_waits_ten_seconds_between_requests() {
        // The refresh button's cooldown, and when by the wall clock the
        // next request is taken, which only a request taken moves.
        let start = Instant::now();
        let mut asked = Cooldown::default();
        assert_eq!(asked.next_ms, 0);
        assert!(asked.open(start) && asked.take(start, 1_000));
        assert_eq!(asked.next_ms, 11_000);
        assert!(!asked.take(start + Duration::from_secs(9), 10_000));
        assert_eq!(asked.next_ms, 11_000);
        assert!(!asked.open(start + Duration::from_millis(9_999)));
        assert!(asked.take(start + COOLDOWN, 11_000));
        assert_eq!(asked.next_ms, 21_000);
        assert!(!asked.take(start + COOLDOWN + Duration::from_secs(1), 12_000));
        // A clock read before the last request does not open it.
        assert!(!asked.open(start));
        // What is left shows in the popup, in whole seconds rounded up.
        let last = start + COOLDOWN;
        assert_eq!(asked.left(last), Some(COOLDOWN));
        assert_eq!(
            asked.left(last + Duration::from_millis(1_500)),
            Some(Duration::from_millis(8_500))
        );
        assert_eq!(asked.left(last + COOLDOWN), None);
        assert_eq!(
            ReadNow::After(Duration::from_millis(8_500)).title(),
            "Refresh now (in 9 s)"
        );
        assert_eq!(
            ReadNow::After(Duration::from_millis(1)).title(),
            "Refresh now (in 1 s)"
        );
        assert_eq!(ReadNow::Ready.title(), "Refresh now");
        assert_eq!(ReadNow::Never.title(), "Refresh now");
    }

    #[test]
    fn a_request_wakes_a_provider_once_and_a_stop_ends_it() {
        let (order, orders) = mpsc::channel();
        // Asked during the read just made: taken as answered.
        order.send(()).unwrap();
        order.send(()).unwrap();
        let began = Instant::now();
        assert!(rest(&orders, Duration::from_millis(50)));
        assert!(began.elapsed() >= Duration::from_millis(50));
        // Asked while resting: wakes it at once.
        let asker = order.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            let _ = asker.send(());
        });
        let began = Instant::now();
        assert!(rest(&orders, Duration::from_secs(10)));
        assert!(began.elapsed() < Duration::from_secs(5));
        drop(order);
        assert!(!rest(&orders, Duration::from_secs(10)));
        // A panic in a read is a failed read.
        let (read, _) = guarded::<()>(|| panic!("inside a read"));
        assert_eq!(read, Err("the read panicked".to_string()));
    }

    #[test]
    fn the_first_reads_wait_for_windows_to_settle_after_sign_in() {
        // Started 7 s after sign-in, as by a task at sign-in, the first
        // reads wait 83 s; started later, they do not.
        let s = Duration::from_secs;
        assert_eq!(settle_wait(s(0)), Some(s(90)));
        assert_eq!(settle_wait(s(7)), Some(s(83)));
        assert_eq!(settle_wait(s(90)), None);
        assert_eq!(settle_wait(s(3600)), None);
    }

    #[test]
    #[ignore = "asks Windows when this session was signed in to"]
    fn this_session_says_when_it_was_signed_in_to() {
        let since = since_sign_in().expect("Windows says when");
        println!("signed in {} s ago", since.as_secs());
        assert!(since > Duration::ZERO);
    }
}
