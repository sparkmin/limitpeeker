//! Windows notifications about the limits read: one almost used up, one used
//! up, one available again, and reads that keep failing. Each is told once
//! for a limit's period; what is told is decided here, plainly, and the tray
//! sends it.

use crate::style::Fill;
use crate::usage::{Meaning, Reading, Service};
use std::collections::HashMap;

/// The shares used that may be chosen as almost used up, and the default.
pub const STEPS: [u8; 3] = [80, 90, 95];
pub const DEFAULT_AT: u8 = 90;
/// How long a service's reads fail before it is told.
pub const FAILING_FOR: i64 = 30 * 60;
/// How far a limit's reset time moves on in a new period: Claude's moves a
/// second or so from one read to the next.
const NEW_PERIOD: i64 = 10 * 60;
/// How far the share used falls in a new period, for a limit with no reset
/// time.
const FALL: u8 = 30;
/// The kinds, as settings.txt and the settings window write them.
pub const KINDS: [&str; 4] = ["near", "full", "back", "failing"];

/// Which notifications are on, and the share used that is almost used up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Alerts {
    pub near: bool,
    pub full: bool,
    pub back: bool,
    pub failing: bool,
    pub at: u8,
}

impl Default for Alerts {
    /// All on, as the person chose.
    fn default() -> Self {
        Self {
            near: true,
            full: true,
            back: true,
            failing: true,
            at: DEFAULT_AT,
        }
    }
}

impl Alerts {
    fn slot(&mut self, kind: &str) -> Option<&mut bool> {
        match kind {
            "near" => Some(&mut self.near),
            "full" => Some(&mut self.full),
            "back" => Some(&mut self.back),
            "failing" => Some(&mut self.failing),
            _ => None,
        }
    }
    /// Turns a kind on or off; false for a kind not known.
    pub fn set(&mut self, kind: &str, on: bool) -> bool {
        self.slot(kind).map(|slot| *slot = on).is_some()
    }
    /// Takes `at` as almost used up when it is one of STEPS.
    pub fn set_at(&mut self, at: u8) -> bool {
        STEPS.contains(&at).then(|| self.at = at).is_some()
    }
    /// Each kind and whether it is on, in KINDS's order.
    pub fn kinds(&self) -> [(&'static str, bool); 4] {
        let on = [self.near, self.full, self.back, self.failing];
        [0, 1, 2, 3].map(|i| (KINDS[i], on[i]))
    }
    /// Only the kinds named on, as a line of settings.txt has them.
    pub fn only(kinds: &[&str], at: u8) -> Self {
        let mut alerts = Self {
            near: false,
            full: false,
            back: false,
            failing: false,
            at: DEFAULT_AT,
        };
        for kind in kinds {
            alerts.set(kind, true);
        }
        alerts.set_at(at);
        alerts
    }
}

/// What the dial in a notification shows: a value as the box shows it, a
/// full disc, the badge of a failed read, or the dial alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Face {
    Value(u8),
    Full,
    Failed,
    Plain,
}

/// One notification.
#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub title: String,
    pub body: String,
    pub face: Face,
}

/// What was last seen of a limit, the most it was used in its period, and
/// what was told in it.
#[derive(Clone, Copy, Debug)]
struct Seen {
    used: u8,
    peak: u8,
    resets: Option<i64>,
    near: bool,
    full: bool,
}

/// What has been seen and told, from one second to the next.
#[derive(Default)]
pub struct Notifier {
    limits: HashMap<(&'static str, String), Seen>,
    /// Since when each service's reads fail, and whether it was told.
    failing: HashMap<&'static str, (i64, bool)>,
}

impl Notifier {
    /// What to tell now of the `watched` services, those read: each limit's
    /// share used against what was seen of it in its period, and how long
    /// reads have failed. A limit's first value is taken as it is, untold,
    /// so starting again tells nothing twice; one first seen used up is told
    /// when it is free again. `offset` is local time less UTC, in seconds.
    pub fn update(
        &mut self,
        services: &[Service],
        watched: &[&str],
        alerts: Alerts,
        fill: Fill,
        now: i64,
        offset: i64,
    ) -> Vec<Alert> {
        // A service no longer read starts afresh when it is read again.
        self.limits.retain(|(id, _), _| watched.contains(id));
        self.failing.retain(|id, _| watched.contains(id));
        let mut out = Vec::new();
        for service in services
            .iter()
            .filter(|s| s.meaning == Meaning::Limit && watched.contains(&s.id))
        {
            match &service.trouble {
                Some(trouble) => {
                    let (since, told) = self.failing.entry(service.id).or_insert((now, false));
                    if !*told && now - *since >= FAILING_FOR {
                        *told = true;
                        if alerts.failing {
                            out.push(failing(service, &trouble.why));
                        }
                    }
                }
                None => {
                    self.failing.remove(service.id);
                }
            }
            for limit in service.limits.iter().flatten() {
                let Reading::Read(used) = limit.reading else {
                    continue;
                };
                let named = format!("{} {}", service.name, limit.label);
                let key = (service.id, limit.label.clone());
                let Some(seen) = self.limits.get_mut(&key) else {
                    self.limits.insert(
                        key,
                        Seen {
                            used,
                            peak: used,
                            resets: limit.resets,
                            near: used >= alerts.at,
                            full: used >= 100,
                        },
                    );
                    continue;
                };
                let new_period = match (seen.resets, limit.resets) {
                    (Some(was), Some(is)) => is > was + NEW_PERIOD,
                    _ => used.saturating_add(FALL) <= seen.peak,
                };
                if new_period {
                    if seen.full && used < 100 && alerts.back {
                        out.push(Alert {
                            title: format!("{named} is available again"),
                            body: format!("{} now.", share(used, fill)),
                            face: Face::Value(shown(used, fill)),
                        });
                    }
                    seen.near = false;
                    seen.full = false;
                    seen.peak = used;
                }
                if used >= 100 {
                    if !seen.full {
                        seen.full = true;
                        seen.near = true;
                        if alerts.full {
                            out.push(Alert {
                                title: format!("{named} limit reached"),
                                body: resets_at(limit.resets, now, offset),
                                face: Face::Full,
                            });
                        }
                    }
                } else if used >= alerts.at && !seen.near {
                    seen.near = true;
                    if alerts.near {
                        out.push(Alert {
                            title: format!("{named}: {}", share(used, fill)),
                            body: limit
                                .resets
                                .map(|at| crate::popup::until(at, now))
                                .filter(|left| !left.is_empty())
                                .map(|left| format!("Resets in {left}."))
                                .unwrap_or_default(),
                            face: Face::Value(shown(used, fill)),
                        });
                    }
                }
                seen.used = used;
                seen.peak = seen.peak.max(used);
                seen.resets = limit.resets;
            }
        }
        out
    }
}

/// The notification sent from the settings window's test button.
pub fn sample() -> Alert {
    Alert {
        title: "Notifications work".into(),
        body:
            "LimitPeeker tells you here when a limit is almost used up, used up or available again."
                .into(),
        face: Face::Plain,
    }
}

/// A service whose reads have failed for a while: what went wrong, named for
/// the service when it is not already, and what to do.
fn failing(service: &Service, why: &str) -> Alert {
    let advice = crate::advice::advise(service.id, &service.name, why);
    Alert {
        title: if advice.what.contains(service.name.as_str()) {
            advice.what
        } else {
            format!("{} · {}", service.name, advice.what)
        },
        body: advice
            .todo
            .unwrap_or_else(|| "LimitPeeker keeps trying.".into()),
        face: Face::Failed,
    }
}

/// The share as the boxes show it: used, or left.
fn share(used: u8, fill: Fill) -> String {
    match fill {
        Fill::Used => format!("{used}% used"),
        Fill::Left => format!("{}% left", shown(used, fill)),
    }
}

fn shown(used: u8, fill: Fill) -> u8 {
    match fill {
        Fill::Used => used,
        Fill::Left => 100u8.saturating_sub(used),
    }
}

/// When a used-up limit starts over: the time of day and how long until it
/// within a day, else only how long; nothing when it is not known.
fn resets_at(resets: Option<i64>, now: i64, offset: i64) -> String {
    let Some(at) = resets.filter(|&at| at > now) else {
        return String::new();
    };
    let left = crate::popup::until(at, now);
    if at - now < 24 * 3600 {
        let day = (at + offset).rem_euclid(24 * 3600);
        format!(
            "Resets at {:02}:{:02}, in {left}.",
            day / 3600,
            day % 3600 / 60
        )
    } else {
        format!("Resets in {left}.")
    }
}

/// Local time less UTC now, in seconds, to whole minutes.
pub fn local_offset() -> i64 {
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    let seconds = |t: SYSTEMTIME| {
        crate::claude_usage::days(i64::from(t.wYear), i64::from(t.wMonth), i64::from(t.wDay))
            * 86400
            + i64::from(t.wHour) * 3600
            + i64::from(t.wMinute) * 60
            + i64::from(t.wSecond)
    };
    let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
    (seconds(local) - seconds(utc) + 30).div_euclid(60) * 60
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{Limit, Trouble};

    const NOW: i64 = 1_792_400_000;
    const SEOUL: i64 = 9 * 3600;

    fn claude(limits: &[(&str, Reading, Option<i64>)], trouble: Option<&str>) -> Service {
        Service {
            id: "claude",
            name: "Claude".into(),
            color: 0,
            limits: Some(
                limits
                    .iter()
                    .map(|(label, reading, resets)| Limit {
                        label: (*label).into(),
                        reading: *reading,
                        resets: *resets,
                        amount: None,
                    })
                    .collect(),
            ),
            trouble: trouble.map(|why| Trouble {
                why: why.into(),
                retry_at: NOW,
            }),
            meaning: Meaning::Limit,
        }
    }

    fn titles(alerts: &[Alert]) -> Vec<&str> {
        alerts.iter().map(|a| a.title.as_str()).collect()
    }

    #[test]
    fn a_limit_is_told_once_almost_used_up_once_used_up_and_once_free_again() {
        // The first value is taken untold; then each crossing once in the
        // period, and a used-up limit's new period.
        let mut notifier = Notifier::default();
        let resets = Some(NOW + 3 * 3600);
        let mut at = |used: u8, resets: Option<i64>, now: i64| {
            notifier.update(
                &[claude(&[("5h", Reading::Read(used), resets)], None)],
                &["claude"],
                Alerts::default(),
                Fill::Used,
                now,
                SEOUL,
            )
        };
        assert!(at(40, resets, NOW).is_empty());
        assert!(at(85, resets, NOW + 60).is_empty());
        let near = at(91, resets, NOW + 120);
        assert_eq!(titles(&near), ["Claude 5h: 91% used"]);
        assert_eq!(near[0].body, "Resets in 2h 58m.");
        assert_eq!(near[0].face, Face::Value(91));
        assert!(at(95, resets, NOW + 180).is_empty());
        // Claude's reset time moves a second between reads.
        let full = at(100, resets.map(|r| r - 1), NOW + 240);
        assert_eq!(titles(&full), ["Claude 5h limit reached"]);
        // 1792410799 is 11:53:19 UTC, 20:53 in Seoul.
        assert_eq!(full[0].body, "Resets at 20:53, in 2h 55m.");
        assert_eq!(full[0].face, Face::Full);
        assert!(at(100, resets, NOW + 300).is_empty());
        let back = at(0, resets.map(|r| r + 5 * 3600), NOW + 3 * 3600 + 30);
        assert_eq!(titles(&back), ["Claude 5h is available again"]);
        assert_eq!(back[0].body, "0% used now.");
        // Straight to 100 in the new period tells only that it is used up.
        let full = at(100, resets.map(|r| r + 5 * 3600), NOW + 4 * 3600);
        assert_eq!(titles(&full), ["Claude 5h limit reached"]);
    }

    #[test]
    fn starting_again_tells_nothing_already_so_and_a_first_full_limit_is_told_free() {
        // A value first seen past the line is taken as told; one first seen
        // used up is told when it starts over, by its reset time or, with
        // none, by a fall of 30.
        let mut notifier = Notifier::default();
        let mut at = |used: u8, resets: Option<i64>| {
            titles(&notifier.update(
                &[claude(&[("7d", Reading::Read(used), resets)], None)],
                &["claude"],
                Alerts::default(),
                Fill::Left,
                NOW,
                SEOUL,
            ))
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
        };
        assert!(at(100, None).is_empty());
        assert!(at(100, None).is_empty());
        assert!(at(80, None).is_empty());
        assert_eq!(at(60, None), ["Claude 7d is available again"]);
        let mut fresh = Notifier::default();
        let shown = fresh.update(
            &[claude(&[("7d", Reading::Read(93), None)], None)],
            &["claude"],
            Alerts::default(),
            Fill::Left,
            NOW,
            SEOUL,
        );
        assert!(shown.is_empty());
    }

    #[test]
    fn what_is_told_follows_the_switches_the_line_and_show() {
        // Each kind off tells nothing of it; the line is the one chosen;
        // with Remaining the share is what is left.
        let run = |alerts: Alerts, values: &[u8], fill: Fill| {
            let mut notifier = Notifier::default();
            let mut told = Vec::new();
            for (step, used) in values.iter().enumerate() {
                told.extend(notifier.update(
                    &[claude(&[("5h", Reading::Read(*used), None)], None)],
                    &["claude"],
                    alerts,
                    fill,
                    NOW + step as i64 * 60,
                    SEOUL,
                ));
            }
            told
        };
        let off = Alerts {
            near: false,
            ..Alerts::default()
        };
        assert!(run(off, &[10, 92], Fill::Used).is_empty());
        let at_80 = Alerts::only(&["near"], 80);
        assert_eq!(
            titles(&run(at_80, &[10, 82], Fill::Left)),
            ["Claude 5h: 18% left"]
        );
        assert_eq!(run(at_80, &[10, 82], Fill::Left)[0].face, Face::Value(18));
        assert!(run(at_80, &[10, 100], Fill::Used).is_empty());
        let no_back = Alerts {
            back: false,
            ..Alerts::default()
        };
        assert_eq!(
            titles(&run(no_back, &[10, 100, 0], Fill::Used)),
            ["Claude 5h limit reached"]
        );
        // A service not read, and this PC's meters, are not looked at.
        let mut notifier = Notifier::default();
        for used in [10, 95] {
            let mut pc = claude(&[("CPU", Reading::Read(used), None)], None);
            pc.meaning = Meaning::Meter;
            assert!(
                notifier
                    .update(&[pc], &["claude"], Alerts::default(), Fill::Used, NOW, 0)
                    .is_empty()
            );
            assert!(
                notifier
                    .update(
                        &[claude(&[("5h", Reading::Read(used), None)], None)],
                        &["gpt"],
                        Alerts::default(),
                        Fill::Used,
                        NOW,
                        0
                    )
                    .is_empty()
            );
        }
    }

    #[test]
    fn reads_failing_for_half_an_hour_are_told_once_with_what_to_do() {
        // From the first failure; a good read starts the count again.
        let mut notifier = Notifier::default();
        let why = "not signed in to a Claude plan (no subscription); /usage not tried";
        let mut at = |trouble: Option<&str>, now: i64| {
            notifier.update(
                &[claude(&[], trouble)],
                &["claude"],
                Alerts::default(),
                Fill::Used,
                now,
                SEOUL,
            )
        };
        assert!(at(Some(why), NOW).is_empty());
        assert!(at(Some(why), NOW + FAILING_FOR - 1).is_empty());
        let told = at(Some(why), NOW + FAILING_FOR);
        assert_eq!(titles(&told), ["Not signed in to a Claude plan"]);
        assert_eq!(told[0].body, "Run claude, then /login with a Claude plan");
        assert_eq!(told[0].face, Face::Failed);
        assert!(at(Some(why), NOW + 2 * FAILING_FOR).is_empty());
        assert!(at(None, NOW + 2 * FAILING_FOR + 60).is_empty());
        assert!(at(Some("no answer in time"), NOW + 3 * FAILING_FOR).is_empty());
        let slow = at(Some("no answer in time"), NOW + 4 * FAILING_FOR);
        assert_eq!(titles(&slow), ["Claude Code did not answer in time"]);
        assert_eq!(slow[0].body, "LimitPeeker keeps trying.");
    }

    #[test]
    fn the_settings_kinds_and_the_line_are_kept_as_written() {
        let mut alerts = Alerts::default();
        assert!(alerts.set("back", false));
        assert!(!alerts.set("loud", true));
        assert!(alerts.set_at(95));
        assert!(!alerts.set_at(85));
        assert_eq!(alerts.at, 95);
        assert_eq!(
            alerts.kinds(),
            [
                ("near", true),
                ("full", true),
                ("back", false),
                ("failing", true)
            ]
        );
        assert_eq!(
            Alerts::only(&["full", "back"], 85),
            Alerts {
                near: false,
                full: true,
                back: true,
                failing: false,
                at: DEFAULT_AT,
            }
        );
        assert_eq!(crate::claude_usage::days(1970, 1, 1), 0);
        assert_eq!(crate::claude_usage::days(2026, 10, 20), 20_746);
        assert_eq!(
            resets_at(Some(NOW + 3 * 86400), NOW, SEOUL),
            "Resets in 3d."
        );
        assert_eq!(resets_at(None, NOW, SEOUL), "");
        let offset = local_offset();
        assert!((-14 * 3600..=14 * 3600).contains(&offset) && offset % 60 == 0);
    }
}
