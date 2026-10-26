//! A scene: blocks the controller places in the box, which the hook draws
//! where each says, in place of the label's cells. The kinds are few, a
//! text, a bar, a ring, a pie and an icon; how they are put together is
//! decided on this side, so a new arrangement needs no new DLL. Must match
//! the Scene words and ranges in native/taskbar_tap.cpp.

use crate::style::Style;
use crate::usage::MAX_CELLS;

/// Where the scene sits in shared memory, after the icons: the mode, the
/// width, the count and the pool's length, then each block's words, then
/// the pool. Must match native/taskbar_tap.cpp.
pub const SCENE_MODE: usize = 16705;
pub const SCENE_WIDTH: usize = 16706;
pub const SCENE_COUNT: usize = 16707;
pub const SCENE_POOL_LENGTH: usize = 16708;
pub const SCENE_ITEMS: usize = 16709;
pub const ITEM_WORDS: usize = 12;
pub const SCENE_POOL: usize = 17285;
/// The most blocks, the pool's UTF-16 units, the width's least and most in
/// hundredths of a DIP, and a ring's thickness's.
pub const MAX_ITEMS: usize = 48;
pub const POOL_CAPACITY: usize = 512;
pub const WIDTH_MIN: u32 = 100;
pub const WIDTH_MAX: u32 = 60000;
pub const THICKNESS_MIN: u32 = 50;
pub const THICKNESS_MAX: u32 = 1600;
/// The longest text a block holds, in UTF-16 units. A text wider than its
/// place is cut; one far wider than any box once ended the hook's session.
/// Must match ItemTextMax.
pub const TEXT_MAX: usize = 64;
/// What a text's font size and weight may be, as the label's. Must match
/// FontMin, FontMax, WeightMin and WeightMax in the hook.
const FONT: (u32, u32) = (800, 2400);
const WEIGHT: (u32, u32) = (100, 900);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Text = 1,
    Bar = 2,
    Ring = 3,
    Pie = 4,
    Icon = 5,
}

/// One block, placed in hundredths of a DIP from the top-left corner inside
/// the box's padding. `size` is a text's font size or a ring's thickness;
/// `options` a text's alignment (0 start, 1 center, 2 end) or, for a bar,
/// ring or pie, whether the colored part sits against the end; `slot` an
/// icon's. A track color of 0 takes the look's.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Item {
    pub kind: Kind,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub fill: Option<u16>,
    pub color: u32,
    pub track: u32,
    pub size: u32,
    pub weight: u32,
    pub options: u32,
    pub text: String,
    pub slot: u32,
}

impl Item {
    pub(crate) fn new(kind: Kind, (x, y, w, h): (u32, u32, u32, u32)) -> Self {
        Self {
            kind,
            x,
            y,
            w,
            h,
            fill: None,
            color: 0,
            track: 0,
            size: 0,
            weight: 0,
            options: 0,
            text: String::new(),
            slot: 0,
        }
    }
}

/// A scene: how wide the blocks' room is, the blocks, the box's look (its
/// color, height, corners, padding, track and motion), and the services
/// whose icons the slots hold, in slot order, with their colors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scene {
    pub width: u32,
    pub items: Vec<Item>,
    pub style: Style,
    pub icons: Vec<(&'static str, u32)>,
}

impl Scene {
    /// Whether the hook will take it, by the hook's own checks. It keeps
    /// what it has otherwise.
    pub fn fits(&self) -> bool {
        let height = self.style.height;
        let within = |value: u32, (least, most): (u32, u32)| (least..=most).contains(&value);
        self.style.fits()
            && within(self.width, (WIDTH_MIN, WIDTH_MAX))
            && (1..=MAX_ITEMS).contains(&self.items.len())
            && self.icons.len() <= MAX_CELLS
            && self.pool().0.len() <= POOL_CAPACITY
            && self.items.iter().all(|item| {
                item.w >= 1
                    && item.h >= 1
                    && item.x <= self.width
                    && item.y <= height
                    && item.w <= self.width - item.x
                    && item.h <= height - item.y
                    && item.fill.is_none_or(|f| f <= 1000)
                    && match item.kind {
                        Kind::Text => {
                            within(item.size, FONT)
                                && within(item.weight, WEIGHT)
                                && item.options <= 2
                                && item.text.encode_utf16().count() <= TEXT_MAX
                        }
                        Kind::Bar | Kind::Pie => item.options <= 1,
                        Kind::Ring => {
                            within(item.size, (THICKNESS_MIN, THICKNESS_MAX))
                                && item.size * 2 <= item.w.min(item.h)
                                && item.options <= 1
                        }
                        Kind::Icon => (item.slot as usize) < MAX_CELLS,
                    }
            })
    }

    /// The texts one after another as UTF-16, and where each text block's
    /// starts and how long it is, in block order.
    fn pool(&self) -> (Vec<u16>, Vec<(u32, u32)>) {
        let mut pool = Vec::new();
        let mut refs = Vec::new();
        for item in &self.items {
            let from = pool.len() as u32;
            if item.kind == Kind::Text {
                pool.extend(item.text.encode_utf16());
            }
            refs.push((from, pool.len() as u32 - from));
        }
        (pool, refs)
    }

    /// The words from SCENE_WIDTH to the pool, and the pool two units a
    /// word, as the hook reads them.
    pub fn words(&self) -> (Vec<u32>, Vec<u32>) {
        let (pool, refs) = self.pool();
        let mut words = vec![self.width, self.items.len() as u32, pool.len() as u32];
        for (item, &(from, length)) in self.items.iter().zip(&refs) {
            let reference = match item.kind {
                Kind::Text => from << 16 | length,
                Kind::Icon => item.slot,
                _ => 0,
            };
            words.extend([
                item.kind as u32,
                item.x,
                item.y,
                item.w,
                item.h,
                item.fill.map_or(u32::MAX, u32::from),
                item.color,
                item.track,
                item.size,
                item.weight,
                item.options,
                reference,
            ]);
        }
        debug_assert_eq!(words.len(), 3 + self.items.len() * ITEM_WORDS);
        let packed = pool
            .chunks(2)
            .map(|pair| u32::from(pair[0]) | u32::from(pair.get(1).copied().unwrap_or(0)) << 16)
            .collect();
        (words, packed)
    }

    /// The tallest icon block, in hundredths of a DIP, which the icons are
    /// drawn for; 0 with none.
    pub fn icon_size(&self) -> u32 {
        self.items
            .iter()
            .filter(|i| i.kind == Kind::Icon)
            .map(|i| i.w.min(i.h))
            .max()
            .unwrap_or(0)
    }

    /// For the log: the width, and each block's kind, place and fill.
    pub fn summary(&self) -> String {
        let blocks: Vec<String> = self
            .items
            .iter()
            .map(|i| {
                let fill = i.fill.map_or("-".into(), |f| f.to_string());
                format!(
                    "{:?}@{},{} {}x{} fill={fill}{}",
                    i.kind,
                    i.x / 100,
                    i.y / 100,
                    i.w / 100,
                    i.h / 100,
                    if i.text.is_empty() {
                        String::new()
                    } else {
                        format!(" \"{}\"", i.text)
                    }
                )
            })
            .collect();
        format!("width={} [{}]", self.width / 100, blocks.join("; "))
    }
}

/// A sample scene for the tests, from the label's first two cells, or the
/// first one twice: its icon, its two limits as rings one inside the other
/// with a text for each, then the next one's icon, a bar under its value,
/// and a pie. Every kind the hook draws is in it. None when there is
/// nothing to show.
#[cfg(test)]
pub fn sample(label: &crate::usage::Label, look: &crate::style::Look) -> Option<Scene> {
    let first = label.cells.first()?;
    let second = label.cells.get(1).unwrap_or(first);
    let style = look.style;
    let height = style.height;
    // DIPs to hundredths, and the middle of the box for a thing so tall.
    let d = |dip: u32| dip * 100;
    let middle = |tall: u32| height.saturating_sub(tall) / 2;
    let mut items = Vec::new();
    let mut icons = vec![(first.id, first.color)];
    let icon = |slot: u32, x: u32| Item {
        slot,
        ..Item::new(Kind::Icon, (x, middle(d(16)), d(16), d(16)))
    };
    items.push(icon(0, 0));
    // Rings one inside the other: the first limit outermost.
    let dial = d(32).min(height);
    for (at, gauge) in first.gauges.iter().take(2).enumerate() {
        let inset = at as u32 * d(5);
        items.push(Item {
            fill: gauge.fill,
            color: gauge.fill_color,
            size: 350,
            ..Item::new(
                Kind::Ring,
                (
                    d(20) + inset,
                    middle(dial) + inset,
                    dial - 2 * inset,
                    dial - 2 * inset,
                ),
            )
        });
    }
    let texts = d(20) + dial + d(6);
    for (at, gauge) in first.gauges.iter().take(2).enumerate() {
        let top = if at == 0 {
            middle(0).saturating_sub(d(14))
        } else {
            middle(0)
        };
        items.push(Item {
            color: gauge.text_color,
            size: 1100,
            weight: 600,
            text: format!("{} {}", gauge.label, gauge.text),
            ..Item::new(Kind::Text, (texts, top, d(56), d(14)))
        });
    }
    let next = texts + d(60);
    let slot = if second.id == first.id {
        0
    } else {
        icons.push((second.id, second.color));
        1
    };
    items.push(icon(slot, next));
    let gauge = second.gauges.first()?;
    items.push(Item {
        color: gauge.text_color,
        size: 1100,
        weight: 600,
        options: 1,
        text: gauge.text.clone(),
        ..Item::new(
            Kind::Text,
            (next + d(20), middle(0).saturating_sub(d(13)), d(40), d(12)),
        )
    });
    items.push(Item {
        fill: gauge.fill,
        color: gauge.fill_color,
        options: style.fill_end,
        ..Item::new(Kind::Bar, (next + d(20), middle(0) + d(3), d(40), d(4)))
    });
    let pie = second.gauges.get(1).unwrap_or(gauge);
    items.push(Item {
        fill: pie.fill,
        color: pie.fill_color,
        options: style.fill_end,
        ..Item::new(Kind::Pie, (next + d(66), middle(d(20)), d(20), d(20)))
    });
    Some(Scene {
        width: next + d(86),
        items,
        style,
        icons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Fill, LOOKS};
    use crate::usage::{Picks, label, mock};

    fn sampled() -> Scene {
        let look = &LOOKS[0];
        sample(
            &label(&mock(0), look, Fill::Used, &Picks::default(), true),
            look,
        )
        .unwrap()
    }

    #[test]
    fn the_sample_has_every_kind_and_fits() {
        // Icons, two rings one inside the other, texts, a bar and a pie,
        // each inside the box and none over another but the rings.
        let scene = sampled();
        assert!(scene.fits(), "{}", scene.summary());
        let kinds: Vec<_> = scene.items.iter().map(|i| i.kind).collect();
        use Kind::*;
        assert_eq!(kinds, [Icon, Ring, Ring, Text, Text, Icon, Text, Bar, Pie]);
        assert_eq!(scene.icons, [("gpt", 0xFFF2_F2F2), ("claude", 0xFFF4_A27A)]);
        assert_eq!(scene.items[5].slot, 1);
        assert_eq!(scene.items[3].text, "5h 28%");
        let rect = |i: &Item| (i.x, i.y, i.x + i.w, i.y + i.h);
        for (a, first) in scene.items.iter().enumerate() {
            for second in &scene.items[a + 1..] {
                if first.kind == Ring && second.kind == Ring {
                    continue;
                }
                let (l1, t1, r1, b1) = rect(first);
                let (l2, t2, r2, b2) = rect(second);
                let overlap = l1 < r2 && l2 < r1 && t1 < b2 && t2 < b1;
                assert!(!overlap, "{first:?} {second:?}");
            }
        }
        assert_eq!(scene.icon_size(), 1600);
    }

    #[test]
    fn the_hook_reads_the_words_it_is_sent() {
        // Each block in twelve words, a text by where it starts in the pool
        // and how long it is, an icon by its slot, no fill as -1, and the
        // pool two UTF-16 units a word.
        let scene = sampled();
        let (words, pool) = scene.words();
        assert_eq!(words.len(), 3 + scene.items.len() * ITEM_WORDS);
        assert_eq!(words[..3], [scene.width, 9, pool_units(&scene)]);
        let block = |n: usize| &words[3 + n * ITEM_WORDS..3 + (n + 1) * ITEM_WORDS];
        assert_eq!(block(0)[0], Kind::Icon as u32);
        assert_eq!(block(0)[5], u32::MAX);
        assert_eq!(block(5)[11], 1);
        assert_eq!(block(3)[11], 6);
        assert_eq!(block(4)[11], 6 << 16 | 6);
        let units: Vec<u16> = pool
            .iter()
            .flat_map(|w| [*w as u16, (*w >> 16) as u16])
            .collect();
        assert_eq!(String::from_utf16_lossy(&units[..6]), "5h 28%");
        assert_eq!(SCENE_ITEMS + MAX_ITEMS * ITEM_WORDS, SCENE_POOL);
        assert_eq!(SCENE_POOL + POOL_CAPACITY / 2, 17541);
    }

    fn pool_units(scene: &Scene) -> u32 {
        scene
            .items
            .iter()
            .filter(|i| i.kind == Kind::Text)
            .map(|i| i.text.encode_utf16().count() as u32)
            .sum()
    }

    #[test]
    fn what_the_hook_would_refuse_does_not_fit() {
        // The same checks as the hook's, so a scene is never sent to be
        // refused.
        let good = sampled();
        let changed = |change: &dyn Fn(&mut Scene)| {
            let mut scene = good.clone();
            change(&mut scene);
            scene.fits()
        };
        assert!(!changed(&|s| s.items.clear()));
        assert!(!changed(&|s| s.width = 99));
        assert!(!changed(&|s| s.width = 60001));
        assert!(!changed(&|s| s.items[1].x = s.width));
        assert!(!changed(&|s| s.items[1].y = s.style.height));
        assert!(!changed(&|s| s.items[1].size = 1700));
        assert!(!changed(&|s| s.items[1].fill = Some(1001)));
        assert!(!changed(&|s| s.items[3].size = 700));
        assert!(!changed(&|s| s.items[3].options = 3));
        assert!(!changed(&|s| s.items[7].options = 2));
        assert!(!changed(&|s| s.items[0].slot = 4));
        assert!(!changed(&|s| s.items[3].text = "x".repeat(600)));
        assert!(!changed(&|s| s.items[3].text = "x".repeat(TEXT_MAX + 1)));
        assert!(changed(&|s| s.items[3].text = "x".repeat(TEXT_MAX)));
        assert!(!changed(&|s| s.items[0].w = 0));
        let many = sampled();
        assert!(!changed(&|s| {
            while s.items.len() <= MAX_ITEMS {
                s.items.push(many.items[0].clone());
            }
        }));
        assert!(changed(&|s| s.items[1].fill = None));
    }
}
