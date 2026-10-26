//! The settings window's link to the controller. The window sends one
//! command a message; the controller applies it by the popup's rules and
//! sends the window the whole state whenever it changes. Both are lines of
//! tab-split words, as UTF-16 in WM_COPYDATA. Must match settings/Link.cs.

use crate::choice::{BoxPlace, Choice};
use crate::logon_task::LogonTask;
use crate::startup::Startup;
use crate::style::{self, Background, Fill};
use crate::usage::{MAX_CELLS, SHOWN, most_limits};

/// What a WM_COPYDATA carries: a command from the window, the state for it,
/// or a request that it come to the front; and for the editor, a box to
/// open, a draft's preview, and what a save or delete came to.
pub const COMMAND: usize = 0x4C50_0001;
pub const STATE: usize = 0x4C50_0002;
pub const SHOW: usize = 0x4C50_0003;
pub const BOX: usize = 0x4C50_0004;
pub const PREVIEW: usize = 0x4C50_0005;
pub const RESULT: usize = 0x4C50_0006;
/// The longest message taken, in UTF-16 units: a box's file is up to 64 KB.
pub const LONGEST: usize = 131_072;
/// The window's first command, which makes it the one the state goes to.
pub const HELLO: &str = "hello";
/// The window's Refresh now button: every service read at once, as from the
/// popup.
pub const REFRESH: &str = "refresh";
/// The window come to the front again: the Task Scheduler tasks are looked
/// for anew, as one may have been made meanwhile.
pub const STARTUP_CHECK: &str = "startup-check";
/// The window's Send a test notification button.
pub const NOTIFY_TEST: &str = "notify-test";
/// The public repository, as owner/name, whose newest release the settings
/// window asks for when its Check for updates is pressed; empty while there
/// is none, and the button with it.
pub const REPOSITORY: &str = "sparkmin/limitpeeker";
/// The languages the settings window has words for, as its Loc lists them;
/// none picked is Windows' own.
pub const LANGUAGES: [&str; 6] = ["en", "es", "fr", "ja", "ko", "pt"];

/// The whole state for the window: the look and the looks to pick from, the
/// fill mode, the background, how many services and limits may show, when
/// Refresh now is taken again, each custom box's style and whether it is a
/// sample, and each service offered, whether it shows,
/// and each of its limits, whether that shows. The services and limits
/// shown come first, in the box's order. `read_now_at` is in Unix
/// milliseconds, 0 before the first request and None with the mock figures,
/// which never take one. Last, whether it starts at sign-in, and each Task
/// Scheduler task found that starts it.
pub fn state(
    choice: &Choice,
    read_now_at: Option<u64>,
    startup: Startup,
    tasks: &[LogonTask],
) -> String {
    // A custom box is a look named "box:" and its name.
    let look = match choice.custom() {
        Some(custom) => format!("box:{}", custom.name),
        None => choice.look().name.to_string(),
    };
    let mut names: Vec<String> = style::LOOKS.iter().map(|l| l.name.to_string()).collect();
    // The person's own custom boxes, and a sample only while in use.
    names.extend(
        choice
            .pickable_boxes()
            .iter()
            .map(|b| format!("box:{}", b.name)),
    );
    let mut lines = vec![
        format!("look\t{look}"),
        format!("looks\t{}", names.join("\t")),
        format!("fill\t{}", choice.fill().name()),
        format!("background\t{}", choice.background().name()),
        // The box's own color, or default for the one every look starts
        // with.
        format!(
            "color\t{}",
            choice
                .color()
                .map_or("default".to_string(), style::argb_text)
        ),
        // Then each service that shows more limits: PC.
        format!(
            "most\t{MAX_CELLS}\t{SHOWN}\t{}\t{}",
            crate::resources::ID,
            most_limits(crate::resources::ID)
        ),
        format!(
            "refresh\t{}",
            read_now_at.map_or("never".to_string(), |at| at.to_string())
        ),
    ];
    // Each box's style and whether it is a sample, for the editor's
    // presets.
    for custom in choice.boxes().iter() {
        lines.push(format!(
            "box\t{}\t{}\t{}\t{}",
            custom.name,
            custom.style(),
            u8::from(custom.file.is_none()),
            custom.shelf.word()
        ));
    }
    let offers = choice.offers();
    let services = choice.services(&offers);
    for offer in choice.listed(&offers) {
        let on = services.contains(&offer.id);
        lines.push(format!(
            "service\t{}\t{}\t{}",
            offer.id,
            offer.name,
            u8::from(on)
        ));
        let shown = choice.shown(offer);
        for label in &choice.listed_limits(offer) {
            lines.push(format!(
                "limit\t{label}\t{}",
                u8::from(shown.contains(label))
            ));
        }
    }
    // The popup summary's services, in its order, apart from the box's.
    let mut summary = vec!["summary"];
    summary.extend(choice.summary(&offers));
    lines.push(summary.join("\t"));
    // The boxes by what they show, the one chosen, where it goes and
    // whether it can move left or right among the boxes there; and whether
    // a box may be dragged.
    let mut tiles = vec!["boxes".to_string(), choice.current().to_string()];
    tiles.extend(choice.tile_names(&offers));
    lines.push(tiles.join("\t"));
    // Which boxes are hidden, in the same order; the chosen one's place is
    // "hidden" while it is.
    let mut hidden = vec!["hidden".to_string()];
    hidden.extend(
        choice
            .tiles()
            .iter()
            .map(|t| u8::from(t.hidden).to_string()),
    );
    lines.push(hidden.join("\t"));
    let place = if choice.box_hidden() {
        crate::choice::HIDDEN
    } else {
        choice.box_place().name()
    };
    lines.push(format!("place\t{place}"));
    let (earlier, later) = choice.can_move_tile();
    lines.push(format!("order\t{}\t{}", u8::from(earlier), u8::from(later)));
    lines.push(format!(
        "drag\t{}",
        if choice.drag() { "on" } else { "off" }
    ));
    // Whether each tool's own update command runs.
    lines.push(format!(
        "updates\t{}",
        if choice.updates() { "on" } else { "off" }
    ));
    // Whether low values warn in color.
    lines.push(format!(
        "warn\t{}",
        if choice.warn() { "on" } else { "off" }
    ));
    // Each notification on or off, in KINDS's order, and the line of almost
    // used up.
    let alerts = choice.alerts();
    let mut notify = vec!["notify".to_string()];
    notify.extend(alerts.kinds().map(|(_, on)| u8::from(on).to_string()));
    notify.push(alerts.at.to_string());
    lines.push(notify.join("\t"));
    // The window's own language, empty for Windows'.
    lines.push(format!("language\t{}", choice.language()));
    // The editor's shelves folded.
    let mut folded = vec!["folded".to_string()];
    folded.extend(choice.folded());
    lines.push(folded.join("\t"));
    lines.push(format!("startup\t{}", startup.word()));
    for task in tasks {
        lines.push(format!(
            "task\t{}\t{}\t{}\t{}\t{}",
            task.path,
            u8::from(task.enabled),
            u8::from(task.this_exe),
            task.problem.word(),
            task.program
        ));
    }
    // This version, the one there is, and where a newer one would be.
    lines.push(format!("version\t{}", env!("CARGO_PKG_VERSION")));
    if !REPOSITORY.is_empty() {
        lines.push(format!("repository\t{REPOSITORY}"));
    }
    lines.join("\n")
}

/// The window's start at sign-in turned on or off: kept in the registry,
/// not in the choice.
pub fn startup(command: &str) -> Option<bool> {
    match command {
        "startup\ton" => Some(true),
        "startup\toff" => Some(false),
        _ => None,
    }
}

/// Applies one command from the window by the popup's rules. False for a
/// command not known, or for a service or limit not offered.
pub fn apply(choice: &Choice, command: &str) -> bool {
    let words: Vec<&str> = command.split('\t').collect();
    let offers = choice.offers();
    let offer = |id: &str| offers.iter().find(|o| o.id == id);
    match words.as_slice() {
        ["look", name] if name.starts_with("box:") => {
            let name = &name["box:".len()..];
            if !choice.boxes().iter().any(|b| b.name == name) {
                return false;
            }
            choice.choose(Some(name.to_string()));
        }
        ["look", name] => match style::find(name) {
            Some(look) => {
                choice.set_look(look);
                choice.choose(None);
            }
            None => return false,
        },
        ["fill", name] => match Fill::find(name) {
            Some(fill) => choice.set_fill(fill),
            None => return false,
        },
        ["background", name] => match Background::find(name) {
            Some(background) => choice.set_background(background),
            None => return false,
        },
        // The box's own color as #AARRGGBB, or default for none.
        ["color", "default"] => choice.set_color(None),
        ["color", color] => match style::parse_argb(color) {
            Some(color) => choice.set_color(Some(color)),
            None => return false,
        },
        ["service", id] => match offer(id) {
            Some(service) => choice.toggle_service(&offers, service.id),
            None => return false,
        },
        // On or off in the popup's summary, apart from the box.
        ["summary-service", id] => match offer(id) {
            Some(service) => choice.toggle_summary_service(&offers, service.id),
            None => return false,
        },
        ["move", id, way @ ("up" | "down")] => match offer(id) {
            Some(service) => choice.move_service(&offers, service.id, *way == "up"),
            None => return false,
        },
        ["limit", id, label] => match offer(id) {
            Some(service) if service.labels.iter().any(|l| l == label) => {
                choice.toggle(service, label)
            }
            _ => return false,
        },
        // Dragged in the popup: to a place counted from 0.
        ["move", id, to] => match (offer(id), place(to)) {
            (Some(service), Some(to)) => choice.move_service_to(&offers, service.id, to),
            _ => return false,
        },
        ["move-limit", id, label, to] => match (offer(id), place(to)) {
            (Some(service), Some(to)) if service.labels.iter().any(|l| l == label) => {
                choice.move_limit_to(service, label, to)
            }
            _ => return false,
        },
        ["move-summary", id, to] => match (offer(id), place(to)) {
            (Some(service), Some(to)) => choice.move_summary_to(&offers, service.id, to),
            _ => return false,
        },
        ["move-summary-limit", id, label, to] => match (offer(id), place(to)) {
            (Some(service), Some(to)) if service.labels.iter().any(|l| l == label) => {
                choice.move_summary_limit_to(service, label, to)
            }
            _ => return false,
        },
        // A box chosen, added or taken away, moved, moved left or right
        // among the boxes in its place, and dragging on or off.
        ["select", index] => match place(index) {
            Some(index) if choice.select(index) => {}
            _ => return false,
        },
        // A box's own name, by its number, as a tab not yet chosen may be
        // the one renamed; an empty one gives back its name from what it
        // shows.
        ["name", index, name @ ..] => match place(index) {
            Some(index) if choice.rename_tile(index, &name.join(" ")) => {}
            _ => return false,
        },
        // Past four, or with one left, nothing changes.
        ["add"] => {
            choice.add_tile();
        }
        ["remove"] => {
            choice.remove_tile();
        }
        ["place", crate::choice::HIDDEN] => choice.hide_box(),
        ["place", name] => match BoxPlace::find(name) {
            Some(to) => choice.set_box_place(to),
            None => return false,
        },
        ["order", "earlier"] => {
            choice.move_tile(true);
        }
        ["order", "later"] => {
            choice.move_tile(false);
        }
        ["drag", "on"] => choice.set_drag(true),
        ["drag", "off"] => choice.set_drag(false),
        // The tools' updates on or off.
        ["updates", "on"] => choice.set_updates(true),
        ["updates", "off"] => choice.set_updates(false),
        // The warning colors on or off.
        ["warn", "on"] => choice.set_warn(true),
        ["warn", "off"] => choice.set_warn(false),
        // A notification on or off, and the line of almost used up, one of
        // the steps offered.
        ["notify", kind, on @ ("on" | "off")] => {
            let mut alerts = choice.alerts();
            if !alerts.set(kind, *on == "on") {
                return false;
            }
            choice.set_alerts(alerts);
        }
        ["notify-at", at] => {
            let mut alerts = choice.alerts();
            if !at.parse().is_ok_and(|at| alerts.set_at(at)) {
                return false;
            }
            choice.set_alerts(alerts);
        }
        // The window's language, one it has words for or empty for Windows'.
        ["language", code] if code.is_empty() || LANGUAGES.contains(code) => {
            choice.set_language(code);
        }
        // The editor's shelves folded, by their words.
        ["folded", words @ ..]
            if words
                .iter()
                .all(|word| crate::boxes::Shelf::find(word).is_some()) =>
        {
            choice.set_folded(words.iter().map(|word| word.to_string()).collect());
        }
        _ => return false,
    }
    true
}

/// A place counted from 0, in plain digits.
fn place(word: &str) -> Option<usize> {
    if word.is_empty() || !word.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::mock;

    #[test]
    fn the_editor_folds_its_shelves() {
        // The state tells the shelves folded; the editor folds and unfolds
        // them by their words, and a word not known is refused.
        let choice = Choice::new(&style::LOOKS[0], Fill::Used, Background::Box);
        let told = |choice: &Choice| {
            state(choice, None, Startup::Off, &[])
                .lines()
                .find(|l| l.starts_with("folded"))
                .unwrap()
                .to_string()
        };
        assert_eq!(told(&choice), "folded\tai\tpc");
        assert!(apply(&choice, "folded\tbasic\tpc"));
        assert_eq!(choice.folded(), ["basic", "pc"]);
        assert!(apply(&choice, "folded"));
        assert_eq!(told(&choice), "folded");
        assert!(!apply(&choice, "folded\tshelf"));
        assert!(choice.folded().is_empty());
    }

    #[test]
    fn the_window_sees_what_the_popup_sees() {
        // Every service offered, whether it shows, and the limits of each, in
        // the list's order.
        let choice = Choice::new(&style::LOOKS[4], Fill::Left, Background::Cards);
        choice.offer(&mock(0));
        let state = state(&choice, Some(1_790_000_010_000), Startup::Off, &[]);
        let lines: Vec<_> = state.lines().collect();
        assert_eq!(
            lines[..9],
            [
                "look\trings",
                "looks\tslim\tinline\tstack\tpie\trings",
                "fill\tleft",
                "background\tcards",
                "color\tdefault",
                "most\t4\t2\tpc\t4",
                "refresh\t1790000010000",
                "box\tColumns sample\tcolumns\t1\tbasic",
                "box\tFree sample\tfree\t1\tbasic",
            ]
        );
        // Each sample is a box the editor lists, locked.
        let samples = crate::boxes::samples();
        for (line, sample) in lines[7..].iter().zip(&samples) {
            assert_eq!(
                *line,
                format!(
                    "box\t{}\t{}\t1\t{}",
                    sample.name,
                    sample.style(),
                    sample.shelf.word()
                )
            );
        }
        let after = 7 + samples.len();
        assert_eq!(
            lines[after..after + 3],
            ["service\tgpt\tGPT\t1", "limit\t5h\t1", "limit\t7d\t1"]
        );
        assert!(state.contains(
            "service\tclaude\tClaude\t1\nlimit\t5h\t1\nlimit\t7d\t1\nlimit\tOpus 7d\t0\n"
        ));
        // A service not shown still tells which of its limits would show.
        assert!(state.contains("service\tgrok\tGrok\t0\nlimit\t7d\t1"));
        // The mock figures never take Refresh now.
        assert!(super::state(&choice, None, Startup::Off, &[]).contains("\nrefresh\tnever\n"));
        assert!(state.encode_utf16().count() <= LONGEST);
    }

    #[test]
    fn the_window_speaks_the_same_words() {
        // Both sides spell the link out, so the window's source is read here
        // and the two cannot drift apart.
        let link = include_str!("../settings/Link.cs");
        let state = include_str!("../settings/State.cs");
        let window = include_str!("../settings/MainWindow.xaml.cs");
        for (name, kind) in [
            ("Command", COMMAND),
            ("State", STATE),
            ("Show", SHOW),
            ("Box", BOX),
            ("Preview", PREVIEW),
            ("Result", RESULT),
        ] {
            assert!(
                link.contains(&format!("internal const nuint {name} = {kind:#X};")),
                "{name}"
            );
        }
        assert!(link.contains(&format!("internal const int Longest = {};", LONGEST)));
        // The version and the repository as the window reads them, and the
        // one place it asks: GitHub's newest release, only when pressed,
        // for at most 10 seconds.
        assert!(state.contains(r#"case ["version", var version]:"#));
        assert!(state.contains(r#"case ["repository", var repository]:"#));
        assert!(window.contains(
            r#"client.GetStringAsync($"https://api.github.com/repos/{repository}/releases/latest")"#
        ));
        assert!(window.contains("const int UpdateSeconds = 10;"));
        assert!(window.contains("Timeout = TimeSpan.FromSeconds(UpdateSeconds)"));
        assert_eq!(window.matches("new HttpClient").count(), 1);
        assert!(window.contains("async void CheckUpdatesClicked("));
        // A box hidden: the place the window offers last, and the marks it
        // reads for the tabs.
        assert!(window.contains(&format!(
            "new Option(\"{}\", Loc.Plain(\"boxes.place.hidden\"))",
            crate::choice::HIDDEN
        )));
        assert!(state.contains(r#"case ["hidden", .. var hidden]:"#));
        // The notifications as the window reads them and turns them, and its
        // test.
        assert!(
            state
                .contains(r#"case ["notify", var near, var full, var back, var failing, var at]:"#)
        );
        assert!(
            window.contains(
                r#"Send($"notify\t{kind}\t{(check.IsChecked == true ? "on" : "off")}");"#
            )
        );
        assert!(window.contains(r#"Pick("notify-at", NotifyAtBox)"#));
        assert!(window.contains(&format!("Send(\"{NOTIFY_TEST}\")")));
        for (kind, _) in crate::notify::Alerts::default().kinds() {
            assert!(
                include_str!("../settings/MainWindow.xaml").contains(&format!("Tag=\"{kind}\"")),
                "{kind}"
            );
        }
        // The editor's commands, as the controller takes them.
        // The editor's parts, as its partial class is split.
        let editor = concat!(
            include_str!("../settings/EditorWindow.xaml.cs"),
            include_str!("../settings/EditorWindow.Blocks.cs"),
            include_str!("../settings/EditorWindow.Stage.cs"),
            include_str!("../settings/EditorWindow.View.cs"),
            include_str!("../settings/EditorWindow.Input.cs"),
            include_str!("../settings/EditorWindow.Color.cs")
        );
        for command in [
            r#"send($"edit\t{name}")"#,
            r#"send($"draft\t{json}")"#,
            r#"send($"save\t{opened}\t{json}")"#,
            r#"send($"delete\t{name}")"#,
            // A new preset starts from its style's sample, and one is put on
            // the taskbar as the settings window picks a look.
            r#"send($"new\t{style}\t{from}")"#,
            r#"send($"look\tbox:{opened}")"#,
            // The taskbar box's color, from the editor's Box line too.
            r#"send($"color\t{MainWindow.Hex(argb)}")"#,
            r#"send("color\tdefault")"#,
        ] {
            assert!(editor.contains(command), "{command}");
        }
        assert!(link.contains(&format!("internal const string Hello = \"{HELLO}\";")));
        assert!(link.contains(&format!(
            "internal const string StartupCheck = \"{STARTUP_CHECK}\";"
        )));
        assert!(window.contains("Send(Link.StartupCheck)"));
        // The Boxes page opens the editor beside a custom box's note, and the
        // icons folder; the editor starts with nothing chosen.
        let page = include_str!("../settings/MainWindow.xaml");
        assert_eq!(page.matches(r#"Click="EditorClicked""#).count(), 2);
        assert!(page.contains(r#"Click="IconsClicked""#));
        assert!(window.contains("EditorLinkButton.Visibility = custom ?"));
        assert!(window.contains(r#"Path.Combine(AppContext.BaseDirectory, "icons")"#));
        assert_eq!(editor.matches("StartUnchosen();").count(), 2);
        assert!(!editor.contains("Choose([0], 0)"));
        for word in [
            "look",
            "looks",
            "fill",
            "background",
            "most",
            "refresh",
            "box",
            "service",
            "limit",
            "summary",
            // Whether it starts at sign-in.
            "startup",
            // Each Task Scheduler task that starts it.
            "task",
            // The boxes, the one chosen, its place and order, and dragging.
            "boxes",
            "place",
            "order",
            "drag",
            // Whether the tools are updated.
            "updates",
            // Whether low values warn in color.
            "warn",
            // The box's own color.
            "color",
        ] {
            assert!(state.contains(&format!("case [\"{word}\"")), "{word}");
        }
        // The box's color, as the window picks it, drops it from the screen
        // and puts it back; the default it shows is the controller's.
        assert!(window.contains(&format!(
            "const string DefaultColor = \"{}\";",
            style::argb_text(style::BOX_COLOR)
        )));
        assert!(window.contains(r#"Send("color\tdefault")"#));
        assert!(window.contains(r#"Send($"color\t{Hex(argb)}")"#));
        assert!(window.contains("Picker.Choose(self, "));
        assert!(window.contains("Picker.PixelAt("));
        assert!(window.contains("AddHandler(Thumb.DragCompletedEvent"));
        for command in [
            r#"Pick("look", StyleBox)"#,
            r#"Pick("fill", FillBox)"#,
            r#"Pick("background", BackgroundBox)"#,
            r#"Send($"service\t{service.Id}")"#,
            r#"Send($"limit\t{service.Id}\t{limit.Label}")"#,
            r#"Send($"move\t{service.Id}\tup")"#,
            r#"Send($"move\t{service.Id}\tdown")"#,
            // A limit moves as when dragged in the popup.
            r#"Send($"move-limit\t{service.Id}\t{limit.Label}\t{place - 1}")"#,
            r#"Send($"move-limit\t{service.Id}\t{limit.Label}\t{place + 1}")"#,
            r#"Send($"startup\t{(StartupCheck.IsChecked == true ? "on" : "off")}")"#,
            r#"Send($"updates\t{(UpdatesCheck.IsChecked == true ? "on" : "off")}")"#,
            r#"Send($"warn\t{(WarnCheck.IsChecked == true ? "on" : "off")}")"#,
            // A box's tab chooses it, and its name is typed.
            r#"Send($"select\t{at}")"#,
            r#"Send($"name\t{state.Current}\t{NameBox.Text}")"#,
            r#"Send("add")"#,
            r#"Send("remove")"#,
            r#"Send("order\tearlier")"#,
            r#"Send("order\tlater")"#,
            r#"Pick("place", PlaceBox)"#,
        ] {
            assert!(window.contains(command), "{command}");
        }
        // Refresh now rests as the controller says, wherever it was
        // pressed, and the window keeps no cooldown of its own.
        assert!(window.contains("refreshAt = state.RefreshAt;"));
        assert!(!window.contains("TimeSpan.FromSeconds(10)"));
        // The window greets the controller until it answers.
        assert!(window.contains("else if (!greeted)"));
        // What the window offers to pick is what the controller knows.
        let names = Fill::ALL
            .iter()
            .map(|f| f.name())
            .chain(Background::ALL.iter().map(|b| b.name()));
        for name in names {
            assert!(window.contains(&format!("new Option(\"{name}\"")), "{name}");
        }
        // There is no look of all looks in turn.
        assert!(!window.contains("Cycle all"));
    }

    #[test]
    fn a_custom_box_is_picked_as_a_look_and_a_preset_puts_it_away() {
        let choice = Choice::new(&style::LOOKS[4], Fill::Left, Background::Box);
        choice.offer(&mock(0));
        assert!(apply(&choice, "look\tbox:Columns sample"));
        assert!(state(&choice, None, Startup::Off, &[]).starts_with("look\tbox:Columns sample\n"));
        assert!(!apply(&choice, "look\tbox:Gone"));
        assert_eq!(choice.tile().custom.as_deref(), Some("Columns sample"));
        assert!(apply(&choice, "look\tpie"));
        assert_eq!(choice.tile().custom, None);
        assert!(state(&choice, None, Startup::Off, &[]).starts_with("look\tpie\n"));
    }

    #[test]
    fn the_window_changes_what_the_popup_changes() {
        // By the same rules: four services and two limits at most, never the
        // last one off, and nothing that is not offered.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        choice.offer(&mock(0));
        // Hide is a place the window sends and is told, and each box's
        // hidden mark follows the boxes; a place shows the box again.
        assert!(apply(&choice, "add"));
        assert!(apply(&choice, "place\thidden"));
        let told = state(&choice, None, Startup::Off, &[]);
        assert!(told.contains("\nhidden\t0\t1\nplace\thidden\n"));
        assert!(apply(&choice, "place\tleft"));
        assert!(!choice.box_hidden());
        assert!(apply(&choice, "remove"));
        for command in [
            "look\tpie",
            "fill\tleft",
            "background\tcards",
            "service\tgrok",
            "limit\tclaude\t5h",
        ] {
            assert!(apply(&choice, command), "{command}");
        }
        assert_eq!(choice.look().name, "pie");
        assert_eq!(
            (choice.fill(), choice.background()),
            (Fill::Left, Background::Cards)
        );
        let offers = choice.offers();
        assert_eq!(
            choice.services(&offers),
            ["gpt", "claude", "gemini", "grok"]
        );
        assert_eq!(choice.shown(&offers[1]), ["7d"]);
        assert!(apply(&choice, "service\tpc"));
        assert_eq!(
            choice.services(&offers),
            ["gpt", "claude", "gemini", "grok"]
        );
        assert!(apply(&choice, "limit\tclaude\t7d"));
        assert_eq!(choice.shown(&offers[1]), ["7d"]);
        // Grok moves ahead of Gemini, and the window lists it there.
        assert!(apply(&choice, "move\tgrok\tup"));
        assert_eq!(
            choice.services(&offers),
            ["gpt", "claude", "grok", "gemini"]
        );
        assert!(
            state(&choice, None, Startup::Off, &[])
                .contains("service\tgrok\tGrok\t1\nlimit\t7d\t1\nservice\tgemini")
        );
        // Dragged in the popup, to a place counted from 0; the window lists
        // shown limits first, in the box's order.
        assert!(apply(&choice, "move\tgrok\t0"));
        assert_eq!(
            choice.services(&offers),
            ["grok", "gpt", "claude", "gemini"]
        );
        assert!(apply(&choice, "limit\tclaude\t5h"));
        assert!(apply(&choice, "move-limit\tclaude\t5h\t0"));
        assert!(state(&choice, None, Startup::Off, &[]).contains(
            "service\tclaude\tClaude\t1\nlimit\t5h\t1\nlimit\t7d\t1\nlimit\tOpus 7d\t0\n"
        ));
        assert!(apply(&choice, "move-summary\tclaude\t9"));
        assert!(apply(&choice, "move-summary-limit\tclaude\t5h\t1"));
        // The summary's own services, apart from the box's, in its order.
        assert!(
            state(&choice, None, Startup::Off, &[])
                .contains("\nsummary\tgrok\tgpt\tgemini\tclaude\n")
        );
        assert!(apply(&choice, "summary-service\tgpt"));
        assert!(apply(&choice, "summary-service\tpc"));
        assert!(
            state(&choice, None, Startup::Off, &[])
                .contains("\nsummary\tgrok\tgemini\tclaude\tpc\n")
        );
        assert_eq!(
            choice.services(&offers),
            ["grok", "gpt", "claude", "gemini"]
        );
        for wrong in [
            "summary-service\tcopilot",
            "summary-service",
            "",
            "look\tcard",
            "fill\thalf",
            "service\tcopilot",
            "limit\tclaude\tSonnet 7d",
            "limit\tclaude",
            "move\tgrok\tleft",
            "move\tgrok\t-1",
            "move\tgrok\t",
            "move-limit\tclaude\tSonnet 7d\t0",
            "move-limit\tclaude\t7d",
            "move-summary\tcopilot\t0",
            "move-summary-limit\tclaude\t7d\tfirst",
            "move\tcopilot\tup",
            "hello",
        ] {
            assert!(!apply(&choice, wrong), "{wrong}");
        }
        assert_eq!(choice.look().name, "pie");
    }

    #[test]
    fn starting_at_sign_in_is_sent_and_taken_apart_from_the_choice() {
        // It is kept in the registry, so the state says it after the choice
        // and the command is not the choice's.
        let choice = Choice::new(&style::LOOKS[4], Fill::Left, Background::Cards);
        choice.offer(&mock(0));
        for startup in [Startup::Off, Startup::On, Startup::Blocked] {
            let state = state(&choice, None, startup, &[]);
            assert!(state.contains(&format!("\nstartup\t{}\n", startup.word())));
        }
        assert_eq!(super::startup("startup\ton"), Some(true));
        assert_eq!(super::startup("startup\toff"), Some(false));
        for wrong in ["startup", "startup\tblocked", "startup\ton\tx", "look\ton"] {
            assert_eq!(super::startup(wrong), None, "{wrong}");
        }
        assert!(!apply(&choice, "startup\ton"));
        assert!(!apply(&choice, STARTUP_CHECK));
    }

    #[test]
    fn each_sign_in_task_found_is_sent_after_the_start() {
        // What the window says of each: where it is, whether it is on,
        // whether it starts this EXE, what keeps the box away, and the
        // program.
        use crate::logon_task::Problem;
        let choice = Choice::new(&style::LOOKS[4], Fill::Left, Background::Cards);
        choice.offer(&mock(0));
        let tasks = [
            LogonTask {
                path: r"\LimitPeeker".into(),
                program: r"D:\x\limitpeeker.exe".into(),
                enabled: true,
                this_exe: true,
                problem: Problem::None,
            },
            LogonTask {
                path: r"\Old one".into(),
                program: r#""E:\y\limitpeeker.exe""#.into(),
                enabled: false,
                this_exe: false,
                problem: Problem::Hidden,
            },
        ];
        let state = state(&choice, None, Startup::On, &tasks);
        let end = concat!(
            "\nstartup\ton",
            "\ntask\t\\LimitPeeker\t1\t1\t\tD:\\x\\limitpeeker.exe",
            "\ntask\t\\Old one\t0\t0\thidden\t\"E:\\y\\limitpeeker.exe\"",
            "\nversion\t",
            env!("CARGO_PKG_VERSION")
        );
        // The repository, once there is one, comes after the version.
        let end = match REPOSITORY {
            "" => end.to_owned(),
            repository => format!("{end}\nrepository\t{repository}"),
        };
        assert!(state.ends_with(&end));
    }

    #[test]
    fn the_version_comes_last_and_where_to_ask_for_a_newer_one() {
        // The controller's version is the one the window shows; the
        // repository is sent only once there is one, and the window's Check
        // for updates shows only then.
        let choice = Choice::new(&style::LOOKS[4], Fill::Left, Background::Cards);
        choice.offer(&mock(0));
        let state = state(&choice, None, Startup::Off, &[]);
        let version = format!("version\t{}", env!("CARGO_PKG_VERSION"));
        if REPOSITORY.is_empty() {
            assert!(state.ends_with(&format!("\n{version}")));
            assert!(!state.contains("\nrepository\t"));
        } else {
            assert!(state.ends_with(&format!("\n{version}\nrepository\t{REPOSITORY}")));
            assert_eq!(REPOSITORY.split('/').count(), 2);
        }
    }

    #[test]
    fn notifications_are_sent_to_the_window_and_turned_from_it() {
        // Each kind in KINDS's order, then the line; a kind or a line not
        // known changes nothing.
        let choice = Choice::new(&style::LOOKS[4], Fill::Left, Background::Cards);
        choice.offer(&mock(0));
        let sent = |choice: &Choice| {
            state(choice, None, Startup::Off, &[])
                .lines()
                .find(|line| line.starts_with("notify	"))
                .map(String::from)
        };
        assert_eq!(sent(&choice).as_deref(), Some("notify	1	1	1	1	90"));
        assert!(apply(&choice, "notify	back	off"));
        assert!(apply(&choice, "notify-at	80"));
        assert_eq!(sent(&choice).as_deref(), Some("notify	1	1	0	1	80"));
        for wrong in [
            "notify	loud	on",
            "notify	near	maybe",
            "notify-at	85",
            "notify-at	ninety",
        ] {
            assert!(!apply(&choice, wrong), "{wrong}");
        }
        assert_eq!(sent(&choice).as_deref(), Some("notify	1	1	0	1	80"));
    }

    #[test]
    fn the_windows_language_is_kept_for_it_alone() {
        // A language it has words for, or empty for Windows'; written to the
        // settings only when picked.
        let choice = Choice::new(&style::LOOKS[4], Fill::Left, Background::Cards);
        choice.offer(&mock(0));
        let told = |choice: &Choice| {
            state(choice, None, Startup::Off, &[])
                .lines()
                .find(|line| line.starts_with("language\t"))
                .map(String::from)
        };
        assert_eq!(told(&choice).as_deref(), Some("language\t"));
        assert!(apply(&choice, "language\tko"));
        assert_eq!(told(&choice).as_deref(), Some("language\tko"));
        assert!(crate::settings::text(&choice).contains("\nlanguage=ko\n"));
        for wrong in [
            "language\tde",
            "language\tKO",
            "language",
            "language\tko\tx",
        ] {
            assert!(!apply(&choice, wrong), "{wrong}");
        }
        assert_eq!(told(&choice).as_deref(), Some("language\tko"));
        assert!(apply(&choice, "language\t"));
        assert_eq!(told(&choice).as_deref(), Some("language\t"));
        assert!(!crate::settings::text(&choice).contains("language="));
    }

    #[test]
    fn the_tools_updates_are_turned_on_and_off() {
        // Off at first; on is told back and written.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        let told = |choice: &Choice| {
            state(choice, None, Startup::Off, &[])
                .lines()
                .find(|line| line.starts_with("updates\t"))
                .map(String::from)
        };
        assert_eq!(told(&choice).as_deref(), Some("updates\toff"));
        assert!(apply(&choice, "updates\ton"));
        assert_eq!(told(&choice).as_deref(), Some("updates\ton"));
        assert!(crate::settings::text(&choice).contains("\nupdates=on\n"));
        for wrong in ["updates\tmaybe", "updates", "updates\toff\tx"] {
            assert!(!apply(&choice, wrong), "{wrong}");
        }
        assert!(choice.updates());
        assert!(apply(&choice, "updates\toff"));
        assert_eq!(told(&choice).as_deref(), Some("updates\toff"));
    }

    #[test]
    fn a_boxs_color_is_told_and_taken() {
        // Default until one is chosen; #AARRGGBB is told back, in upper
        // case, and written; default puts it back; anything else changes
        // nothing.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        let told = |choice: &Choice| {
            state(choice, None, Startup::Off, &[])
                .lines()
                .find(|line| line.starts_with("color\t"))
                .map(String::from)
        };
        assert_eq!(told(&choice).as_deref(), Some("color\tdefault"));
        assert!(apply(&choice, "color\t#80ff8800"));
        assert_eq!(told(&choice).as_deref(), Some("color\t#80FF8800"));
        assert_eq!(choice.color(), Some(0x80FF_8800));
        assert!(crate::settings::text(&choice).contains("\ncolor=#80FF8800\n"));
        for wrong in [
            "color\t#FF8800",
            "color\tred",
            "color",
            "color\t#80FF8800\tx",
        ] {
            assert!(!apply(&choice, wrong), "{wrong}");
        }
        assert_eq!(choice.color(), Some(0x80FF_8800));
        assert!(apply(&choice, "color\tdefault"));
        assert_eq!(told(&choice).as_deref(), Some("color\tdefault"));
        assert!(!crate::settings::text(&choice).contains("color="));
    }

    #[test]
    fn the_warning_colors_are_turned_off_and_on() {
        // On at first; off is told back and written.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        let told = |choice: &Choice| {
            state(choice, None, Startup::Off, &[])
                .lines()
                .find(|line| line.starts_with("warn\t"))
                .map(String::from)
        };
        assert_eq!(told(&choice).as_deref(), Some("warn\ton"));
        assert!(apply(&choice, "warn\toff"));
        assert_eq!(told(&choice).as_deref(), Some("warn\toff"));
        assert!(crate::settings::text(&choice).contains("\nwarn=off\n"));
        assert!(!apply(&choice, "warn\tmaybe"));
        assert!(!choice.warn());
        assert!(apply(&choice, "warn\ton"));
        assert!(choice.warn());
    }
}
