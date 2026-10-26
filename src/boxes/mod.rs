//! Custom boxes: blocks a person puts together, in columns or each where it
//! says, showing a service's limit as a ring, a bar, a pie or a text, or an
//! icon or a text of its own. Each box is a JSON file of its own beside the
//! settings, and samples are built in: two to start from, and each preset
//! look made of blocks, for the first box's services and for PC. A box
//! becomes a scene, which the hook draws as it is sent.
//!
//! Reading is in `read`, making scenes in `render`, and the editor's files
//! in `files`.

mod files;
mod read;
mod render;
#[cfg(test)]
mod tests;

use read::*;

pub use files::{delete, opened, preview, save, started};
pub use read::{load, parse, samples, template};
pub use render::text_width;

use std::path::PathBuf;

/// The most files read, the longest one, the longest name in characters,
/// the most columns, and the widest box in DIPs, which the hook takes.
const MOST_FILES: usize = 32;
const LONGEST_FILE: u64 = 64 * 1024;
const LONGEST_NAME: usize = 40;
const MOST_COLUMNS: usize = 24;
const WIDEST: f64 = 600.0;

/// In hundredths of a DIP: between columns and between two blocks in one;
/// an icon; a ring and a pie alone in a column, and either of two; a bar's
/// length and thickness; a space; and what a measured text is given more.
const GAP: u32 = 600;
const STACK_GAP: u32 = 300;
const ICON: u32 = 1600;
const RING: u32 = 3200;
const PIE: u32 = 2000;
const STACKED_ROUND: u32 = 1600;
const BAR_LENGTH: u32 = 4000;
const BAR_THICKNESS: u32 = 400;
const SPACE: u32 = 600;
const TEXT_ROOM: u32 = 100;
/// A text's size and weight when its block does not say, and how much
/// taller than its size its block is. A ring inside another sits its
/// thickness and this further in.
const TEXT_SIZE: u32 = 1100;
const TEXT_WEIGHT: u32 = 600;
const TEXT_LEADING: u32 = 300;
const RING_STEP: u32 = 150;

/// The two samples, as files hold boxes.
pub const COLUMNS_SAMPLE: &str = r##"{
  "name": "Columns sample",
  "mode": "columns",
  "columns": [
    [{ "block": "icon", "service": "claude" }],
    [{ "block": "rings", "service": "claude", "limits": [1, 2] }],
    [
      { "block": "text", "service": "claude", "limit": 1, "show": "both" },
      { "block": "text", "service": "claude", "limit": 2, "show": "both" }
    ],
    [{ "block": "space", "width": 10 }],
    [{ "block": "icon", "service": "gpt" }],
    [
      { "block": "text", "service": "gpt", "limit": 1, "show": "both" },
      { "block": "bar", "service": "gpt", "limit": 1 }
    ]
  ]
}"##;
pub const FREE_SAMPLE: &str = r##"{
  "name": "Free sample",
  "mode": "free",
  "width": 112,
  "blocks": [
    { "block": "ring", "service": "claude", "limit": 1, "x": 0, "y": 4, "w": 32, "h": 32 },
    { "block": "text", "service": "claude", "limit": 1, "size": 9, "align": "center",
      "x": 0, "y": 13.5, "w": 32, "h": 13 },
    { "block": "icon", "service": "gpt", "x": 40, "y": 5 },
    { "block": "text", "service": "gpt", "limit": 1, "show": "both", "x": 60, "y": 6, "w": 52, "h": 14 },
    { "block": "bar", "service": "gpt", "limit": 1, "x": 40, "y": 28, "w": 72, "h": 4 }
  ]
}"##;

/// The five preset looks made of blocks, for GPT, Claude and Gemini, then
/// for PC, to start a box from. Limits by place for the services, so plans
/// named otherwise fit, and by name for PC's meters. Slim, pie and rings are
/// columns, which close up over a limit not there; inline and stack line
/// their rows up, which columns cannot, so they are free.
pub const LOOK_SAMPLES: [&str; 10] = [
    include_str!("samples/ai-slim.json"),
    include_str!("samples/ai-inline.json"),
    include_str!("samples/ai-stack.json"),
    include_str!("samples/ai-pie.json"),
    include_str!("samples/ai-rings.json"),
    include_str!("samples/pc-slim.json"),
    include_str!("samples/pc-inline.json"),
    include_str!("samples/pc-stack.json"),
    include_str!("samples/pc-pie.json"),
    include_str!("samples/pc-rings.json"),
];

/// Where a box stands in the editor's list: one's own, or among the samples
/// to start from, the first box's or PC's.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shelf {
    Mine,
    Basic,
    Ai,
    Pc,
}
impl Shelf {
    pub const ALL: [Shelf; 4] = [Shelf::Mine, Shelf::Basic, Shelf::Ai, Shelf::Pc];
    pub fn word(self) -> &'static str {
        match self {
            Shelf::Mine => "mine",
            Shelf::Basic => "basic",
            Shelf::Ai => "ai",
            Shelf::Pc => "pc",
        }
    }
    pub fn find(word: &str) -> Option<Shelf> {
        Self::ALL.into_iter().find(|shelf| shelf.word() == word)
    }
}

/// A box as its file says, by its name.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomBox {
    pub name: String,
    /// The file it was read from; none for a sample.
    pub file: Option<PathBuf>,
    /// Its shelf in the editor's list.
    pub shelf: Shelf,
    /// Its text as written, which the editor opens.
    pub source: String,
    layout: Layout,
}

#[derive(Clone, Debug, PartialEq)]
enum Layout {
    /// Columns from the left, each one block or two, one above the other.
    Columns(Vec<Vec<Block>>),
    /// Blocks each where it says, in a box so wide, in hundredths of a DIP.
    Free(u32, Vec<Placed>),
}

/// A block placed freely: where, and how large when not its own size.
#[derive(Clone, Debug, PartialEq)]
struct Placed {
    block: Block,
    x: u32,
    y: u32,
    w: Option<u32>,
    h: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shape {
    Icon,
    Ring,
    Rings,
    Pie,
    Bar,
    Text,
    Space,
}

/// One of a service's limits: the nth, from 0, or the one so labeled. By
/// its place, a box fits plans whose limits are named otherwise.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Which {
    Nth(usize),
    Named(String),
}

/// What a text says: its limit's value, label or both, how long until it
/// resets, its service's name, or words of its own.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Says {
    Value,
    Label,
    Both,
    Resets,
    Name,
    Own(String),
}

/// One block. Sizes are in hundredths of a DIP.
#[derive(Clone, Debug, PartialEq)]
struct Block {
    shape: Shape,
    service: Option<String>,
    limits: Vec<Which>,
    color: Option<u32>,
    says: Says,
    size: u32,
    weight: u32,
    align: u32,
    width: Option<u32>,
    thickness: Option<u32>,
}
