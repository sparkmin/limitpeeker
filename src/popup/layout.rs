//! Where everything in the popup goes, in DIPs, as a pure function of the
//! view and a text measure, with the controls it makes.

use super::*;
use crate::providers::ReadNow;
use std::time::Instant;
use windows::Win32::Foundation::*;

/// The popup's width, its margin and the room a limit's columns take, in
/// DIPs: the service's name, and for each limit its label, bar, value and
/// the time until it starts over.
pub const WIDTH: f32 = 340.0;
pub(super) const PAD: f32 = 12.0;
pub(super) const RIGHT: f32 = WIDTH - PAD;
pub(super) const NAME_X: f32 = PAD + 22.0;
pub(super) const BAR_X: f32 = NAME_X + 52.0;
pub(super) const VALUE_W: f32 = 40.0;
pub(super) const RESETS_W: f32 = 58.0;
/// The right column when a line tells an amount, such as `12.9/15.8 GB`;
/// every bar in the popup is shorter by as much, so the lines stay even.
pub(super) const AMOUNTS_W: f32 = 72.0;
/// The right of a failed read's line, where the next try is told, as
/// `Next try in 14m`.
pub(super) const NEXT_TRY_W: f32 = 96.0;
/// Where chips start after a row's title, their height, the space between
/// them and inside them, and the height of Refresh now, Settings and Exit.
pub(super) const CHIPS_X: f32 = PAD + 80.0;
pub(super) const CHIP_H: f32 = 24.0;
pub(super) const CHIP_GAP: f32 = 4.0;
pub(super) const CHIP_PAD: f32 = 10.0;
pub(super) const ITEM_H: f32 = 32.0;
/// The box tabs: their height, the room around a name, the gap between
/// them, and the room a close button takes; a glyph button's size; the room
/// inside the chosen box's card; the room around a segment's name; and a
/// note row's height and its chips' width.
pub(super) const TAB_H: f32 = 28.0;
pub(super) const TAB_PAD: f32 = 10.0;
pub(super) const TAB_GAP: f32 = 2.0;
pub(super) const CLOSE_W: f32 = 22.0;
/// How much of the tabs' room the chosen tab may take when they do not all
/// fit.
pub(super) const CHOSEN_TAB: f32 = 0.45;
pub(super) const GLYPH_W: f32 = 22.0;
pub(super) const CARD_INSET: f32 = 8.0;
pub(super) const SEGMENT_PAD: f32 = 8.0;
pub(super) const NOTE_H: f32 = 20.0;
pub(super) const TOGGLE_H: f32 = 22.0;
pub(super) const TOGGLE_PAD: f32 = 8.0;
pub(super) const NOTE_CHIP_W: f32 = 28.0;
/// Segoe Fluent Icons' close and add.
pub(super) const CLOSE_GLYPH: char = '\u{E711}';
pub(super) const ADD_GLYPH: char = '\u{E710}';
/// The button at the end of the boxes' tabs that unfolds their settings and
/// folds them: its words, Segoe Fluent Icons' chevrons, the room around the
/// words and the chevron's width.
pub(super) const UNFOLD: (&str, char) = ("Edit", '\u{E70D}');
pub(super) const FOLD: (&str, char) = ("Done", '\u{E70E}');
pub(super) const DISCLOSE_PAD: f32 = 10.0;
pub(super) const DISCLOSE_GLYPH_W: f32 = 16.0;
/// The summary's filter line: an item's height, the glyph that marks the
/// line, and how strongly a service left out of the summary is drawn in it.
pub(super) const FILTER_H: f32 = 22.0;
pub(super) const FILTER_GLYPH: char = '\u{E71C}';
pub(super) const FILTER_OFF: f32 = 0.4;
/// The room inside the filter line's card, and its corners.
pub(super) const FILTER_INSET: f32 = 4.0;
pub(super) const CARD_RADIUS: f32 = 6.0;
/// How far the popup keeps from the work area's edges.
pub(super) const MARGIN: f32 = 8.0;
/// Segoe Fluent Icons' eyedropper, beside the box's color.
pub(super) const EYEDROPPER_GLYPH: char = '\u{EF3C}';
/// Segoe Fluent Icons' refresh, gear and power.
pub(super) const REFRESH_GLYPH: char = '\u{E72C}';
pub(super) const SETTINGS_GLYPH: char = '\u{E713}';
pub(super) const EXIT_GLYPH: char = '\u{E7E8}';
/// How much of its opacity a faded value, or a control that cannot be
/// pressed, keeps. Faded values match the box.
pub(super) const FADED: f32 = 0.45;
/// How far the pointer moves with the button down before a press becomes a
/// drag, in DIPs, and how thick the mark of where it would go is.
pub(super) const DRAG_START: f32 = 4.0;
pub(super) const DROP_MARK: f32 = 2.0;

/// A place and size in DIPs, from the popup's top-left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Rect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}
impl Rect {
    pub(super) fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    #[cfg(test)]
    pub(super) fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum Font {
    Title,
    Strong,
    Body,
    Caption,
    /// A caption for what is on, among the summary's filter and a box's
    /// services.
    Picked,
    Glyph,
    /// A smaller glyph, for a tab's close and a new tab.
    Small,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum Align {
    Start,
    End,
    Center,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Ink {
    Primary,
    Secondary,
    Tertiary,
    Warning,
    Caution,
}

/// One thing drawn that cannot be pressed.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Part {
    Text {
        at: Rect,
        text: String,
        font: Font,
        align: Align,
        ink: Ink,
        faded: bool,
    },
    Bar {
        at: Rect,
        share: Option<u8>,
        color: u32,
        warn: crate::usage::Warn,
        faded: bool,
    },
    Icon {
        at: Rect,
        id: &'static str,
        color: u32,
    },
    Rule {
        at: Rect,
    },
    /// A card behind what it holds: the summary's filter line.
    Panel {
        at: Rect,
    },
    /// The chosen box's tab and the card under it, as one shape: the tab's
    /// sides run into the card with no edge between.
    TabCard {
        tab: Rect,
        card: Rect,
    },
    /// The frame of a row of segments.
    Segments {
        at: Rect,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Kind {
    Chip {
        on: bool,
    },
    Item {
        glyph: char,
    },
    /// A box's tab, the chosen one drawn by its card; a hidden box's is
    /// faint.
    Tab {
        on: bool,
        hidden: bool,
    },
    /// A small button that is a glyph: a tab's close, or a new tab.
    Glyph {
        glyph: char,
    },
    /// One of a row of segments, the chosen one in the accent.
    Segment {
        on: bool,
    },
    /// A chip in the small font: a limit, or a custom box.
    Toggle {
        on: bool,
    },
    /// Edit or Done at the end of the boxes' tabs, in the accent: the words
    /// and a chevron, down while folded, up while unfolded.
    Disclose {
        glyph: char,
    },
    /// A service in the summary's filter line: its icon and name, as the
    /// summary draws them, faint when the summary leaves it out; a box's
    /// services are drawn so too.
    Filter {
        on: bool,
        id: &'static str,
        color: u32,
    },
    /// The box's color, at the end of the Background line; pressed, it
    /// opens the color dialog.
    Swatch {
        color: u32,
    },
    /// The eyedropper beside it, on while a color is taken from the
    /// screen.
    Dropper,
}

/// One thing that can be pressed, in the order focus moves through them;
/// `drag` is its place in the layout's drags, if it can be dragged.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Control {
    pub(super) at: Rect,
    pub(super) text: String,
    pub(super) kind: Kind,
    pub(super) enabled: bool,
    pub(super) action: Action,
    pub(super) drag: Option<usize>,
}

/// Something that can be dragged to another place in its group: a chip that
/// is on, a service's name in the summary, or one of its lines. Dropped at
/// place `to`, it sends `command` with `to` after it. `across` groups lie
/// side by side, the others one under another.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Drag {
    pub(super) at: Rect,
    pub(super) group: String,
    pub(super) index: usize,
    pub(super) command: String,
    pub(super) across: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Layout {
    pub(super) parts: Vec<Part>,
    pub(super) controls: Vec<Control>,
    pub(super) drags: Vec<Drag>,
    pub(super) height: f32,
}
impl Layout {
    pub(super) fn text(&mut self, at: Rect, text: &str, font: Font, align: Align, ink: Ink) {
        self.parts.push(Part::Text {
            at,
            text: text.into(),
            font,
            align,
            ink,
            faded: false,
        });
    }
    /// A line across, with room above and below; where what follows starts.
    pub(super) fn rule(&mut self, y: f32) -> f32 {
        self.parts.push(Part::Rule {
            at: Rect {
                x: PAD,
                y: y + 8.0,
                w: RIGHT - PAD,
                h: 1.0,
            },
        });
        y + 17.0
    }
}

/// Where everything goes, `measure` giving a text's width in DIPs. Chips
/// that do not fit on a line go on to the next. `now` counts Refresh now
/// down.
pub(super) fn layout(view: &View, now: Instant, measure: &dyn Fn(&str, Font) -> f32) -> Layout {
    let mut out = Layout::default();
    let full = |y: f32, h: f32| Rect {
        x: PAD,
        y,
        w: RIGHT - PAD,
        h,
    };
    let mut y = PAD;
    out.text(
        full(y, 20.0),
        "LimitPeeker",
        Font::Title,
        Align::Start,
        Ink::Primary,
    );
    y += 20.0;
    out.text(
        full(y, 16.0),
        &view.status,
        Font::Caption,
        Align::Start,
        Ink::Secondary,
    );
    y += 16.0;
    if let Some(warning) = view.warning {
        out.text(
            full(y, 16.0),
            warning,
            Font::Caption,
            Align::Start,
            Ink::Warning,
        );
        y += 16.0;
        // What the warning asks for, in one press.
        if view.offer_restart {
            let text = if view.restarting {
                "Restarting Explorer…"
            } else {
                "Restart Explorer"
            };
            out.controls.push(Control {
                at: Rect {
                    x: PAD,
                    y: y + 4.0,
                    w: measure(text, Font::Body).ceil() + 2.0 * CHIP_PAD,
                    h: CHIP_H,
                },
                text: text.into(),
                kind: Kind::Chip { on: false },
                enabled: !view.restarting,
                action: Action::RestartExplorer,
                drag: None,
            });
            y += CHIP_H + 4.0;
            // What pressing it also does.
            out.text(
                full(y, 16.0),
                "Closes open File Explorer windows.",
                Font::Caption,
                Align::Start,
                Ink::Tertiary,
            );
            y += 16.0;
        }
    }
    // What the last restart of Explorer came to, or that the settings were
    // not saved.
    if let Some(notice) = view.notice {
        out.text(
            full(y, 16.0),
            notice,
            Font::Caption,
            Align::Start,
            Ink::Secondary,
        );
        y += 16.0;
    }
    y = out.rule(y);
    // Which services the summary shows, as a legend that turns them on and
    // off, in the summary's own look: on top, so it stays put as the
    // summary grows and shrinks.
    if !view.filter.is_empty() {
        // On a card of its own, so it reads apart from the summary under it.
        let card = out.parts.len();
        let inner = PAD + FILTER_INSET;
        out.text(
            Rect {
                x: inner,
                y: y + FILTER_INSET,
                w: 16.0,
                h: FILTER_H,
            },
            &FILTER_GLYPH.to_string(),
            Font::Glyph,
            Align::Center,
            Ink::Tertiary,
        );
        let first = inner + 16.0 + FILTER_INSET;
        let mut x = first;
        let mut top = y + FILTER_INSET;
        // As wide as its name on, so a pick does not move the others.
        for pick in &view.filter {
            let w = 6.0 + 16.0 + 4.0 + measure(&pick.name, Font::Picked).ceil() + 6.0;
            if x > first && x + w > RIGHT - FILTER_INSET {
                x = first;
                top += FILTER_H + 2.0;
            }
            out.controls.push(Control {
                at: Rect {
                    x,
                    y: top,
                    w,
                    h: FILTER_H,
                },
                text: pick.name.clone(),
                kind: Kind::Filter {
                    on: pick.on,
                    id: pick.id,
                    color: pick.color,
                },
                enabled: pick.enabled,
                action: Action::Apply(format!("summary-service\t{}", pick.id)),
                drag: None,
            });
            x += w + 2.0;
        }
        let bottom = top + FILTER_H + FILTER_INSET;
        out.parts.insert(
            card,
            Part::Panel {
                at: Rect {
                    x: PAD,
                    y,
                    w: RIGHT - PAD,
                    h: bottom - y,
                },
            },
        );
        y = bottom + 10.0;
    }
    if view.services.is_empty() {
        out.text(
            full(y, 20.0),
            "No service connected",
            Font::Body,
            Align::Start,
            Ink::Secondary,
        );
        y += 20.0;
    }
    let right_w = if view
        .services
        .iter()
        .flat_map(|s| s.lines.iter().flatten())
        .any(|l| !l.amount.is_empty())
    {
        AMOUNTS_W
    } else {
        RESETS_W
    };
    let value_x = RIGHT - right_w - VALUE_W;
    for (index, service) in view.services.iter().enumerate() {
        if index > 0 {
            y += 6.0;
        }
        out.drags.push(Drag {
            at: full(y, 22.0),
            group: "summary".into(),
            index,
            command: format!("move-summary\t{}\t", service.id),
            across: false,
        });
        out.parts.push(Part::Icon {
            at: Rect {
                x: PAD,
                y: y + 3.0,
                w: 16.0,
                h: 16.0,
            },
            id: service.id,
            color: service.color,
        });
        let named = |y: f32, h: f32| Rect {
            x: NAME_X,
            y,
            w: RIGHT - NAME_X,
            h,
        };
        out.text(
            named(y, 22.0),
            &service.name,
            Font::Strong,
            Align::Start,
            Ink::Primary,
        );
        if let Some(aside) = service.aside {
            out.text(
                named(y, 22.0),
                aside,
                Font::Caption,
                Align::End,
                Ink::Tertiary,
            );
        }
        y += 22.0;
        let note = match &service.lines {
            None => Some("Waiting for the first read"),
            Some(lines) if lines.is_empty() => Some("No limits reported"),
            Some(_) => None,
        };
        if let Some(note) = note {
            out.text(
                named(y, 20.0),
                note,
                Font::Caption,
                Align::Start,
                Ink::Tertiary,
            );
            y += 20.0;
        }
        // What went wrong with the last read, with the next try on the
        // right as a limit's reset is, and under it what the person can do.
        if let Some(trouble) = &service.trouble {
            let line = named(y, 20.0);
            out.text(
                Rect {
                    w: line.w - NEXT_TRY_W,
                    ..line
                },
                &trouble.what,
                Font::Caption,
                Align::Start,
                Ink::Secondary,
            );
            out.text(
                Rect {
                    x: RIGHT - NEXT_TRY_W,
                    w: NEXT_TRY_W,
                    ..line
                },
                &trouble.when,
                Font::Caption,
                Align::End,
                Ink::Tertiary,
            );
            y += 20.0;
            if let Some(todo) = &trouble.todo {
                out.text(
                    named(y, 20.0),
                    todo,
                    Font::Caption,
                    Align::Start,
                    Ink::Tertiary,
                );
                y += 20.0;
            }
        }
        for (place, line) in service.lines.iter().flatten().enumerate() {
            out.drags.push(Drag {
                at: named(y, 20.0),
                group: format!("summary\t{}", service.id),
                index: place,
                command: format!("move-summary-limit\t{}\t{}\t", service.id, line.label),
                across: false,
            });
            out.text(
                Rect {
                    x: NAME_X,
                    y,
                    w: BAR_X - NAME_X - 4.0,
                    h: 20.0,
                },
                &line.label,
                Font::Caption,
                Align::Start,
                Ink::Secondary,
            );
            out.parts.push(Part::Bar {
                at: Rect {
                    x: BAR_X,
                    y: y + 8.0,
                    w: value_x - 8.0 - BAR_X,
                    h: 4.0,
                },
                share: line.share,
                color: service.color,
                warn: line.warn,
                faded: line.stale,
            });
            out.parts.push(Part::Text {
                at: Rect {
                    x: value_x,
                    y,
                    w: VALUE_W,
                    h: 20.0,
                },
                text: line.text.clone(),
                font: Font::Body,
                align: Align::End,
                ink: match line.warn {
                    crate::usage::Warn::Warning => Ink::Warning,
                    crate::usage::Warn::Caution => Ink::Caution,
                    crate::usage::Warn::None => Ink::Primary,
                },
                faded: line.stale,
            });
            out.text(
                Rect {
                    x: RIGHT - right_w,
                    y,
                    w: right_w,
                    h: 20.0,
                },
                if line.amount.is_empty() {
                    &line.resets
                } else {
                    &line.amount
                },
                Font::Caption,
                Align::End,
                Ink::Tertiary,
            );
            y += 20.0;
        }
    }
    y = out.rule(y);
    // The boxes as tabs, the chosen one joined to the card its rows are on;
    // folded, only their names, each unfolding on its box, and Edit at the
    // end; unfolded, Done there.
    let unfolded = view.unfolded;
    let (words, glyph) = if unfolded { FOLD } else { UNFOLD };
    let disclose_w = measure(words, Font::Body).ceil() + 2.0 * DISCLOSE_PAD + DISCLOSE_GLYPH_W;
    let add_w = if unfolded { GLYPH_W } else { 0.0 };
    let room = RIGHT - PAD - add_w - disclose_w - TAB_GAP * (view.tabs.len() + 1) as f32;
    let asked: Vec<f32> = view
        .tabs
        .iter()
        .map(|tab| {
            let close = if tab.close && unfolded { CLOSE_W } else { 0.0 };
            measure(&tab.text, Font::Body).ceil() + 2.0 * TAB_PAD + close
        })
        .collect();
    let chosen_tab = view.tabs.iter().position(|t| t.on).filter(|_| unfolded);
    let widths = tab_widths(&asked, chosen_tab, room);
    let mut x = PAD;
    let mut chosen = Rect {
        x: PAD,
        y,
        w: 0.0,
        h: TAB_H,
    };
    for (tab, &w) in view.tabs.iter().zip(&widths) {
        let close = if tab.close && unfolded { CLOSE_W } else { 0.0 };
        let at = Rect { x, y, w, h: TAB_H };
        out.controls.push(Control {
            at: Rect { w: w - close, ..at },
            text: fit(&tab.text, w - close - 2.0 * TAB_PAD, measure),
            kind: Kind::Tab {
                on: tab.on && unfolded,
                hidden: tab.hidden,
            },
            enabled: true,
            action: if unfolded {
                Action::Apply(tab.command.clone())
            } else {
                Action::Unfold(Some(tab.command.clone()))
            },
            drag: None,
        });
        if tab.close && unfolded {
            out.controls.push(Control {
                at: Rect {
                    x: x + w - close,
                    y: y + (TAB_H - GLYPH_W) / 2.0,
                    w: GLYPH_W,
                    h: GLYPH_W,
                },
                text: "Remove box".into(),
                kind: Kind::Glyph { glyph: CLOSE_GLYPH },
                enabled: true,
                action: Action::Apply("remove".into()),
                drag: None,
            });
        }
        if tab.on {
            chosen = at;
        }
        x += w + TAB_GAP;
    }
    if unfolded {
        out.controls.push(Control {
            at: Rect {
                x,
                y: y + (TAB_H - GLYPH_W) / 2.0,
                w: GLYPH_W,
                h: GLYPH_W,
            },
            text: "Add box".into(),
            kind: Kind::Glyph { glyph: ADD_GLYPH },
            enabled: view.adding,
            action: Action::Apply("add".into()),
            drag: None,
        });
    }
    out.controls.push(Control {
        at: Rect {
            x: RIGHT - disclose_w,
            y: y + 2.0,
            w: disclose_w,
            h: TAB_H - 4.0,
        },
        text: words.into(),
        kind: Kind::Disclose { glyph },
        enabled: true,
        action: if unfolded {
            Action::Fold
        } else {
            Action::Unfold(None)
        },
        drag: None,
    });
    if !unfolded {
        let y = values(&mut out, view, y + TAB_H + 8.0, measure);
        return buttons(out, view, y, now);
    }
    let card = Rect {
        x: PAD,
        y: y + TAB_H,
        w: RIGHT - PAD,
        h: 0.0,
    };
    let under = out.parts.len();
    let bottom = rows(
        &mut out,
        &view.rows,
        "box",
        card.y + CARD_INSET,
        (PAD + CARD_INSET, RIGHT - CARD_INSET),
        measure,
    ) - 6.0
        + CARD_INSET;
    out.parts.insert(
        under,
        Part::TabCard {
            tab: chosen,
            card: Rect {
                h: bottom - card.y,
                ..card
            },
        },
    );
    let y = values(&mut out, view, bottom + 8.0, measure);
    buttons(out, view, y, now)
}

/// What every box shares, under the boxes' settings, folded or not, as its
/// caption says: the popup's summary and the notifications show values so
/// too. Where what follows starts.
fn values(out: &mut Layout, view: &View, y: f32, measure: &dyn Fn(&str, Font) -> f32) -> f32 {
    out.text(
        Rect {
            x: PAD,
            y,
            w: RIGHT - PAD,
            h: 16.0,
        },
        "Show values as",
        Font::Caption,
        Align::Start,
        Ink::Tertiary,
    );
    rows(out, &view.all, "all", y + 20.0, (PAD, RIGHT), measure)
}

/// Refresh now, Settings and Exit under a line from `y`, and the popup's
/// height after them.
fn buttons(mut out: Layout, view: &View, y: f32, now: Instant) -> Layout {
    let mut y = out.rule(y - 6.0) - 4.0;
    let read_now = read_now(view.refresh, now);
    for (glyph, text, enabled, action) in [
        (
            REFRESH_GLYPH,
            read_now.title(),
            read_now == ReadNow::Ready,
            Action::Refresh,
        ),
        (
            SETTINGS_GLYPH,
            "Settings…".into(),
            view.settings,
            Action::Settings,
        ),
        (EXIT_GLYPH, "Exit".into(), true, Action::Exit),
    ] {
        out.controls.push(Control {
            at: Rect {
                x: PAD - 4.0,
                y,
                w: WIDTH - 2.0 * (PAD - 4.0),
                h: ITEM_H,
            },
            text,
            kind: Kind::Item { glyph },
            enabled,
            action,
            drag: None,
        });
        y += ITEM_H;
    }
    out.height = y + PAD - 4.0;
    out
}

/// Lays out rows from `y`, titles at `left` and what they hold from
/// CHIPS_X to `right`, by each row's shape; where the next would start.
fn rows(
    out: &mut Layout,
    rows: &[Row],
    name: &str,
    mut y: f32,
    (left, right): (f32, f32),
    measure: &dyn Fn(&str, Font) -> f32,
) -> f32 {
    for (number, row) in rows.iter().enumerate() {
        let mut dragged = 0;
        let group = format!("{name} {number}");
        if !row.title.is_empty() {
            out.text(
                Rect {
                    x: left,
                    y,
                    w: CHIPS_X - left - 4.0,
                    h: CHIP_H,
                },
                row.title,
                Font::Caption,
                Align::Start,
                Ink::Secondary,
            );
        }
        // Segments with no title take the whole width.
        let mut x = if row.title.is_empty() && row.shape == Shape::Segments {
            left
        } else {
            CHIPS_X
        };
        if let Some(lead) = &row.lead {
            let w = measure(lead, Font::Caption).ceil().min(72.0);
            let h = if row.shape == Shape::Toggles {
                TOGGLE_H
            } else {
                CHIP_H
            };
            out.text(
                Rect { x, y, w, h },
                lead,
                Font::Caption,
                Align::Start,
                Ink::Tertiary,
            );
            x += w + 8.0;
        }
        let start = x;
        let mut top = y;
        let mut height = CHIP_H;
        let mut drag = |out: &mut Layout, at: Rect, command: &Option<String>| {
            command.as_ref().map(|command| {
                out.drags.push(Drag {
                    at,
                    group: group.clone(),
                    index: dragged,
                    command: command.clone(),
                    across: true,
                });
                dragged += 1;
                out.drags.len() - 1
            })
        };
        let action = |chip: &Chip| {
            if chip.command == CUSTOMIZE {
                Action::Customize
            } else {
                Action::Apply(chip.command.clone())
            }
        };
        match row.shape {
            Shape::Chips | Shape::Toggles => {
                let small = row.shape == Shape::Toggles;
                let (font, h, pad) = if small {
                    (Font::Caption, TOGGLE_H, TOGGLE_PAD)
                } else {
                    (Font::Body, CHIP_H, CHIP_PAD)
                };
                height = h;
                for chip in &row.chips {
                    let w = (measure(&chip.text, font).ceil() + 2.0 * pad).min(right - start);
                    if x > start && x + w > right {
                        x = start;
                        top += h + CHIP_GAP;
                    }
                    let at = Rect { x, y: top, w, h };
                    let drag = drag(out, at, &chip.drag);
                    out.controls.push(Control {
                        at,
                        text: chip.text.clone(),
                        kind: if small {
                            Kind::Toggle { on: chip.on }
                        } else {
                            Kind::Chip { on: chip.on }
                        },
                        enabled: chip.enabled,
                        action: action(chip),
                        drag,
                    });
                    x += w + CHIP_GAP;
                }
            }
            // One line, each segment as wide as its name asks, all of them
            // as wide as the line.
            Shape::Segments => {
                // The box's color and the eyedropper take the line's end,
                // and the segments what is left.
                let end = if row.color.is_some() {
                    right - 2.0 * (GLYPH_W + CHIP_GAP) - 4.0
                } else {
                    right
                };
                let asked: Vec<f32> = row
                    .chips
                    .iter()
                    .map(|c| measure(&c.text, Font::Body).ceil() + 2.0 * SEGMENT_PAD)
                    .collect();
                let scale = (end - start) / asked.iter().sum::<f32>().max(1.0);
                out.parts.push(Part::Segments {
                    at: Rect {
                        x: start,
                        y,
                        w: end - start,
                        h: CHIP_H,
                    },
                });
                for (chip, asked) in row.chips.iter().zip(&asked) {
                    let w = asked * scale;
                    out.controls.push(Control {
                        at: Rect { x, y, w, h: CHIP_H },
                        text: chip.text.clone(),
                        kind: Kind::Segment { on: chip.on },
                        enabled: chip.enabled,
                        action: action(chip),
                        drag: None,
                    });
                    x += w;
                }
                if let Some(color) = row.color {
                    let top = y + (CHIP_H - GLYPH_W) / 2.0;
                    out.controls.push(Control {
                        at: Rect {
                            x: right - 2.0 * GLYPH_W - CHIP_GAP,
                            y: top,
                            w: GLYPH_W,
                            h: GLYPH_W,
                        },
                        text: "Box color".into(),
                        kind: Kind::Swatch { color },
                        enabled: true,
                        action: Action::PickColor,
                        drag: None,
                    });
                    out.controls.push(Control {
                        at: Rect {
                            x: right - GLYPH_W,
                            y: top,
                            w: GLYPH_W,
                            h: GLYPH_W,
                        },
                        text: "Eyedropper".into(),
                        kind: Kind::Dropper,
                        enabled: true,
                        action: Action::Eyedrop,
                        drag: None,
                    });
                }
            }
            // As the summary's filter: an icon and a small name.
            Shape::Services => {
                for chip in &row.chips {
                    let w = 6.0 + 16.0 + 4.0 + measure(&chip.text, Font::Picked).ceil() + 6.0;
                    if x > start && x + w > right {
                        x = start;
                        top += FILTER_H + 2.0;
                    }
                    let at = Rect {
                        x,
                        y: top,
                        w,
                        h: FILTER_H,
                    };
                    let drag = drag(out, at, &chip.drag);
                    let (id, color) = chip.icon.unwrap_or(("", 0));
                    out.controls.push(Control {
                        at,
                        text: chip.text.clone(),
                        kind: Kind::Filter {
                            on: chip.on,
                            id,
                            color,
                        },
                        enabled: chip.enabled,
                        action: action(chip),
                        drag,
                    });
                    x += w + 2.0;
                }
                height = FILTER_H;
            }
            // A small line of text, its chips at the end.
            Shape::Note => {
                height = NOTE_H;
                let mut end = right;
                for chip in row.chips.iter().rev() {
                    end -= NOTE_CHIP_W;
                    out.controls.push(Control {
                        at: Rect {
                            x: end,
                            y,
                            w: NOTE_CHIP_W,
                            h: NOTE_H,
                        },
                        text: chip.text.clone(),
                        kind: Kind::Chip { on: chip.on },
                        enabled: chip.enabled,
                        action: action(chip),
                        drag: None,
                    });
                    end -= CHIP_GAP;
                }
                // The chips were pushed from the end; focus goes from the
                // start.
                let pushed = out.controls.len() - row.chips.len();
                out.controls[pushed..].reverse();
                if let Some(note) = &row.note {
                    out.text(
                        Rect {
                            x: start,
                            y,
                            w: end - start,
                            h: NOTE_H,
                        },
                        note,
                        Font::Caption,
                        Align::Start,
                        Ink::Tertiary,
                    );
                }
            }
        }
        if row.shape != Shape::Note
            && let Some(note) = &row.note
        {
            out.text(
                Rect {
                    x,
                    y,
                    w: right - x,
                    h: CHIP_H,
                },
                note,
                Font::Caption,
                Align::Start,
                Ink::Tertiary,
            );
        }
        y = top + height + 6.0;
    }
    y
}

/// Each tab's width: as its name asks while they all fit in `room`; when
/// not, the chosen one keeps up to CHOSEN_TAB of the room, as it is the box
/// being changed, and the others share the rest, the narrow ones whole and
/// the wide ones cut to the same width.
pub(super) fn tab_widths(asked: &[f32], chosen: Option<usize>, room: f32) -> Vec<f32> {
    if asked.iter().sum::<f32>() <= room {
        return asked.to_vec();
    }
    let mut widths = asked.to_vec();
    let mut left = room;
    // A tab alone is cut like any other.
    let chosen = chosen.filter(|&at| at < asked.len() && asked.len() > 1);
    if let Some(at) = chosen {
        widths[at] = asked[at].min(room * CHOSEN_TAB);
        left -= widths[at];
    }
    let mut rest: Vec<usize> = (0..asked.len()).filter(|&i| Some(i) != chosen).collect();
    rest.sort_by(|&a, &b| asked[a].total_cmp(&asked[b]));
    let mut count = rest.len();
    for i in rest {
        let share = left / count as f32;
        widths[i] = asked[i].min(share);
        left -= widths[i];
        count -= 1;
    }
    widths
}

/// The text, cut with an ellipsis to fit `width` in the body font.
pub(super) fn fit(text: &str, width: f32, measure: &dyn Fn(&str, Font) -> f32) -> String {
    if measure(text, Font::Body) <= width {
        return text.into();
    }
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len())
        .rev()
        .map(|end| format!("{}…", chars[..end].iter().collect::<String>().trim_end()))
        .find(|cut| measure(cut, Font::Body) <= width)
        .unwrap_or_else(|| "…".into())
}

/// The place in its group that the drag `from` would go to with the pointer
/// at `point`: that of the group's item nearest the pointer.
pub(super) fn target(drags: &[Drag], from: usize, (x, y): (f32, f32)) -> Option<usize> {
    let group = &drags.get(from)?.group;
    let distance = |d: &Drag| {
        let (cx, cy) = (d.at.x + d.at.w / 2.0, d.at.y + d.at.h / 2.0);
        (cx - x).powi(2) + (cy - y).powi(2)
    };
    drags
        .iter()
        .filter(|d| &d.group == group)
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
        .map(|d| d.index)
}

/// Where the mark of the drop goes: before the item now at `to` when the
/// drag moves earlier, after it when later. None when it would not move.
pub(super) fn drop_mark(drags: &[Drag], from: usize, to: usize) -> Option<Rect> {
    let dragged = drags.get(from)?;
    if to == dragged.index {
        return None;
    }
    let at = drags
        .iter()
        .find(|d| d.group == dragged.group && d.index == to)?
        .at;
    let later = to > dragged.index;
    Some(if dragged.across {
        let edge = if later {
            at.x + at.w + CHIP_GAP / 2.0
        } else {
            at.x - CHIP_GAP / 2.0
        };
        Rect {
            x: edge - DROP_MARK / 2.0,
            y: at.y,
            w: DROP_MARK,
            h: at.h,
        }
    } else {
        let edge = if later { at.y + at.h } else { at.y };
        Rect {
            x: at.x,
            y: edge - DROP_MARK / 2.0,
            w: at.w,
            h: DROP_MARK,
        }
    })
}

/// Whether Refresh now can be pressed at `now`, or how long until it can.
pub(super) fn read_now(at: Option<Instant>, now: Instant) -> ReadNow {
    match at {
        None => ReadNow::Never,
        Some(at) if at <= now => ReadNow::Ready,
        Some(at) => ReadNow::After(at - now),
    }
}

/// The next control focus moves to, forward or back, past those that cannot
/// be pressed, round from one end to the other.
pub(super) fn step(controls: &[Control], from: Option<usize>, forward: bool) -> Option<usize> {
    let count = controls.len();
    let mut at = from;
    for _ in 0..count {
        let next = match (at, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(i), true) => (i + 1) % count,
            (Some(i), false) => (i + count - 1) % count,
        };
        if controls[next].enabled {
            return Some(next);
        }
        at = Some(next);
    }
    None
}

/// Where focus goes when the controls are laid out anew: to the control that
/// does what the focused one did, as numbers shift when the popup unfolds.
/// Unfolding and folding pair Edit with Done, and a folded tab with the same
/// tab unfolded. None when nothing does it any more.
pub(super) fn follow(was: &[Control], focus: Option<usize>, now: &[Control]) -> Option<usize> {
    let action = &was.get(focus?)?.action;
    let paired = match action {
        Action::Unfold(None) => Some(Action::Fold),
        Action::Fold => Some(Action::Unfold(None)),
        Action::Unfold(Some(command)) => Some(Action::Apply(command.clone())),
        Action::Apply(command) => Some(Action::Unfold(Some(command.clone()))),
        _ => None,
    };
    now.iter()
        .position(|c| &c.action == action)
        .or_else(|| now.iter().position(|c| Some(&c.action) == paired.as_ref()))
}

/// Where the popup goes, in pixels: its bottom-right corner at the point it
/// was opened from, moved inside the work area and `margin` from its edges.
pub(super) fn place(anchor: POINT, size: SIZE, work: RECT, margin: i32) -> POINT {
    let fit =
        |at: i32, size: i32, low: i32, high: i32| at.min(high - margin - size).max(low + margin);
    POINT {
        x: fit(anchor.x - size.cx, size.cx, work.left, work.right),
        y: fit(anchor.y - size.cy, size.cy, work.top, work.bottom),
    }
}
