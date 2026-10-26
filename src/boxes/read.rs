//! Reading boxes: the samples and the files, each checked against what the
//! hook takes, and the template a new preset starts from.

use super::*;
use crate::json::{self, Json};
use crate::scene::{MAX_ITEMS, TEXT_MAX};
use crate::usage::MAX_CELLS;
use std::path::Path;

/// The samples, which the EXE holds, each on its shelf: the two to start
/// from, then the preset looks for the first box's services and for PC.
pub fn samples() -> Vec<CustomBox> {
    let (ai, pc) = LOOK_SAMPLES.split_at(5);
    [COLUMNS_SAMPLE, FREE_SAMPLE]
        .iter()
        .map(|text| (text, Shelf::Basic))
        .chain(ai.iter().map(|text| (text, Shelf::Ai)))
        .chain(pc.iter().map(|text| (text, Shelf::Pc)))
        .filter_map(|(text, shelf)| {
            parse(text).ok().map(|mut custom| {
                custom.shelf = shelf;
                custom
            })
        })
        .collect()
}

/// The samples, then each `.json` file in `dir` by its name, and what was
/// left out and why: a file that cannot be read, is too long, is not a box
/// or names a box already there. No folder is no box of one's own.
pub fn load(dir: Option<&Path>) -> (Vec<CustomBox>, Vec<String>) {
    let mut boxes = samples();
    let mut problems = Vec::new();
    let Some(entries) = dir.and_then(|dir| std::fs::read_dir(dir).ok()) else {
        return (boxes, problems);
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    if files.len() > MOST_FILES {
        problems.push(format!(
            "only the first {MOST_FILES} of {} box files are read",
            files.len()
        ));
        files.truncate(MOST_FILES);
    }
    for path in files {
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let read = std::fs::metadata(&path)
            .map_err(|error| error.to_string())
            .and_then(|meta| {
                if meta.len() > LONGEST_FILE {
                    Err(format!("longer than {} KB", LONGEST_FILE / 1024))
                } else {
                    std::fs::read_to_string(&path).map_err(|error| error.to_string())
                }
            });
        match read.and_then(|text| parse(&text)) {
            Ok(custom) if boxes.iter().any(|b| b.name == custom.name) => problems.push(format!(
                "{file}: a box is named \"{}\" already",
                custom.name
            )),
            Ok(mut custom) => {
                custom.file = Some(path.clone());
                boxes.push(custom);
            }
            Err(why) => problems.push(format!("{file}: {why}")),
        }
    }
    (boxes, problems)
}

/// A box from its file's text, or what is wrong with it.
pub fn parse(text: &str) -> Result<CustomBox, String> {
    let json = json::parse(text.trim_start_matches('\u{feff}')).ok_or("not JSON")?;
    let name = json
        .get("name")
        .and_then(Json::text)
        .map(str::trim)
        .unwrap_or_default();
    if name.is_empty() || name.chars().count() > LONGEST_NAME || name.contains(['\t', '\n', '\r']) {
        return Err(format!(
            "\"name\" is 1 to {LONGEST_NAME} characters on one line"
        ));
    }
    let layout = match json.get("mode").and_then(Json::text) {
        Some("columns") => {
            let columns = list(&json, "columns")?;
            if columns.is_empty() || columns.len() > MOST_COLUMNS {
                return Err(format!("\"columns\" holds 1 to {MOST_COLUMNS} columns"));
            }
            let mut built = Vec::new();
            for (c, column) in columns.iter().enumerate() {
                let at = format!("column {}", c + 1);
                let Json::List(blocks) = column else {
                    return Err(format!("{at}: not a list of blocks"));
                };
                if !(1..=2).contains(&blocks.len()) {
                    return Err(format!("{at}: 1 or 2 blocks"));
                }
                let blocks = blocks
                    .iter()
                    .enumerate()
                    .map(|(b, json)| block(json, &format!("{at} block {}", b + 1)))
                    .collect::<Result<Vec<_>, _>>()?;
                built.push(blocks);
            }
            Layout::Columns(built)
        }
        Some("free") => {
            let width = dips(&json, "width", 1.0, WIDEST)?.ok_or("no \"width\"")?;
            let blocks = list(&json, "blocks")?;
            if blocks.is_empty() || blocks.len() > MAX_ITEMS {
                return Err(format!("\"blocks\" holds 1 to {MAX_ITEMS} blocks"));
            }
            let mut placed = Vec::new();
            for (b, json) in blocks.iter().enumerate() {
                let at = format!("block {}", b + 1);
                let place =
                    |key: &str| dips(json, key, 0.0, WIDEST).map_err(|why| format!("{at}: {why}"));
                placed.push(Placed {
                    block: block(json, &at)?,
                    x: place("x")?.ok_or(format!("{at}: no \"x\""))?,
                    y: place("y")?.ok_or(format!("{at}: no \"y\""))?,
                    w: place("w")?,
                    h: place("h")?,
                });
            }
            Layout::Free(width, placed)
        }
        _ => return Err("\"mode\" is \"columns\" or \"free\"".into()),
    };
    let custom = CustomBox {
        name: name.to_string(),
        file: None,
        shelf: Shelf::Mine,
        source: text.to_string(),
        layout,
    };
    if custom.services().len() > MAX_CELLS {
        return Err(format!("more than {MAX_CELLS} services"));
    }
    Ok(custom)
}

pub(super) fn list<'a>(json: &'a Json, key: &str) -> Result<&'a [Json], String> {
    match json.get(key) {
        Some(Json::List(items)) => Ok(items),
        _ => Err(format!("no \"{key}\" list")),
    }
}

/// A size in DIPs from `least` to `most`, as hundredths; None when absent.
pub(super) fn dips(json: &Json, key: &str, least: f64, most: f64) -> Result<Option<u32>, String> {
    match json.get(key) {
        None => Ok(None),
        Some(Json::Number(n)) if (least..=most).contains(n) => Ok(Some((n * 100.0).round() as u32)),
        Some(_) => Err(format!("\"{key}\" is a number from {least} to {most}")),
    }
}

pub(super) fn block(json: &Json, at: &str) -> Result<Block, String> {
    let word = |key: &str| json.get(key).and_then(Json::text);
    let wrong = |what: &str| Err(format!("{at}: {what}"));
    let shape = match word("block") {
        Some("icon") => Shape::Icon,
        Some("ring") => Shape::Ring,
        Some("rings") => Shape::Rings,
        Some("pie") => Shape::Pie,
        Some("bar") => Shape::Bar,
        Some("text") => Shape::Text,
        Some("space") => Shape::Space,
        _ => return wrong("\"block\" is icon, ring, rings, pie, bar, text or space"),
    };
    let says = match (shape, word("text"), word("show")) {
        (Shape::Text, Some(own), _) => {
            if own.is_empty()
                || own.encode_utf16().count() > TEXT_MAX
                || own.contains(['\t', '\n', '\r'])
            {
                return wrong(&format!(
                    "\"text\" is 1 to {TEXT_MAX} characters on one line"
                ));
            }
            Says::Own(own.to_string())
        }
        (Shape::Text, None, None | Some("value")) => Says::Value,
        (Shape::Text, None, Some("label")) => Says::Label,
        (Shape::Text, None, Some("both")) => Says::Both,
        (Shape::Text, None, Some("resets")) => Says::Resets,
        (Shape::Text, None, Some("name")) => Says::Name,
        (Shape::Text, None, Some(_)) => {
            return wrong("\"show\" is value, label, both, resets or name");
        }
        _ => Says::Value,
    };
    let service = word("service").map(String::from);
    if service.is_none() && shape != Shape::Space && !matches!(says, Says::Own(_)) {
        return wrong("no \"service\"");
    }
    let which = |json: &Json| match json {
        Json::Number(n) if *n >= 1.0 && n.fract() == 0.0 => Some(Which::Nth(*n as usize - 1)),
        Json::Text(label) if !label.is_empty() => Some(Which::Named(label.clone())),
        _ => None,
    };
    let reads = match shape {
        Shape::Ring | Shape::Pie | Shape::Bar => true,
        Shape::Text => matches!(says, Says::Value | Says::Label | Says::Both | Says::Resets),
        _ => false,
    };
    let limits = if shape == Shape::Rings {
        match json.get("limits") {
            Some(Json::List(items)) if (1..=3).contains(&items.len()) => {
                match items.iter().map(which).collect::<Option<Vec<_>>>() {
                    Some(limits) => limits,
                    None => return wrong("a limit is a number from 1 or a label"),
                }
            }
            _ => return wrong("\"limits\" lists 1 to 3 limits"),
        }
    } else if reads {
        match json.get("limit").map(which) {
            None => vec![Which::Nth(0)],
            Some(Some(limit)) => vec![limit],
            Some(None) => return wrong("\"limit\" is a number from 1 or a label"),
        }
    } else {
        Vec::new()
    };
    let color = match word("color") {
        None => None,
        Some(hex) => match rgb(hex) {
            Some(color) => Some(color),
            None => return wrong("\"color\" is #RRGGBB"),
        },
    };
    let number =
        |key: &str, least, most| dips(json, key, least, most).map_err(|why| format!("{at}: {why}"));
    let weight = match json.get("weight") {
        None => TEXT_WEIGHT,
        Some(Json::Number(n)) if (100.0..=900.0).contains(n) => *n as u32,
        Some(_) => return wrong("\"weight\" is a number from 100 to 900"),
    };
    let align = match word("align") {
        None | Some("start") => 0,
        Some("center") => 1,
        Some("end") => 2,
        Some(_) => return wrong("\"align\" is start, center or end"),
    };
    Ok(Block {
        shape,
        service,
        limits,
        color,
        says,
        size: number("size", 8.0, 24.0)?.unwrap_or(TEXT_SIZE),
        weight,
        align,
        width: number("width", 1.0, WIDEST)?,
        thickness: number("thickness", 0.5, 16.0)?,
    })
}

/// `#RRGGBB` as an opaque ARGB color.
pub(super) fn rgb(hex: &str) -> Option<u32> {
    let digits = hex.strip_prefix('#').filter(|d| d.len() == 6)?;
    u32::from_str_radix(digits, 16)
        .ok()
        .map(|rgb| 0xFF00_0000 | rgb)
}

/// Which label a limit named by place or label is, out of a service's.
pub(super) fn pick<'a>(which: &Which, labels: &'a [String]) -> Option<&'a String> {
    match which {
        Which::Nth(n) => labels.get(*n),
        Which::Named(name) => labels.iter().find(|label| *label == name),
    }
}

/// A new box's start in a style, "columns" or "free": that style's sample,
/// marked "new" on the first line, for the editor to open under a name of
/// its own. None for a style not known.
pub fn template(style: &str) -> Option<String> {
    let sample = match style {
        "columns" => COLUMNS_SAMPLE,
        "free" => FREE_SAMPLE,
        _ => return None,
    };
    Some(format!("new\n{sample}"))
}
