//! A box made into a scene from the services read: in columns or each block
//! where it says, with the text widths it is laid out by.

use super::*;
use crate::scene::{Item, Kind, Scene, TEXT_MAX};
use crate::style::{Fill, Look, Numbers};
use crate::usage::{self, Meaning, Picks, Reading, Service};
use std::collections::HashMap;
use std::sync::Mutex;
use windows::Win32::Graphics::DirectWrite::*;
use windows::core::{BOOL, PCWSTR, w};

impl CustomBox {
    /// Its style as the editor names it: "columns" or "free".
    pub fn style(&self) -> &'static str {
        match self.layout {
            Layout::Columns(_) => "columns",
            Layout::Free(..) => "free",
        }
    }
    pub(super) fn blocks(&self) -> Vec<&Block> {
        match &self.layout {
            Layout::Columns(columns) => columns.iter().flatten().collect(),
            Layout::Free(_, placed) => placed.iter().map(|p| &p.block).collect(),
        }
    }
    /// The services the blocks name, in the order first named.
    pub fn services(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = Vec::new();
        for block in self.blocks() {
            if let Some(id) = block.service.as_deref()
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        ids
    }
    /// Of a service's limits, by their labels, those the blocks show, in the
    /// order first named.
    pub fn limits(&self, id: &str, labels: &[String]) -> Vec<String> {
        let mut shown: Vec<String> = Vec::new();
        for block in self.blocks() {
            if block.service.as_deref() != Some(id) {
                continue;
            }
            for which in &block.limits {
                if let Some(label) = pick(which, labels)
                    && !shown.contains(label)
                {
                    shown.push(label.clone());
                }
            }
        }
        shown
    }
    /// The services and limits the blocks show as picks, for the tooltip
    /// and the log.
    pub fn picks(&self, services: &[Service]) -> Picks {
        let mut picks = Picks::default();
        let ids: Vec<String> = self.services().iter().map(|id| id.to_string()).collect();
        if !ids.is_empty() {
            picks.set_services(ids);
        }
        for service in services {
            let labels: Vec<String> = service
                .limits
                .iter()
                .flatten()
                .map(|l| l.label.clone())
                .collect();
            let shown = self.limits(service.id, &labels);
            if !shown.is_empty() {
                picks.set_limits(service.id, shown);
            }
        }
        picks
    }
    /// The scene from the services now, in the fill mode, low values warning
    /// in color when `warn`, and a limit's time to reset counted from `now`,
    /// in Unix seconds. A block whose service, or every limit it names, is
    /// not there is left out, and a column with it. `measure` gives a text's
    /// width in hundredths of a DIP at a size and weight. None when no block
    /// is left.
    pub fn scene(
        &self,
        services: &[Service],
        look: &Look,
        fill: Fill,
        warn: bool,
        now: i64,
        measure: &mut dyn FnMut(&str, u32, u32) -> u32,
    ) -> Option<Scene> {
        self.laid_out(services, look, fill, warn, now, measure)
            .map(|(scene, _)| scene)
    }
    /// The scene, and for each of its items the block it comes from,
    /// counted as the editor lists them: the columns from the left, each top
    /// to bottom.
    pub fn laid_out(
        &self,
        services: &[Service],
        look: &Look,
        fill: Fill,
        warn: bool,
        now: i64,
        measure: &mut dyn FnMut(&str, u32, u32) -> u32,
    ) -> Option<(Scene, Vec<usize>)> {
        // The box's color, and the plain text, label and service colors
        // that read on it, come from the look; its blocks are its own.
        let style = look.style;
        let height = style.height;
        let mut items = Vec::new();
        let mut blocks = Vec::new();
        let mut icons = Vec::new();
        let showing = Showing {
            look,
            fill,
            warn,
            now,
        };
        let mut put = |r: &Ready, place, icons: &mut Vec<(&'static str, u32)>| {
            let drawn = draw(r, place, showing, icons);
            blocks.extend(std::iter::repeat_n(r.index, drawn.len()));
            items.extend(drawn);
        };
        let width = match &self.layout {
            Layout::Columns(columns) => {
                let mut index = 0;
                let mut x = 0;
                // What comes before the next column: nothing before the
                // first, then the gap, or the spaces that stand for it.
                let mut between: Option<u32> = None;
                let mut spaced = false;
                for column in columns {
                    let first = index;
                    index += column.len();
                    let ready: Vec<Ready> = column
                        .iter()
                        .enumerate()
                        .filter_map(|(at, b)| prepare(b, services, first + at))
                        .collect();
                    if ready.is_empty() {
                        continue;
                    }
                    let stacked = ready.len() > 1;
                    let mut sizes = Vec::new();
                    for r in &ready {
                        sizes.push(natural(r, stacked, height, &mut *measure));
                    }
                    let wide = sizes.iter().map(|s| s.0).max().unwrap_or(0);
                    // A space stands between its neighbours in place of the
                    // gap; one first or last stands between nothing.
                    if ready.iter().all(|r| r.block.shape == Shape::Space) {
                        if let Some(before) = between {
                            between = Some(if spaced { before + wide } else { wide });
                            spaced = true;
                        }
                        continue;
                    }
                    x += between.unwrap_or(0);
                    let tall = sizes.iter().map(|s| s.1).sum::<u32>()
                        + STACK_GAP * (sizes.len() as u32 - 1);
                    let mut y = height.saturating_sub(tall) / 2;
                    for (r, &(w, h)) in ready.iter().zip(&sizes) {
                        // Texts and bars take the column's width; the round
                        // ones sit in its middle.
                        let place = if matches!(r.block.shape, Shape::Text | Shape::Bar) {
                            (x, y, wide, h)
                        } else {
                            (x + (wide - w) / 2, y, w, h)
                        };
                        put(r, place, &mut icons);
                        y += h + STACK_GAP;
                    }
                    x += wide;
                    between = Some(GAP);
                    spaced = false;
                }
                x
            }
            Layout::Free(width, placed) => {
                for (index, p) in placed.iter().enumerate() {
                    let Some(r) = prepare(&p.block, services, index) else {
                        continue;
                    };
                    let (w, h) = natural(&r, false, height, &mut *measure);
                    let place = (p.x, p.y, p.w.unwrap_or(w), p.h.unwrap_or(h));
                    put(&r, place, &mut icons);
                }
                *width
            }
        };
        drop(put);
        (!items.is_empty()).then_some((
            Scene {
                width,
                items,
                style,
                icons,
            },
            blocks,
        ))
    }
}

/// How the values show: the look whose colors the plain blocks take, used
/// or left, whether low ones warn in color, and the time now, in Unix
/// seconds.
#[derive(Clone, Copy)]
pub(super) struct Showing<'a> {
    pub(super) look: &'a Look,
    pub(super) fill: Fill,
    pub(super) warn: bool,
    pub(super) now: i64,
}

/// A block with what it shows now: its service, and each limit it names
/// that is there, by label, with its reading and when it resets.
pub(super) struct Ready<'a> {
    pub(super) block: &'a Block,
    /// Where the block is in the editor's list.
    pub(super) index: usize,
    pub(super) service: Option<&'a Service>,
    pub(super) limits: Vec<(String, Reading, Option<i64>)>,
}

/// A block ready to draw, or None when its service is not there or none of
/// the limits it names is. A service not read yet shows a waiting gauge for
/// each limit named.
pub(super) fn prepare<'a>(
    block: &'a Block,
    services: &'a [Service],
    index: usize,
) -> Option<Ready<'a>> {
    let service = match &block.service {
        Some(id) => Some(services.iter().find(|s| s.id == id)?),
        None => None,
    };
    let limits = match (service, block.limits.is_empty()) {
        (_, true) => Vec::new(),
        (None, false) => return None,
        (Some(service), false) => match &service.limits {
            None => block
                .limits
                .iter()
                .map(|_| (String::new(), Reading::Pending, None))
                .collect(),
            Some(limits) => {
                let labels: Vec<String> = limits.iter().map(|l| l.label.clone()).collect();
                let found: Vec<(String, Reading, Option<i64>)> = block
                    .limits
                    .iter()
                    .filter_map(|which| pick(which, &labels))
                    .filter_map(|label| limits.iter().find(|l| &l.label == label))
                    .map(|l| (l.label.clone(), l.reading, l.resets))
                    .collect();
                if found.is_empty() {
                    return None;
                }
                found
            }
        },
    };
    Some(Ready {
        block,
        index,
        service,
        limits,
    })
}

/// A reading as a block shows it: the share colored in permille, the
/// fill's color, the value, and the value's color. A value that warns is
/// orange or red, whatever the colors, while warning colors are on, and one
/// kept from before a failed read is faded.
pub(super) fn meter(
    reading: Reading,
    showing: Showing,
    meaning: Meaning,
    color: u32,
    text: u32,
) -> (Option<u16>, u32, String, u32) {
    let value = usage::shown(reading, showing.fill, meaning);
    let level = value
        .map_or(usage::Warn::None, |(_, level)| level)
        .when(showing.warn);
    let stale = matches!(reading, Reading::Failed(_));
    let dim = |argb| if stale { usage::dimmed(argb) } else { argb };
    let (color, text) = match level.color() {
        Some(warning) => (warning, warning),
        None => (color, text),
    };
    let words = match (reading, value) {
        (_, Some((share, _))) => usage::number(share, Numbers::Percent),
        (Reading::Failed(None), None) => "?".into(),
        _ => "--".into(),
    };
    (
        value.map(|(share, _)| u16::from(share.min(100)) * 10),
        dim(color),
        words,
        dim(text),
    )
}

/// The forms a time to reset takes at its widest, which a text block that
/// shows it is as wide as.
const WIDEST_RESETS: [&str; 2] = ["23h 59m", "29d 23h"];

/// How long until a limit resets, as the popup says it, or `--` when that is
/// not known or has passed.
pub(super) fn resets_in(resets: Option<i64>, now: i64) -> String {
    match resets.map(|at| crate::popup::until(at, now)) {
        Some(left) if !left.is_empty() => left,
        _ => "--".into(),
    }
}

/// What a text block says with this value and this time to reset.
pub(super) fn says(ready: &Ready, value: &str, resets: &str) -> String {
    let label = ready
        .limits
        .first()
        .map_or("", |(label, ..)| label.as_str());
    let text = match &ready.block.says {
        Says::Value => value.to_string(),
        Says::Label => label.to_string(),
        Says::Both if label.is_empty() => value.to_string(),
        Says::Both => format!("{label} {value}"),
        Says::Resets => resets.to_string(),
        Says::Name => ready.service.map_or(String::new(), |s| s.name.clone()),
        Says::Own(own) => own.clone(),
    };
    // What the hook holds of a text.
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= TEXT_MAX
        })
        .collect()
}

/// A block's own width and height: a text as wide as it is at its widest
/// value, 100%, or its widest time to reset, so the box does not move as
/// values change.
pub(super) fn natural(
    ready: &Ready,
    stacked: bool,
    height: u32,
    measure: &mut dyn FnMut(&str, u32, u32) -> u32,
) -> (u32, u32) {
    let block = ready.block;
    let round = |alone: u32| {
        let side = if stacked {
            STACKED_ROUND
        } else {
            alone.min(height)
        };
        (side, side)
    };
    match block.shape {
        Shape::Icon => (ICON, ICON),
        Shape::Ring | Shape::Rings => round(RING),
        Shape::Pie => round(PIE),
        Shape::Bar => (
            block.width.unwrap_or(BAR_LENGTH),
            block.thickness.unwrap_or(BAR_THICKNESS),
        ),
        Shape::Space => (block.width.unwrap_or(SPACE), 0),
        Shape::Text => {
            let value = usage::number(100, Numbers::Percent);
            let wide = WIDEST_RESETS
                .iter()
                .map(|resets| measure(&says(ready, &value, resets), block.size, block.weight))
                .max()
                .unwrap_or(0);
            (wide + TEXT_ROOM, block.size + TEXT_LEADING)
        }
    }
}

/// The items a ready block draws in a place, giving its service an icon
/// slot when it draws the icon.
pub(super) fn draw(
    ready: &Ready,
    (x, y, w, h): (u32, u32, u32, u32),
    showing: Showing,
    icons: &mut Vec<(&'static str, u32)>,
) -> Vec<Item> {
    let block = ready.block;
    let look = showing.look;
    let own = block.color.unwrap_or(
        ready
            .service
            .map_or(look.text, |s| look.service_color(s.color)),
    );
    let reading = |at: usize| ready.limits.get(at).map_or(Reading::Pending, |l| l.1);
    let meaning = ready.service.map_or(Meaning::Limit, |s| s.meaning);
    // What is left sits against a dial's end; a meter's share is always the
    // share used.
    let dial_end = u32::from(showing.fill == Fill::Left && meaning == Meaning::Limit);
    match block.shape {
        Shape::Space => Vec::new(),
        Shape::Icon => {
            let Some(service) = ready.service else {
                return Vec::new();
            };
            let slot = match icons.iter().position(|(id, _)| *id == service.id) {
                Some(slot) => slot,
                None => {
                    icons.push((service.id, look.service_color(service.color)));
                    icons.len() - 1
                }
            };
            vec![Item {
                slot: slot as u32,
                ..Item::new(Kind::Icon, (x, y, w, h))
            }]
        }
        Shape::Ring | Shape::Rings | Shape::Pie => {
            let side = w.min(h);
            let thickness = block
                .thickness
                .unwrap_or((side / 9).clamp(150, 400))
                .min(side / 2);
            let count = if block.shape == Shape::Rings {
                ready.limits.len()
            } else {
                1
            };
            let kind = if block.shape == Shape::Pie {
                Kind::Pie
            } else {
                Kind::Ring
            };
            let (left, top) = (x + (w - side) / 2, y + (h - side) / 2);
            (0..count)
                .filter_map(|at| {
                    let inset = at as u32 * (thickness + RING_STEP);
                    let across = side.checked_sub(2 * inset)?;
                    if across < 2 * thickness + 100 {
                        return None;
                    }
                    let (share, color, _, _) = meter(reading(at), showing, meaning, own, 0);
                    Some(Item {
                        fill: share,
                        color,
                        size: if kind == Kind::Ring { thickness } else { 0 },
                        options: dial_end,
                        ..Item::new(kind, (left + inset, top + inset, across, across))
                    })
                })
                .collect()
        }
        Shape::Bar => {
            let (share, color, _, _) = meter(reading(0), showing, meaning, own, 0);
            vec![Item {
                fill: share,
                color,
                ..Item::new(Kind::Bar, (x, y, w, h))
            }]
        }
        Shape::Text => {
            // A time to reset is quiet, as a limit's name is, and does not
            // warn.
            let shows_value = matches!(block.says, Says::Value | Says::Both);
            let plain = if shows_value {
                look.text
            } else {
                look.style.label_color
            };
            let chosen = block.color.unwrap_or(plain);
            let (_, _, value, value_color) = meter(reading(0), showing, meaning, 0, chosen);
            let resets = resets_in(ready.limits.first().and_then(|l| l.2), showing.now);
            vec![Item {
                color: if shows_value { value_color } else { chosen },
                size: block.size,
                weight: block.weight,
                options: block.align,
                text: says(ready, &value, &resets),
                ..Item::new(Kind::Text, (x, y, w, h))
            }]
        }
    }
}

/// How wide a text is at a size and weight, in hundredths of a DIP, as the
/// taskbar draws it: measured with DirectWrite in the taskbar's font and
/// kept, since a box asks for the same texts each second.
pub fn text_width(text: &str, size: u32, weight: u32) -> u32 {
    static KEPT: Mutex<Option<HashMap<(String, u32, u32), u32>>> = Mutex::new(None);
    let key = (text.to_string(), size, weight);
    if let Ok(kept) = KEPT.lock()
        && let Some(width) = kept.as_ref().and_then(|kept| kept.get(&key))
    {
        return *width;
    }
    // Should DirectWrite fail, about six tenths of the size a character.
    let width =
        measured(text, size, weight).unwrap_or_else(|| text.chars().count() as u32 * size * 6 / 10);
    if let Ok(mut kept) = KEPT.lock() {
        let kept = kept.get_or_insert_with(HashMap::new);
        if kept.len() > 512 {
            kept.clear();
        }
        kept.insert(key, width);
    }
    width
}

pub(super) fn measured(text: &str, size: u32, weight: u32) -> Option<u32> {
    unsafe {
        let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
        let mut fonts: Option<IDWriteFontCollection> = None;
        write.GetSystemFontCollection(&mut fonts, false).ok()?;
        let has = |name: PCWSTR| {
            fonts.as_ref().is_some_and(|fonts| {
                let (mut index, mut exists) = (0, BOOL(0));
                fonts.FindFamilyName(name, &mut index, &mut exists).is_ok() && exists.as_bool()
            })
        };
        // The taskbar's font, as the popup finds it.
        let family = if has(w!("Segoe UI Variable Text")) {
            w!("Segoe UI Variable Text")
        } else {
            w!("Segoe UI")
        };
        let format = write
            .CreateTextFormat(
                family,
                None::<&IDWriteFontCollection>,
                DWRITE_FONT_WEIGHT(weight as i32),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size as f32 / 100.0,
                w!("en-us"),
            )
            .ok()?;
        let units: Vec<u16> = text.encode_utf16().collect();
        let layout = write
            .CreateTextLayout(&units, &format, 10_000.0, 100.0)
            .ok()?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut metrics).ok()?;
        Some((metrics.widthIncludingTrailingWhitespace * 100.0).ceil() as u32)
    }
}
