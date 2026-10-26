//! What the popup shows: the view taken from the choice and the controller,
//! its summary, filter line and rows of chips.

use crate::choice::{BoxPlace, Choice, MAX_TILES};
use crate::style::{self, Background, Fill};
use crate::usage::{self, Amount, Limit, MAX_CELLS, Meaning, Reading, Service, most_limits};
use std::time::Instant;

/// The Customize chip's command, which the popup takes as its own action.
pub(super) const CUSTOMIZE: &str = "customize";

/// Where the box stands, for the popup's first lines.
#[derive(Clone, Copy, Debug, Default)]
pub struct Status {
    /// Why the box is not on the taskbar.
    pub reason: Option<crate::xaml::Reason>,
    /// What the last restart of Explorer came to, or that the settings were
    /// not saved.
    pub notice: Option<&'static str>,
    pub mock: bool,
    pub on_taskbar: bool,
    /// The attach waits for Explorer's Snap layouts thread.
    pub snap_wait: bool,
    /// Explorer is being restarted from the popup.
    pub restarting: bool,
    /// The boxes' settings are unfolded, by Edit or a tab; the popup opens
    /// with only the boxes' tabs.
    pub unfolded: bool,
}

/// Everything the popup shows, taken from the choice and the controller.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub(super) status: String,
    pub(super) warning: Option<&'static str>,
    /// Whether the warning offers Restart Explorer.
    pub(super) offer_restart: bool,
    /// A line under the status.
    pub(super) notice: Option<&'static str>,
    pub(super) services: Vec<Summary>,
    /// Every service offered, as the summary's filter line turns it on and
    /// off.
    pub(super) filter: Vec<Pick>,
    /// The boxes as tabs, and whether another can be added.
    pub(super) tabs: Vec<Tab>,
    pub(super) adding: bool,
    /// The chosen box's rows, on its card, and those of every box under it.
    pub(super) rows: Vec<Row>,
    pub(super) all: Vec<Row>,
    /// When Refresh now can be pressed; never with the mock figures.
    pub(super) refresh: Option<Instant>,
    pub(super) settings: bool,
    pub(super) restarting: bool,
    /// Whether the boxes' settings show, or only their tabs.
    pub(super) unfolded: bool,
}

/// A service in the summary's filter line: whether the summary shows it,
/// and whether that can change now, which it cannot for the last one on.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Pick {
    pub(super) id: &'static str,
    pub(super) name: String,
    pub(super) color: u32,
    pub(super) on: bool,
    pub(super) enabled: bool,
}

/// A service in the box and each of its limits; None until its first read.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Summary {
    pub(super) id: &'static str,
    pub(super) name: String,
    pub(super) color: u32,
    pub(super) lines: Option<Vec<Line>>,
    /// Why the last read failed and when the next is.
    pub(super) trouble: Option<Note>,
    /// What the title row says on its right: that VRAM is the memory a GPU
    /// built in shares.
    pub(super) aside: Option<&'static str>,
}

/// A failed read as the person can act on it: what went wrong, what they
/// can do when there is something, and when it is tried again. The tool's
/// own words are in the log.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Note {
    pub(super) what: String,
    pub(super) todo: Option<String>,
    pub(super) when: String,
}

/// A limit as the popup shows it: the value under the fill mode, how much of
/// the bar it colors, whether it warns or is kept from a failed read, the
/// time until it starts over, and, for this computer's memory, how much is
/// in use of how much there is.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Line {
    pub(super) label: String,
    pub(super) text: String,
    pub(super) share: Option<u8>,
    /// How much it warns, with the warning colors on.
    pub(super) warn: usage::Warn,
    pub(super) stale: bool,
    pub(super) resets: String,
    pub(super) amount: String,
}

/// A row of chips under a title; `lead` names the service a row of limits
/// is for, and `note` stands in for chips there are none of, or, in a note
/// row, is its text.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Row {
    pub(super) title: &'static str,
    pub(super) lead: Option<String>,
    pub(super) chips: Vec<Chip>,
    pub(super) note: Option<String>,
    pub(super) shape: Shape,
    /// The box's color, as a swatch at the row's end with an eyedropper
    /// beside it.
    pub(super) color: Option<u32>,
}

/// How a row's chips are drawn: chips that wrap; smaller ones in the small
/// font; one line of segments, one of which is chosen; services as the
/// summary's filter draws them, an icon and a small name, faint when off; or
/// a small line of text with its chips at the end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Shape {
    Chips,
    Toggles,
    Segments,
    Services,
    Note,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Chip {
    pub(super) text: String,
    pub(super) on: bool,
    pub(super) enabled: bool,
    pub(super) command: String,
    /// What dropping it at another place sends, less the place: only a chip
    /// that is on moves, among those on.
    pub(super) drag: Option<String>,
    /// A service's icon and color, in a row of services.
    pub(super) icon: Option<(&'static str, u32)>,
}

/// A box as a tab over the chosen box's card: its name, whether it is the
/// one chosen, what choosing it sends, and whether it can be removed from
/// its tab.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Tab {
    pub(super) text: String,
    pub(super) on: bool,
    pub(super) command: String,
    pub(super) close: bool,
    /// The box is hidden: its tab is faint.
    pub(super) hidden: bool,
}

/// The popup's view of the choice and the services now. `refresh` is when
/// Refresh now is taken again, a moment that stays put, or never.
pub fn view(choice: &Choice, status: Status, refresh: Option<Instant>, settings: bool) -> View {
    view_at(choice, status, refresh, settings, usage::unix_now())
}

pub(super) fn view_at(
    choice: &Choice,
    status: Status,
    refresh: Option<Instant>,
    settings: bool,
    unix: i64,
) -> View {
    let offers = choice.offers();
    let read = choice.read();
    let fill = choice.fill();
    // In the summary's own order, apart from the box's.
    let summary_ids = choice.summary(&offers);
    // Each offered service in the providers' order, which stays put as
    // services are turned on and off.
    let filter = offers
        .iter()
        .map(|o| {
            let on = summary_ids.contains(&o.id);
            Pick {
                id: o.id,
                name: o.name.clone(),
                color: read.iter().find(|s| s.id == o.id).map_or(0, |s| s.color),
                on,
                enabled: !on || summary_ids.len() > 1,
            }
        })
        .collect();
    let services = summary_ids
        .iter()
        .filter_map(|id| read.iter().find(|s| s.id == *id))
        .map(|s| {
            let order = offers
                .iter()
                .find(|o| o.id == s.id)
                .map(|o| choice.summary_limits(o))
                .unwrap_or_default();
            summary(s, &order, fill, choice.warn(), unix)
        })
        .collect();
    let mut words = vec![if status.on_taskbar {
        "On the taskbar"
    } else {
        "Tray only"
    }];
    if status.mock {
        words.push("Mock data");
    }
    View {
        status: words.join(" · "),
        // The Snap layouts wait first, then why the box is off.
        warning: if status.snap_wait {
            Some("Restart Explorer to show the box.")
        } else {
            status.reason.map(|r| r.text)
        },
        offer_restart: status.snap_wait || status.reason.is_some_and(|r| r.restart),
        notice: status.notice,
        services,
        filter,
        tabs: tabs(choice),
        adding: choice.tiles().len() < MAX_TILES,
        rows: rows(choice, settings),
        all: all_rows(choice),
        refresh,
        settings,
        restarting: status.restarting,
        unfolded: status.unfolded,
    }
}

/// A service's summary, its lines in `order` by label, and any limit it
/// leaves out after them.
pub(super) fn summary(
    service: &Service,
    order: &[String],
    fill: Fill,
    warn: bool,
    unix: i64,
) -> Summary {
    Summary {
        id: service.id,
        name: service.name.clone(),
        color: service.color,
        lines: service.limits.as_ref().map(|limits| {
            let listed = order
                .iter()
                .filter_map(|label| limits.iter().find(|l| &l.label == label));
            let rest = limits.iter().filter(|l| !order.contains(&l.label));
            listed
                .chain(rest)
                .map(|l| line(l, fill, service.meaning, warn, unix))
                .collect()
        }),
        trouble: service.trouble.as_ref().map(|t| {
            let advice = crate::advice::advise(service.id, &service.name, &t.why);
            Note {
                what: advice.what,
                todo: advice.todo,
                when: match until(t.retry_at, unix) {
                    left if left.is_empty() => "Trying again".into(),
                    left => format!("Next try in {left}"),
                },
            }
        }),
        aside: service
            .limits
            .iter()
            .flatten()
            .any(|l| l.amount.is_some_and(|a| a.shared))
            .then_some("VRAM: shared GPU memory"),
    }
}

/// A limit's value as the box shows it, `--` not read yet and `?` a failed
/// read with nothing kept, warning as the box does when `warn`.
pub(super) fn line(limit: &Limit, fill: Fill, meaning: Meaning, warn: bool, unix: i64) -> Line {
    let value = usage::shown(limit.reading, fill, meaning);
    Line {
        label: limit.label.clone(),
        text: match (limit.reading, value) {
            (_, Some((share, _))) => format!("{share}%"),
            (Reading::Failed(None), None) => "?".into(),
            _ => "--".into(),
        },
        share: value.map(|(share, _)| share.min(100)),
        warn: value
            .map_or(usage::Warn::None, |(_, level)| level)
            .when(warn),
        stale: matches!(limit.reading, Reading::Failed(_)),
        resets: limit.resets.map(|at| until(at, unix)).unwrap_or_default(),
        amount: limit.amount.map(amount_text).unwrap_or_default(),
    }
}

/// An amount as the popup shows it, in GB of 2^30 bytes as Task Manager
/// counts them: one decimal under 100, whole from there.
pub(super) fn amount_text(amount: Amount) -> String {
    let gb = |bytes: u64| {
        let gb = bytes as f64 / (1u64 << 30) as f64;
        if gb < 99.95 {
            format!("{gb:.1}")
        } else {
            format!("{gb:.0}")
        }
    };
    format!("{}/{} GB", gb(amount.used), gb(amount.total))
}

/// How long until `at`, for people: `2h 14m`, `3d 4h`, `45m` or `<1m`;
/// nothing once it has passed. The notifications say it too.
pub fn until(at: i64, now: i64) -> String {
    let left = at - now;
    if left <= 0 {
        return String::new();
    }
    let minutes = left / 60;
    match (minutes / 1440, minutes / 60 % 24, minutes % 60) {
        (0, 0, 0) => "<1m".into(),
        (0, 0, m) => format!("{m}m"),
        (0, h, 0) => format!("{h}h"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, 0, _) => format!("{d}d"),
        (d, h, _) => format!("{d}d {h}h"),
    }
}

/// A look's name as the popup shows it.
pub(super) fn title(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
pub(super) fn fill_title(fill: Fill) -> &'static str {
    match fill {
        Fill::Used => "Used",
        Fill::Left => "Remaining",
    }
}
pub(super) fn background_title(background: Background) -> &'static str {
    match background {
        Background::Box => "One box",
        Background::Cards => "Per service",
    }
}

/// A box's places as the popup's segments name them, and the line under
/// them says the chosen one in full.
pub(super) fn box_place_title(place: BoxPlace) -> &'static str {
    match place {
        BoxPlace::Tray => "Tray",
        BoxPlace::Left => "Left",
        BoxPlace::BeforeStart => "Start",
        BoxPlace::AfterApps => "Apps",
    }
}
/// The line under the places while the box is hidden.
pub(super) const HIDDEN_ABOUT: &str = "Hidden: not on the taskbar";
pub(super) fn box_place_about(place: BoxPlace) -> &'static str {
    match place {
        BoxPlace::Tray => "Beside the tray",
        BoxPlace::Left => "Left end of the taskbar",
        BoxPlace::BeforeStart => "Before the Start button",
        BoxPlace::AfterApps => "After the last app",
    }
}

pub(super) fn chip(text: &str, on: bool, enabled: bool, command: String) -> Chip {
    Chip {
        text: text.into(),
        on,
        enabled,
        command,
        drag: None,
        icon: None,
    }
}
pub(super) fn row(title: &'static str, chips: Vec<Chip>) -> Row {
    Row {
        title,
        lead: None,
        chips,
        note: None,
        shape: Shape::Chips,
        color: None,
    }
}
fn segments(title: &'static str, chips: Vec<Chip>) -> Row {
    Row {
        shape: Shape::Segments,
        ..row(title, chips)
    }
}

/// The boxes as tabs by what they show, the chosen one on and, with
/// another left, removable from its tab.
pub(super) fn tabs(choice: &Choice) -> Vec<Tab> {
    let current = choice.current();
    let tiles = choice.tiles();
    let count = tiles.len();
    choice
        .tile_names(&choice.offers())
        .into_iter()
        .enumerate()
        .map(|(index, text)| Tab {
            text,
            on: index == current,
            command: format!("select\t{index}"),
            close: index == current && count > 1,
            hidden: tiles.get(index).is_some_and(|t| t.hidden),
        })
        .collect()
}

/// What every box shares: whether gauges and numbers show what is used or
/// what is left, under a caption that says so, the row as wide as the
/// popup.
pub(super) fn all_rows(choice: &Choice) -> Vec<Row> {
    let fill = choice.fill();
    vec![segments(
        "",
        Fill::ALL
            .iter()
            .map(|&f| {
                chip(
                    fill_title(f),
                    f == fill,
                    true,
                    format!("fill\t{}", f.name()),
                )
            })
            .collect(),
    )]
}
/// Whether an item can be turned on or off: at most `most` on, and never the
/// last one off.
pub(super) fn unlocked(on: bool, count: usize, most: usize) -> bool {
    if on { count > 1 } else { count < most }
}

/// The chosen box's rows, on its card: its look, the presets as segments
/// and the custom boxes as chips under them; its place as segments, with a
/// line saying it in full and its order among the boxes there; its
/// background; its services; and each shown service's limits. Services and
/// limits are listed as the menu listed them, the shown ones first, and
/// those can be dragged.
pub(super) fn rows(choice: &Choice, settings: bool) -> Vec<Row> {
    let offers = choice.offers();
    let mut rows = Vec::new();
    // A custom box's blocks choose the services and limits, and it is drawn
    // as one box: those rows are dimmed while it shows.
    let custom = choice.custom();
    let free = custom.is_none();
    let look = match &custom {
        Some(_) => "",
        None => choice.look().name,
    };
    let looks: Vec<Chip> = style::LOOKS
        .iter()
        .map(|l| {
            chip(
                &title(l.name),
                l.name == look,
                true,
                format!("look\t{}", l.name),
            )
        })
        .collect();
    // The samples are the editor's, unless one is in use.
    let mut customs: Vec<Chip> = choice
        .pickable_boxes()
        .iter()
        .map(|b| {
            chip(
                &b.name,
                custom.as_ref().is_some_and(|c| c.name == b.name),
                true,
                format!("look\tbox:{}", b.name),
            )
        })
        .collect();
    // The editor, in the settings window.
    customs.push(chip("Customize…", false, settings, CUSTOMIZE.into()));
    let background = choice.background();
    rows.extend([
        segments("Style", looks),
        Row {
            shape: Shape::Toggles,
            ..row("", customs)
        },
    ]);
    // Where the box goes, said in full under the segments, and its order
    // among the boxes there.
    // Last, Hide: the box keeps its place, which shows again when one is
    // pressed.
    let at = choice.box_place();
    let hidden = choice.box_hidden();
    let mut places: Vec<Chip> = BoxPlace::ALL
        .iter()
        .map(|&p| {
            chip(
                box_place_title(p),
                p == at && !hidden,
                true,
                format!("place\t{}", p.name()),
            )
        })
        .collect();
    places.push(chip(
        "Hide",
        hidden,
        true,
        format!("place\t{}", crate::choice::HIDDEN),
    ));
    rows.push(segments("Place", places));
    let (earlier, later) = choice.can_move_tile();
    rows.push(Row {
        note: Some(
            if hidden {
                HIDDEN_ABOUT
            } else {
                box_place_about(at)
            }
            .into(),
        ),
        shape: Shape::Note,
        ..row(
            "",
            if earlier || later {
                vec![
                    chip("←", false, earlier, "order\tearlier".into()),
                    chip("→", false, later, "order\tlater".into()),
                ]
            } else {
                Vec::new()
            },
        )
    });
    // The box's color at the line's end, which a custom box takes too.
    rows.push(Row {
        color: Some(choice.color().unwrap_or(style::BOX_COLOR)),
        ..segments(
            "Background",
            Background::ALL
                .iter()
                .map(|&b| {
                    chip(
                        background_title(b),
                        b == background,
                        free,
                        format!("background\t{}", b.name()),
                    )
                })
                .collect(),
        )
    });
    let listed = choice.listed(&offers);
    let shown = choice.services(&offers);
    // Each service by its icon in its color, as the summary's filter draws
    // it.
    let read = choice.read();
    let color = |id: &str| read.iter().find(|s| s.id == id).map_or(0, |s| s.color);
    let mut services = Row {
        shape: Shape::Services,
        ..row(
            "Services",
            listed
                .iter()
                .map(|o| {
                    let on = shown.contains(&o.id);
                    Chip {
                        drag: (on && free).then(|| format!("move\t{}\t", o.id)),
                        icon: Some((o.id, color(o.id))),
                        ..chip(
                            &o.name,
                            on,
                            free && unlocked(on, shown.len(), MAX_CELLS),
                            format!("service\t{}", o.id),
                        )
                    }
                })
                .collect(),
        )
    };
    if services.chips.is_empty() {
        services.note = Some("None connected".into());
    }
    rows.push(services);
    for (index, offer) in listed.iter().filter(|o| shown.contains(&o.id)).enumerate() {
        let limits = choice.shown(offer);
        rows.push(Row {
            title: if index == 0 { "Limits" } else { "" },
            lead: Some(offer.name.clone()),
            chips: choice
                .listed_limits(offer)
                .iter()
                .map(|label| {
                    let on = limits.contains(label);
                    Chip {
                        drag: (on && free).then(|| format!("move-limit\t{}\t{label}\t", offer.id)),
                        ..chip(
                            label,
                            on,
                            free && unlocked(on, limits.len(), most_limits(offer.id)),
                            format!("limit\t{}\t{label}", offer.id),
                        )
                    }
                })
                .collect(),
            note: offer.labels.is_empty().then(|| "Not read yet".into()),
            shape: Shape::Toggles,
            color: None,
        });
    }
    rows
}
