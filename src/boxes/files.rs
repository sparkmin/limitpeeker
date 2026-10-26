//! The editor's side of the boxes: the text a box is opened with, a draft's
//! preview, saving and deleting, and the file a box goes in.

use super::*;
use crate::style::{Fill, Look};
use crate::usage::{self, Service};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A box for the editor to open: whether it is a sample, which cannot be
/// changed, on the first line, then its text.
pub fn opened(boxes: &[Arc<CustomBox>], name: &str) -> Option<String> {
    let custom = boxes.iter().find(|b| b.name == name)?;
    Some(format!(
        "{}\n{}",
        u8::from(custom.file.is_none()),
        custom.source
    ))
}

/// What a new box starts as, for the editor, marked new: the box named
/// `from`, a sample or one's own, when it is of `style`; else, as when
/// nothing was picked, the style's sample.
pub fn started(boxes: &[Arc<CustomBox>], style: &str, from: &str) -> Option<String> {
    match boxes.iter().find(|b| b.name == from && b.style() == style) {
        Some(custom) => Some(format!("new\n{}", custom.source)),
        None => template(style),
    }
}

/// What the editor's preview draws for a box's text: the box's frame, the
/// icons' services by slot, and each item with the block it comes from; or
/// what is wrong. A service the box names that is not connected stands in
/// with the mock figures, which a line tells. Low values warn in color when
/// `warn`, as on the taskbar, and the box is drawn in `look`, the chosen
/// box's color, so the stage shows the preset as the taskbar would.
pub fn preview(
    text: &str,
    services: &[Service],
    look: &Look,
    fill: Fill,
    warn: bool,
    measure: &mut dyn FnMut(&str, u32, u32) -> u32,
) -> String {
    let one_line = |why: &str| why.replace(['\t', '\n', '\r'], " ");
    let custom = match parse(text) {
        Ok(custom) => custom,
        Err(why) => return format!("error\t{}", one_line(&why)),
    };
    let mut services = services.to_vec();
    let mut lines = Vec::new();
    let now = usage::unix_now();
    for stand_in in usage::mock_at(0, now) {
        if custom.services().contains(&stand_in.id) && !services.iter().any(|s| s.id == stand_in.id)
        {
            lines.push(format!("stand-in\t{}", stand_in.name));
            services.push(stand_in);
        }
    }
    let Some((scene, blocks)) = custom.laid_out(&services, look, fill, warn, now, measure) else {
        lines.push("error\tNo block of this box has a service to show.".into());
        return lines.join("\n");
    };
    if !scene.fits() {
        lines.push(
            "error\tThe taskbar would not take this box: a block is outside it, or it is wider than 600 DIPs.".into(),
        );
    }
    let style = scene.style;
    lines.push(format!(
        "box\t{}\t{}\t{}\t{}\t{}\t{}",
        scene.width, style.height, style.padding, style.radius, style.box_color, style.track_color
    ));
    for (slot, (id, color)) in scene.icons.iter().enumerate() {
        lines.push(format!("icon\t{slot}\t{id}\t{color}"));
    }
    for (item, block) in scene.items.iter().zip(&blocks) {
        lines.push(format!(
            "item\t{block}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            item.kind as u32,
            item.x,
            item.y,
            item.w,
            item.h,
            item.fill.map_or(-1, i32::from),
            item.color,
            item.track,
            item.size,
            item.weight,
            item.options,
            item.slot,
            item.text
        ));
    }
    lines.join("\n")
}

/// Writes a box the editor made into `dir`, from its text, in a file named
/// for it, and takes away the file it had under the name it was opened
/// with, `was`. Refused, with why, when the text is not a box, or a sample
/// or another box has its name. The name it was saved under otherwise.
pub fn save(dir: &Path, boxes: &[Arc<CustomBox>], was: &str, text: &str) -> Result<String, String> {
    let custom = parse(text)?;
    if let Some(taken) = boxes.iter().find(|b| b.name == custom.name) {
        if taken.file.is_none() {
            return Err(format!(
                "\"{}\" is a sample; save it under another name",
                custom.name
            ));
        }
        if taken.name != was {
            return Err(format!("a box is named \"{}\" already", custom.name));
        }
    }
    let old = boxes
        .iter()
        .find(|b| b.name == was)
        .and_then(|b| b.file.clone());
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let path = file_for(dir, &custom.name, old.as_deref());
    std::fs::write(&path, text).map_err(|error| error.to_string())?;
    if let Some(old) = old
        && old != path
    {
        let _ = std::fs::remove_file(old);
    }
    Ok(custom.name)
}

/// Takes away a box's file. A sample has none.
pub fn delete(boxes: &[Arc<CustomBox>], name: &str) -> Result<(), String> {
    let custom = boxes
        .iter()
        .find(|b| b.name == name)
        .ok_or(format!("no box is named \"{name}\""))?;
    let file = custom
        .file
        .as_ref()
        .ok_or(format!("\"{name}\" is a sample and cannot be deleted"))?;
    std::fs::remove_file(file).map_err(|error| error.to_string())
}

/// A file for a box named so: the name, with what a file name cannot hold
/// as `_`, and a number after it when another file has it. The box's own
/// file, `own`, is taken again.
pub(super) fn file_for(dir: &Path, name: &str, own: Option<&Path>) -> PathBuf {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let stem = stem.trim().trim_end_matches('.');
    let stem = if stem.is_empty() { "box" } else { stem };
    (1..100)
        .map(|n| {
            if n == 1 {
                dir.join(format!("{stem}.json"))
            } else {
                dir.join(format!("{stem} {n}.json"))
            }
        })
        .find(|path| !path.exists() || Some(path.as_path()) == own)
        .unwrap_or_else(|| dir.join(format!("{stem} {}.json", std::process::id())))
}
