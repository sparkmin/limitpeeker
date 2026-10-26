//! What was picked, kept between runs in a small text file: the fill mode,
//! the popup summary's picks, dragging, the tools' updates, the warning
//! colors, the notifications and the settings window's language, then each
//! box on the taskbar in
//! a `[box]` block of its look, background, color, services and limits
//! picked, and place. Only the controller's tray thread writes it, whether the popup or
//! the settings window changed it. `key=value` lines, values split by tabs,
//! read and written without a new crate.

use crate::choice::{BoxPlace, Choice, Tile};
use crate::style::{self, Background, Fill, Look};
use crate::usage::Picks;
use std::path::PathBuf;

/// One box as a settings file held it.
#[derive(Debug, Default)]
pub struct SavedTile {
    /// The custom box chosen, by name.
    pub custom: Option<String>,
    pub look: Option<&'static Look>,
    pub background: Option<Background>,
    pub picks: Picks,
    pub place: Option<BoxPlace>,
    /// Its own name.
    pub name: Option<String>,
    /// Kept but not on the taskbar.
    pub hidden: bool,
    /// Its own color, when one was chosen.
    pub color: Option<u32>,
}

/// What a settings file held. Anything missing or not known is left out.
#[derive(Debug, Default)]
pub struct Saved {
    pub fill: Option<Fill>,
    /// The popup summary's own picks.
    pub summary: Picks,
    /// Whether a box may be dragged, when written.
    pub drag: Option<bool>,
    /// Whether the tools are updated, when written.
    pub updates: Option<bool>,
    /// Whether low values warn in color, when written.
    pub warn: Option<bool>,
    /// The notifications on, and the line of almost used up, when written.
    pub notify: Option<Vec<String>>,
    pub notify_at: Option<u8>,
    /// The settings window's language, when one was picked.
    pub language: Option<String>,
    /// The editor's shelves folded, when not as at first.
    pub folded: Option<Vec<String>>,
    /// The boxes, in their order.
    pub tiles: Vec<SavedTile>,
}

/// This program's folder under LOCALAPPDATA: the settings, and the hidden
/// Claude Code's folder and files.
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("LimitPeeker"))
}

pub fn path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("settings.txt"))
}

/// A value that a line of tab-split words can hold.
fn clean(word: &str) -> bool {
    !word.is_empty() && !word.contains(['\t', '\n', '\r'])
}

/// The settings as written. A pick not made is left out, so its default
/// holds.
pub fn text(choice: &Choice) -> String {
    let words = |words: &[String]| {
        words
            .iter()
            .filter(|w| clean(w))
            .cloned()
            .collect::<Vec<_>>()
            .join("\t")
    };
    let mut lines = vec![
        "# LimitPeeker settings, written by LimitPeeker.".to_string(),
        format!("fill={}", choice.fill().name()),
    ];
    // The popup's summary keeps an order and services of its own.
    let summary = choice.summary_picks();
    if let Some(ids) = summary.picked_summary() {
        lines.push(format!("summary={}", words(ids)));
    }
    for (id, labels) in summary.picked_summary_limits() {
        if clean(id) {
            lines.push(format!("summary-limits={id}\t{}", words(labels)));
        }
    }
    if let Some(ids) = summary.picked_summary_services() {
        lines.push(format!("summary-services={}", words(ids)));
    }
    if !choice.drag() {
        lines.push("drag=off".into());
    }
    // The tools' updates, only when turned on: off is how it starts.
    if choice.updates() {
        lines.push("updates=on".into());
    }
    // The warning colors, only when turned off.
    if !choice.warn() {
        lines.push("warn=off".into());
    }
    // The notifications, only when not as they start.
    let alerts = choice.alerts();
    let start = crate::notify::Alerts::default();
    if alerts.kinds() != start.kinds() {
        let on: Vec<&str> = alerts
            .kinds()
            .iter()
            .filter(|(_, on)| *on)
            .map(|(kind, _)| *kind)
            .collect();
        lines.push(format!("notify={}", on.join("	")));
    }
    if alerts.at != start.at {
        lines.push(format!("notify-at={}", alerts.at));
    }
    // The settings window's language, only when one was picked.
    let language = choice.language();
    if !language.is_empty() {
        lines.push(format!("language={language}"));
    }
    // The editor's shelves folded, only when not as at first.
    let folded = choice.folded();
    if folded != crate::choice::FOLDED {
        lines.push(format!("folded={}", folded.join("\t")));
    }
    // Each box in its order.
    for tile in choice.tiles() {
        lines.push("[box]".into());
        lines.push(format!("look={}", tile.look().name));
        lines.push(format!("background={}", tile.background.name()));
        // Its own color, only when one was chosen.
        if let Some(color) = tile.color {
            lines.push(format!("color={}", style::argb_text(color)));
        }
        // Kept while its file is away, so it comes back with it.
        if let Some(name) = tile.custom.as_deref().filter(|name| clean(name)) {
            lines.push(format!("box={name}"));
        }
        if let Some(ids) = tile.picks.picked_services() {
            lines.push(format!("services={}", words(ids)));
        }
        for (id, labels) in tile.picks.picked_limits() {
            if clean(id) {
                lines.push(format!("limits={id}\t{}", words(labels)));
            }
        }
        if tile.place != BoxPlace::default() {
            lines.push(format!("place={}", tile.place.name()));
        }
        if let Some(name) = tile.name.as_deref().filter(|name| clean(name)) {
            lines.push(format!("name={name}"));
        }
        if tile.hidden {
            lines.push("hidden=1".into());
        }
    }
    lines.join("\n") + "\n"
}

pub fn parse(text: &str) -> Saved {
    let mut saved = Saved::default();
    // A box's lines before any [box] belong to no box and are left out.
    let mut none = SavedTile::default();
    for line in text.lines() {
        if line == "[box]" {
            saved.tiles.push(SavedTile::default());
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let mut words = value.split('\t').filter(|w| !w.is_empty());
        let tile = saved.tiles.last_mut().unwrap_or(&mut none);
        match key {
            "look" => tile.look = style::find(value),
            "background" => tile.background = Background::find(value),
            "color" => tile.color = style::parse_argb(value),
            "box" if !value.is_empty() => tile.custom = Some(value.to_string()),
            "services" => {
                let ids: Vec<String> = words.map(String::from).collect();
                if !ids.is_empty() {
                    tile.picks.set_services(ids);
                }
            }
            "limits" => {
                if let Some(id) = words.next() {
                    let labels: Vec<String> = words.map(String::from).collect();
                    if !labels.is_empty() {
                        tile.picks.set_limits(id, labels);
                    }
                }
            }
            "place" => tile.place = BoxPlace::find(value),
            "name" if !value.trim().is_empty() => tile.name = Some(value.trim().to_string()),
            "hidden" => tile.hidden = value.trim() == "1",
            "fill" => saved.fill = Fill::find(value),
            "summary" => {
                let ids: Vec<String> = words.map(String::from).collect();
                if !ids.is_empty() {
                    saved.summary.set_summary(ids);
                }
            }
            "summary-limits" => {
                if let Some(id) = words.next() {
                    let labels: Vec<String> = words.map(String::from).collect();
                    if !labels.is_empty() {
                        saved.summary.set_summary_limits(id, labels);
                    }
                }
            }
            "summary-services" => {
                let ids: Vec<String> = words.map(String::from).collect();
                if !ids.is_empty() {
                    saved.summary.set_summary_services(ids);
                }
            }
            "drag" if value == "off" => saved.drag = Some(false),
            "drag" if value == "on" => saved.drag = Some(true),
            "updates" if value == "off" => saved.updates = Some(false),
            "updates" if value == "on" => saved.updates = Some(true),
            "warn" if value == "off" => saved.warn = Some(false),
            "warn" if value == "on" => saved.warn = Some(true),
            "notify" => saved.notify = Some(words.map(String::from).collect()),
            "notify-at" => saved.notify_at = value.parse().ok(),
            "language" if crate::settings_link::LANGUAGES.contains(&value) => {
                saved.language = Some(value.to_string());
            }
            "folded" => {
                saved.folded = Some(
                    words
                        .filter(|word| crate::boxes::Shelf::find(word).is_some())
                        .map(String::from)
                        .collect(),
                );
            }
            _ => {}
        }
    }
    saved
}

/// The boxes saved, each value missing at its default; the first takes the
/// command line's look, which comes before its custom box, and background.
fn tiles_of(
    saved: Vec<SavedTile>,
    look: Option<&'static Look>,
    background: Option<Background>,
) -> Vec<Tile> {
    let mut tiles: Vec<Tile> = saved
        .into_iter()
        .map(|saved| {
            let mut tile = Tile::new(
                saved.look.unwrap_or(style::DEFAULT),
                saved.background.unwrap_or(Background::Box),
            );
            tile.custom = saved.custom;
            tile.picks = saved.picks;
            tile.place = saved.place.unwrap_or_default();
            tile.name = saved.name;
            tile.hidden = saved.hidden;
            tile.color = saved.color;
            tile
        })
        .collect();
    if tiles.is_empty() {
        tiles.push(Tile::default());
    }
    if let Some(look) = look {
        tiles[0].set_look(look);
        tiles[0].custom = None;
    }
    if let Some(background) = background {
        tiles[0].background = background;
    }
    tiles
}

/// The settings of the last run, or none when there are none or they cannot
/// be read.
pub fn load() -> Saved {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// Writes them through a temporary file, so a crash never leaves half of
/// one.
pub fn save(text: &str) -> std::io::Result<()> {
    let Some(path) = path() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, &path)
}

/// The choice to start with: each value from the command line, or else the
/// one saved, or else the default; the command line's look and background
/// are the first box's.
pub fn starting(
    saved: Saved,
    look: Option<&'static Look>,
    fill: Option<Fill>,
    background: Option<Background>,
) -> Choice {
    let choice = Choice::new(
        style::DEFAULT,
        fill.or(saved.fill).unwrap_or(Fill::Used),
        Background::Box,
    );
    choice.set_tiles(tiles_of(saved.tiles, look, background));
    choice.set_summary_picks(saved.summary);
    choice.set_drag(saved.drag.unwrap_or(true));
    // Off unless the file says on.
    choice.set_updates(saved.updates.unwrap_or(false));
    choice.set_warn(saved.warn.unwrap_or(true));
    let mut alerts = match &saved.notify {
        Some(kinds) => crate::notify::Alerts::only(
            &kinds.iter().map(String::as_str).collect::<Vec<_>>(),
            crate::notify::DEFAULT_AT,
        ),
        None => crate::notify::Alerts::default(),
    };
    if let Some(at) = saved.notify_at {
        alerts.set_at(at);
    }
    choice.set_alerts(alerts);
    choice.set_language(saved.language.as_deref().unwrap_or_default());
    if let Some(folded) = saved.folded {
        choice.set_folded(folded);
    }
    choice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::mock;

    #[test]
    fn the_editors_folded_shelves_are_kept_when_changed() {
        // As at first, the looks' samples folded, nothing is written; any
        // other, none folded too, is written and comes back.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        assert_eq!(choice.folded(), ["ai", "pc"]);
        assert!(!text(&choice).contains("folded="));
        for folded in [vec!["pc"], vec![], vec!["mine", "basic", "ai", "pc"]] {
            choice.set_folded(folded.iter().map(|w| w.to_string()).collect());
            let written = text(&choice);
            assert!(
                written.contains(&format!("\nfolded={}\n", folded.join("\t"))),
                "{written}"
            );
            let back = starting(parse(&written), None, None, None);
            assert_eq!(back.folded(), folded);
        }
        // A shelf not known is left out.
        let back = starting(parse("folded=ai\tshelf\n"), None, None, None);
        assert_eq!(back.folded(), ["ai"]);
    }

    #[test]
    fn settings_come_back_as_they_were_written() {
        // Every pick made comes back; one not made is not written, so its
        // default holds.
        let choice = Choice::new(style::DEFAULT, Fill::Left, Background::Cards);
        assert_eq!(
            text(&choice),
            "# LimitPeeker settings, written by LimitPeeker.\nfill=left\n[box]\nlook=slim\nbackground=cards\n"
        );
        choice.offer(&mock(0));
        let offers = choice.offers();
        choice.toggle_service(&offers, "grok");
        choice.toggle(&offers[1], "Opus 7d");
        choice.toggle(&offers[1], "5h");
        choice.toggle(&offers[1], "Opus 7d");
        choice.set_look(&style::LOOKS[4]);
        // The summary's own order.
        choice.move_summary_to(&offers, "grok", 0);
        choice.move_summary_limit_to(&offers[1], "7d", 0);
        // And its own services, apart from the boxes'.
        choice.toggle_summary_service(&offers, "gemini");
        choice.toggle_summary_service(&offers, "pc");
        // A second box, of PC alone, at the left end, dragging off.
        assert!(choice.add_tile());
        for id in ["gpt", "claude", "gemini", "grok"] {
            choice.toggle_service(&offers, id);
        }
        choice.toggle_service(&offers, "pc");
        choice.toggle_service(&offers, "grok");
        choice.set_box_place(BoxPlace::Left);
        choice.set_drag(false);
        let written = text(&choice);
        assert!(written.contains("summary=grok\tgpt\tclaude\tgemini\n"));
        assert!(written.contains("summary-limits=claude\t7d\t5h\tOpus 7d\n"));
        assert!(
            written
                .contains("summary-services=grok\tgpt\tclaude\tpc\ndrag=off\n[box]\nlook=rings\n")
        );
        assert!(
            written.contains(
                "services=gpt\tclaude\tgemini\tgrok\nlimits=claude\t7d\tOpus 7d\n[box]\n"
            )
        );
        assert!(written.ends_with("services=pc\nlimits=claude\t7d\tOpus 7d\nplace=left\n"));
        let back = starting(parse(&written), None, None, None);
        assert_eq!(text(&back), written);
        assert_eq!(back.tiles(), choice.tiles());
        // Hidden, it keeps its place and stays hidden.
        choice.hide_box();
        let written = text(&choice);
        assert!(written.ends_with("place=left\nhidden=1\n"));
        let back = starting(parse(&written), None, None, None);
        assert_eq!(back.tiles(), choice.tiles());
        assert!(back.tiles()[1].hidden);
    }

    #[test]
    fn a_chosen_box_is_kept_and_a_look_on_the_command_line_comes_first() {
        let choice = Choice::new(&style::LOOKS[0], Fill::Used, Background::Box);
        choice.choose(Some("Free sample".into()));
        let written = text(&choice);
        assert!(written.contains("[box]\nlook=slim\nbackground=box\nbox=Free sample\n"));
        let back = starting(parse(&written), None, None, None);
        assert_eq!(
            back.custom().map(|b| b.name.clone()).as_deref(),
            Some("Free sample")
        );
        let told = starting(parse(&written), Some(style::DEFAULT), None, None);
        assert!(told.custom().is_none());
        assert!(!text(&told).contains("box="));
    }

    #[test]
    fn the_command_line_comes_before_what_was_saved() {
        let saved = || parse("fill=left\n[box]\nlook=pie\nbackground=cards\n");
        let choice = starting(saved(), Some(&style::LOOKS[2]), None, Some(Background::Box));
        assert_eq!(choice.look().name, "stack");
        assert_eq!(
            (choice.fill(), choice.background()),
            (Fill::Left, Background::Box)
        );
        let choice = starting(saved(), None, None, None);
        assert_eq!(choice.look().name, "pie");
        // Nothing saved, or nothing known, starts from the defaults.
        for text in [
            "",
            "look=card\nfill=half\nlimits=\nservices=\nsomething\nplace=top\ndrag=maybe\n",
            // A box's lines before any [box] belong to no box.
            "look=pie\nbackground=cards\nservices=gpt\nplace=left\n",
            "[box]\nlook=cycle\n",
        ] {
            let choice = starting(parse(text), None, None, None);
            assert_eq!(choice.look().name, style::DEFAULT.name);
            assert_eq!(
                (
                    choice.fill(),
                    choice.background(),
                    choice.box_place(),
                    choice.drag()
                ),
                (Fill::Used, Background::Box, BoxPlace::Tray, true)
            );
            assert_eq!(choice.tile().picks, Picks::default());
            assert_eq!(choice.tiles().len(), 1);
        }
    }

    #[test]
    fn notifications_are_written_only_when_not_as_they_start() {
        // All on at 90 is not written; anything else is, and comes back.
        let choice = starting(Saved::default(), None, None, None);
        assert_eq!(choice.alerts(), crate::notify::Alerts::default());
        assert!(!text(&choice).contains("notify"));
        let mut alerts = choice.alerts();
        alerts.set("near", false);
        alerts.set("failing", false);
        alerts.set_at(95);
        choice.set_alerts(alerts);
        let written = text(&choice);
        assert!(
            written.contains(
                "notify=full	back
notify-at=95
"
            ),
            "{written}"
        );
        let back = starting(parse(&written), None, None, None);
        assert_eq!(back.alerts(), alerts);
        let none = starting(
            parse(
                "notify=
notify-at=85
",
            ),
            None,
            None,
            None,
        );
        assert_eq!(none.alerts().kinds().map(|(_, on)| on), [false; 4]);
        assert_eq!(none.alerts().at, crate::notify::DEFAULT_AT);
    }

    #[test]
    fn the_tools_updates_are_written_only_when_turned_on() {
        // Off, as at first, nothing is written; on is, and comes back. A file
        // from before, with updates=off or nothing, stays off.
        let choice = starting(Saved::default(), None, None, None);
        assert!(!choice.updates());
        assert!(!text(&choice).contains("updates"));
        choice.set_updates(true);
        let written = text(&choice);
        assert!(written.contains("\nupdates=on\n"), "{written}");
        assert!(starting(parse(&written), None, None, None).updates());
        for other in ["updates=off\n", "updates=maybe\n", ""] {
            assert!(
                !starting(parse(other), None, None, None).updates(),
                "{other}"
            );
        }
    }

    #[test]
    fn the_warning_colors_are_written_only_when_turned_off() {
        let choice = starting(Saved::default(), None, None, None);
        assert!(choice.warn());
        assert!(!text(&choice).contains("warn"));
        choice.set_warn(false);
        let written = text(&choice);
        assert!(written.contains("\nwarn=off\n"), "{written}");
        assert!(!starting(parse(&written), None, None, None).warn());
        for other in ["warn=on\n", "warn=maybe\n", ""] {
            assert!(starting(parse(other), None, None, None).warn(), "{other}");
        }
    }

    #[test]
    fn a_boxs_color_is_written_only_when_chosen_and_comes_back() {
        // No line until a color is chosen, then #AARRGGBB in its box's
        // block, in either case when read; a value that is not that is
        // left out, and so is a line before any [box].
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        assert_eq!(choice.color(), None);
        assert!(!text(&choice).contains("color="));
        choice.set_color(Some(0x80FF_A001));
        assert!(choice.add_tile());
        choice.set_color(None);
        let written = text(&choice);
        assert!(
            written.contains(
                "[box]\nlook=slim\nbackground=box\ncolor=#80FFA001\n[box]\nlook=slim\nbackground=box\n"
            ),
            "{written}"
        );
        let back = starting(parse(&written), None, None, None);
        assert_eq!(back.tiles()[0].color, Some(0x80FF_A001));
        assert_eq!(back.tiles()[1].color, None);
        assert_eq!(back.tiles(), choice.tiles());
        let lower = starting(parse("[box]\ncolor=#ff1f2227\n"), None, None, None);
        assert_eq!(lower.color(), Some(0xFF1F_2227));
        for wrong in [
            "[box]\ncolor=#FFF\n",
            "[box]\ncolor=FF1F2227\n",
            "[box]\ncolor=#1F2227\n",
            "[box]\ncolor=#GG1F2227\n",
            "[box]\ncolor=\n",
            "color=#80FFA001\n[box]\n",
        ] {
            assert_eq!(
                starting(parse(wrong), None, None, None).color(),
                None,
                "{wrong}"
            );
        }
    }

    #[test]
    fn the_settings_windows_language_comes_back() {
        // One the window has words for; another, or none, is Windows'.
        let choice = starting(parse("language=ko\n"), None, None, None);
        assert_eq!(choice.language(), "ko");
        assert!(text(&choice).contains("\nlanguage=ko\n"));
        for other in ["language=de\n", "language=\n", ""] {
            let choice = starting(parse(other), None, None, None);
            assert_eq!(choice.language(), "", "{other}");
            assert!(!text(&choice).contains("language="));
        }
    }
}
