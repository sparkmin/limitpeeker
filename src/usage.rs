//! The usage figures the taskbar shows, and the label they become. A service
//! reports a list of limits, whatever its provider has: five hours, a week, a
//! day per model, a billing cycle. Nothing here or in the hook knows which; the
//! provider's adapter names them. The hook draws whatever label it is sent,
//! so neither a new value nor a new format needs a new DLL.

use crate::style::{Fill, Look, Numbers, Style};
use std::collections::HashMap;

/// The longest string, in UTF-16 units, that the hook takes. Must match
/// TextCapacity in native/taskbar_tap.cpp.
pub const CAPACITY: usize = 192;
/// The most cells the hook draws, gauges in one cell, and gauges in all.
/// Must match MaxCells, MaxCellGauges and MaxGauges. A cell holds four for
/// the PC service's four meters side by side.
pub const MAX_CELLS: usize = 4;
pub const MAX_CELL_GAUGES: usize = 4;
pub const MAX_GAUGES: usize = 12;
/// Split the cells, a cell's gauges, and a gauge's label, mark and value, in
/// each string sent. Must match CellBreak, GaugeBreak and LabelBreak.
pub const CELL_BREAK: char = '\u{1f}';
pub const GAUGE_BREAK: char = '\u{1e}';
pub const LABEL_BREAK: char = '\u{1d}';

/// The most limits a cell shows; the rest are in the tray icon's text.
pub const SHOWN: usize = 2;

/// How many of a service's limits a box shows at most: two, and PC's four
/// meters.
pub fn most_limits(id: &str) -> usize {
    if id == crate::resources::ID {
        MAX_CELL_GAUGES
    } else {
        SHOWN
    }
}

/// The cells a service with `limits` shown takes: one, or one for each two
/// of more than two, where the look stacks gauges or rings them and four
/// would not fit the box's height.
pub fn cell_count(limits: usize) -> usize {
    if limits > 2 { limits.div_ceil(2) } else { 1 }
}
/// How many services show until others are picked.
pub const SERVICES: usize = 3;

/// What is left, at or below which a limit warns on its own value and gauge
/// alone: orange, then red.
pub const LOW: u8 = 20;
pub const VERY_LOW: u8 = 5;
/// The share used at or above which one of this computer's meters warns, in
/// red alone: its CPU and GPU spike for moments.
pub const HIGH: u8 = 90;
pub const WARNING: u32 = 0xFFFF_5A5F;
pub const CAUTION: u32 = 0xFFFF_A53D;
/// The mark before a value that warns in red, so it stands out. It is sent
/// apart from the value: the hook keeps its room before every value and puts
/// a gap, not a space, between it and the digits. People read it with a
/// space.
pub const MARK: &str = "!!";

/// How much a value warns: not, in orange, or in red with the mark.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Warn {
    #[default]
    None,
    Caution,
    Warning,
}
impl Warn {
    /// Its color, when it has one.
    pub fn color(self) -> Option<u32> {
        match self {
            Warn::None => None,
            Warn::Caution => Some(CAUTION),
            Warn::Warning => Some(WARNING),
        }
    }
    /// The mark before the value.
    pub fn mark(self) -> &'static str {
        if self == Warn::Warning { MARK } else { "" }
    }
    /// As shown with the warning colors on or off, which the settings window
    /// turns.
    pub fn when(self, on: bool) -> Warn {
        if on { self } else { Warn::None }
    }
}
/// How much of its colors' opacity a value kept from before a failed read
/// keeps.
const STALE_PERCENT: u32 = 45;

/// One service as its provider reports it. `limits` is None until the first
/// read, and then lists the limits the service has; one known not to exist is
/// simply absent. `id` names the service's icon file, and `color` is the
/// service's own, on its fills and a one-color icon, whichever services show.
#[derive(Clone, Debug)]
pub struct Service {
    pub id: &'static str,
    pub name: String,
    pub color: u32,
    pub limits: Option<Vec<Limit>>,
    /// Why the last read failed, until one succeeds.
    pub trouble: Option<Trouble>,
    pub meaning: Meaning,
}

/// What a service's values are. A limit's share used shows as it is or as
/// what is left, and warns when little is left; one of this computer's
/// meters always shows the share used, and warns when it is high.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Meaning {
    #[default]
    Limit,
    Meter,
}

/// Why a service's last read failed, and when, in Unix seconds, the next
/// read is.
#[derive(Clone, Debug, PartialEq)]
pub struct Trouble {
    pub why: String,
    pub retry_at: i64,
}

/// Now, in Unix seconds.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// One limit: the short label its provider's adapter chose, such as `5h`,
/// `7d`, `Opus 7d` or `1d`, where its reading stands, when it starts over,
/// in Unix seconds, if the provider said, and, for this computer's memory,
/// how much is in use of how much there is.
#[derive(Clone, Debug)]
pub struct Limit {
    pub label: String,
    pub reading: Reading,
    pub resets: Option<i64>,
    pub amount: Option<Amount>,
}

/// A meter's amount in bytes: how much is in use of how much there is, and
/// whether it is the computer's memory that a GPU built in shares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Amount {
    pub used: u64,
    pub total: u64,
    pub shared: bool,
}

/// A limit's reading: the share used, 0 to 100 or above when a provider
/// reports overage (an adapter converts what is left); not read yet; or a
/// failed read, with the value it had, if any.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reading {
    Read(u8),
    Pending,
    Failed(Option<u8>),
}

/// What one read of a provider found: the limits, and what the log says of
/// them.
pub struct Found {
    pub limits: Vec<Limit>,
    pub told: String,
}

/// One gauge as sent: a limit's label, the mark of a low value (empty
/// otherwise), its value now and at its widest, the share its color covers in
/// permille, and its value's and fill's colors, which alone show a warning or
/// a stale value. The mark is MARK at its widest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gauge {
    pub label: String,
    pub mark: &'static str,
    pub text: String,
    pub sizing: String,
    pub fill: Option<u16>,
    pub text_color: u32,
    pub fill_color: u32,
    /// Whether its colored part sits against the end: what is left, on a
    /// dial. Each gauge says it, so a meter beside a limit can differ.
    pub end: bool,
}

/// One service's cell: its icon, and a gauge per limit shown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cell {
    /// The service's icon file.
    pub id: &'static str,
    /// The service's color, which tints a one-color icon.
    pub color: u32,
    /// What a person reads for the service, every limit included, shown or
    /// not: the tray icon and the log use it.
    pub plain: String,
    pub gauges: Vec<Gauge>,
}

/// Which services show, and which limits each shows by label, as picked in
/// the popup or the settings window, each in the order picked. Where
/// nothing picked still exists, the first SERVICES services and a service's
/// first SHOWN limits show. The popup's summary keeps an order of its own.
/// Picks are held by the provider's names, so one saved for a service not
/// offered now stays.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Picks {
    services: Option<Vec<String>>,
    limits: HashMap<String, Vec<String>>,
    summary: Option<Vec<String>>,
    summary_limits: HashMap<String, Vec<String>>,
    /// The services the summary shows, apart from the box's; until any is
    /// picked, it shows the box's.
    summary_services: Option<Vec<String>>,
}

impl Picks {
    pub fn set_summary_services(&mut self, ids: Vec<String>) {
        self.summary_services = Some(ids);
    }
    /// The services picked for the summary, if any have been.
    pub fn picked_summary_services(&self) -> Option<&[String]> {
        self.summary_services.as_deref()
    }
    pub fn set_services(&mut self, ids: Vec<String>) {
        self.services = Some(ids);
    }
    pub fn set_limits(&mut self, id: &str, labels: Vec<String>) {
        self.limits.insert(id.to_string(), labels);
    }
    pub fn set_summary(&mut self, ids: Vec<String>) {
        self.summary = Some(ids);
    }
    pub fn set_summary_limits(&mut self, id: &str, labels: Vec<String>) {
        self.summary_limits.insert(id.to_string(), labels);
    }
    /// The services picked, if any have been.
    pub fn picked_services(&self) -> Option<&[String]> {
        self.services.as_deref()
    }
    /// Each service's picked limits, by the service's id.
    pub fn picked_limits(&self) -> Vec<(&str, &[String])> {
        sorted(&self.limits)
    }
    /// The summary's order of services, if one has been set.
    pub fn picked_summary(&self) -> Option<&[String]> {
        self.summary.as_deref()
    }
    /// Each service's order of lines in the summary.
    pub fn picked_summary_limits(&self) -> Vec<(&str, &[String])> {
        sorted(&self.summary_limits)
    }
    /// The services shown out of these: the picked ones that exist, in the
    /// order picked, as many as the hook has cells; or, when none of them
    /// exists, the first SERVICES in the list's order but PC, this
    /// computer's meters, which shows only when picked.
    pub fn services(&self, ids: &[&'static str]) -> Vec<&'static str> {
        let picked = self.services.as_deref().unwrap_or_default();
        let shown = in_picked_order(ids, picked, |id: &&str| *id, MAX_CELLS);
        if shown.is_empty() {
            ids.iter()
                .filter(|id| **id != crate::resources::ID)
                .take(SERVICES)
                .copied()
                .collect()
        } else {
            shown
        }
    }
    /// The limits a service shows out of these labels: the picked ones that
    /// exist, in the order picked, at most SHOWN; or the first SHOWN.
    pub fn limits(&self, id: &str, labels: &[String]) -> Vec<String> {
        let picked = self.limits.get(id).map(Vec::as_slice).unwrap_or_default();
        let shown = in_picked_order(labels, picked, String::as_str, most_limits(id));
        if shown.is_empty() {
            labels.iter().take(SHOWN).cloned().collect()
        } else {
            shown
        }
    }
    /// The summary's services out of those shown: those in its order first,
    /// then the rest in the box's order.
    pub fn summary(&self, shown: &[&'static str]) -> Vec<&'static str> {
        let order = self.summary.as_deref().unwrap_or_default();
        in_order(shown, order, |id: &&str| *id)
    }
    /// A service's lines in the summary out of these labels: those in its
    /// order first, then the rest in the provider's order.
    pub fn summary_limits(&self, id: &str, labels: &[String]) -> Vec<String> {
        let order = self
            .summary_limits
            .get(id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        in_order(labels, order, String::as_str)
    }
}

fn sorted(map: &HashMap<String, Vec<String>>) -> Vec<(&str, &[String])> {
    let mut all: Vec<_> = map
        .iter()
        .map(|(id, labels)| (id.as_str(), labels.as_slice()))
        .collect();
    all.sort();
    all
}

/// The items `picked` names, each once, in the order picked, at most `most`.
fn in_picked_order<T: Clone>(
    items: &[T],
    picked: &[String],
    name: impl Fn(&T) -> &str,
    most: usize,
) -> Vec<T> {
    let mut chosen: Vec<T> = Vec::new();
    for pick in picked {
        if chosen.len() == most {
            break;
        }
        if let Some(item) = items.iter().find(|item| name(item) == pick.as_str())
            && !chosen.iter().any(|c| name(c) == pick.as_str())
        {
            chosen.push(item.clone());
        }
    }
    chosen
}

/// Every item: those `order` names first, in its order, then the rest in
/// the list's.
fn in_order<T: Clone>(items: &[T], order: &[String], name: impl Fn(&T) -> &str) -> Vec<T> {
    let mut all = in_picked_order(items, order, &name, usize::MAX);
    for item in items {
        if !all.iter().any(|a| name(a) == name(item)) {
            all.push(item.clone());
        }
    }
    all
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Label {
    pub cells: Vec<Cell>,
    pub style: Style,
}

impl Label {
    /// Each gauge's label, mark and value, split as the hook expects.
    pub fn text(&self) -> String {
        self.join(|g| (g.mark, &g.text))
    }
    /// The same with each mark and value at its widest, so the room for a
    /// mark is always kept.
    pub fn sizing(&self) -> String {
        self.join(|g| (MARK, &g.sizing))
    }
    fn join(&self, parts: impl Fn(&Gauge) -> (&str, &String)) -> String {
        self.cells
            .iter()
            .map(|c| {
                c.gauges
                    .iter()
                    .map(|g| {
                        let (mark, value) = parts(g);
                        format!("{}{LABEL_BREAK}{mark}{LABEL_BREAK}{value}", g.label)
                    })
                    .collect::<Vec<_>>()
                    .join(GAUGE_BREAK.encode_utf8(&mut [0; 4]))
            })
            .collect::<Vec<_>>()
            .join(CELL_BREAK.encode_utf8(&mut [0; 4]))
    }
    /// The cells for people: the tray icon and the log.
    pub fn summary(&self) -> String {
        self.cells
            .iter()
            .map(|c| c.plain.as_str())
            .collect::<Vec<_>>()
            .join("  |  ")
    }
    /// Whether the hook will take it. It keeps its previous label otherwise.
    pub fn fits(&self) -> bool {
        let gauges = || self.cells.iter().flat_map(|c| &c.gauges);
        let breaks = [CELL_BREAK, GAUGE_BREAK, LABEL_BREAK];
        self.style.fits()
            && (1..=MAX_CELLS).contains(&self.cells.len())
            && self
                .cells
                .iter()
                .all(|c| (1..=MAX_CELL_GAUGES).contains(&c.gauges.len()))
            && gauges().count() <= MAX_GAUGES
            && gauges().all(|g| {
                !g.sizing.is_empty()
                    && [g.label.as_str(), g.mark, &g.text, &g.sizing]
                        .iter()
                        .all(|s| !s.contains(breaks))
                    && g.fill.is_none_or(|f| f <= 1000)
            })
            && [self.text(), self.sizing()]
                .iter()
                .all(|s| (1..=CAPACITY).contains(&s.encode_utf16().count()))
    }
}

/// The label for the services as the look draws them, the fill mode reads
/// them, and the picks choose the services, their order and their limits. A
/// service with no limits at all has no cell; one with more than two shown,
/// PC, takes a cell for each two unless the look sets gauges side by side.
/// At most MAX_CELLS cells. Values warn in color when `warn`.
pub fn label(services: &[Service], look: &Look, fill: Fill, picks: &Picks, warn: bool) -> Label {
    let ids: Vec<&'static str> = services.iter().map(|s| s.id).collect();
    Label {
        cells: picks
            .services(&ids)
            .iter()
            .filter_map(|id| services.iter().find(|s| s.id == *id))
            .filter(|s| s.limits.as_ref().is_none_or(|l| !l.is_empty()))
            .flat_map(|s| cells(s, look, fill, picks, warn))
            .take(MAX_CELLS)
            .collect(),
        // What is left stays at a bar's start and gives way from the right;
        // on a dial it is what the used share has not yet swept, so it sits
        // against the end.
        style: Style {
            fill_end: u32::from(fill == Fill::Left && look.style.dial()),
            ..look.style
        },
    }
}

/// A service's cells: one, or more for more than two gauges where the look
/// stacks them or rings them.
fn cells(service: &Service, look: &Look, fill: Fill, picks: &Picks, warn: bool) -> Vec<Cell> {
    let whole = cell(service, look, fill, picks, warn);
    if whole.gauges.len() <= 2 || look.style.side() {
        return vec![whole];
    }
    whole
        .gauges
        .chunks(2)
        .map(|gauges| Cell {
            id: whole.id,
            color: whole.color,
            plain: words(&[
                &service.name,
                &gauges
                    .iter()
                    .map(|g| words(&[&g.label, &g.text]))
                    .collect::<Vec<_>>()
                    .join(" "),
            ]),
            gauges: gauges.to_vec(),
        })
        .collect()
}

/// Until the first read, one gauge with no label stands for the service.
static UNKNOWN: [Limit; 1] = [Limit {
    label: String::new(),
    reading: Reading::Pending,
    resets: None,
    amount: None,
}];

/// What a person reads for the service, every limit included, shown or not:
/// a cell's words, and the tray icon's.
pub fn summary_of(service: &Service, fill: Fill) -> String {
    let limits = service.limits.as_deref().unwrap_or(&UNKNOWN);
    let plain: Vec<String> = limits
        .iter()
        .map(|l| words(&[&l.label, &plain(l.reading, fill, service.meaning)]))
        .collect();
    words(&[&service.name, &plain.join(" ")])
}

fn cell(service: &Service, look: &Look, fill: Fill, picks: &Picks, warn: bool) -> Cell {
    // Deepened on a light box, for the icon and the gauges alike.
    let color = look.service_color(service.color);
    let limits = service.limits.as_deref().unwrap_or(&UNKNOWN);
    let labels: Vec<String> = limits.iter().map(|l| l.label.clone()).collect();
    let shown = picks.limits(service.id, &labels);
    Cell {
        id: service.id,
        color,
        plain: summary_of(service, fill),
        // In the order picked.
        gauges: shown
            .iter()
            .filter_map(|label| limits.iter().find(|l| &l.label == label))
            .enumerate()
            .map(|(index, limit)| gauge(service, index, limit, color, look, fill, warn))
            .collect(),
    }
}

fn gauge(
    service: &Service,
    index: usize,
    limit: &Limit,
    color: u32,
    look: &Look,
    fill: Fill,
    warn: bool,
) -> Gauge {
    // With no icon, the first gauge's label names the service.
    let label = if look.style.icon == 0 && index == 0 {
        words(&[&service.name, &limit.label])
    } else {
        limit.label.clone()
    };
    let value = shown(limit.reading, fill, service.meaning);
    let level = value.map_or(Warn::None, |(_, level)| level).when(warn);
    let (text_color, fill_color) = match level.color() {
        Some(warning) => (warning, warning),
        None => (look.text, color),
    };
    let stale = matches!(limit.reading, Reading::Failed(_));
    let dim = |argb| if stale { dimmed(argb) } else { argb };
    Gauge {
        label,
        mark: level.mark(),
        text: match (limit.reading, value) {
            (_, Some((share, _))) => number(share, look.numbers),
            (Reading::Failed(None), None) => "?".into(),
            _ => "--".into(),
        },
        // The mark's room is kept apart, so one never moves the value, and a
        // u8 has at most three digits, all as wide.
        sizing: number(100, look.numbers),
        fill: value.map(|(share, _)| u16::from(share.min(100)) * 10),
        text_color: dim(text_color),
        fill_color: dim(fill_color),
        end: fill == Fill::Left && service.meaning == Meaning::Limit && look.style.dial(),
    }
}

/// What a reading shows under the fill mode, the share used or what is left,
/// and how much it warns: a limit as what is left runs low, orange and then
/// red, and a meter, whose share is always the share used, in red when that
/// is high. None when there is no value to show.
pub fn shown(reading: Reading, fill: Fill, meaning: Meaning) -> Option<(u8, Warn)> {
    let used = match reading {
        Reading::Read(used) | Reading::Failed(Some(used)) => used,
        Reading::Pending | Reading::Failed(None) => return None,
    };
    let left = 100 - used.min(100);
    let level = match meaning {
        Meaning::Meter if used >= HIGH => Warn::Warning,
        Meaning::Meter => Warn::None,
        Meaning::Limit if left <= VERY_LOW => Warn::Warning,
        Meaning::Limit if left <= LOW => Warn::Caution,
        Meaning::Limit => Warn::None,
    };
    Some(match (meaning, fill) {
        (Meaning::Limit, Fill::Left) => (left, level),
        _ => (used, level),
    })
}

/// The services with each meter's readings and amounts taken back to not
/// read yet, and every limit's reset time left out: a label or a scene of
/// these changes when anything but a meter, which moves each second, or a
/// time to reset, which counts down each minute, does. The log tells of a
/// label then.
pub fn steady(services: &[Service]) -> Vec<Service> {
    services
        .iter()
        .map(|s| {
            let meter = s.meaning == Meaning::Meter;
            Service {
                limits: s.limits.as_ref().map(|limits| {
                    limits
                        .iter()
                        .map(|l| Limit {
                            reading: if meter { Reading::Pending } else { l.reading },
                            amount: if meter { None } else { l.amount },
                            resets: None,
                            ..l.clone()
                        })
                        .collect()
                }),
                ..s.clone()
            }
        })
        .collect()
}

/// A reading for people. `*` marks a value kept from before a failed read,
/// and the mark one that warns in red, whether the colors are on or not: the
/// tooltip and the log are a record.
fn plain(reading: Reading, fill: Fill, meaning: Meaning) -> String {
    match (reading, shown(reading, fill, meaning)) {
        (Reading::Failed(_), Some((share, level))) => words(&[level.mark(), &format!("{share}%*")]),
        (_, Some((share, level))) => words(&[level.mark(), &format!("{share}%")]),
        (Reading::Failed(None), None) => "?".into(),
        _ => "--".into(),
    }
}

pub(crate) fn number(share: u8, numbers: Numbers) -> String {
    match numbers {
        Numbers::Percent => format!("{share}%"),
        Numbers::Digits => share.to_string(),
    }
}

pub(crate) fn dimmed(argb: u32) -> u32 {
    ((argb >> 24) * STALE_PERCENT / 100) << 24 | (argb & 0x00FF_FFFF)
}

/// The parts that are not empty, joined by spaces.
fn words(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// How long `--cycle-mock` shows each step.
pub const CYCLE_STEP_MS: u64 = 4000;

/// The mock figures. Step 0 is what the product shows: Claude has three
/// limits, of which two show, and its week is low; Gemini has one limit only.
/// `--cycle-mock` walks the others: values at their ends; every reading state,
/// with a service not read yet; and GPT's five hours found not to exist, the
/// only step that may move the reservation. Grok, off until picked, keeps
/// its value, 20% left. Only the services read are in it. A limit with a
/// value starts over a while from now, by its length. Last is PC, this
/// computer's meters,
/// also off until picked, which are high on the step of the ends; its RAM
/// and VRAM tell amounts as a computer with 15.8 GB and a GPU built in
/// would.
pub fn mock(step: u64) -> Vec<Service> {
    mock_at(step, unix_now())
}

/// The mock figures as they stand at `now`, in Unix seconds.
pub fn mock_at(step: u64, now: i64) -> Vec<Service> {
    use Reading::*;
    let list = |pairs: &[(&str, Reading)]| {
        Some(
            pairs
                .iter()
                .map(|&(label, reading)| Limit {
                    label: label.into(),
                    reading,
                    resets: mock_resets(label, reading).map(|after| now + after),
                    amount: None,
                })
                .collect(),
        )
    };
    let limits: [Option<Vec<Limit>>; 3] = match step % 4 {
        0 => [
            list(&[("5h", Read(28)), ("7d", Read(42))]),
            list(&[("5h", Read(42)), ("7d", Read(90)), ("Opus 7d", Read(60))]),
            list(&[("1d", Read(9))]),
        ],
        1 => [
            list(&[("5h", Read(100)), ("7d", Read(100))]),
            list(&[("5h", Read(0)), ("7d", Read(0)), ("Opus 7d", Read(0))]),
            list(&[("1d", Read(100))]),
        ],
        2 => [
            list(&[("5h", Pending), ("7d", Read(5))]),
            list(&[
                ("5h", Failed(Some(42))),
                ("7d", Failed(None)),
                ("Opus 7d", Pending),
            ]),
            None,
        ],
        _ => [
            list(&[("7d", Read(42))]),
            list(&[("5h", Read(42)), ("7d", Read(90)), ("Opus 7d", Read(60))]),
            list(&[("1d", Read(9))]),
        ],
    };
    // Grok's one period, as it has it.
    let more = [list(&[("7d", Read(80))])];
    // Each service's own color, pale enough for a dark taskbar; GPT's own is
    // white.
    [
        ("gpt", "GPT", 0xFFF2_F2F2),
        ("claude", "Claude", 0xFFF4_A27A),
        ("gemini", "Gemini", 0xFF7A_A7FF),
        ("grok", "Grok", 0xFFCF_CFCF),
    ]
    .into_iter()
    .zip(limits.into_iter().chain(more))
    .map(|((id, name, color), limits)| Service {
        id,
        name: name.into(),
        color,
        limits,
        trouble: None,
        meaning: Meaning::Limit,
    })
    .chain([{
        let readings = match step % 4 {
            1 => [Read(100), Read(92), Read(0), Read(0)],
            2 => [Pending, Read(61), Failed(None), Pending],
            _ => [Read(24), Read(61), Read(8), Read(43)],
        };
        let amount = |reading: Reading, total: u64, shared: bool| match reading {
            Read(share) => Some(Amount {
                used: total / 100 * u64::from(share),
                total,
                shared,
            }),
            _ => None,
        };
        let (memory, shared) = ((158 << 30) / 10, (79 << 30) / 10);
        crate::resources::service(
            readings,
            [
                None,
                amount(readings[1], memory, false),
                None,
                amount(readings[3], shared, true),
            ],
        )
    }])
    .collect()
}

/// How long after now a mock limit starts over, in seconds: part of its
/// length, so each shows a different span. None without a value.
fn mock_resets(label: &str, reading: Reading) -> Option<i64> {
    if matches!(reading, Reading::Pending | Reading::Failed(None)) {
        return None;
    }
    let (hour, day) = (3600, 86_400);
    Some(match label {
        "5h" => 2 * hour + 14 * 60,
        "1d" => 9 * hour + 5 * 60,
        _ => 3 * day + 4 * hour,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{LOOKS, find};
    use Fill::{Left, Used};

    fn slim() -> &'static Look {
        find("slim").unwrap()
    }
    /// A look with no icon, which names the service on its first gauge.
    fn named() -> Look {
        Look {
            style: Style {
                icon: 0,
                ..slim().style
            },
            ..*slim()
        }
    }
    fn none() -> Picks {
        Picks::default()
    }
    fn claude_color() -> u32 {
        mock(0)[1].color
    }
    fn labels(cell: &Cell) -> Vec<&str> {
        cell.gauges.iter().map(|g| g.label.as_str()).collect()
    }

    #[test]
    fn a_service_shows_a_gauge_per_limit() {
        let label = label(&mock(0), &named(), Used, &none(), true);
        assert_eq!(
            label.summary(),
            "GPT 5h 28% 7d 42%  |  Claude 5h 42% 7d 90% Opus 7d 60%  |  Gemini 1d 9%"
        );
        // With no icon, the first gauge's label names the service.
        assert_eq!(
            label.text(),
            "GPT 5h\u{1d}\u{1d}28%\u{1e}7d\u{1d}\u{1d}42%\u{1f}Claude 5h\u{1d}\u{1d}42%\u{1e}7d\u{1d}\u{1d}90%\u{1f}Gemini 1d\u{1d}\u{1d}9%"
        );
        assert_eq!(
            label.sizing(),
            "GPT 5h\u{1d}!!\u{1d}100%\u{1e}7d\u{1d}!!\u{1d}100%\u{1f}Claude 5h\u{1d}!!\u{1d}100%\u{1e}7d\u{1d}!!\u{1d}100%\u{1f}Gemini 1d\u{1d}!!\u{1d}100%"
        );
        let fills: Vec<Vec<_>> = label
            .cells
            .iter()
            .map(|c| c.gauges.iter().map(|g| g.fill).collect())
            .collect();
        assert_eq!(
            fills,
            [
                vec![Some(280), Some(420)],
                vec![Some(420), Some(900)],
                vec![Some(90)]
            ]
        );
        assert!(label.fits());
        let with_icons = super::label(&mock(0), slim(), Used, &none(), true);
        assert_eq!(labels(&with_icons.cells[0]), ["5h", "7d"]);
    }

    #[test]
    fn only_the_low_limit_warns() {
        // Claude's week has 10% left: its value and gauge warn in orange,
        // with no mark, and nothing else of Claude does.
        for fill in [Used, Left] {
            let label = label(&mock(0), slim(), fill, &none(), true);
            let claude = &label.cells[1];
            let (five, week) = (&claude.gauges[0], &claude.gauges[1]);
            assert_eq!((week.text_color, week.fill_color), (CAUTION, CAUTION));
            assert_eq!(
                (five.text_color, five.fill_color),
                (slim().text, claude_color())
            );
            assert_eq!((claude.color, week.label.as_str()), (claude_color(), "7d"));
            assert_eq!((five.mark, week.mark), ("", ""));
        }
        let text = |fill| {
            label(&mock(0), slim(), fill, &none(), true).cells[1].gauges[1]
                .text
                .clone()
        };
        assert_eq!(text(Used), "90%");
        assert_eq!(text(Left), "10%");
    }

    #[test]
    fn a_mark_is_sent_apart_and_its_room_kept() {
        // The mark is its own part of a gauge's text, empty when nothing is
        // low and always MARK at its widest, so the hook keeps its room before
        // every value and two values start in one place. It is drawn heavier
        // than the digits. People read it with a space. Claude's week is
        // put at 97% used, which warns in red.
        let mut services = mock(0);
        if let Some(limits) = services[1].limits.as_mut() {
            limits[1].reading = Reading::Read(97);
        }
        let rings = label(&services, find("rings").unwrap(), Used, &none(), true);
        let claude = &rings.cells[1];
        let parts: Vec<_> = claude
            .gauges
            .iter()
            .map(|g| (g.mark, g.text.as_str(), g.sizing.as_str()))
            .collect();
        assert_eq!(parts, [("", "42", "100"), (MARK, "97", "100")]);
        assert!(
            rings
                .text()
                .contains("5h\u{1d}\u{1d}42\u{1e}7d\u{1d}!!\u{1d}97")
        );
        assert!(
            rings
                .sizing()
                .contains("5h\u{1d}!!\u{1d}100\u{1e}7d\u{1d}!!\u{1d}100")
        );
        assert!(claude.plain.contains("7d !! 97%"));
        for look in &LOOKS {
            assert!(look.style.mark_weight > look.style.weight, "{}", look.name);
        }
    }

    #[test]
    fn nothing_left_is_zero_and_an_empty_gauge() {
        let label = label(&mock(1), slim(), Left, &none(), true);
        let gpt = &label.cells[0].gauges[0];
        assert_eq!(
            (gpt.mark, gpt.text.as_str(), gpt.fill, gpt.fill_color),
            (MARK, "0%", Some(0), WARNING)
        );
        // Nothing used leaves the whole gauge in the service's color.
        let claude = &label.cells[1].gauges[0];
        assert_eq!((claude.text.as_str(), claude.fill), ("100%", Some(1000)));
    }

    #[test]
    fn every_reading_state_is_told_apart() {
        // Not read yet, a value kept from before a failed read, a failed read
        // with nothing kept, and a service not read at all.
        let label = label(&mock(2), slim(), Used, &none(), true);
        assert_eq!(
            label.summary(),
            "GPT 5h -- 7d 5%  |  Claude 5h 42%* 7d ? Opus 7d --  |  Gemini --"
        );
        let gauges: Vec<_> = label
            .cells
            .iter()
            .flat_map(|c| &c.gauges)
            .map(|g| (g.label.as_str(), g.text.as_str(), g.fill))
            .collect();
        assert_eq!(
            gauges,
            [
                ("5h", "--", None),
                ("7d", "5%", Some(50)),
                ("5h", "42%", Some(420)),
                ("7d", "?", None),
                ("", "--", None)
            ]
        );
        let kept = &label.cells[1].gauges[0];
        assert_eq!(kept.text_color, dimmed(slim().text));
        assert_eq!(kept.fill_color, dimmed(claude_color()));
        assert_eq!(kept.text_color >> 24, 0xF2 * 45 / 100);
        assert!(label.fits());
    }

    #[test]
    fn a_limit_known_not_to_exist_has_no_gauge() {
        // GPT without its five hours shows the week alone; a service with no
        // limits shows no cell, and the others keep their colors.
        assert_eq!(
            labels(&label(&mock(3), slim(), Used, &none(), true).cells[0]),
            ["7d"]
        );
        let mut services = mock(0);
        services[0].limits = Some(vec![]);
        let shown = label(&services, slim(), Used, &none(), true);
        let ids: Vec<_> = shown.cells.iter().map(|c| c.id).collect();
        assert_eq!(ids, ["claude", "gemini"]);
        assert_eq!(shown.cells[0].color, claude_color());
    }

    #[test]
    fn a_cell_shows_two_limits_picked_in_the_popup() {
        // Claude's first two show by default; a pick swaps one for its Opus
        // week. Picked limits show in the order picked, a pick that no longer
        // exists drops out, and people still read all three.
        let claude = |picks: &Picks| label(&mock(0), slim(), Used, picks, true).cells[1].clone();
        for look in &LOOKS {
            let cell = &label(&mock(0), look, Used, &none(), true).cells[1];
            assert_eq!(labels(cell), ["5h", "7d"], "{}", look.name);
            assert!(cell.plain.ends_with("Opus 7d 60%"), "{}", cell.plain);
        }
        let mut picks = Picks::default();
        for (picked, shown) in [
            (vec!["Opus 7d", "7d"], vec!["Opus 7d", "7d"]),
            (vec!["7d", "5h"], vec!["7d", "5h"]),
            (vec!["7d", "7d", "5h"], vec!["7d", "5h"]),
            (vec!["Opus 7d"], vec!["Opus 7d"]),
            (vec!["Sonnet 7d"], vec!["5h", "7d"]),
            (vec!["Sonnet 7d", "5h"], vec!["5h"]),
        ] {
            picks.set_limits("claude", picked.iter().map(|&l| l.into()).collect());
            assert_eq!(labels(&claude(&picks)), shown);
        }
        // Another service's picks leave Claude alone.
        let mut others = Picks::default();
        others.set_limits("gpt", vec!["7d".into()]);
        assert_eq!(labels(&claude(&others)), ["5h", "7d"]);
        assert_eq!(
            labels(&label(&mock(0), slim(), Used, &others, true).cells[0]),
            ["7d"]
        );
    }

    #[test]
    fn three_services_show_until_others_are_picked() {
        // GPT, Claude and Gemini show by default; picks show in the order
        // picked, up to the hook's four cells, and a service keeps its color
        // wherever it lands.
        let ids = |picks: &Picks| {
            label(&mock(0), slim(), Used, picks, true)
                .cells
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>()
        };
        // PC, this computer's meters, is listed last and off until picked.
        assert_eq!(mock(0).len(), 5);
        assert_eq!(mock(0)[4].id, "pc");
        assert_eq!(ids(&none()), ["gpt", "claude", "gemini"]);
        assert_eq!(none().services(&["gpt", "pc"]), ["gpt"]);
        assert!(none().services(&["pc"]).is_empty());
        let mut picks = Picks::default();
        for (picked, shown) in [
            (vec!["grok", "claude"], vec!["grok", "claude"]),
            (
                vec!["grok", "pc", "gpt", "claude", "gemini"],
                vec!["grok", "pc", "gpt", "claude"],
            ),
            (vec!["copilot", "grok", "grok"], vec!["grok"]),
            (vec!["copilot"], vec!["gpt", "claude", "gemini"]),
        ] {
            picks.set_services(picked.into_iter().map(String::from).collect());
            assert_eq!(ids(&picks), shown);
        }
        picks.set_services(vec!["grok".into(), "claude".into()]);
        let label = label(&mock(0), slim(), Used, &picks, true);
        assert_eq!(label.cells[1].color, claude_color());
        // Grok has 20% left, which warns in orange.
        let grok = &label.cells[0].gauges[0];
        assert_eq!(
            (grok.mark, grok.text.as_str(), grok.text_color),
            ("", "80%", CAUTION)
        );
    }

    #[test]
    fn left_colors_what_the_used_share_has_not_reached() {
        // The used share turns to track as it grows: from a bar's right end,
        // and clockwise from twelve o'clock on a dial, so what is left sits at
        // a bar's start and against a dial's end. The number shows what is
        // left too.
        for look in &LOOKS {
            let left = label(&mock(0), look, Left, &none(), true);
            let fills: Vec<_> = left
                .cells
                .iter()
                .flat_map(|c| &c.gauges)
                .map(|g| g.fill)
                .collect();
            let expected = [Some(720), Some(580), Some(580), Some(100), Some(910)];
            assert_eq!(fills, expected, "{}", look.name);
            assert_eq!(
                left.sizing(),
                label(&mock(0), look, Used, &none(), true).sizing()
            );
            assert_eq!(left.style.fill_end, u32::from(look.style.dial()));
            assert_eq!(
                Style {
                    fill_end: 0,
                    ..left.style
                },
                look.style
            );
        }
        assert_eq!(
            label(&mock(0), slim(), Left, &none(), true).summary(),
            "GPT 5h 72% 7d 58%  |  Claude 5h 58% 7d 10% Opus 7d 40%  |  Gemini 1d 91%"
        );
        let pie = label(&mock(0), find("pie").unwrap(), Left, &none(), true);
        assert_eq!(pie.cells[0].gauges[0].text, "72");
    }

    #[test]
    fn a_limit_warns_in_orange_at_20_left_and_in_red_with_the_mark_at_5() {
        // Measured by what is left, shown used or left; the mark is red's
        // alone, and with the colors off nothing warns, while the log still
        // marks red.
        use Reading::*;
        for (used, level) in [
            (79, Warn::None),
            (80, Warn::Caution),
            (94, Warn::Caution),
            (95, Warn::Warning),
            (100, Warn::Warning),
            (120, Warn::Warning),
        ] {
            assert_eq!(shown(Read(used), Used, Meaning::Limit), Some((used, level)));
            assert_eq!(
                shown(Read(used), Left, Meaning::Limit),
                Some((100 - used.min(100), level))
            );
            assert_eq!(
                shown(Failed(Some(used)), Used, Meaning::Limit).unwrap().1,
                level
            );
        }
        assert_eq!(
            (
                Warn::None.mark(),
                Warn::Caution.mark(),
                Warn::Warning.mark()
            ),
            ("", "", MARK)
        );
        assert_eq!(
            (
                Warn::None.color(),
                Warn::Caution.color(),
                Warn::Warning.color()
            ),
            (None, Some(CAUTION), Some(WARNING))
        );
        assert_eq!(Warn::Warning.when(false), Warn::None);
        // Gauges: orange without the mark, red with it, and neither off.
        let at = |used: u8| {
            let mut services = mock(0);
            services[0].limits = Some(vec![Limit {
                label: "5h".into(),
                reading: Read(used),
                resets: None,
                amount: None,
            }]);
            services
        };
        let one = |used: u8, warn: bool| {
            label(&at(used), slim(), Used, &none(), warn).cells[0].gauges[0].clone()
        };
        let orange = one(85, true);
        assert_eq!(
            (orange.mark, orange.text_color, orange.fill_color),
            ("", CAUTION, CAUTION)
        );
        let red = one(97, true);
        assert_eq!(
            (red.mark, red.text_color, red.fill_color),
            (MARK, WARNING, WARNING)
        );
        let off = one(97, false);
        assert_eq!(off.mark, "");
        assert_ne!(off.text_color, WARNING);
        assert_ne!(off.fill_color, WARNING);
        assert!(summary_of(&at(97)[0], Used).ends_with("5h !! 97%"));
        assert!(summary_of(&at(85)[0], Used).ends_with("5h 85%"));
    }

    #[test]
    fn a_light_box_draws_dark_values_and_deepens_the_service_colors() {
        // On a white box the values and labels are black at their usual
        // opacity, and a pale service color is deepened for the icon and
        // the gauge; a warning keeps its color. On a dark box nothing
        // changes.
        let white = slim().colored(Some(0xFFFF_FFFF), false);
        assert_eq!(
            (white.text, white.style.label_color),
            (0xF200_0000, 0x9900_0000)
        );
        let label = label(&mock(0), &white, Used, &none(), true);
        assert_eq!(label.style.box_color, 0xFFFF_FFFF);
        let gpt = &label.cells[0];
        assert_eq!(gpt.color, white.service_color(mock(0)[0].color));
        assert_ne!(gpt.color, mock(0)[0].color);
        assert_eq!(gpt.gauges[0].fill_color, gpt.color);
        assert_eq!(gpt.gauges[0].text_color, 0xF200_0000);
        let week = &label.cells[1].gauges[1];
        assert_eq!((week.text_color, week.fill_color), (CAUTION, CAUTION));
        let dark = slim().colored(None, true);
        let plain = super::label(&mock(0), &dark, Used, &none(), true);
        assert_eq!(plain.cells[0].color, mock(0)[0].color);
        assert_eq!(plain.cells[0].gauges[0].text_color, slim().text);
    }

    #[test]
    fn a_meter_shows_the_share_used_and_warns_when_high() {
        // PC's meters are this computer's share in use, whatever the fill
        // mode, and warn in red alone at 90% used, where a limit warns in
        // orange at 20% left; the log's label leaves their values out.
        use Reading::*;
        for fill in [Used, Left] {
            assert_eq!(
                shown(Read(24), fill, Meaning::Meter),
                Some((24, Warn::None))
            );
            assert_eq!(
                shown(Read(89), fill, Meaning::Meter),
                Some((89, Warn::None))
            );
            assert_eq!(
                shown(Read(90), fill, Meaning::Meter),
                Some((90, Warn::Warning))
            );
            assert_eq!(shown(Pending, fill, Meaning::Meter), None);
        }
        assert_eq!(
            shown(Read(80), Left, Meaning::Limit),
            Some((20, Warn::Caution))
        );
        let mut picks = Picks::default();
        picks.set_services(vec!["pc".into(), "gpt".into()]);
        let pc = |step, fill| label(&mock(step), slim(), fill, &picks, true).cells[0].clone();
        assert_eq!(pc(0, Left).plain, "PC CPU 24% RAM 61% GPU 8% VRAM 43%");
        assert_eq!(pc(0, Left).gauges, pc(0, Used).gauges);
        let high = pc(1, Left);
        assert_eq!(high.plain, "PC CPU !! 100% RAM !! 92% GPU 0% VRAM 0%");
        assert!(
            high.gauges
                .iter()
                .all(|g| g.fill_color == WARNING && g.mark == MARK)
        );
        assert_eq!(pc(2, Used).plain, "PC CPU -- RAM 61% GPU ? VRAM --");
        // Only the meters' readings and amounts are taken out.
        let quiet = steady(&mock(0));
        assert!(
            quiet[4]
                .limits
                .iter()
                .flatten()
                .all(|l| l.reading == Pending && l.amount.is_none())
        );
        assert!(
            mock(0)[4]
                .limits
                .iter()
                .flatten()
                .any(|l| l.amount.is_some())
        );
        assert_eq!(quiet[4].limits.as_ref().map(Vec::len), Some(4));
        assert_eq!(
            label(&quiet, slim(), Used, &none(), true),
            label(&mock(0), slim(), Used, &none(), true)
        );
        let mut busier = mock(0);
        busier[4] = mock(1)[4].clone();
        assert_ne!(
            label(&busier, slim(), Used, &picks, true),
            label(&mock(0), slim(), Used, &picks, true)
        );
        assert_eq!(
            label(&steady(&busier), slim(), Used, &picks, true),
            label(&quiet, slim(), Used, &picks, true)
        );
    }

    #[test]
    fn pc_shows_four_meters_as_the_look_fits_them() {
        // In any box, PC's four meters in one cell where the look sets
        // gauges side by side; stacked or as rings, two a cell. Never more
        // cells than the hook has; other services two limits at most.
        let meters: Vec<String> = ["CPU", "RAM", "GPU", "VRAM"].map(String::from).to_vec();
        let mut picks = Picks::default();
        picks.set_services(vec!["pc".into()]);
        picks.set_limits("pc", meters.clone());
        let shape = |look: &str, picks: &Picks| {
            label(&mock(0), find(look).unwrap(), Used, picks, true)
                .cells
                .iter()
                .map(|c| (c.id, c.gauges.len()))
                .collect::<Vec<_>>()
        };
        assert_eq!(shape("slim", &picks), [("pc", 4)]);
        for look in ["inline", "stack", "pie", "rings"] {
            assert_eq!(shape(look, &picks), [("pc", 2), ("pc", 2)], "{look}");
        }
        let four = label(&mock(0), slim(), Used, &picks, true);
        assert!(four.fits());
        assert_eq!(four.summary(), "PC CPU 24% RAM 61% GPU 8% VRAM 43%");
        assert_eq!(
            label(&mock(0), find("stack").unwrap(), Used, &picks, true).summary(),
            "PC CPU 24% RAM 61%  |  PC GPU 8% VRAM 43%"
        );
        picks.set_services(["gpt", "claude", "gemini", "pc"].map(String::from).to_vec());
        assert_eq!(shape("stack", &picks).len(), MAX_CELLS);
        let labels = ["5h", "7d", "Opus 7d"].map(String::from);
        picks.set_limits("claude", labels.to_vec());
        assert_eq!(picks.limits("claude", &labels), ["5h", "7d"]);
        assert_eq!((most_limits("pc"), most_limits("claude")), (4, 2));
        assert_eq!([1, 2, 3, 4].map(cell_count), [1, 1, 2, 2]);
    }

    #[test]
    fn each_gauge_says_where_its_colored_part_sits() {
        // What is left sits against a dial's end; a meter, always the share
        // used, starts at twelve beside it; a bar starts at its left end.
        let mut picks = Picks::default();
        picks.set_services(vec!["gpt".into(), "pc".into()]);
        let ends = |look: &str, fill| {
            label(&mock(0), find(look).unwrap(), fill, &picks, true)
                .cells
                .iter()
                .map(|c| c.gauges.iter().map(|g| g.end).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };
        assert_eq!(ends("rings", Left), [vec![true, true], vec![false, false]]);
        assert_eq!(ends("pie", Left), [vec![true, true], vec![false, false]]);
        assert_eq!(
            ends("rings", Used),
            [vec![false, false], vec![false, false]]
        );
        assert_eq!(ends("slim", Left), [vec![false, false], vec![false, false]]);
    }

    #[test]
    fn a_gauge_never_overfills() {
        let over = [Service {
            id: "gpt",
            name: "GPT".into(),
            color: WARNING,
            trouble: None,
            meaning: Meaning::Limit,
            limits: Some(vec![Limit {
                label: "5h".into(),
                reading: Reading::Read(255),
                resets: None,
                amount: None,
            }]),
        }];
        let gauge = &label(&over, slim(), Used, &none(), true).cells[0].gauges[0];
        assert_eq!(
            (gauge.mark, gauge.text.as_str(), gauge.fill),
            (MARK, "255%", Some(1000))
        );
        let gauge = &label(&over, slim(), Left, &none(), true).cells[0].gauges[0];
        assert_eq!(
            (gauge.mark, gauge.text.as_str(), gauge.fill),
            (MARK, "0%", Some(0))
        );
    }

    #[test]
    fn values_and_states_never_move_the_widest_form() {
        // Only a limit coming or going, or a different pick, may change the
        // widest form and so move the apps; a value or a reading state never
        // does.
        for look in &LOOKS {
            let labels: Vec<_> = (0..4)
                .map(|step| label(&mock(step), look, Used, &none(), true))
                .collect();
            assert!(labels.iter().all(Label::fits), "{}", look.name);
            assert_eq!(labels[1].sizing(), labels[0].sizing(), "{}", look.name);
            // Not read yet and failed reads keep their labels.
            let widest = |label: &Label, cell: usize| label.cells[cell].gauges[0].sizing.clone();
            assert_eq!(widest(&labels[2], 0), widest(&labels[0], 0));
            assert_eq!(widest(&labels[2], 1), widest(&labels[0], 1));
            for label in &labels {
                for g in label.cells.iter().flat_map(|c| &c.gauges) {
                    assert!(g.text.chars().count() <= g.sizing.chars().count());
                }
            }
            assert_eq!(label(&mock(4), look, Used, &none(), true), labels[0]);
        }
    }

    #[test]
    fn a_label_the_hook_would_refuse_is_caught_here() {
        let gauge = |text: &str, sizing: &str, fill| Gauge {
            label: String::new(),
            mark: "",
            text: text.into(),
            sizing: sizing.into(),
            fill,
            text_color: 0,
            fill_color: 0,
            end: false,
        };
        let cell = |gauges: Vec<Gauge>| Cell {
            id: "gpt",
            color: 0,
            plain: String::new(),
            gauges,
        };
        let label = |cells| Label {
            cells,
            style: slim().style,
        };
        let one = || cell(vec![gauge("x", "x", None)]);
        let refused = [
            vec![],
            vec![one(); MAX_CELLS + 1],
            vec![cell(vec![])],
            vec![cell(vec![gauge("x", "x", None); MAX_CELL_GAUGES + 1])],
            vec![cell(vec![gauge(&"x".repeat(CAPACITY), "x", None)])],
            vec![cell(vec![gauge("x", "", None)])],
            vec![cell(vec![gauge("a\u{1e}b", "x", None)])],
            vec![cell(vec![Gauge {
                mark: "\u{1d}",
                ..gauge("x", "x", None)
            }])],
            vec![cell(vec![gauge("x", "x", Some(1001))])],
        ];
        for cells in refused {
            assert!(!label(cells).fits());
        }
        let full = cell(vec![gauge("", "x", Some(1000)); MAX_CELL_GAUGES]);
        assert!(label(vec![full.clone(); MAX_GAUGES / MAX_CELL_GAUGES]).fits());
        assert!(!label(vec![full; MAX_GAUGES / MAX_CELL_GAUGES + 1]).fits());
        // A look out of range refuses the whole label.
        let mut outsized = label(vec![one()]);
        outsized.style.font = 2401;
        assert!(!outsized.fits());
    }
}
