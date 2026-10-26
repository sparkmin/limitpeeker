use super::*;
use crate::scene::{Item, Kind, Scene};
use crate::style::{self, Fill};
use crate::usage::{self, Reading, Service};
use std::sync::Arc;

use crate::choice::Choice;
use crate::style::Background;
use crate::usage::{Trouble, mock_at};

const UNIX: i64 = 1_790_000_000;

/// Six tenths of the size a character, so the tests do not hang on a font.
fn guess(text: &str, size: u32, _: u32) -> u32 {
    text.chars().count() as u32 * size * 6 / 10
}
fn sample(name: &str) -> CustomBox {
    samples().into_iter().find(|b| b.name == name).unwrap()
}
fn scene(custom: &CustomBox, services: &[Service], fill: Fill) -> Option<Scene> {
    custom.scene(services, style::DEFAULT, fill, true, UNIX, &mut guess)
}
fn kinds(scene: &Scene) -> Vec<Kind> {
    scene.items.iter().map(|i| i.kind).collect()
}

#[test]
fn the_samples_are_boxes() {
    let names: Vec<String> = samples().into_iter().map(|b| b.name).collect();
    assert_eq!(
        names,
        [
            "Columns sample",
            "Free sample",
            "AI slim",
            "AI inline",
            "AI stack",
            "AI pie",
            "AI rings",
            "PC slim",
            "PC inline",
            "PC stack",
            "PC pie",
            "PC rings"
        ]
    );
    let columns = sample("Columns sample");
    assert_eq!(columns.services(), ["claude", "gpt"]);
    let labels = |list: &[&str]| list.iter().map(|l| l.to_string()).collect::<Vec<_>>();
    assert_eq!(
        columns.limits("claude", &labels(&["5h", "7d", "Opus 7d"])),
        ["5h", "7d"]
    );
    assert_eq!(columns.limits("gpt", &labels(&["7d"])), ["7d"]);
}

#[test]
fn a_file_that_is_not_a_box_says_why() {
    let wrong = |text: &str| parse(text).unwrap_err();
    assert_eq!(wrong("{"), "not JSON");
    assert!(wrong(r#"{"name": "", "mode": "free"}"#).contains("\"name\""));
    assert!(wrong(r#"{"name": "a", "mode": "grid"}"#).contains("\"mode\""));
    let column =
        |blocks: &str| format!(r#"{{"name": "a", "mode": "columns", "columns": [{blocks}]}}"#);
    assert!(wrong(&column("[]")).contains("1 or 2 blocks"));
    let icon = r#"{"block": "icon", "service": "gpt"}"#;
    assert!(wrong(&column(&format!("[{icon}, {icon}, {icon}]"))).contains("1 or 2 blocks"));
    assert!(wrong(&column(r#"[{"block": "star"}]"#)).contains("\"block\""));
    assert!(wrong(&column(r#"[{"block": "bar"}]"#)).contains("no \"service\""));
    assert!(
        wrong(&column(
            r#"[{"block": "bar", "service": "gpt", "limit": 0}]"#
        ))
        .contains("\"limit\"")
    );
    assert!(
        wrong(&column(
            r#"[{"block": "bar", "service": "gpt", "color": "red"}]"#
        ))
        .contains("\"color\"")
    );
    assert!(
        wrong(&column(
            r#"[{"block": "rings", "service": "gpt", "limits": [1, 2, 3, 4]}]"#
        ))
        .contains("\"limits\"")
    );
    assert!(
        wrong(&column(
            r#"[{"block": "text", "service": "gpt", "size": 40}]"#
        ))
        .contains("\"size\"")
    );
    let five = ["a", "b", "c", "d", "e"]
        .map(|id| format!(r#"[{{"block": "icon", "service": "{id}"}}]"#))
        .join(",");
    assert!(wrong(&column(&five)).contains("more than 4 services"));
    assert!(wrong(r#"{"name": "a", "mode": "free", "blocks": []}"#).contains("\"width\""));
    let free = r#"{"name": "a", "mode": "free", "width": 50, "blocks": [{"block": "icon", "service": "gpt", "y": 0}]}"#;
    assert!(wrong(free).contains("block 1: no \"x\""));
    // A text of its own needs no service; a limit may be named.
    let fine = column(
        r##"[{"block": "text", "text": "Hi"}, {"block": "bar", "service": "gpt", "limit": "7d", "color": "#7AA7FF"}]"##,
    );
    assert!(parse(&fine).is_ok());
}

#[test]
fn columns_stack_their_blocks_and_take_the_widest() {
    // Claude's icon, two rings one in the other, two texts; a space;
    // Codex's icon, then a text over a bar, as wide as the text.
    let custom = sample("Columns sample");
    let drawn = scene(&custom, &mock_at(0, UNIX), Fill::Used).unwrap();
    use Kind::*;
    assert_eq!(
        kinds(&drawn),
        [Icon, Ring, Ring, Text, Text, Icon, Text, Bar]
    );
    assert_eq!(
        drawn.icons.iter().map(|i| i.0).collect::<Vec<_>>(),
        ["claude", "gpt"]
    );
    assert_eq!((drawn.items[0].slot, drawn.items[5].slot), (0, 1));
    assert!(drawn.fits());
    let texts: Vec<&str> = drawn
        .items
        .iter()
        .filter(|i| i.kind == Text)
        .map(|i| i.text.as_str())
        .collect();
    assert_eq!(texts, ["5h 42%", "7d 90%", "5h 28%"]);
    // Claude's week at 90% used is low: orange, ring and text alike.
    assert_eq!(drawn.items[2].color, usage::CAUTION);
    assert_eq!(drawn.items[4].color, usage::CAUTION);
    // The inner ring sits inside the outer one.
    let (outer, inner) = (&drawn.items[1], &drawn.items[2]);
    assert!(inner.x > outer.x && inner.x + inner.w < outer.x + outer.w);
    // Blocks do not overlap, the rings apart.
    for (a, first) in drawn.items.iter().enumerate() {
        for second in &drawn.items[a + 1..] {
            if first.kind == Ring && second.kind == Ring {
                continue;
            }
            let apart = first.x + first.w <= second.x
                || second.x + second.w <= first.x
                || first.y + first.h <= second.y
                || second.y + second.h <= first.y;
            assert!(apart, "{first:?} {second:?}");
        }
    }
    // The text and the bar under it share the column's width.
    assert_eq!(
        (drawn.items[6].x, drawn.items[6].w),
        (drawn.items[7].x, drawn.items[7].w)
    );
    // Widths hold whatever the values: each text is measured at 100%.
    let later = scene(&custom, &mock_at(1, UNIX), Fill::Used).unwrap();
    assert_eq!(later.width, drawn.width);
}

#[test]
fn what_is_not_there_is_left_out() {
    let custom = sample("Columns sample");
    // Without Claude, Codex's column starts the box.
    let codex: Vec<Service> = mock_at(0, UNIX)
        .into_iter()
        .filter(|s| s.id == "gpt")
        .collect();
    let drawn = scene(&custom, &codex, Fill::Used).unwrap();
    assert_eq!(kinds(&drawn), [Kind::Icon, Kind::Text, Kind::Bar]);
    assert_eq!(drawn.items[0].x, 0);
    assert_eq!(drawn.icons.len(), 1);
    // With neither, nothing is left.
    assert_eq!(scene(&custom, &[], Fill::Used), None);
    // Not read yet: waiting gauges and texts.
    let mut waiting = mock_at(0, UNIX);
    for service in &mut waiting {
        service.limits = None;
    }
    let drawn = scene(&custom, &waiting, Fill::Used).unwrap();
    assert!(
        drawn
            .items
            .iter()
            .filter(|i| i.kind == Kind::Text)
            .all(|i| i.text == "--")
    );
    assert!(
        drawn
            .items
            .iter()
            .filter(|i| i.kind != Kind::Text)
            .all(|i| i.fill.is_none())
    );
}

#[test]
fn the_fill_mode_and_colors_are_as_in_the_presets() {
    let custom = sample("Columns sample");
    let drawn = scene(&custom, &mock_at(0, UNIX), Fill::Left).unwrap();
    // What is left: 58% of Claude's 5 hours, against a dial's end.
    assert_eq!(drawn.items[1].fill, Some(580));
    assert_eq!(drawn.items[1].options, 1);
    assert_eq!(drawn.items[7].options, 0);
    assert_eq!(drawn.items[3].text, "5h 58%");
    // A color of the block's own, until the value is low.
    let bar = r##"{"name": "a", "mode": "columns", "columns": [[{"block": "bar", "service": "gpt", "color": "#7AA7FF"}]]}"##;
    let bar = parse(bar).unwrap();
    let drawn = scene(&bar, &mock_at(0, UNIX), Fill::Used).unwrap();
    assert_eq!(drawn.items[0].color, 0xFF7A_A7FF);
    let drawn = scene(&bar, &mock_at(1, UNIX), Fill::Used).unwrap();
    assert_eq!(drawn.items[0].color, usage::WARNING);
    // A value kept from before a failed read is faded.
    let mut failed = mock_at(0, UNIX);
    failed[0].limits.as_mut().unwrap()[0].reading = Reading::Failed(Some(28));
    let drawn = scene(&bar, &failed, Fill::Used).unwrap();
    assert_eq!(drawn.items[0].color, usage::dimmed(0xFF7A_A7FF));
}

#[test]
fn pc_s_blocks_show_the_share_used_whatever_the_fill_mode() {
    // A meter's ring starts at twelve and its value is the share in use,
    // beside a limit's ring showing what is left; it warns at 90% used.
    let pc = r#"{"name": "a", "mode": "columns", "columns": [
        [{"block": "ring", "service": "pc", "limit": "CPU"}],
        [{"block": "ring", "service": "claude", "limit": 1}],
        [{"block": "text", "service": "pc", "limit": "RAM", "show": "both"}]]}"#;
    let pc = parse(pc).unwrap();
    let drawn = scene(&pc, &mock_at(0, UNIX), Fill::Left).unwrap();
    assert_eq!(
        (drawn.items[0].fill, drawn.items[0].options),
        (Some(240), 0)
    );
    assert_eq!(
        (drawn.items[1].fill, drawn.items[1].options),
        (Some(580), 1)
    );
    assert_eq!(drawn.items[2].text, "RAM 61%");
    let high = scene(&pc, &mock_at(1, UNIX), Fill::Left).unwrap();
    assert_eq!(high.items[0].color, usage::WARNING);
    assert_eq!(high.items[2].color, usage::WARNING);
    // Its icon is the program's own chip.
    let icon =
        r#"{"name": "a", "mode": "columns", "columns": [[{"block": "icon", "service": "pc"}]]}"#;
    let drawn = scene(&parse(icon).unwrap(), &mock_at(0, UNIX), Fill::Used).unwrap();
    assert_eq!(drawn.icons, [("pc", crate::resources::COLOR)]);
}

#[test]
fn free_blocks_go_where_they_say() {
    let custom = sample("Free sample");
    let drawn = scene(&custom, &mock_at(0, UNIX), Fill::Used).unwrap();
    use Kind::*;
    assert_eq!(kinds(&drawn), [Ring, Text, Icon, Text, Bar]);
    assert_eq!(drawn.width, 11200);
    let place = |i: &Item| (i.x, i.y, i.w, i.h);
    assert_eq!(place(&drawn.items[0]), (0, 400, 3200, 3200));
    assert_eq!(place(&drawn.items[1]), (0, 1350, 3200, 1300));
    // An icon with no size says is its own size.
    assert_eq!(place(&drawn.items[2]), (4000, 500, 1600, 1600));
    assert_eq!(drawn.items[1].options, 1);
    assert!(drawn.fits());
    // A block past the box's edge: the hook would not take it.
    let outside = r#"{"name": "a", "mode": "free", "width": 20, "blocks": [{"block": "icon", "service": "gpt", "x": 10, "y": 0}]}"#;
    let drawn = scene(&parse(outside).unwrap(), &mock_at(0, UNIX), Fill::Used).unwrap();
    assert!(!drawn.fits());
}

#[test]
fn a_chosen_box_decides_what_shows() {
    let choice = Choice::new(&style::LOOKS[0], Fill::Used, Background::Box);
    choice.offer(&mock_at(0, UNIX));
    let offers = choice.offers();
    let picked = choice.services(&offers);
    choice.choose(Some("Columns sample".into()));
    assert_eq!(choice.services(&offers), ["claude", "gpt"]);
    let claude = offers.iter().find(|o| o.id == "claude").unwrap();
    assert_eq!(choice.shown(claude), ["5h", "7d"]);
    // The picks stay, for the presets.
    let custom = choice.custom().unwrap();
    let picks = custom.picks(&mock_at(0, UNIX));
    assert_eq!(picks.services(&["gpt", "claude"]), ["claude", "gpt"]);
    // A box not there: the picks again.
    choice.choose(Some("Gone".into()));
    assert!(choice.custom().is_none());
    assert_eq!(choice.services(&offers), picked);
    // A service a box shows whose last read failed.
    let mut services = mock_at(0, UNIX);
    services[1].trouble = Some(Trouble {
        why: "offline".into(),
        retry_at: UNIX,
    });
    choice.choose(Some("Free sample".into()));
    choice.offer(&services);
    assert!(crate::tray_icon::figures(&choice, true).1);
}

#[test]
fn box_files_are_read_and_what_is_wrong_is_told() {
    let dir = std::env::temp_dir().join(format!("limitpeeker-boxes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mine = FREE_SAMPLE.replace("Free sample", "Mine");
    std::fs::write(dir.join("b-mine.json"), format!("\u{feff}{mine}")).unwrap();
    std::fs::write(dir.join("a-broken.json"), "{").unwrap();
    std::fs::write(dir.join("c-again.JSON"), FREE_SAMPLE).unwrap();
    std::fs::write(dir.join("notes.txt"), "not a box").unwrap();
    let (boxes, problems) = load(Some(&dir));
    let names: Vec<&str> = boxes.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names[..2], ["Columns sample", "Free sample"]);
    assert_eq!(names[samples().len()..], ["Mine"]);
    assert_eq!(
        problems,
        [
            "a-broken.json: not JSON",
            "c-again.JSON: a box is named \"Free sample\" already"
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
    // No folder is no box of one's own.
    assert_eq!(load(Some(&dir)).0.len(), samples().len());
    assert_eq!(load(None).1.len(), 0);
}

#[test]
fn each_item_tells_the_block_it_comes_from() {
    // The blocks as the editor lists them: the columns from the left,
    // each top to bottom; the rings are one block.
    let custom = sample("Columns sample");
    let (_, blocks) = custom
        .laid_out(
            &mock_at(0, UNIX),
            style::DEFAULT,
            Fill::Used,
            true,
            UNIX,
            &mut guess,
        )
        .unwrap();
    assert_eq!(blocks, [0, 1, 1, 2, 3, 5, 6, 7]);
    let custom = sample("Free sample");
    let (_, blocks) = custom
        .laid_out(
            &mock_at(0, UNIX),
            style::DEFAULT,
            Fill::Used,
            true,
            UNIX,
            &mut guess,
        )
        .unwrap();
    assert_eq!(blocks, [0, 1, 2, 3, 4]);
}

#[test]
fn the_preview_tells_the_frame_the_items_and_what_stands_in() {
    let codex: Vec<Service> = mock_at(0, UNIX)
        .into_iter()
        .filter(|s| s.id == "gpt")
        .collect();
    let lines = preview(
        COLUMNS_SAMPLE,
        &codex,
        style::DEFAULT,
        Fill::Used,
        true,
        &mut guess,
    );
    let lines: Vec<&str> = lines.lines().collect();
    // Claude is not connected: it stands in with the mock figures.
    assert_eq!(lines[0], "stand-in\tClaude");
    assert!(lines[1].starts_with("box\t"));
    assert!(lines[1].ends_with(&format!(
        "\t4000\t1000\t800\t{}\t{}",
        style::DEFAULT.style.box_color,
        style::DEFAULT.style.track_color
    )));
    assert_eq!(
        lines[2..4],
        [
            format!("icon\t0\tclaude\t{}", 0xFFF4_A27Au32),
            format!("icon\t1\tgpt\t{}", 0xFFF2_F2F2u32)
        ]
    );
    let items: Vec<Vec<&str>> = lines[4..].iter().map(|l| l.split('\t').collect()).collect();
    assert_eq!(items.len(), 8);
    assert_eq!(items[0][..3], ["item", "0", "5"]);
    assert_eq!(items[3][14], "5h 42%");
    assert!(items.iter().all(|i| i.len() == 15));
    // What is wrong, on one line.
    assert_eq!(
        preview("{", &codex, style::DEFAULT, Fill::Used, true, &mut guess),
        "error\tnot JSON"
    );
    let outside = r#"{"name": "a", "mode": "free", "width": 20, "blocks": [{"block": "icon", "service": "gpt", "x": 10, "y": 0}]}"#;
    assert!(
        preview(
            outside,
            &codex,
            style::DEFAULT,
            Fill::Used,
            true,
            &mut guess
        )
        .starts_with("error\tThe taskbar would not take this box")
    );
    let nothing = r#"{"name": "a", "mode": "columns", "columns": [[{"block": "icon", "service": "nobody"}]]}"#;
    assert_eq!(
        preview(
            nothing,
            &codex,
            style::DEFAULT,
            Fill::Used,
            true,
            &mut guess
        ),
        "error\tNo block of this box has a service to show."
    );
}

#[test]
fn boxes_are_saved_renamed_and_deleted_in_their_files() {
    let dir = std::env::temp_dir().join(format!("limitpeeker-saved-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let boxes = || {
        load(Some(&dir))
            .0
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>()
    };
    let named = |name: &str| FREE_SAMPLE.replace("Free sample", name);
    // A new one, then the same under another name: one file.
    assert_eq!(save(&dir, &boxes(), "", &named("Mine")), Ok("Mine".into()));
    assert!(dir.join("Mine.json").is_file());
    assert_eq!(
        save(&dir, &boxes(), "Mine", &named("Yours")),
        Ok("Yours".into())
    );
    assert!(dir.join("Yours.json").is_file() && !dir.join("Mine.json").exists());
    // Saved again under its own name, its file is kept.
    assert_eq!(
        save(&dir, &boxes(), "Yours", &named("Yours")),
        Ok("Yours".into())
    );
    // A sample's name, another box's, or no box at all.
    assert!(
        save(&dir, &boxes(), "", FREE_SAMPLE)
            .unwrap_err()
            .contains("is a sample")
    );
    assert!(
        save(&dir, &boxes(), "", &named("Yours"))
            .unwrap_err()
            .contains("already")
    );
    assert_eq!(save(&dir, &boxes(), "", "{"), Err("not JSON".into()));
    // What a file name cannot hold, and a file already there.
    std::fs::write(dir.join("a_b.json"), "not a box").unwrap();
    assert_eq!(save(&dir, &boxes(), "", &named("a/b")), Ok("a/b".into()));
    assert!(dir.join("a_b 2.json").is_file());
    let names: Vec<String> = boxes().iter().map(|b| b.name.clone()).collect();
    assert_eq!(names[samples().len()..], ["Yours", "a/b"]);
    // The editor opens a box as it was written, and knows a sample.
    let opened_yours = opened(&boxes(), "Yours").unwrap();
    assert!(opened_yours.starts_with("0\n{") && opened_yours.contains("\"Yours\""));
    assert!(opened(&boxes(), "Free sample").unwrap().starts_with("1\n{"));
    assert!(
        delete(&boxes(), "Free sample")
            .unwrap_err()
            .contains("sample")
    );
    assert_eq!(delete(&boxes(), "Yours"), Ok(()));
    assert!(!dir.join("Yours.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_box_starts_from_its_styles_sample() {
    for style in ["columns", "free"] {
        let text = template(style).unwrap();
        let (mark, source) = text.split_once('\n').unwrap();
        assert_eq!(mark, "new");
        assert_eq!(parse(source).unwrap().style(), style);
    }
    assert_eq!(template("rows"), None);
    let styles: Vec<&str> = samples().iter().take(2).map(|s| s.style()).collect();
    assert_eq!(styles, ["columns", "free"]);
}

#[test]
fn each_sample_is_on_its_shelf_and_a_file_is_ones_own() {
    let shelves: Vec<&str> = samples().iter().map(|s| s.shelf.word()).collect();
    let mut expected = vec!["basic"; 2];
    expected.extend(["ai"; 5]);
    expected.extend(["pc"; 5]);
    assert_eq!(shelves, expected);
    assert_eq!(parse(FREE_SAMPLE).unwrap().shelf, Shelf::Mine);
    for shelf in Shelf::ALL {
        assert_eq!(Shelf::find(shelf.word()), Some(shelf));
    }
}

#[test]
fn a_new_box_starts_from_the_box_picked_of_its_style() {
    // A sample or one's own of that style; else, as when nothing was
    // picked, the style's sample.
    let boxes: Vec<Arc<CustomBox>> = samples().into_iter().map(Arc::new).collect();
    let start = |style: &str, from: &str| {
        let text = started(&boxes, style, from).unwrap();
        let (mark, source) = text.split_once('\n').unwrap();
        assert_eq!(mark, "new");
        parse(source).unwrap().name
    };
    assert_eq!(start("columns", "AI slim"), "AI slim");
    assert_eq!(start("free", "PC stack"), "PC stack");
    assert_eq!(start("columns", "PC stack"), "Columns sample");
    assert_eq!(start("free", "Nothing"), "Free sample");
    assert_eq!(start("columns", ""), "Columns sample");
    assert_eq!(started(&boxes, "rows", "AI slim"), None);
}

#[test]
fn each_preset_look_is_a_sample_of_blocks() {
    // Every one of the ten is read, names the first box's services or PC
    // alone, fits the hook and draws each block it has; the columns close
    // up over a limit not there, as Gemini's second here.
    assert_eq!(LOOK_SAMPLES.len(), 10);
    let services = mock_at(0, UNIX);
    let gemini_limits = services
        .iter()
        .find(|s| s.id == "gemini")
        .and_then(|s| s.limits.as_ref())
        .map_or(0, Vec::len);
    assert_eq!(gemini_limits, 1);
    for text in LOOK_SAMPLES {
        let custom = parse(text).unwrap();
        let ai = custom.name.starts_with("AI ");
        assert_eq!(
            custom.services(),
            if ai {
                vec!["gpt", "claude", "gemini"]
            } else {
                vec!["pc"]
            },
            "{}",
            custom.name
        );
        let expected = match custom.name.split(' ').nth(1).unwrap() {
            "inline" | "stack" => "free",
            _ => "columns",
        };
        assert_eq!(custom.style(), expected, "{}", custom.name);
        for fill in [Fill::Used, Fill::Left] {
            let (scene, drawn) = custom
                .laid_out(&services, style::DEFAULT, fill, true, UNIX, &mut text_width)
                .unwrap_or_else(|| panic!("{}", custom.name));
            assert!(scene.width <= (WIDEST * 100.0) as u32, "{}", custom.name);
            assert!(
                scene.items.len() <= crate::scene::MAX_ITEMS,
                "{}",
                custom.name
            );
            // Each block drawn but Gemini's second limit's and the spaces.
            let blocks = custom.blocks();
            for (index, block) in blocks.iter().enumerate() {
                let missing =
                    block.service.as_deref() == Some("gemini") && block.limits == [Which::Nth(1)];
                let shown = drawn.contains(&index);
                assert_eq!(
                    shown,
                    !missing && block.shape != Shape::Space,
                    "{} block {index}",
                    custom.name
                );
            }
        }
    }
}

#[test]
fn a_text_is_measured_in_the_taskbar_font() {
    let value = text_width("100%", 1100, 600);
    assert!((1500..4000).contains(&value), "{value}");
    assert!(text_width("7d 100%", 1100, 600) > value);
    assert!(text_width("100%", 1400, 600) > value);
    assert_eq!(text_width("100%", 1100, 600), value);
}

#[test]
fn a_text_tells_how_long_until_its_limit_resets() {
    // Quiet, as a limit's name is, and as wide as its widest time, so the box
    // does not move as it counts down; `--` when not known or passed; left
    // out of the steady services, so the log's scene line does not move each
    // minute.
    let custom = parse(
        r#"{ "name": "Resets", "mode": "columns", "columns": [[
            { "block": "text", "service": "claude", "limit": 1, "show": "resets" },
            { "block": "text", "service": "claude", "limit": 2, "show": "resets" }
        ]] }"#,
    )
    .unwrap();
    let services = mock_at(0, UNIX);
    let drawn = scene(&custom, &services, Fill::Used).unwrap();
    let quiet = style::DEFAULT.style.label_color;
    let texts: Vec<(&str, u32)> = drawn
        .items
        .iter()
        .map(|i| (i.text.as_str(), i.color))
        .collect();
    assert_eq!(texts, [("2h 14m", quiet), ("3d 4h", quiet)]);
    let widest = ["23h 59m", "29d 23h"]
        .map(|text| guess(text, TEXT_SIZE, TEXT_WEIGHT))
        .into_iter()
        .max()
        .unwrap();
    assert_eq!(drawn.items[0].w, widest + TEXT_ROOM);
    let at = |now: i64, services: &[Service]| {
        custom
            .scene(services, style::DEFAULT, Fill::Used, true, now, &mut guess)
            .unwrap()
    };
    let later = at(UNIX + 3600, &services);
    assert_eq!(
        (later.items[0].text.as_str(), later.width),
        ("1h 14m", drawn.width)
    );
    assert_eq!(at(UNIX + 30 * 86_400, &services).items[0].text, "--");
    let mut unknown = services.clone();
    if let Some(claude) = unknown.iter_mut().find(|s| s.id == "claude")
        && let Some(limits) = claude.limits.as_mut()
    {
        limits[0].resets = None;
    }
    assert_eq!(at(UNIX, &unknown).items[0].text, "--");
    assert_eq!(at(UNIX, &usage::steady(&services)).items[0].text, "--");
    // The file says it back, and only a limit's text may.
    assert!(custom.source.contains(r#""show": "resets""#));
    let no_limit = r#"{ "name": "X", "mode": "columns", "columns": [[
        { "block": "text", "service": "claude", "show": "soon" } ]] }"#;
    assert!(parse(no_limit).unwrap_err().contains("resets"));
}

#[test]
fn the_preview_draws_the_preset_on_the_boxs_color() {
    // The editor's stage shows the preset as the taskbar would: on a white
    // box the frame line carries the color and a black track, and a text
    // block's value is black.
    let white = style::DEFAULT.colored(Some(0xFFFF_FFFF), false);
    let lines = preview(
        COLUMNS_SAMPLE,
        &mock_at(0, UNIX),
        &white,
        Fill::Used,
        true,
        &mut guess,
    );
    let lines: Vec<&str> = lines.lines().collect();
    assert!(
        lines[0].ends_with(&format!("\t{}\t{}", 0xFFFF_FFFFu32, 0x1F00_0000u32)),
        "{}",
        lines[0]
    );
    let items: Vec<Vec<&str>> = lines[3..].iter().map(|l| l.split('\t').collect()).collect();
    assert_eq!(items[3][14], "5h 42%");
    assert_eq!(items[3][8], 0xF200_0000u32.to_string());
    // The icons are deepened as the box draws them.
    assert_eq!(
        lines[2],
        format!("icon\t1\tgpt\t{}", white.service_color(0xFFF2_F2F2))
    );
}

#[test]
fn a_scene_takes_the_boxs_color_and_dark_text_on_a_light_one() {
    // The look carries the box's color; on a white one the plain text is
    // black, a pale service color deepens for the bar and the icon alike,
    // and orange stays orange; on the default nothing changes.
    let custom = sample("Columns sample");
    let services = mock_at(0, UNIX);
    let gpt = services[0].color;
    let white = style::DEFAULT.colored(Some(0xFFFF_FFFF), false);
    let drawn = custom
        .scene(&services, &white, Fill::Used, true, UNIX, &mut guess)
        .unwrap();
    assert_eq!(drawn.style.box_color, 0xFFFF_FFFF);
    assert_eq!(drawn.style.track_color, 0x1F00_0000);
    assert_eq!(drawn.items[3].color, 0xF200_0000);
    assert_eq!(drawn.items[7].color, white.service_color(gpt));
    assert_ne!(drawn.items[7].color, gpt);
    assert_eq!(drawn.icons[1], ("gpt", white.service_color(gpt)));
    assert_eq!(drawn.items[4].color, usage::CAUTION);
    let plain = custom
        .scene(
            &services,
            style::DEFAULT,
            Fill::Used,
            true,
            UNIX,
            &mut guess,
        )
        .unwrap();
    assert_eq!(plain.style.box_color, style::BOX_COLOR);
    assert_eq!(plain.items[3].color, style::DEFAULT.text);
    assert_eq!((plain.items[7].color, plain.icons[1]), (gpt, ("gpt", gpt)));
}

#[test]
fn blocks_warn_in_orange_then_red_unless_turned_off() {
    // Claude's week at 90% used is orange, at 97% red; off, it keeps the
    // service's color.
    let custom = sample("Columns sample");
    let services = mock_at(0, UNIX);
    let week = |services: &[Service], warn: bool| {
        let drawn = custom
            .scene(services, style::DEFAULT, Fill::Used, warn, UNIX, &mut guess)
            .unwrap();
        (drawn.items[2].color, drawn.items[4].color)
    };
    assert_eq!(week(&services, true), (usage::CAUTION, usage::CAUTION));
    let mut red = services.clone();
    if let Some(claude) = red.iter_mut().find(|s| s.id == "claude")
        && let Some(limits) = claude.limits.as_mut()
    {
        limits[1].reading = Reading::Read(97);
    }
    assert_eq!(week(&red, true), (usage::WARNING, usage::WARNING));
    let (ring, text) = week(&red, false);
    assert!(![usage::WARNING, usage::CAUTION].contains(&ring));
    assert!(![usage::WARNING, usage::CAUTION].contains(&text));
}
