//! The settings window's words, one file for each language. The window has no
//! tests of its own, so these read its sources and its language files and
//! fail when a word is missing, left over, or written into the window
//! instead of taken from the files.

#[cfg(test)]
mod tests {
    use crate::json::{Json, parse};
    use std::collections::BTreeSet;

    /// Each language's file, in the order the window lists them, with its
    /// name in itself.
    const FILES: [(&str, &str, &str); 6] = [
        ("en", "English", include_str!("../settings/Strings/en.json")),
        ("es", "Español", include_str!("../settings/Strings/es.json")),
        (
            "fr",
            "Français",
            include_str!("../settings/Strings/fr.json"),
        ),
        ("ja", "日本語", include_str!("../settings/Strings/ja.json")),
        ("ko", "한국어", include_str!("../settings/Strings/ko.json")),
        (
            "pt",
            "Português (Brasil)",
            include_str!("../settings/Strings/pt.json"),
        ),
    ];
    const ENGLISH: &str = FILES[0].2;
    const XAML: [&str; 3] = [
        include_str!("../settings/App.xaml"),
        include_str!("../settings/MainWindow.xaml"),
        include_str!("../settings/EditorWindow.xaml"),
    ];
    const CODE: [&str; 9] = [
        include_str!("../settings/Loc.cs"),
        include_str!("../settings/App.xaml.cs"),
        include_str!("../settings/MainWindow.xaml.cs"),
        include_str!("../settings/EditorWindow.xaml.cs"),
        include_str!("../settings/EditorWindow.Blocks.cs"),
        include_str!("../settings/EditorWindow.Stage.cs"),
        include_str!("../settings/EditorWindow.View.cs"),
        include_str!("../settings/EditorWindow.Input.cs"),
        include_str!("../settings/EditorWindow.Color.cs"),
    ];
    /// Names kept as they are in every language: the product, services,
    /// Windows, files, keys and units.
    const NAMES: [&str; 16] = [
        "LimitPeeker",
        "Codex",
        "Claude",
        "Antigravity",
        "Grok",
        "PC",
        "Windows",
        "uninstall.exe",
        "limitpeeker.exe",
        "--remove",
        "AppData",
        "DIP",
        "Ctrl",
        "Shift",
        "%",
        "#RRGGBB",
    ];
    /// What the XAML may still say itself: names, keys and the place's letters.
    const WRITTEN: [&str; 7] = ["LimitPeeker", "Ctrl+N", "DIPs", "X", "Y", "W", "H"];

    /// A language file's keys and words, in the file's order.
    fn words(file: &str) -> Vec<(String, String)> {
        match parse(file) {
            Some(Json::Object(fields)) => fields
                .into_iter()
                .map(|(key, value)| match value {
                    Json::Text(text) => (key, text),
                    other => panic!("{key} is {other:?}, not words"),
                })
                .collect(),
            other => panic!("not a JSON object: {other:?}"),
        }
    }

    /// The {name} places in words.
    fn places(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
            .collect()
    }

    /// The keys a XAML file takes words by: {l:T key}.
    fn xaml_keys(xaml: &str) -> Vec<&str> {
        xaml.split("{l:T ")
            .skip(1)
            .map(|rest| rest.split_once('}').expect("an unclosed {l:T").0)
            .collect()
    }

    /// The keys C# names: each "group.word" string whose group is one the
    /// files have, given to Loc.T straight or chosen first.
    fn code_keys<'a>(code: &'a str, groups: &BTreeSet<&str>) -> Vec<&'a str> {
        code.split('"')
            .filter(|part| {
                part.contains('.')
                    && part.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.'
                    })
                    && groups.contains(part.split('.').next().unwrap_or(""))
            })
            .collect()
    }

    #[test]
    fn every_language_has_every_word() {
        let english = words(ENGLISH);
        let keys: Vec<&str> = english.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(
            keys.iter().collect::<BTreeSet<_>>().len(),
            keys.len(),
            "a key twice in en.json"
        );
        assert_eq!(keys[0], "language.name");
        for (code, name, file) in FILES {
            let other = words(file);
            // The same keys in the same order, so the files read side by side.
            assert_eq!(
                other
                    .iter()
                    .map(|(key, _)| key.as_str())
                    .collect::<Vec<_>>(),
                keys,
                "{code}.json's keys are not en.json's"
            );
            assert_eq!(other[0].1, name, "{code}'s name in itself");
            for ((key, english), (_, words)) in english.iter().zip(&other) {
                assert!(!words.trim().is_empty(), "{code} {key} is empty");
                assert_eq!(places(english), places(words), "{code} {key}'s places");
                for kept in NAMES {
                    if english.contains(kept) {
                        assert!(words.contains(kept), "{code} {key} lost {kept}");
                    }
                }
            }
        }
        // Each language the window offers, as the controller takes it, has
        // its file inside the program; Japanese has a Japanese font.
        let loc = include_str!("../settings/Loc.cs");
        assert_eq!(
            FILES.map(|(code, _, _)| code),
            crate::settings_link::LANGUAGES
        );
        let languages: Vec<String> = crate::settings_link::LANGUAGES
            .iter()
            .map(|code| format!("\"{code}\""))
            .collect();
        assert!(loc.contains(&format!("Languages = [{}];", languages.join(", "))));
        assert!(loc.contains(r#"["ja"] = "Segoe UI, Yu Gothic UI, Malgun Gothic""#));
        assert!(include_str!("../settings/LimitPeeker.Settings.csproj").contains(
            r#"<EmbeddedResource Include="Strings\*.json" LogicalName="LimitPeeker.Settings.Strings.%(Filename).json" />"#
        ));
        assert!(
            loc.contains(
                r#"GetManifestResourceStream($"LimitPeeker.Settings.Strings.{code}.json")"#
            )
        );
    }

    #[test]
    fn the_window_opens_in_the_language_picked() {
        // The controller starts the window in the language picked, which it
        // takes before anything is drawn; another one picked is sent, and
        // the window starts again in it on the General page.
        let start = include_str!("windows_app/settings_window.rs");
        let loc = include_str!("../settings/Loc.cs");
        let app = include_str!("../settings/App.xaml.cs");
        let window = include_str!("../settings/MainWindow.xaml.cs");
        assert!(start.contains(r#"command.arg(format!("--language={language}"));"#));
        assert!(loc.contains(r#"StartsWith("--language=", StringComparison.Ordinal)"#));
        assert!(
            app.contains("public App() => Loc.Use(Loc.From(Environment.GetCommandLineArgs()));")
        );
        assert!(window.contains(r#"Send($"language\t{code}");"#));
        assert!(window.contains(r#"start.ArgumentList.Add($"--language={language}");"#));
        assert!(window.contains(r#"start.ArgumentList.Add("--page=general");"#));
        assert!(app.contains(r#"StartsWith("--page=", StringComparison.Ordinal)"#));
        assert!(XAML[1].contains(r#"Tag="GeneralPage""#));
        assert!(
            include_str!("../settings/State.cs").contains(r#"case ["language", var language]:"#)
        );
    }

    #[test]
    fn the_windows_take_every_word_from_the_files() {
        let english = words(ENGLISH);
        let keys: BTreeSet<&str> = english.iter().map(|(key, _)| key.as_str()).collect();
        let groups: BTreeSet<&str> = keys
            .iter()
            .filter_map(|key| key.split('.').next())
            .collect();
        let mut used = BTreeSet::new();
        for xaml in XAML {
            used.extend(xaml_keys(xaml));
        }
        for code in CODE {
            used.extend(code_keys(code, &groups));
        }
        let unknown: Vec<_> = used.difference(&keys).collect();
        assert!(unknown.is_empty(), "not in en.json: {unknown:?}");
        let unused: Vec<_> = keys.difference(&used).collect();
        assert!(unused.is_empty(), "never used: {unused:?}");
    }

    #[test]
    fn no_words_are_written_into_the_xaml() {
        for xaml in XAML {
            for attribute in [
                "Text",
                "Content",
                "ToolTip",
                "Title",
                "AutomationProperties.Name",
            ] {
                let start = format!("{attribute}=\"");
                for (at, _) in xaml.match_indices(&start) {
                    if !xaml[..at].ends_with(char::is_whitespace) {
                        continue;
                    }
                    let value = xaml[at + start.len()..].split('"').next().unwrap_or("");
                    assert!(
                        value.is_empty()
                            || value.starts_with('{')
                            || value.starts_with("&#x")
                            || WRITTEN.contains(&value),
                        "{attribute}=\"{value}\" is written into the XAML"
                    );
                }
            }
        }
        // The switch's On, in a trigger's setter, is taken from the files too.
        assert!(XAML[0].contains(r#"Property="Text" Value="{l:T switch.on}""#));
    }
}
