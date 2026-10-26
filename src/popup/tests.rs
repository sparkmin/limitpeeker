use super::*;
use crate::choice::Choice;
use crate::providers::ReadNow;
use crate::settings_link;
use crate::style::{self, Background, Fill};
use crate::usage::{self, mock_at};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;

const UNIX: i64 = 1_790_000_000;

fn choice(step: u64) -> Choice {
    let choice = Choice::new(&style::LOOKS[0], Fill::Used, Background::Box);
    choice.offer(&mock_at(step, UNIX));
    choice
}
#[test]
fn a_custom_box_is_a_style_and_dims_what_its_blocks_choose() {
    // The box's toggle is on and no preset is; Background, Services and
    // Limits cannot be changed while it shows, and Show can.
    let choice = choice(0);
    choice.choose(Some("Columns sample".into()));
    let rows = rows(&choice, true);
    let on: Vec<&str> = rows[..2]
        .iter()
        .flat_map(|r| &r.chips)
        .filter(|c| c.on)
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(on, ["Columns sample"]);
    assert_eq!(
        (rows[0].shape, rows[1].shape),
        (Shape::Segments, Shape::Toggles)
    );
    // Last, the editor; dim without the settings window.
    let last = rows[1].chips.last().unwrap();
    assert_eq!((last.text.as_str(), last.enabled), ("Customize…", true));
    assert!(!super::rows(&choice, false)[1].chips.last().unwrap().enabled);
    // A sample is offered only while in use; the other is the editor's.
    let customs: Vec<&str> = rows[1].chips.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(customs, ["Columns sample", "Customize…"]);
    assert!(
        all_rows(&choice)
            .iter()
            .flat_map(|r| &r.chips)
            .all(|c| c.enabled)
    );
    // The box goes anywhere still, the places as on the taskbar from the
    // left, and last Hide.
    assert_eq!(rows[2].title, "Place");
    assert!(rows[2].chips.iter().all(|c| c.enabled));
    let places: Vec<&str> = rows[2].chips.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(places, ["Left", "Start", "Apps", "Tray", "Hide"]);
    for row in &rows[4..] {
        assert!(
            row.chips.iter().all(|c| !c.enabled && c.drag.is_none()),
            "{}",
            row.title
        );
    }
    // The box's services, in its order, and its limits.
    let services: Vec<&str> = rows[5]
        .chips
        .iter()
        .filter(|c| c.on)
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(services, ["Claude", "GPT"]);
    // Back to a preset, no sample is offered.
    assert!(settings_link::apply(&choice, "look\tslim"));
    let customs: Vec<String> = super::rows(&choice, true)[1]
        .chips
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert_eq!(customs, ["Customize…"]);
}
fn measure(text: &str, _: Font) -> f32 {
    text.chars().count() as f32 * 7.0
}
/// When Refresh now is taken again, as the controller gives it.
fn at(read_now: ReadNow, now: Instant) -> Option<Instant> {
    match read_now {
        ReadNow::Ready => Some(now),
        ReadNow::After(left) => Some(now + left),
        ReadNow::Never => None,
    }
}
/// The popup as a box opens it, its settings unfolded; the tray icon's is
/// folded.
fn shown(choice: &Choice, read_now: ReadNow, now: Instant) -> View {
    let unfolded = Status {
        unfolded: true,
        ..Status::default()
    };
    view_at(choice, unfolded, at(read_now, now), true, UNIX)
}
fn laid(view: &View, now: Instant) -> Layout {
    layout(view, now, &measure)
}

#[test]
fn a_service_on_is_bold_and_as_wide_on_or_off() {
    // The summary's filter and a box's services: each as wide as its name
    // in bold, on or off, so a pick does not move the others.
    let now = Instant::now();
    let bold = |text: &str, font: Font| {
        let per = if font == Font::Picked { 8.0 } else { 7.0 };
        text.chars().count() as f32 * per
    };
    let layout = layout(&shown(&choice(0), ReadNow::Ready, now), now, &bold);
    let services: Vec<_> = layout
        .controls
        .iter()
        .filter(|c| matches!(c.kind, Kind::Filter { .. }))
        .collect();
    assert!(
        services
            .iter()
            .any(|c| matches!(c.kind, Kind::Filter { on: true, .. }))
    );
    assert!(
        services
            .iter()
            .any(|c| matches!(c.kind, Kind::Filter { on: false, .. }))
    );
    for control in services {
        assert_eq!(
            control.at.w,
            32.0 + bold(&control.text, Font::Picked).ceil()
        );
    }
}

#[test]
fn the_summary_filter_sits_on_top_in_the_summarys_look() {
    // Every offered service in the providers' order, on top of the
    // summary and in its own look, not chips; what it sends is what the
    // settings window sends; the last one on cannot be pressed.
    let now = Instant::now();
    let picked = choice(0);
    let offers = picked.offers();
    let filter = |layout: &Layout| -> Vec<(String, bool, bool, Rect)> {
        layout
            .controls
            .iter()
            .filter(|c| in_filter(c))
            .filter_map(|c| match c.kind {
                Kind::Filter { on, .. } => Some((c.text.clone(), on, c.enabled, c.at)),
                _ => None,
            })
            .collect()
    };
    let layout = laid(&shown(&picked, ReadNow::Ready, now), now);
    let line = filter(&layout);
    let names: Vec<&str> = line.iter().map(|(name, ..)| name.as_str()).collect();
    assert_eq!(
        names,
        offers.iter().map(|o| o.name.as_str()).collect::<Vec<_>>()
    );
    let on: Vec<&str> = line
        .iter()
        .filter(|(_, on, ..)| *on)
        .map(|(name, ..)| name.as_str())
        .collect();
    assert_eq!(on, ["GPT", "Claude", "Gemini"]);
    assert!(line.iter().all(|(_, _, enabled, _)| *enabled));
    // Above the summary's first service, and within the popup.
    let first = layout
        .parts
        .iter()
        .find_map(|p| match p {
            Part::Text { text, at, .. } if text == "GPT" => Some(at.y),
            _ => None,
        })
        .unwrap();
    assert!(
        line.iter()
            .all(|(_, _, _, at)| at.y + at.h <= first && at.x + at.w <= RIGHT)
    );
    // On a card of its own, as wide as the rules, holding the whole line
    // and ending before the summary starts.
    let card = layout
        .parts
        .iter()
        .find_map(|p| match p {
            Part::Panel { at } => Some(*at),
            _ => None,
        })
        .unwrap();
    assert_eq!((card.x, card.x + card.w), (PAD, RIGHT));
    assert!(card.y + card.h < first);
    assert!(line.iter().all(|(_, _, _, at)| {
        card.x < at.x
            && at.x + at.w < card.x + card.w
            && card.y < at.y
            && at.y + at.h < card.y + card.h
    }));
    for control in layout.controls.iter().filter(|c| in_filter(c)) {
        let Action::Apply(command) = &control.action else {
            panic!("{:?}", control.action)
        };
        assert!(command.starts_with("summary-service\t"));
        assert!(settings_link::apply(&choice(0), command), "{command}");
    }
    // Down to one on: it stays drawn on and cannot be pressed.
    for id in ["gpt", "claude"] {
        picked.toggle_summary_service(&offers, id);
    }
    let line = filter(&laid(&shown(&picked, ReadNow::Ready, now), now));
    let last: Vec<_> = line
        .iter()
        .filter(|(_, on, ..)| *on)
        .map(|(name, _, enabled, _)| (name.as_str(), *enabled))
        .collect();
    assert_eq!(last, [("Gemini", false)]);
    // No chip row for it is left among the menu's rows.
    assert!(rows(&picked, true).iter().all(|row| row.title != "Summary"));
}

/// Whether a control is in the summary's filter line, not among a box's
/// services.
fn in_filter(control: &Control) -> bool {
    matches!(&control.action, Action::Apply(command) if command.starts_with("summary-service\t"))
}

/// What can be chosen under the summary, as its text, whether it is on, and
/// whether it can be pressed: tabs, segments, chips and services.
fn chips(layout: &Layout) -> Vec<(&str, bool, bool)> {
    layout
        .controls
        .iter()
        .filter(|c| !in_filter(c))
        .filter_map(|c| {
            let on = match c.kind {
                Kind::Chip { on }
                | Kind::Toggle { on }
                | Kind::Segment { on }
                | Kind::Tab { on, .. }
                | Kind::Filter { on, .. } => on,
                _ => return None,
            };
            Some((c.text.as_str(), on, c.enabled))
        })
        .collect()
}

#[test]
fn the_popup_shows_what_is_used_and_when_it_starts_over() {
    // Every limit of each service in the box, in the box's order, shown
    // or not, with the time until it starts over.
    let now = Instant::now();
    let view = shown(&choice(0), ReadNow::Ready, now);
    let lines: Vec<Vec<_>> = view
        .services
        .iter()
        .map(|s| {
            s.lines
                .iter()
                .flatten()
                .map(|l| (l.label.as_str(), l.text.as_str(), l.warn, l.resets.as_str()))
                .collect()
        })
        .collect();
    let names: Vec<_> = view.services.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["GPT", "Claude", "Gemini"]);
    // Claude's week has 10% left, which warns in orange.
    use usage::Warn::{Caution, None as Fine};
    assert_eq!(
        lines,
        [
            vec![("5h", "28%", Fine, "2h 14m"), ("7d", "42%", Fine, "3d 4h")],
            vec![
                ("5h", "42%", Fine, "2h 14m"),
                ("7d", "90%", Caution, "3d 4h"),
                ("Opus 7d", "60%", Fine, "3d 4h")
            ],
            vec![("1d", "9%", Fine, "9h 5m")]
        ]
    );
    assert_eq!(view.status, "Tray only");
    assert_eq!(view.warning, None);
    let status = Status {
        mock: true,
        on_taskbar: true,
        snap_wait: true,
        restarting: false,
        unfolded: false,
        reason: None,
        notice: None,
    };
    let view = view_at(&choice(0), status, at(ReadNow::Never, now), true, UNIX);
    assert_eq!(view.status, "On the taskbar · Mock data");
    assert_eq!(view.warning, Some("Restart Explorer to show the box."));
}

#[test]
fn pc_tells_how_much_memory_and_that_its_vram_is_shared() {
    // RAM and VRAM tell their amounts where a limit tells when it starts
    // over, in a column wide enough for them, so every bar in the popup ends
    // as much sooner; a GPU built in says on PC's title row that VRAM is the
    // memory it shares. Without PC, the popup is as it was.
    let now = Instant::now();
    let picked = choice(0);
    let before = laid(&shown(&picked, ReadNow::Ready, now), now);
    let offers = picked.offers();
    picked.toggle_summary_service(&offers, crate::resources::ID);
    let view = shown(&picked, ReadNow::Ready, now);
    let pc = view.services.iter().find(|s| s.id == "pc").unwrap();
    let lines: Vec<_> = pc
        .lines
        .iter()
        .flatten()
        .map(|l| (l.label.as_str(), l.text.as_str(), l.amount.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            ("CPU", "24%", ""),
            ("RAM", "61%", "9.6/15.8 GB"),
            ("GPU", "8%", ""),
            ("VRAM", "43%", "3.4/7.9 GB")
        ]
    );
    assert_eq!(pc.aside, Some("VRAM: shared GPU memory"));
    assert!(
        view.services
            .iter()
            .filter(|s| s.id != "pc")
            .all(|s| s.aside.is_none())
    );
    let layout = laid(&view, now);
    let texts = |layout: &Layout, text: &str| -> Vec<Rect> {
        layout
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { at, text: t, .. } if t == text => Some(*at),
                _ => None,
            })
            .collect()
    };
    assert_eq!(texts(&layout, "9.6/15.8 GB")[0].w, AMOUNTS_W);
    assert_eq!(texts(&layout, "VRAM: shared GPU memory").len(), 1);
    let bars = |layout: &Layout| -> Vec<f32> {
        layout
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Bar { at, .. } => Some(at.x + at.w),
                _ => None,
            })
            .collect()
    };
    let (ends, ends_before) = (bars(&layout), bars(&before));
    assert!(ends.iter().all(|&end| end == ends[0]));
    assert_eq!(ends_before[0] - ends[0], AMOUNTS_W - RESETS_W);
    assert_eq!(texts(&before, "2h 14m")[0].w, RESETS_W);
    // As Task Manager counts them: one decimal under 100 GB.
    let gb = |used: u64, total: u64| {
        amount_text(usage::Amount {
            used,
            total,
            shared: false,
        })
    };
    assert_eq!(gb(0, 1 << 30), "0.0/1.0 GB");
    assert_eq!(gb(99 << 30, 128 << 30), "99.0/128 GB");
}

#[test]
fn values_follow_the_fill_mode_and_failed_reads_fade() {
    // What is left shows when picked; a failed read keeps its value
    // faded or shows `?`, a limit not read yet `--`, and a service not
    // read at all waits.
    let now = Instant::now();
    let left = choice(0);
    left.set_fill(Fill::Left);
    let gpt = &shown(&left, ReadNow::Ready, now).services[0];
    let first = &gpt.lines.as_ref().unwrap()[0];
    assert_eq!((first.text.as_str(), first.share), ("72%", Some(72)));
    let failing = shown(&choice(2), ReadNow::Ready, now);
    let claude: Vec<_> = failing.services[1]
        .lines
        .iter()
        .flatten()
        .map(|l| (l.text.as_str(), l.share, l.stale, l.resets.as_str()))
        .collect();
    assert_eq!(
        claude,
        [
            ("42%", Some(42), true, "2h 14m"),
            ("?", None, true, ""),
            ("--", None, false, "")
        ]
    );
    assert_eq!(failing.services[2].lines, None);
    let layout = laid(&failing, now);
    assert!(layout.parts.iter().any(|p| matches!(
        p,
        Part::Text { text, .. } if text == "Waiting for the first read"
    )));
}

#[test]
fn the_boxes_are_chosen_added_removed_and_moved_from_the_popup() {
    // The boxes as tabs by name, the chosen one on and removable from its tab
    // while another is left; the rows below are the chosen box's; its order
    // under its place only while another box is in that place; PC's limits
    // four at most.
    let choice = choice(0);
    let named = |choice: &Choice| -> Vec<(String, bool, bool)> {
        tabs(choice)
            .into_iter()
            .map(|t| (t.text, t.on, t.close))
            .collect()
    };
    let order = |choice: &Choice| -> Vec<(String, bool, bool)> {
        rows(choice, true)
            .iter()
            .filter(|r| r.shape == Shape::Note)
            .flat_map(|r| r.chips.iter().map(|c| (c.text.clone(), c.on, c.enabled)))
            .collect()
    };
    let tab = |text: &str, on, close| (text.to_string(), on, close);
    let chip = |text: &str, on, enabled| (text.to_string(), on, enabled);
    assert_eq!(named(&choice), [tab("GPT · Claude · Gemini", true, false)]);
    assert!(order(&choice).is_empty());
    assert!(settings_link::apply(&choice, "add"));
    for id in ["gpt", "claude", "gemini"] {
        assert!(settings_link::apply(&choice, &format!("service\t{id}")));
    }
    assert!(settings_link::apply(&choice, "service\tpc"));
    assert!(settings_link::apply(&choice, "service\tgemini"));
    assert_eq!(named(&choice)[1], tab("PC", true, true));
    assert_eq!(named(&choice)[0].2, false);
    // Both beside the tray: the second moves left, once.
    assert_eq!(
        order(&choice),
        [chip("←", false, true), chip("→", false, false)]
    );
    assert!(settings_link::apply(&choice, "order\tearlier"));
    assert_eq!(named(&choice)[0], tab("PC", true, true));
    assert_eq!(
        order(&choice),
        [chip("←", false, false), chip("→", false, true)]
    );
    // PC's meters: four at most, in one box.
    for meter in ["GPU", "VRAM"] {
        assert!(settings_link::apply(
            &choice,
            &format!("limit\tpc\t{meter}")
        ));
    }
    let offers = choice.offers();
    let pc = offers.iter().find(|o| o.id == "pc").unwrap();
    assert_eq!(choice.shown(pc), ["CPU", "RAM", "GPU", "VRAM"]);
    // Elsewhere, no order; the other box chosen, then this one removed.
    assert!(settings_link::apply(&choice, "place\tleft"));
    assert!(order(&choice).is_empty());
    assert!(settings_link::apply(&choice, "select\t1"));
    assert_eq!(named(&choice)[1].1, true);
    assert!(!settings_link::apply(&choice, "select\t2"));
    assert!(settings_link::apply(&choice, "remove"));
    assert_eq!(named(&choice), [tab("PC", true, false)]);
    assert!(settings_link::apply(&choice, "remove"));
    assert_eq!(choice.tiles().len(), 1);
    // To the settings window: the boxes by name, the chosen one, its place
    // and whether it can move, and dragging.
    assert!(settings_link::apply(&choice, "drag\toff"));
    let state = settings_link::state(&choice, None, crate::startup::Startup::Off, &[]);
    for line in [
        "boxes\t0\tPC<N>",
        "place\tleft<N>",
        "order\t0\t0<N>",
        "drag\toff",
    ] {
        assert!(state.contains(&line.replace("<N>", "\n")), "{line}");
    }
}

#[test]
fn the_boxes_are_tabs_over_the_chosen_ones_card() {
    // The tabs, the chosen one's close after it, and a new tab at the end;
    // the chosen tab and the card drawn as one shape, the rows on the card;
    // segments on one line as wide as the card's room; the place said in
    // full under them; and what every box shares under the card. Done at
    // the end of the tabs takes its room.
    let now = Instant::now();
    let picked = choice(0);
    assert!(settings_link::apply(&picked, "add"));
    let layout = laid(&shown(&picked, ReadNow::Ready, now), now);
    let tabs: Vec<_> = layout
        .controls
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.kind, Kind::Tab { .. } | Kind::Glyph { .. }))
        .map(|(_, c)| (c.text.as_str(), c.kind.clone(), c.action.clone()))
        .collect();
    assert_eq!(
        tabs,
        [
            // Two names too long for their tabs, cut; the chosen one
            // shorter for its close.
            (
                "GPT · Claude…",
                Kind::Tab {
                    on: false,
                    hidden: false
                },
                Action::Apply("select\t0".into())
            ),
            (
                "GPT · C…",
                Kind::Tab {
                    on: true,
                    hidden: false
                },
                Action::Apply("select\t1".into())
            ),
            (
                "Remove box",
                Kind::Glyph { glyph: CLOSE_GLYPH },
                Action::Apply("remove".into())
            ),
            (
                "Add box",
                Kind::Glyph { glyph: ADD_GLYPH },
                Action::Apply("add".into())
            ),
        ]
    );
    let chosen = layout
        .controls
        .iter()
        .find(|c| matches!(c.kind, Kind::Tab { on: true, .. }))
        .unwrap()
        .at;
    let (tab, card) = layout
        .parts
        .iter()
        .find_map(|p| match p {
            Part::TabCard { tab, card } => Some((*tab, *card)),
            _ => None,
        })
        .unwrap();
    assert_eq!((tab.x, tab.y), (chosen.x, chosen.y));
    assert!(tab.w > chosen.w);
    assert_eq!(card.y, tab.y + tab.h);
    assert_eq!((card.x, card.x + card.w), (PAD, RIGHT));
    let places: Vec<Rect> = layout
        .controls
        .iter()
        .filter(|c| matches!(&c.action, Action::Apply(a) if a.starts_with("place\t")))
        .map(|c| c.at)
        .collect();
    assert_eq!(places.len(), 5);
    assert!(places.iter().all(|p| p.y == places[0].y));
    let width: f32 = places.iter().map(|p| p.w).sum();
    assert!((width - (RIGHT - CARD_INSET - CHIPS_X)).abs() < 0.01);
    assert!(
        places
            .iter()
            .all(|p| card.y < p.y && p.y + p.h < card.y + card.h)
    );
    let text = |want: &str| {
        layout
            .parts
            .iter()
            .find_map(|p| match p {
                Part::Text { text, at, .. } if text == want => Some(*at),
                _ => None,
            })
            .unwrap()
    };
    assert!(text("Beside the tray").y > places[0].y);
    let all = text("Show values as");
    assert!(all.y > card.y + card.h);
    let shows: Vec<_> = layout
        .controls
        .iter()
        .filter(|c| matches!(&c.action, Action::Apply(a) if a.starts_with("fill\t")))
        .map(|c| (c.text.as_str(), c.kind.clone(), c.at.y > all.y))
        .collect();
    assert_eq!(
        shows,
        [
            ("Used", Kind::Segment { on: true }, true),
            ("Remaining", Kind::Segment { on: false }, true)
        ]
    );
    // Tabs that do not fit: the chosen one keeps up to its share, the
    // narrow ones whole, the wide ones cut to the same width.
    assert_eq!(
        tab_widths(&[173.0, 40.0, 214.0, 84.0], Some(0), 280.0),
        [126.0, 40.0, 57.0, 57.0]
    );
    assert_eq!(tab_widths(&[60.0, 40.0], Some(1), 280.0), [60.0, 40.0]);
    // A tab alone is cut too.
    assert_eq!(tab_widths(&[300.0], Some(0), 200.0), [200.0]);
    // A name too long for its tab is cut.
    let cut = fit("Claude · GPT · Gemini · Grok", 60.0, &measure);
    assert!(cut.ends_with('…') && measure(&cut, Font::Body) <= 60.0);
    assert_eq!(fit("PC", 60.0, &measure), "PC");
}

#[test]
fn chips_send_what_the_settings_window_sends() {
    // Each chip's command is taken by the settings window's rules, and
    // what is chosen shows as on.
    let now = Instant::now();
    let layout = laid(&shown(&choice(0), ReadNow::Ready, now), now);
    for control in &layout.controls {
        if let Action::Apply(command) = &control.action {
            assert!(settings_link::apply(&choice(0), command), "{command}");
        }
    }
    let on: Vec<_> = chips(&layout)
        .into_iter()
        .filter(|&(_, on, _)| on)
        .map(|(text, _, _)| text)
        .collect();
    assert_eq!(
        on,
        [
            "GPT · Claude · Gemini",
            "Slim",
            "Tray",
            "One box",
            "GPT",
            "Claude",
            "Gemini",
            "5h",
            "7d",
            "5h",
            "7d",
            "1d",
            "Used"
        ]
    );
    let actions: Vec<_> = layout.controls.iter().map(|c| c.action.clone()).collect();
    assert_eq!(
        actions[actions.len() - 3..],
        [Action::Refresh, Action::Settings, Action::Exit]
    );
    assert!(actions.contains(&Action::Apply("limit\tclaude\tOpus 7d".into())));
    // There is no look of all looks in turn.
    assert!(!actions.contains(&Action::Apply("look\tcycle".into())));
}

#[test]
fn chips_that_are_on_and_summary_lines_can_be_dragged() {
    // Services and limits that show, among themselves; the summary's
    // services and each one's lines; a drop sends the window's command,
    // which its rules take.
    let now = Instant::now();
    let picked = choice(0);
    let layout = laid(&shown(&picked, ReadNow::Ready, now), now);
    let dragged: Vec<_> = layout
        .controls
        .iter()
        .filter_map(|c| c.drag.map(|d| (c.text.as_str(), layout.drags[d].index)))
        .collect();
    assert_eq!(
        dragged,
        [
            ("GPT", 0),
            ("Claude", 1),
            ("Gemini", 2),
            ("5h", 0),
            ("7d", 1),
            ("5h", 0),
            ("7d", 1),
            ("1d", 0)
        ]
    );
    let groups: Vec<_> = layout
        .drags
        .iter()
        .filter(|d| !d.across)
        .map(|d| (d.group.as_str(), d.index, d.command.as_str()))
        .collect();
    assert_eq!(
        groups[..2],
        [
            ("summary", 0, "move-summary\tgpt\t"),
            ("summary\tgpt", 0, "move-summary-limit\tgpt\t5h\t"),
        ]
    );
    assert_eq!(groups.len(), 3 + 2 + 3 + 1);
    for drag in &layout.drags {
        let command = format!("{}{}", drag.command, 0);
        assert!(settings_link::apply(&choice(0), &command), "{command}");
    }
    // Dropped nearest another of its group, it takes that one's place;
    // the mark goes after it when moving later, before when earlier.
    let services: Vec<_> = (0..layout.drags.len())
        .filter(|&d| layout.drags[d].group == "summary")
        .collect();
    let (first, third) = (layout.drags[services[0]].at, layout.drags[services[2]].at);
    let over_third = (third.x + 10.0, third.y + 5.0);
    assert_eq!(target(&layout.drags, services[0], over_third), Some(2));
    let mark = drop_mark(&layout.drags, services[0], 2).unwrap();
    assert!((mark.y - (third.y + third.h - DROP_MARK / 2.0)).abs() < 0.01);
    let mark = drop_mark(&layout.drags, services[2], 0).unwrap();
    assert!((mark.y - (first.y - DROP_MARK / 2.0)).abs() < 0.01);
    assert_eq!(drop_mark(&layout.drags, services[1], 1), None);
    // A chip is dropped among its own row only.
    let gpt = layout
        .controls
        .iter()
        .find(|c| c.action == Action::Apply("service\tgpt".into()))
        .and_then(|c| c.drag)
        .unwrap();
    let far_away = (third.x, third.y);
    assert!(target(&layout.drags, gpt, far_away).unwrap() < 3);
    let chip = layout.drags[gpt].at;
    let mark = drop_mark(&layout.drags, gpt, 1).unwrap();
    assert!(mark.x > chip.x + chip.w && mark.h == chip.h);
}

#[test]
fn the_summary_keeps_an_order_of_its_own() {
    // Moved in the summary, the summary changes and the box does not;
    // moved in the box, the box changes and the summary keeps its own.
    let now = Instant::now();
    let picked = choice(0);
    let names = |picked: &Choice| {
        let view = shown(picked, ReadNow::Ready, now);
        let summary: Vec<_> = view.services.iter().map(|s| s.id).collect();
        let lines: Vec<_> = view.services[0]
            .lines
            .iter()
            .flatten()
            .map(|l| l.label.clone())
            .collect();
        (summary, lines)
    };
    assert!(settings_link::apply(&picked, "move-summary\tgemini\t0"));
    assert!(settings_link::apply(
        &picked,
        "move-summary-limit\tgemini\t1d\t0"
    ));
    assert!(settings_link::apply(&picked, "move-summary\tclaude\t0"));
    assert!(settings_link::apply(
        &picked,
        "move-summary-limit\tclaude\tOpus 7d\t0"
    ));
    assert_eq!(
        names(&picked),
        (
            vec!["claude", "gemini", "gpt"],
            vec!["Opus 7d".to_string(), "5h".into(), "7d".into()]
        )
    );
    assert_eq!(
        picked.services(&picked.offers()),
        ["gpt", "claude", "gemini"]
    );
    assert!(settings_link::apply(&picked, "move\tgemini\t0"));
    assert_eq!(
        picked.services(&picked.offers()),
        ["gemini", "gpt", "claude"]
    );
    assert_eq!(names(&picked).0, ["claude", "gemini", "gpt"]);
    // A service turned off leaves the summary; turned on again, it comes
    // back where the summary had it.
    let offers = picked.offers();
    picked.toggle_service(&offers, "gemini");
    assert_eq!(names(&picked).0, ["claude", "gpt"]);
    picked.toggle_service(&offers, "gemini");
    assert_eq!(names(&picked).0, ["claude", "gemini", "gpt"]);
}

#[test]
fn what_cannot_be_turned_on_or_off_is_dimmed() {
    // Four services and two limits at most, and never the last one off.
    let now = Instant::now();
    let picked = choice(0);
    let offers = picked.offers();
    picked.toggle_service(&offers, "grok");
    let layout = laid(&shown(&picked, ReadNow::Ready, now), now);
    let listed = chips(&layout);
    let state = |name: &str| {
        listed
            .iter()
            .filter(|c| c.0 == name)
            .copied()
            .collect::<Vec<_>>()
    };
    assert_eq!(state("Grok"), [("Grok", true, true)]);
    assert_eq!(state("PC"), [("PC", false, false)]);
    assert_eq!(state("Opus 7d"), [("Opus 7d", false, false)]);
    // Gemini's one limit cannot be turned off.
    assert_eq!(state("1d"), [("1d", true, false)]);
    let one = choice(0);
    for id in ["claude", "gemini"] {
        one.toggle_service(&offers, id);
    }
    let layout = laid(&shown(&one, ReadNow::Ready, now), now);
    assert!(chips(&layout).contains(&("GPT", true, false)));
    assert!(chips(&layout).contains(&("Claude", false, true)));
}

#[test]
fn refresh_counts_down_and_settings_needs_its_window() {
    // Greyed with the seconds left while it waits, and for good with the
    // mock figures.
    let now = Instant::now();
    let refresh = |layout: &Layout| {
        let c = layout
            .controls
            .iter()
            .find(|c| c.action == Action::Refresh)
            .unwrap();
        (c.text.clone(), c.enabled)
    };
    let waiting = shown(
        &choice(0),
        ReadNow::After(Duration::from_millis(8_500)),
        now,
    );
    assert_eq!(
        refresh(&laid(&waiting, now)),
        ("Refresh now (in 9 s)".into(), false)
    );
    assert_eq!(
        refresh(&laid(&waiting, now + Duration::from_secs(8))),
        ("Refresh now (in 1 s)".into(), false)
    );
    assert_eq!(
        refresh(&laid(&waiting, now + Duration::from_secs(9))),
        ("Refresh now".into(), true)
    );
    let mock = shown(&choice(0), ReadNow::Never, now);
    assert_eq!(refresh(&laid(&mock, now)), ("Refresh now".into(), false));
    let without = view_at(
        &choice(0),
        Status::default(),
        at(ReadNow::Ready, now),
        false,
        UNIX,
    );
    let settings = laid(&without, now)
        .controls
        .into_iter()
        .find(|c| c.action == Action::Settings)
        .unwrap();
    assert!(!settings.enabled);
}

#[test]
fn controls_fit_the_width_and_never_overlap() {
    // Chips that do not fit go on to the next line, a row of limits
    // after its service's name.
    let now = Instant::now();
    let all = choice(0);
    let offers = all.offers();
    all.toggle_service(&offers, "grok");
    let layout = laid(&shown(&all, ReadNow::Ready, now), now);
    for (index, control) in layout.controls.iter().enumerate() {
        assert!(control.at.x >= PAD - 4.0, "{}", control.text);
        assert!(
            control.at.x + control.at.w <= WIDTH - PAD + 4.0,
            "{}",
            control.text
        );
        for other in &layout.controls[index + 1..] {
            assert!(
                !control.at.overlaps(&other.at),
                "{} {}",
                control.text,
                other.text
            );
        }
    }
    let looks = [
        Action::Apply("look\tslim".into()),
        Action::Apply("look\trings".into()),
    ];
    let tops: Vec<_> = layout
        .controls
        .iter()
        .filter(|c| looks.contains(&c.action))
        .map(|c| c.at.y)
        .collect();
    assert_eq!(tops[0], tops[1], "the looks keep to one line");
    assert!(layout.height > layout.controls.last().unwrap().at.y);
    // A point in a chip is that chip.
    let slim = &layout.controls[0];
    assert!(slim.at.contains(slim.at.x + 1.0, slim.at.y + 1.0));
}

#[test]
fn the_background_row_ends_with_the_boxs_color_and_an_eyedropper() {
    // The swatch shows the box's color, the default until one is chosen,
    // and the eyedropper stands beside it, both at the end of the
    // Background line with the segments before them; a custom box dims the
    // segments and keeps the two, as it takes the color too.
    let now = Instant::now();
    let choice = choice(0);
    let background = |choice: &Choice| {
        rows(choice, true)
            .into_iter()
            .find(|r| r.title == "Background")
            .unwrap()
    };
    assert_eq!(background(&choice).color, Some(style::BOX_COLOR));
    assert!(settings_link::apply(&choice, "color\t#80336699"));
    assert_eq!(background(&choice).color, Some(0x8033_6699));
    let layout = laid(&shown(&choice, ReadNow::Ready, now), now);
    let swatch = layout
        .controls
        .iter()
        .find(|c| matches!(c.kind, Kind::Swatch { .. }))
        .unwrap();
    let dropper = layout
        .controls
        .iter()
        .find(|c| c.kind == Kind::Dropper)
        .unwrap();
    assert_eq!(swatch.kind, Kind::Swatch { color: 0x8033_6699 });
    assert_eq!(
        (&swatch.action, &dropper.action),
        (&Action::PickColor, &Action::Eyedrop)
    );
    assert!(swatch.enabled && dropper.enabled);
    assert_eq!(swatch.at.x + swatch.at.w + CHIP_GAP, dropper.at.x);
    assert_eq!(dropper.at.x + dropper.at.w, RIGHT - CARD_INSET);
    let cards = layout
        .controls
        .iter()
        .find(|c| c.action == Action::Apply("background\tcards".into()))
        .unwrap();
    assert!(cards.at.x + cards.at.w < swatch.at.x);
    assert_eq!(cards.at.y + 1.0, swatch.at.y);
    choice.choose(Some("Columns sample".into()));
    let layout = laid(&shown(&choice, ReadNow::Ready, now), now);
    assert!(
        layout
            .controls
            .iter()
            .any(|c| matches!(c.kind, Kind::Swatch { .. }) && c.enabled)
    );
    assert!(
        !layout
            .controls
            .iter()
            .find(|c| c.action == Action::Apply("background\tcards".into()))
            .unwrap()
            .enabled
    );
}

#[test]
fn restart_explorer_shows_only_while_the_box_waits() {
    // Under the warning while the attach waits for the Snap layouts
    // thread, greyed while a restart is under way, and gone otherwise.
    let now = Instant::now();
    let restart = |status: Status| {
        let view = view_at(&choice(0), status, at(ReadNow::Ready, now), true, UNIX);
        laid(&view, now)
            .controls
            .into_iter()
            .find(|c| c.action == Action::RestartExplorer)
            .map(|c| (c.text, c.enabled))
    };
    let waiting = Status {
        snap_wait: true,
        ..Status::default()
    };
    assert_eq!(restart(waiting), Some(("Restart Explorer".into(), true)));
    let restarting = Status {
        restarting: true,
        ..waiting
    };
    assert_eq!(
        restart(restarting),
        Some(("Restarting Explorer…".into(), false))
    );
    assert_eq!(restart(Status::default()), None);
    // It comes first, and the rest still fit below it.
    let view = view_at(&choice(0), waiting, at(ReadNow::Ready, now), true, UNIX);
    let layout = laid(&view, now);
    assert_eq!(layout.controls[0].action, Action::RestartExplorer);
    assert!(layout.controls[1].at.y > layout.controls[0].at.y + CHIP_H);
}

#[test]
fn why_the_box_is_off_shows_with_a_restart_when_that_helps() {
    // A reason stands where the Snap layouts warning does, which comes
    // first; Restart Explorer comes with the reasons a new Explorer
    // clears.
    use crate::xaml::Reason;
    let now = Instant::now();
    let shown = |reason: Reason| {
        let status = Status {
            reason: Some(reason),
            ..Status::default()
        };
        let view = view_at(&choice(0), status, at(ReadNow::Ready, now), true, UNIX);
        let layout = laid(&view, now);
        let restart = layout
            .controls
            .iter()
            .any(|c| c.action == Action::RestartExplorer);
        (view.warning, restart, layout)
    };
    let (warning, restart, layout) = shown(Reason::new("Could not attach to Explorer.", true));
    assert_eq!(warning, Some("Could not attach to Explorer."));
    assert!(restart);
    assert_eq!(layout.controls[0].action, Action::RestartExplorer);
    let admin = "Run without administrator rights to show the box.";
    let (warning, restart, layout) = shown(Reason::new(admin, false));
    assert_eq!((warning, restart), (Some(admin), false));
    assert!(layout.parts.iter().any(|p| matches!(
        p,
        Part::Text { text, ink: Ink::Warning, .. } if text == admin
    )));
    let both = Status {
        snap_wait: true,
        reason: Some(Reason::new(admin, false)),
        ..Status::default()
    };
    let view = view_at(&choice(0), both, at(ReadNow::Ready, now), true, UNIX);
    assert_eq!(view.warning, Some("Restart Explorer to show the box."));
    assert!(view.offer_restart);
    assert!(!shown(Reason::new(admin, false)).2.parts.is_empty());
}

#[test]
fn a_failed_read_says_why_and_a_restart_what_it_came_to() {
    // Under a service's name, what went wrong with its last read, when the
    // next is, and what the person can do; under the status, what the last
    // restart came to; under Restart Explorer, that it closes File Explorer
    // windows. The tool's own words are not shown.
    let now = Instant::now();
    let picked = choice(0);
    let mut read = picked.read();
    read[1].trouble = Some(crate::usage::Trouble {
        why: "no answer in time".into(),
        retry_at: UNIX + 4 * 60,
    });
    read[0].trouble = Some(crate::usage::Trouble {
        why: "Codex refused: codex account authentication required to read rate limits".into(),
        retry_at: UNIX,
    });
    picked.offer(&read);
    let view = shown(&picked, ReadNow::Ready, now);
    let slow = view.services[1].trouble.clone().unwrap();
    assert!(
        slow.what.ends_with(" did not answer in time"),
        "{}",
        slow.what
    );
    assert_eq!((slow.todo, slow.when.as_str()), (None, "Next try in 4m"));
    let signed_out = view.services[0].trouble.clone().unwrap();
    assert!(
        signed_out.what.starts_with("Not signed in to "),
        "{}",
        signed_out.what
    );
    assert!(signed_out.todo.is_some());
    assert_eq!(signed_out.when, "Trying again");
    let why = slow.what;
    let todo = signed_out.todo.unwrap();
    let texts = |layout: &Layout| {
        layout
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let all = texts(&laid(&view, now));
    for text in [&why, &todo, "Next try in 4m", "Trying again"] {
        assert!(all.contains(&text.to_string()), "{text}");
    }
    assert!(!all.iter().any(|t| t.contains("authentication required")));
    let status = Status {
        notice: Some("Explorer restarted."),
        snap_wait: true,
        ..Status::default()
    };
    let layout = laid(
        &view_at(&picked, status, at(ReadNow::Ready, now), true, UNIX),
        now,
    );
    let shown = texts(&layout);
    assert!(shown.contains(&"Explorer restarted.".to_string()));
    assert!(shown.contains(&"Closes open File Explorer windows.".to_string()));
    for (index, control) in layout.controls.iter().enumerate() {
        for other in &layout.controls[index + 1..] {
            assert!(!control.at.overlaps(&other.at), "{}", control.text);
        }
    }
    // The same moment twice is the same view, so nothing is laid out
    // again each second.
    let again = view_at(&picked, status, at(ReadNow::Ready, now), true, UNIX);
    assert_eq!(
        view_at(&picked, status, at(ReadNow::Ready, now), true, UNIX),
        again
    );
}

#[test]
fn time_until_a_limit_starts_over() {
    for (left, text) in [
        (-5, ""),
        (0, ""),
        (30, "<1m"),
        (60, "1m"),
        (45 * 60 + 59, "45m"),
        (2 * 3600, "2h"),
        (2 * 3600 + 14 * 60, "2h 14m"),
        (86_400, "1d"),
        (3 * 86_400 + 4 * 3600 + 59 * 60, "3d 4h"),
    ] {
        assert_eq!(until(UNIX + left, UNIX), text, "{left}");
    }
}

#[test]
fn service_colors_stay_legible_in_light_mode() {
    // GPT's white and the warning red darken on a light popup until they
    // stand out 3:1; on a dark one they stay as the box has them.
    let light = Theme {
        light: true,
        accent: 0,
    }
    .palette()
    .background;
    let dark = Theme {
        light: false,
        accent: 0,
    }
    .palette()
    .background;
    for color in [0xFFF2_F2F2, 0xFFF4_A27A, 0xFF7A_A7FF, usage::WARNING] {
        let drawn = legible(color, light);
        assert!(contrast(drawn, light) >= 3.0, "{color:#x}");
        assert_eq!(drawn >> 24, color >> 24);
        assert_eq!(legible(color, dark), color, "{color:#x}");
    }
    assert_ne!(legible(0xFFF2_F2F2, light), 0xFFF2_F2F2);
    assert_eq!(faded(0xFFFF_FFFF, 0.45), 0x73FF_FFFF);
}

#[test]
fn the_popup_keeps_inside_the_work_area() {
    // Its corner at the point, then inside the work area, whichever
    // edge the taskbar is on.
    let size = SIZE { cx: 510, cy: 800 };
    let work = |left, top, right, bottom| RECT {
        left,
        top,
        right,
        bottom,
    };
    let at = |x, y| POINT { x, y };
    let bottom = place(at(1900, 1060), size, work(0, 0, 1920, 1032), 12);
    assert_eq!((bottom.x, bottom.y), (1390, 220));
    let top = place(at(5, 5), size, work(0, 48, 1920, 1080), 12);
    assert_eq!((top.x, top.y), (12, 60));
    let middle = place(at(1000, 900), size, work(0, 0, 1920, 1032), 12);
    assert_eq!((middle.x, middle.y), (490, 100));
}

#[test]
fn focus_skips_what_cannot_be_pressed_and_goes_round() {
    let now = Instant::now();
    let layout = laid(&shown(&choice(0), ReadNow::Never, now), now);
    let controls = &layout.controls;
    let last = controls.len() - 1;
    assert_eq!(step(controls, None, true), Some(0));
    assert_eq!(step(controls, None, false), Some(last));
    assert_eq!(step(controls, Some(last), true), Some(0));
    // Refresh now is greyed with the mock figures: from Settings, back
    // goes past it.
    let refresh = controls
        .iter()
        .position(|c| c.action == Action::Refresh)
        .unwrap();
    let before = controls[..refresh].iter().rposition(|c| c.enabled);
    assert!(before.is_some_and(|b| b < refresh));
    assert_eq!(step(controls, Some(refresh + 1), false), before);
    // So are the limits that cannot be turned off.
    let greyed = controls
        .iter()
        .position(|c| !c.enabled && matches!(c.action, Action::Apply(_)))
        .unwrap();
    let before = controls[..greyed].iter().rposition(|c| c.enabled);
    assert_eq!(step(controls, Some(greyed + 1), false), before);
    assert_eq!(step(&[], None, true), None);
}

#[test]
fn a_box_takes_a_name_of_its_own() {
    // Typed on its tab: its ends trimmed, no control characters, 32 at
    // most; an empty one gives back the name from what it shows; two the
    // same are numbered; kept in the settings file; a new box does not take
    // the first's.
    let choice = choice(0);
    let names =
        |choice: &Choice| -> Vec<String> { tabs(choice).into_iter().map(|t| t.text).collect() };
    assert!(settings_link::apply(&choice, "name\t0\t  Work\u{7}  "));
    assert_eq!(names(&choice), ["Work"]);
    assert!(settings_link::apply(&choice, "add"));
    assert_eq!(choice.tiles()[1].name, None);
    assert!(settings_link::apply(&choice, "name\t1\tWork"));
    assert_eq!(names(&choice), ["Work", "Work 2"]);
    let long = "가".repeat(40);
    assert!(settings_link::apply(&choice, &format!("name\t1\t{long}")));
    assert_eq!(names(&choice)[1].chars().count(), crate::choice::MAX_NAME);
    assert!(settings_link::apply(&choice, "name\t1\t"));
    assert_eq!(names(&choice)[1], "GPT · Claude · Gemini");
    assert!(!settings_link::apply(&choice, "name\t2\tNone there"));
    // Kept in its box's block, and read back.
    let text = crate::settings::text(&choice);
    assert_eq!(text.matches("name=").count(), 1);
    assert!(text.contains("\nname=Work\n"));
    let saved = crate::settings::parse(&text);
    let saved: Vec<_> = saved.tiles.iter().map(|t| t.name.clone()).collect();
    assert_eq!(saved, [Some("Work".to_string()), None]);
}

#[test]
fn the_boxes_settings_open_folded_to_their_tabs_with_the_values_still_shown() {
    // Folded, as the popup opens: the tabs, each unfolding on its box, and
    // Edit at the end; no card, close or new tab, but Show values as under
    // them. Unfolded: Done there, the card, and Show values as under it, its
    // segments as wide as the popup either way.
    let now = Instant::now();
    let picked = choice(0);
    assert!(settings_link::apply(&picked, "add"));
    let tray = view_at(
        &picked,
        Status::default(),
        at(ReadNow::Ready, now),
        true,
        UNIX,
    );
    let folded = laid(&tray, now);
    let open = laid(&shown(&picked, ReadNow::Ready, now), now);
    let texts = |layout: &Layout| {
        layout
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let tabs: Vec<_> = folded
        .controls
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                Kind::Tab { .. } | Kind::Glyph { .. } | Kind::Disclose { .. }
            )
        })
        .map(|c| (c.text.as_str(), c.kind.clone(), c.action.clone()))
        .collect();
    let tab = |on| Kind::Tab { on, hidden: false };
    assert_eq!(
        tabs,
        [
            (
                tabs[0].0,
                tab(false),
                Action::Unfold(Some("select\t0".into()))
            ),
            (
                tabs[1].0,
                tab(false),
                Action::Unfold(Some("select\t1".into()))
            ),
            (
                "Edit",
                Kind::Disclose { glyph: UNFOLD.1 },
                Action::Unfold(None)
            ),
        ]
    );
    assert!(
        !folded
            .parts
            .iter()
            .any(|p| matches!(p, Part::TabCard { .. }))
    );
    assert!(
        !folded
            .controls
            .iter()
            .any(|c| matches!(c.kind, Kind::Chip { .. } | Kind::Toggle { .. }))
    );
    let segments = |layout: &Layout| {
        layout
            .controls
            .iter()
            .filter(|c| matches!(c.kind, Kind::Segment { .. }))
            .map(|c| (c.action.clone(), c.at.x))
            .collect::<Vec<_>>()
    };
    let values = [
        (Action::Apply("fill\tused".into()), PAD),
        (Action::Apply("fill\tleft".into()), segments(&folded)[1].1),
    ];
    assert_eq!(segments(&folded), values);
    assert!(texts(&folded).contains(&"Show values as".to_string()));
    let edit_at = folded
        .controls
        .iter()
        .find(|c| c.text == "Edit")
        .unwrap()
        .at;
    assert!(segments(&folded).iter().all(|(_, x)| *x >= PAD) && edit_at.y < folded.height);
    assert!(
        folded.height + 200.0 < open.height,
        "{} {}",
        folded.height,
        open.height
    );
    let edit = folded.controls.iter().find(|c| c.text == "Edit").unwrap();
    assert_eq!(edit.at.x + edit.at.w, RIGHT);
    // Unfolded, Done where Edit was, after the new tab.
    let done = open.controls.iter().find(|c| c.text == "Done").unwrap();
    assert_eq!(
        (&done.kind, &done.action),
        (&Kind::Disclose { glyph: FOLD.1 }, &Action::Fold)
    );
    assert_eq!(done.at.x + done.at.w, RIGHT);
    let add = open.controls.iter().find(|c| c.text == "Add box").unwrap();
    assert!(add.at.x + add.at.w <= done.at.x);
    let shown_texts = texts(&open);
    assert!(shown_texts.contains(&"Show values as".to_string()));
    assert!(!shown_texts.contains(&"All boxes".to_string()));
    // The values' segments run from the popup's edge, not after a title.
    let used = open
        .controls
        .iter()
        .find(|c| c.action == Action::Apply("fill\tused".into()))
        .unwrap();
    assert_eq!(used.at.x, PAD);
    // Nothing folded or unfolded overlaps.
    for layout in [&folded, &open] {
        for (index, control) in layout.controls.iter().enumerate() {
            for other in &layout.controls[index + 1..] {
                assert!(
                    !control.at.overlaps(&other.at),
                    "{} {}",
                    control.text,
                    other.text
                );
            }
        }
    }
}

#[test]
fn focus_follows_what_the_focused_control_did_as_the_popup_unfolds() {
    // Unfolded, the tabs gain their close and new buttons and the numbers
    // shift: Edit's focus goes to Done, a folded tab's to the same tab, and
    // Show values as to itself; back again on folding, and let go where
    // nothing does the same.
    let now = Instant::now();
    let picked = choice(0);
    assert!(settings_link::apply(&picked, "add"));
    let folded = laid(
        &view_at(
            &picked,
            Status::default(),
            at(ReadNow::Ready, now),
            true,
            UNIX,
        ),
        now,
    );
    let open = laid(&shown(&picked, ReadNow::Ready, now), now);
    let (folded, open) = (&folded.controls, &open.controls);
    let edit = find_in(folded, &Action::Unfold(None));
    let done = find_in(open, &Action::Fold);
    assert_ne!(edit, done, "the numbers shift");
    assert_eq!(follow(folded, Some(edit), open), Some(done));
    assert_eq!(follow(open, Some(done), folded), Some(edit));
    let tab = Action::Unfold(Some("select\t0".into()));
    let unfolded_tab = Action::Apply("select\t0".into());
    assert_eq!(
        follow(folded, Some(find_in(folded, &tab)), open),
        Some(find_in(open, &unfolded_tab))
    );
    assert_eq!(
        follow(open, Some(find_in(open, &unfolded_tab)), folded),
        Some(find_in(folded, &tab))
    );
    let left = Action::Apply("fill\tleft".into());
    assert_eq!(
        follow(folded, Some(find_in(folded, &left)), open),
        Some(find_in(open, &left))
    );
    // Add box is not there folded; nothing focused stays so.
    let add = find_in(open, &Action::Apply("add".into()));
    assert_eq!(follow(open, Some(add), folded), None);
    assert_eq!(follow(folded, None, open), None);
}

fn find_in(controls: &[Control], action: &Action) -> usize {
    controls.iter().position(|c| &c.action == action).unwrap()
}

#[test]
fn the_summary_warns_as_the_boxes_do_unless_turned_off() {
    // Claude's week has 10% left: orange in the popup too, and nothing with
    // the warning colors off.
    let now = Instant::now();
    let picked = choice(0);
    let week = |choice: &Choice| {
        shown(choice, ReadNow::Ready, now).services[1]
            .lines
            .iter()
            .flatten()
            .find(|l| l.label == "7d")
            .unwrap()
            .warn
    };
    assert_eq!(week(&picked), usage::Warn::Caution);
    picked.set_warn(false);
    assert_eq!(week(&picked), usage::Warn::None);
}
