//! Where the hidden Claude Code runs and how it starts: an empty folder of
//! its own, trusted once in Claude Code's own file, a settings file that
//! names only the status line, and its command line and environment. The
//! person's own settings are not touched.

use crate::json::{self, Json, escaped};
use crate::places;
use std::path::{Path, PathBuf};

/// Where Claude Code is put when it is not on the PATH, tried in this
/// order: its own installer's place, npm's own folder for what it installs
/// for everyone (the platform package, as NPM_PLATFORM), then the copy
/// inside the VS Code extension, the same program. A `*` stands for the
/// extension's version.
const PLACES: [(&str, &str); 4] = [
    ("USERPROFILE", r".local\bin\claude.exe"),
    (
        "APPDATA",
        r"npm\node_modules\@anthropic-ai\claude-code\node_modules\@anthropic-ai\claude-code-win32-x64\claude.exe",
    ),
    (
        "USERPROFILE",
        r".vscode\extensions\anthropic.claude-code-*-win32-x64\resources\native-binary\claude.exe",
    ),
    (
        "USERPROFILE",
        r".vscode-insiders\extensions\anthropic.claude-code-*-win32-x64\resources\native-binary\claude.exe",
    ),
];
/// Claude Code's mark that a folder is trusted, per project in its file.
const TRUSTED: &str = "hasTrustDialogAccepted";
/// How often trusting is tried when Claude Code writes its file meanwhile.
const TRIES: u32 = 3;

/// Read only the settings passed here, not the person's; start no MCP
/// servers.
const RESTRICTED: [&str; 2] = ["--restricted", "--strict-mcp-config"];
/// Added to the hidden Claude Code's environment: keep no prompt history or
/// transcript, leave updating to the person's own sessions, and let the
/// status line command know it was run from here.
pub const ADDED: [(&str, &str); 3] = [
    ("CLAUDE_CODE_SKIP_PROMPT_HISTORY", "1"),
    ("DISABLE_AUTOUPDATER", "1"),
    (crate::claude::HEARD_VAR, "1"),
];
/// Taken out of it: what marks a session started from inside Claude Code,
/// had this program been.
pub const REMOVED: [&str; 4] = [
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SSE_PORT",
];

/// npm's Claude Code keeps the program in a package for each platform, here
/// Windows x64.
const NPM_PLATFORM: (&str, &str) = ("claude-code-win32-x64", "claude.exe");
/// The least a program is: npm's package carries a placeholder of 500 bytes
/// where its install script links the program, and it stays when that
/// script is not run.
const PROGRAM_LEAST: u64 = 1024 * 1024;

/// Where Claude Code is: what a person gets by typing `claude` on the PATH,
/// a `claude.exe` or npm's shim, or else the first of PLACES that has one,
/// as for Codex.
pub fn find() -> Option<PathBuf> {
    places::typed(std::env::var_os("PATH"), "claude", from_npm)
        .or_else(|| places::in_places(&PLACES, places::environment))
}

/// The program behind npm's shim, from the file it starts
/// (`…\@anthropic-ai\claude-code\bin\claude.exe`): the platform package's,
/// else that file itself when it is the program and not the placeholder.
fn from_npm(started: &Path) -> Option<PathBuf> {
    let package = started.parent()?.parent()?;
    let (platform, inside) = NPM_PLATFORM;
    places::npm_platform(package, platform, inside).or_else(|| {
        let length = started.metadata().ok()?.len();
        (length >= PROGRAM_LEAST).then(|| started.to_path_buf())
    })
}

/// The hidden Claude Code's folder, made if it is not there. Empty and its
/// own, so trusting it trusts nothing else; not the home folder, whose
/// trust Claude Code does not keep.
pub fn home() -> Result<PathBuf, String> {
    let home = crate::settings::data_dir()
        .ok_or("no LOCALAPPDATA")?
        .join("claude-home");
    std::fs::create_dir_all(&home)
        .map_err(|e| format!("could not make {}: {e}", home.display()))?;
    Ok(home)
}

/// Claude Code at `exe` with `args`, as the reads run it: in the hidden
/// Claude Code's folder, without the marks of a session started inside
/// Claude Code. For its own update, which the reads leave off.
pub fn command(exe: &Path, args: &[&str]) -> Option<std::process::Command> {
    Some(command_in(&home().ok()?, exe, args))
}

fn command_in(home: &Path, exe: &Path, args: &[&str]) -> std::process::Command {
    let mut command = std::process::Command::new(exe);
    command.args(args).current_dir(home);
    for name in REMOVED {
        command.env_remove(name);
    }
    command
}

/// Writes the settings file for this EXE when it says anything else, so a
/// moved EXE is still the one run.
pub fn write_settings(exe: &Path) -> Result<PathBuf, String> {
    let file = crate::settings::data_dir()
        .ok_or("no LOCALAPPDATA")?
        .join("claude-settings.json");
    let text = settings(exe);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&file, text)
            .map_err(|e| format!("could not write {}: {e}", file.display()))?;
    }
    Ok(file)
}

/// Only the status line: this EXE, by a path with `/`, as Git Bash runs it.
fn settings(exe: &Path) -> String {
    let command = format!(
        "\"{}\" --claude-statusline",
        exe.to_string_lossy().replace('\\', "/")
    );
    format!(
        "{{\"statusLine\":{{\"type\":\"command\",\"command\":\"{}\"}}}}\n",
        escaped(&command)
    )
}

/// Claude Code's own file, where trust is kept: under CLAUDE_CONFIG_DIR
/// when that is set, which the hidden Claude Code sees too, or else in the
/// profile.
pub fn config() -> Option<PathBuf> {
    config_in(
        std::env::var_os("CLAUDE_CONFIG_DIR"),
        std::env::var_os("USERPROFILE"),
    )
}

fn config_in(
    config_dir: Option<std::ffi::OsString>,
    profile: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    config_dir
        .filter(|dir| !dir.is_empty())
        .or(profile)
        .map(|dir| PathBuf::from(dir).join(".claude.json"))
}

/// The arguments: the settings file, the session's id, and unless told not
/// to, only those settings and no MCP servers.
pub fn arguments(settings: &Path, session: &str, restricted: bool) -> Vec<String> {
    let mut args: Vec<String> = if restricted {
        RESTRICTED.iter().map(|a| a.to_string()).collect()
    } else {
        vec![RESTRICTED[1].to_string()]
    };
    args.extend([
        "--settings".into(),
        settings.to_string_lossy().into_owned(),
        "--session-id".into(),
        session.into(),
    ]);
    args
}

/// A new session id, a random UUID in the form Claude Code takes.
pub fn session_id() -> Result<String, String> {
    let g = unsafe { windows::Win32::System::Com::CoCreateGuid() }.map_err(|e| e.to_string())?;
    let d = g.data4;
    Ok(format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1, g.data2, g.data3, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
    ))
}

#[derive(Debug, PartialEq)]
pub enum Trust {
    /// Nothing was written: it was trusted already, or, taking it out,
    /// there was nothing to take.
    Already,
    /// It was written, on the given try.
    Wrote(u32),
    Failed(String),
}

/// The keys Claude Code may keep a folder's trust under: the path as
/// written, as older versions keep it, and with `/` for `\`, as 2.1.283
/// looks it up on Windows. Both are set, so whichever version runs finds
/// it.
pub fn keys(folder: &str) -> Vec<String> {
    let slashed = folder.replace('\\', "/");
    if slashed == folder {
        vec![slashed]
    } else {
        vec![folder.to_string(), slashed]
    }
}

/// Trusts a folder under each of `keys` in Claude Code's file at `config`,
/// only where it is not yet: those keys are set and every other byte is
/// left. What is written is first checked to read back trusted, and it
/// replaces the file only if the file is still as it was read; otherwise
/// it is tried again. Nothing is made when there is no file, and no copy is
/// kept: Claude Code keeps its own.
pub fn trust(config: &Path, keys: &[String]) -> Trust {
    rewrite(config, |text| trusted(text, keys))
}

/// Takes the hidden Claude Code's folder back out of Claude Code's file at
/// `config`: the `projects` entry under each of `keys`, with what Claude
/// Code wrote in it, and every other byte left, written as `trust` writes.
/// No file is nothing to take out.
pub fn untrust(config: &Path, keys: &[String]) -> Trust {
    if !config.exists() {
        return Trust::Already;
    }
    rewrite(config, |text| untrusted(text, keys))
}

/// Writes `change` of Claude Code's file in its place, only if the file is
/// still as it was read; otherwise reads it again, up to TRIES times.
fn rewrite(config: &Path, change: impl Fn(&str) -> Result<Option<String>, String>) -> Trust {
    let temp = config.with_file_name(".claude.json.limitpeeker.tmp");
    for attempt in 1..=TRIES {
        let Ok(read) = std::fs::read(config) else {
            return Trust::Failed("Claude Code's file is not there; it has not run here".into());
        };
        let Ok(text) = String::from_utf8(read.clone()) else {
            return Trust::Failed("Claude Code's file is not text".into());
        };
        let changed = match change(&text) {
            Ok(None) => return Trust::Already,
            Ok(Some(changed)) => changed,
            Err(why) => return Trust::Failed(why),
        };
        if let Err(e) = std::fs::write(&temp, &changed) {
            return Trust::Failed(format!("could not write beside it: {e}"));
        }
        if std::fs::read(config).ok().as_deref() != Some(read.as_slice()) {
            let _ = std::fs::remove_file(&temp);
            std::thread::sleep(std::time::Duration::from_millis(200));
            continue;
        }
        return match std::fs::rename(&temp, config) {
            Ok(()) => Trust::Wrote(attempt),
            Err(e) => {
                let _ = std::fs::remove_file(&temp);
                Trust::Failed(format!("could not replace it: {e}"))
            }
        };
    }
    Trust::Failed(format!("it changed while being written, {TRIES} times"))
}

/// Whether `folder` is trusted in `config`'s text, by its exact key.
pub fn is_trusted(config: &Json, folder: &str) -> bool {
    config
        .get("projects")
        .and_then(|p| p.get(folder))
        .and_then(|p| p.get(TRUSTED))
        == Some(&Json::Bool(true))
}

/// The text without the `projects` entry of any of `keys`, or None when
/// there is none.
fn untrusted(text: &str, keys: &[String]) -> Result<Option<String>, String> {
    let root = json::parse(text).ok_or("Claude Code's file is not JSON")?;
    if !matches!(root.get("projects"), Some(Json::Object(_))) {
        return Ok(None);
    }
    let mut changed = text.to_string();
    for key in keys {
        changed = json::without(&changed, &["projects"], key)
            .ok_or("a value was not where it was read")?;
    }
    Ok((changed != text).then_some(changed))
}

/// The text with every one of `keys` trusted, or None when they all are.
fn trusted(text: &str, keys: &[String]) -> Result<Option<String>, String> {
    let mut changed: Option<String> = None;
    for key in keys {
        if let Some(next) = trusted_one(changed.as_deref().unwrap_or(text), key)? {
            changed = Some(next);
        }
    }
    Ok(changed)
}

/// The text with `folder` trusted, or None when it already is. Only what
/// that takes is changed: the value set to `true`, or the one field, the
/// folder, or `projects` put first in its object.
fn trusted_one(text: &str, folder: &str) -> Result<Option<String>, String> {
    let root = json::parse(text).ok_or("Claude Code's file is not JSON")?;
    if !matches!(root, Json::Object(_)) {
        return Err("Claude Code's file is not an object".into());
    }
    if is_trusted(&root, folder) {
        return Ok(None);
    }
    let key = format!("\"{}\"", escaped(folder));
    let flag = format!("\"{TRUSTED}\": true");
    let place = |keys: &[&str]| json::span(text, keys).ok_or("a value was not where it was read");
    let changed = match root.get("projects") {
        None => first(
            text,
            &root,
            place(&[])?,
            &format!("\"projects\": {{{key}: {{{flag}}}}}"),
        ),
        Some(projects @ Json::Object(_)) => match projects.get(folder) {
            None => first(
                text,
                projects,
                place(&["projects"])?,
                &format!("{key}: {{{flag}}}"),
            ),
            Some(project @ Json::Object(_)) => match project.get(TRUSTED) {
                None => first(text, project, place(&["projects", folder])?, &flag),
                Some(_) => {
                    let value = place(&["projects", folder, TRUSTED])?;
                    format!("{}true{}", &text[..value.start], &text[value.end..])
                }
            },
            Some(_) => return Err("the folder's entry is not an object".into()),
        },
        Some(_) => return Err("projects is not an object".into()),
    };
    match json::parse(&changed) {
        Some(back) if is_trusted(&back, folder) => Ok(Some(changed)),
        _ => Err("the change did not read back trusted".into()),
    }
}

/// `text` with `field` put first in the object at `span`.
fn first(text: &str, object: &Json, span: std::ops::Range<usize>, field: &str) -> String {
    let empty = matches!(object, Json::Object(fields) if fields.is_empty());
    let at = span.start + 1;
    let comma = if empty { "" } else { "," };
    format!("{}{field}{comma}{}", &text[..at], &text[at..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOLDER: &str = r"C:\Users\me\AppData\Local\LimitPeeker\claude-home";

    #[test]
    fn its_update_runs_in_the_hidden_claude_codes_folder() {
        // As the reads run it, without the marks of a session started
        // inside Claude Code, and with its own update left on.
        let command = command_in(
            Path::new(FOLDER),
            Path::new(r"C:\Users\me\.local\bin\claude.exe"),
            &["update"],
        );
        assert_eq!(command.get_current_dir(), Some(Path::new(FOLDER)));
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["update"]);
        let envs: Vec<_> = command.get_envs().collect();
        for name in REMOVED {
            assert!(envs.contains(&(std::ffi::OsStr::new(name), None)), "{name}");
        }
        assert!(envs.iter().all(|(name, _)| *name != "DISABLE_AUTOUPDATER"));
    }

    fn trusted_in(text: &str) -> bool {
        is_trusted(&json::parse(text).unwrap(), FOLDER)
    }

    #[test]
    fn a_folder_is_trusted_by_changing_only_what_it_takes() {
        // Claude Code's file is changed in one place and left as it was
        // everywhere else.
        let key = r#""C:\\Users\\me\\AppData\\Local\\LimitPeeker\\claude-home""#;
        let other = r#""D:\\work": {"hasTrustDialogAccepted": true, "x": [1, 2]}"#;
        let cases = [
            // Set false: only the value changes.
            (
                format!(
                    r#"{{"n": 1, "projects": {{{other}, {key}: {{"a": 1, "hasTrustDialogAccepted": false}}}}}}"#
                ),
                format!(
                    r#"{{"n": 1, "projects": {{{other}, {key}: {{"a": 1, "hasTrustDialogAccepted": true}}}}}}"#
                ),
            ),
            // The folder without the field.
            (
                format!(r#"{{"projects": {{{key}: {{"a": 1}}}}}}"#),
                format!(r#"{{"projects": {{{key}: {{"hasTrustDialogAccepted": true,"a": 1}}}}}}"#),
            ),
            (
                format!(r#"{{"projects": {{{key}: {{}}}}}}"#),
                format!(r#"{{"projects": {{{key}: {{"hasTrustDialogAccepted": true}}}}}}"#),
            ),
            // No folder yet.
            (
                format!(r#"{{"projects": {{{other}}}, "n": 1}}"#),
                format!(
                    r#"{{"projects": {{{key}: {{"hasTrustDialogAccepted": true}},{other}}}, "n": 1}}"#
                ),
            ),
            (
                r#"{"projects": {}}"#.to_string(),
                format!(r#"{{"projects": {{{key}: {{"hasTrustDialogAccepted": true}}}}}}"#),
            ),
            // No projects yet.
            (
                "{\n  \"n\": 1\n}".to_string(),
                format!(
                    "{{\"projects\": {{{key}: {{\"hasTrustDialogAccepted\": true}}}},\n  \"n\": 1\n}}"
                ),
            ),
            (
                "{}".to_string(),
                format!(r#"{{"projects": {{{key}: {{"hasTrustDialogAccepted": true}}}}}}"#),
            ),
        ];
        for (before, after) in cases {
            assert!(!trusted_in(&before), "{before}");
            let changed = trusted_one(&before, FOLDER).unwrap().unwrap();
            assert_eq!(changed, after);
            assert!(trusted_in(&changed));
            assert_eq!(trusted_one(&changed, FOLDER), Ok(None));
        }
    }

    #[test]
    fn a_trusted_folder_or_an_odd_file_is_left_alone() {
        let key = r#""C:\\Users\\me\\AppData\\Local\\LimitPeeker\\claude-home""#;
        let text = format!(r#"{{"projects": {{{key}: {{"hasTrustDialogAccepted": true}}}}}}"#);
        assert_eq!(trusted_one(&text, FOLDER), Ok(None));
        // Another spelling of the folder is not Claude Code's key for it.
        let slashed = r#"{"projects": {"C:/Users/me/AppData/Local/LimitPeeker/claude-home": {"hasTrustDialogAccepted": true}}}"#;
        assert!(trusted_one(slashed, FOLDER).unwrap().is_some());
        let not_an_object = format!(r#"{{"projects": {{{key}: true}}}}"#);
        for odd in [
            "",
            "[]",
            "{\"projects\": [] }",
            "{\"projects\": 1",
            not_an_object.as_str(),
        ] {
            assert!(trusted_one(odd, FOLDER).is_err(), "{odd}");
        }
    }

    #[test]
    fn trusting_writes_once_and_makes_no_file() {
        let dir = std::env::temp_dir().join(format!("limitpeeker-trust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join(".claude.json");
        let _ = std::fs::remove_file(&config);
        let keys = keys(FOLDER);
        assert_eq!(
            keys,
            [FOLDER, "C:/Users/me/AppData/Local/LimitPeeker/claude-home"]
        );
        assert!(matches!(trust(&config, &keys), Trust::Failed(_)));
        assert!(!config.exists());
        std::fs::write(&config, "{\"n\": 1}").unwrap();
        assert_eq!(trust(&config, &keys), Trust::Wrote(1));
        // Both spellings, in one write; then nothing more.
        let back = json::parse(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert!(keys.iter().all(|key| is_trusted(&back, key)));
        assert_eq!(back.get("n").and_then(Json::number), Some(1.0));
        assert_eq!(trust(&config, &keys), Trust::Already);
        // One already there is left; only the other is added.
        std::fs::write(
            &config,
            r#"{"projects": {"C:/Users/me/AppData/Local/LimitPeeker/claude-home": {"hasTrustDialogAccepted": true}}}"#,
        )
        .unwrap();
        assert_eq!(trust(&config, &keys), Trust::Wrote(1));
        let back = json::parse(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert!(keys.iter().all(|key| is_trusted(&back, key)));
        assert_eq!(super::keys("/tmp/x"), ["/tmp/x"]);
        assert!(!dir.join(".claude.json.limitpeeker.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removing_takes_out_only_the_hidden_folder() {
        // Both spellings go, with what Claude Code wrote in them; the
        // person's own projects and every other byte stay.
        let dir = std::env::temp_dir().join(format!("limitpeeker-untrust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join(".claude.json");
        let _ = std::fs::remove_file(&config);
        let keys = keys(FOLDER);
        assert_eq!(untrust(&config, &keys), Trust::Already);
        assert!(!config.exists());
        let own = "{\n  \"n\": 1,\n  \"projects\": {\n    \"D:/work\": {\n      \"hasTrustDialogAccepted\": true\n    }\n  }\n}";
        std::fs::write(&config, own).unwrap();
        assert_eq!(trust(&config, &keys), Trust::Wrote(1));
        // What Claude Code adds when it runs there goes too.
        let text = std::fs::read_to_string(&config).unwrap();
        let slashed = escaped(&keys[1]);
        let text = text.replacen(
            &format!("\"{slashed}\": {{"),
            &format!("\"{slashed}\": {{\"lastVersionBase\": \"2.1\", "),
            1,
        );
        std::fs::write(&config, &text).unwrap();
        assert_eq!(untrust(&config, &keys), Trust::Wrote(1));
        assert_eq!(std::fs::read_to_string(&config).unwrap(), own);
        assert_eq!(untrust(&config, &keys), Trust::Already);
        std::fs::write(&config, "{\"n\": 1}").unwrap();
        assert_eq!(untrust(&config, &keys), Trust::Already);
        std::fs::write(&config, "not json").unwrap();
        assert!(matches!(untrust(&config, &keys), Trust::Failed(_)));
        assert!(!dir.join(".claude.json.limitpeeker.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "reads this PC's own Claude Code file, and writes nothing"]
    fn the_real_file_would_lose_only_the_hidden_folder() {
        // What --remove would write, read back: the same values but the
        // hidden folder's entries.
        let config = config().unwrap();
        let text = std::fs::read_to_string(&config).unwrap();
        let home = crate::settings::data_dir().unwrap().join("claude-home");
        let keys = keys(home.to_str().unwrap());
        let changed = untrusted(&text, &keys)
            .unwrap()
            .expect("entries to take out");
        let mut expected = json::parse(&text).unwrap();
        if let Json::Object(fields) = &mut expected {
            for (name, value) in fields.iter_mut() {
                if let (true, Json::Object(projects)) = (name == "projects", value) {
                    projects.retain(|(folder, _)| !keys.contains(folder));
                }
            }
        }
        assert_eq!(json::parse(&changed), Some(expected));
        println!(
            "{} bytes -> {}, {} lines -> {}",
            text.len(),
            changed.len(),
            text.lines().count(),
            changed.lines().count()
        );
    }

    #[test]
    fn claude_code_is_found_by_its_installer_then_in_the_editor_extension() {
        // Its own installer's place first; else the copy inside the VS Code
        // extension, the newest version where there are more than one.
        let dir = std::env::temp_dir().join(format!("limitpeeker-claude-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let put = |path: &str, age_secs: u64| {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, b"").unwrap();
            let when = std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs);
            std::fs::File::options()
                .write(true)
                .open(&file)
                .unwrap()
                .set_modified(when)
                .unwrap();
            file
        };
        let extension = r".vscode\extensions\anthropic.claude-code-";
        let native = r"-win32-x64\resources\native-binary\claude.exe";
        put(&format!("{extension}2.1.200{native}"), 600);
        let newer = put(&format!("{extension}2.1.258{native}"), 60);
        let root = |name: &str| (name == "USERPROFILE").then(|| dir.clone());
        assert_eq!(places::in_places(&PLACES, root), Some(newer));
        let installed = put(r".local\bin\claude.exe", 3600);
        assert_eq!(places::in_places(&PLACES, root), Some(installed));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(places::in_places(&PLACES, root), None);
    }

    #[test]
    fn npm_s_claude_code_is_its_platform_package_s_program_not_the_placeholder() {
        // The placeholder npm's package carries stays when its install
        // script is not run; the platform package's program is taken first,
        // and npm's own folder comes after the installer's place and before
        // the editor's extension.
        let dir =
            std::env::temp_dir().join(format!("limitpeeker-claude-npm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let put = |path: &str, length: usize| {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, vec![0u8; length]).unwrap();
            file
        };
        let package = r"npm\node_modules\@anthropic-ai\claude-code";
        let started = put(&format!(r"{package}\bin\claude.exe"), 500);
        assert_eq!(from_npm(&started), None);
        put(
            &format!(r"{package}\bin\claude.exe"),
            PROGRAM_LEAST as usize,
        );
        assert_eq!(from_npm(&started), Some(started.clone()));
        let platform = put(
            &format!(r"{package}\node_modules\@anthropic-ai\claude-code-win32-x64\claude.exe"),
            0,
        );
        assert_eq!(from_npm(&started), Some(platform.clone()));
        let extension = put(
            r"home\.vscode\extensions\anthropic.claude-code-2.1.258-win32-x64\resources\native-binary\claude.exe",
            0,
        );
        let root = |name: &str| match name {
            "APPDATA" => Some(dir.clone()),
            "USERPROFILE" => Some(dir.join("home")),
            _ => None,
        };
        assert_eq!(places::in_places(&PLACES, root), Some(platform));
        let installed = put(r"home\.local\bin\claude.exe", 0);
        assert_eq!(places::in_places(&PLACES, root), Some(installed.clone()));
        std::fs::remove_file(&installed).unwrap();
        std::fs::remove_dir_all(dir.join("npm")).unwrap();
        assert_eq!(places::in_places(&PLACES, root), Some(extension));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trust_goes_where_claude_code_keeps_its_file() {
        // CLAUDE_CONFIG_DIR moves Claude Code's file; an empty one does not.
        let at = |dir: Option<&str>, profile: Option<&str>| {
            config_in(dir.map(Into::into), profile.map(Into::into))
        };
        assert_eq!(
            at(Some(r"D:\cc"), Some(r"C:\Users\me")),
            Some(PathBuf::from(r"D:\cc\.claude.json"))
        );
        assert_eq!(
            at(None, Some(r"C:\Users\me")),
            Some(PathBuf::from(r"C:\Users\me\.claude.json"))
        );
        assert_eq!(
            at(Some(""), Some(r"C:\Users\me")),
            Some(PathBuf::from(r"C:\Users\me\.claude.json"))
        );
        assert_eq!(at(None, None), None);
    }

    #[test]
    fn the_hidden_claude_code_is_started_with_only_its_own_settings() {
        let text = settings(Path::new(r"C:\Program Files\LimitPeeker\limitpeeker.exe"));
        let back = json::parse(&text).unwrap();
        let line = back.get("statusLine").unwrap();
        assert_eq!(line.get("type").and_then(Json::text), Some("command"));
        assert_eq!(
            line.get("command").and_then(Json::text),
            Some("\"C:/Program Files/LimitPeeker/limitpeeker.exe\" --claude-statusline")
        );
        let file = Path::new(r"C:\x\claude-settings.json");
        assert_eq!(
            arguments(file, "id", true),
            [
                "--restricted",
                "--strict-mcp-config",
                "--settings",
                r"C:\x\claude-settings.json",
                "--session-id",
                "id"
            ]
        );
        assert_eq!(arguments(file, "id", false)[0], "--strict-mcp-config");
        let id = session_id().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
        assert_ne!(id, session_id().unwrap());
    }
}
