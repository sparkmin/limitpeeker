//! Codex's usage limits, as Codex's own app-server tells them. Codex signs in
//! and asks OpenAI; this side starts `codex.exe app-server` with no window,
//! asks for the rate limits over its standard input and output, and ends it.
//! No token is read, kept or sent from here.

use crate::job::Cost;
use crate::job::{self, Job};
use crate::json::{self, Json};
use crate::log::short;
use crate::places;
use crate::usage::{Found, Limit, Reading};
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HANDLE;

/// How long one read may take, from starting Codex to its answer.
const TIMEOUT: Duration = Duration::from_secs(20);
/// The longest line taken from Codex.
const LONGEST_LINE: u64 = 1 << 20;

/// Where Codex is put when it is not on the PATH, under a folder the
/// environment names, tried in this order. A `*` stands for any one folder
/// name, such as the Codex app's build or the extension's version.
const PLACES: [(&str, &str); 7] = [
    // The Codex app keeps its own copy.
    ("LOCALAPPDATA", r"OpenAI\Codex\bin\*\codex.exe"),
    // Codex's own installer.
    ("LOCALAPPDATA", r"Programs\OpenAI\Codex\bin\codex.exe"),
    (
        "USERPROFILE",
        r".codex\packages\standalone\current\bin\codex.exe",
    ),
    // npm's own folder for what it installs for everyone, its platform
    // package first, as NPM_PLATFORM.
    (
        "APPDATA",
        r"npm\node_modules\@openai\codex\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin\codex.exe",
    ),
    (
        "APPDATA",
        r"npm\node_modules\@openai\codex\vendor\x86_64-pc-windows-msvc\bin\codex.exe",
    ),
    // The VS Code extension.
    (
        "USERPROFILE",
        r".vscode\extensions\openai.chatgpt-*-win32-x64\bin\windows-x86_64\codex.exe",
    ),
    (
        "USERPROFILE",
        r".vscode-insiders\extensions\openai.chatgpt-*-win32-x64\bin\windows-x86_64\codex.exe",
    ),
];

/// npm's Codex keeps the program in a package for each platform, here
/// Windows x64, under its folder for that platform.
const NPM_PLATFORM: (&str, &str) = (
    "codex-win32-x64",
    r"vendor\x86_64-pc-windows-msvc\bin\codex.exe",
);

/// Where Codex is: what a person gets by typing `codex` on the PATH, a
/// `codex.exe` or npm's shim, or else the first of PLACES that has one.
pub fn find() -> Option<PathBuf> {
    places::typed(std::env::var_os("PATH"), "codex", from_npm)
        .or_else(|| places::in_places(&PLACES, places::environment))
}

/// The program behind npm's shim, from the script it starts
/// (`…\@openai\codex\bin\codex.js`): the platform package's, as that script
/// looks for it, then the one in the package's own vendor folder. The
/// program is started itself, so Node is not needed.
fn from_npm(script: &Path) -> Option<PathBuf> {
    let package = script.parent()?.parent()?;
    let (platform, inside) = NPM_PLATFORM;
    places::npm_platform(package, platform, inside)
        .or_else(|| Some(package.join(inside)).filter(|own| own.is_file()))
}

/// Codex with `args`, as its reads start it: in its folder, in the
/// person's own environment. Its update runs so too.
pub fn command(exe: &Path, args: &[&str]) -> Option<Command> {
    let mut command = Command::new(exe);
    command.args(args).current_dir(exe.parent()?);
    Some(command)
}

/// Why the Codex at `exe` is not updated here, or None when `codex update`
/// updates it: the one Codex's installer keeps in its releases, which the
/// installer's folder on the PATH leads to. A Codex npm installed is
/// started without npm's script, which tells Codex how it was installed, so
/// npm is left to update it; the Codex app and the VS Code extension update
/// their own.
pub fn kept_by(exe: &Path) -> Option<&'static str> {
    let full = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    if installed_by_its_installer(&full) {
        None
    } else if let Some(editor) = crate::cli_update::kept_by_editor(exe) {
        Some(editor)
    } else {
        Some(
            "is updated by what installed it; codex update updates only the Codex its own installer put in",
        )
    }
}

/// Whether `full`, with no links in it, is in the releases Codex's
/// installer keeps (`.codex\packages\standalone\releases`).
fn installed_by_its_installer(full: &Path) -> bool {
    let parts: Vec<String> = full
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => name.to_str().map(str::to_ascii_lowercase),
            _ => None,
        })
        .collect();
    parts
        .windows(3)
        .any(|three| three == ["packages", "standalone", "releases"])
}

/// Asks the Codex at `exe` for its rate limits, once: a session started,
/// asked once and ended.
pub fn read(exe: &Path) -> (Result<Found, String>, Cost) {
    let began = Instant::now();
    let mut session = match Session::spawn(exe) {
        Ok(session) => session,
        Err(why) => return (Err(why), Cost::default()),
    };
    let result = session.initialize().and_then(|()| session.ask());
    // Without a job, what Codex started may outlive it.
    let result = if session.job.held() {
        result
    } else {
        result
            .map(|mut found| {
                found.told.push_str(" (not in a job)");
                found
            })
            .map_err(|why| format!("{why} (not in a job)"))
    };
    let wall_ms = began.elapsed().as_secs_f64() * 1000.0;
    let cost = Cost {
        wall_ms,
        ..session.end()
    };
    (result, cost)
}

/// A running `codex.exe app-server` that has been greeted and can be asked.
/// Codex is in a job that ends it when the session is ended or dropped, and
/// when this process ends first; the job also counts what Codex used.
struct Session {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    replies: mpsc::Receiver<Result<Vec<u8>, String>>,
    next: u32,
    job: Job,
}

impl Session {
    /// Starts Codex, not yet greeted.
    fn spawn(exe: &Path) -> Result<Self, String> {
        let mut child = command(exe, &["app-server"])
            .ok_or("its folder is not known")?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(job::CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("could not start: {e}"))?;
        let job = Job::around(HANDLE(child.as_raw_handle()));
        let input = child.stdin.take();
        let replies = match child.stdout.take() {
            Some(output) => job::lines(output, LONGEST_LINE),
            None => mpsc::channel().1,
        };
        Ok(Self {
            child,
            input,
            replies,
            next: 1,
            job,
        })
    }
    fn send(&mut self, message: &str) -> Result<(), String> {
        let input = self.input.as_mut().ok_or("no input")?;
        input
            .write_all(format!("{message}\n").as_bytes())
            .and_then(|_| input.flush())
            .map_err(|e| format!("could not write: {e}"))
    }
    /// Asks `method`, and waits for its result.
    fn request(&mut self, method: &str, params: &str) -> Result<Json, String> {
        let id = self.next;
        self.next += 1;
        let params = if params.is_empty() {
            String::new()
        } else {
            format!(r#","params":{params}"#)
        };
        self.send(&format!(r#"{{"id":{id},"method":"{method}"{params}}}"#))?;
        answer(&self.replies, id, Instant::now() + TIMEOUT)
    }
    /// Greets Codex, which then takes requests.
    fn initialize(&mut self) -> Result<(), String> {
        self.request("initialize", &client_info())?;
        self.send(r#"{"method":"initialized"}"#)
    }
    /// Asks for the rate limits.
    fn ask(&mut self) -> Result<Found, String> {
        let reply = self.request("account/rateLimits/read", "")?;
        limits(&reply)
    }
    /// Ends Codex, and tells what it used in all.
    fn end(mut self) -> Cost {
        self.stop();
        self.job.cost()
    }
    fn stop(&mut self) {
        self.input = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The result of the request `id`, passing over what else Codex says.
fn answer(
    replies: &mpsc::Receiver<Result<Vec<u8>, String>>,
    id: u32,
    deadline: Instant,
) -> Result<Json, String> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = match replies.recv_timeout(left) {
            Ok(line) => line?,
            Err(mpsc::RecvTimeoutError::Timeout) => return Err("no answer in time".into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err("Codex ended".into()),
        };
        let Some(message) = std::str::from_utf8(&line).ok().and_then(json::parse) else {
            return Err("an answer that is not JSON".into());
        };
        if message.get("method").is_some() {
            continue;
        }
        if message.get("id").and_then(Json::number) != Some(f64::from(id)) {
            continue;
        }
        if let Some(error) = message.get("error") {
            let text = error
                .get("message")
                .and_then(Json::text)
                .unwrap_or("no message");
            return Err(format!("Codex refused: {}", short(text)));
        }
        return message
            .get("result")
            .cloned()
            .ok_or("an answer with no result".into());
    }
}

/// The limits in a `account/rateLimits/read` result: the primary window and
/// the secondary one, each labeled by its length.
pub fn limits(result: &Json) -> Result<Found, String> {
    let snapshot = result
        .get("rateLimits")
        .filter(|s| matches!(s, Json::Object(_)))
        .ok_or("no rate limits")?;
    let mut limits = Vec::new();
    let mut told = Vec::new();
    for (index, key) in ["primary", "secondary"].into_iter().enumerate() {
        let window = match snapshot.get(key) {
            None | Some(Json::Null) => continue,
            Some(window) => window,
        };
        let used = window
            .get("usedPercent")
            .and_then(Json::number)
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("{key} has no usedPercent"))?;
        let minutes = window.get("windowDurationMins").and_then(Json::number);
        let label = window_label(minutes, index);
        let used = used.round().clamp(0.0, 255.0) as u8;
        let resets = window
            .get("resetsAt")
            .and_then(Json::number)
            .filter(|at| at.is_finite())
            .map(|at| at.round() as i64);
        let told_resets = resets.map_or("unknown".to_string(), |at| at.to_string());
        told.push(format!("{label}={used}% resets_at={told_resets}"));
        limits.push(Limit {
            label,
            reading: Reading::Read(used),
            resets,
            amount: None,
        });
    }
    // No window at all is not a read: it would leave nothing to show and
    // no sign of a fault.
    if limits.is_empty() {
        return Err("no rate limit windows".into());
    }
    if let Some(plan) = snapshot.get("planType").and_then(Json::text) {
        told.push(format!("plan={}", short(plan)));
    }
    Ok(Found {
        limits,
        told: told.join(" "),
    })
}

/// A window's label from its length: days, hours or minutes, whichever is
/// whole. With no length, its place.
fn window_label(minutes: Option<f64>, index: usize) -> String {
    match minutes.filter(|m| m.is_finite() && *m >= 1.0 && m.fract() == 0.0) {
        Some(m) if m % 1440.0 == 0.0 => format!("{}d", m / 1440.0),
        Some(m) if m % 60.0 == 0.0 => format!("{}h", m / 60.0),
        Some(m) => format!("{m}m"),
        None => format!("L{}", index + 1),
    }
}

/// Who asks, as Codex's initialize takes it: this program, with the
/// version in Cargo.toml.
fn client_info() -> String {
    format!(
        r#"{{"clientInfo":{{"name":"limitpeeker","title":"LimitPeeker","version":"{}"}}}}"#,
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_is_told_this_version() {
        // Not a version written by hand, which stayed 0.1.0.
        let info = json::parse(&client_info()).expect("JSON");
        let client = info.get("clientInfo").expect("clientInfo");
        assert_eq!(client.get("name").and_then(Json::text), Some("limitpeeker"));
        assert_eq!(
            client.get("version").and_then(Json::text),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    fn read_limits(text: &str) -> Result<Found, String> {
        limits(&json::parse(text).unwrap())
    }

    #[test]
    fn only_the_codex_its_installer_put_in_is_updated_here() {
        // codex update updates the installer's releases; npm, the Codex app
        // and the editor update their own.
        let kept = |path: &str| kept_by(Path::new(path));
        assert_eq!(
            kept(
                r"C:\Users\me\.codex\packages\standalone\releases\0.163.0-x86_64-pc-windows-msvc\bin\codex.exe"
            ),
            None
        );
        assert!(
            kept(r"C:\Users\me\AppData\Local\OpenAI\Codex\bin\896a0166b774037a\codex.exe")
                .is_some()
        );
        assert!(
            kept(r"C:\Users\me\AppData\Roaming\npm\node_modules\@openai\codex\vendor\x86_64-pc-windows-msvc\bin\codex.exe")
                .is_some()
        );
        assert_eq!(
            kept(
                r"C:\Users\me\.vscode\extensions\openai.chatgpt-26.9-win32-x64\bin\windows-x86_64\codex.exe"
            ),
            Some(crate::cli_update::EDITOR)
        );
        // The installer's folder on the PATH is a link to the release it
        // installed, and is taken as that release.
        let dir = std::env::temp_dir().join(format!(
            "limitpeeker-codex-installed-{}",
            std::process::id()
        ));
        let release = dir.join(r"home\.codex\packages\standalone\releases\0.163.0\bin");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(release.join("codex.exe"), b"").unwrap();
        let programs = dir.join(r"local\Programs\OpenAI\Codex");
        std::fs::create_dir_all(&programs).unwrap();
        crate::cli_update::tests::junction(&programs.join("bin"), &release);
        assert_eq!(kept_by(&programs.join(r"bin\codex.exe")), None);
        std::fs::remove_dir(programs.join("bin")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn codex_windows_become_limits_labeled_by_length() {
        // Five hours and a week, as the schema of Codex 0.153.4 has them.
        let found = read_limits(
            r#"{"accountId": "someone", "rateLimits": {"limitId": "codex", "planType": "plus",
               "primary": {"usedPercent": 28, "windowDurationMins": 300, "resetsAt": 1790000000},
               "secondary": {"usedPercent": 42, "windowDurationMins": 10080, "resetsAt": null},
               "credits": null}, "rateLimitsByLimitId": null}"#,
        )
        .unwrap();
        let limits: Vec<_> = found
            .limits
            .iter()
            .map(|l| (l.label.as_str(), l.reading, l.resets))
            .collect();
        // When each starts over, for the popup.
        assert_eq!(
            limits,
            [
                ("5h", Reading::Read(28), Some(1_790_000_000)),
                ("7d", Reading::Read(42), None)
            ]
        );
        assert_eq!(
            found.told,
            "5h=28% resets_at=1790000000 7d=42% resets_at=unknown plan=plus"
        );
        // The account is never written down.
        assert!(!found.told.contains("someone"));
    }

    #[test]
    fn a_missing_window_is_no_limit_and_odd_ones_still_read() {
        let found = read_limits(
            r#"{"rateLimits": {"primary": {"usedPercent": 130.4, "windowDurationMins": 90},
               "secondary": null}}"#,
        )
        .unwrap();
        assert_eq!(found.limits.len(), 1);
        assert_eq!(found.limits[0].label, "90m");
        assert_eq!(found.limits[0].reading, Reading::Read(130));
        let found = read_limits(r#"{"rateLimits": {"primary": {"usedPercent": -3}}}"#).unwrap();
        assert_eq!(
            (found.limits[0].label.as_str(), found.limits[0].reading),
            ("L1", Reading::Read(0))
        );
        // Neither window is a failed read, not a read of nothing.
        for empty in [
            r#"{"rateLimits": {}}"#,
            r#"{"rateLimits": {"primary": null, "secondary": null}}"#,
        ] {
            assert_eq!(
                read_limits(empty).err().as_deref(),
                Some("no rate limit windows")
            );
        }
        for (minutes, label) in [(60.0, "1h"), (1440.0, "1d"), (2880.0, "2d"), (0.5, "L2")] {
            assert_eq!(window_label(Some(minutes), 1), label);
        }
    }

    #[test]
    fn a_reply_that_is_not_the_one_asked_for_fails() {
        for wrong in [
            r#"{}"#,
            r#"{"rateLimits": null}"#,
            r#"{"rateLimits": {"primary": {"windowDurationMins": 300}}}"#,
            r#"{"rateLimits": {"primary": {"usedPercent": "28"}}}"#,
        ] {
            assert!(read_limits(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn codex_is_looked_for_on_the_path_then_in_places_in_order() {
        // The PATH first, and only its absolute folders; then the first
        // place that has one, and the newest where a `*` matches more than
        // one.
        let dir = std::env::temp_dir().join(format!("limitpeeker-codex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let put = |path: &str, age_secs: u64| {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, b"").unwrap();
            let when = std::time::SystemTime::now() - Duration::from_secs(age_secs);
            std::fs::File::options()
                .write(true)
                .open(&file)
                .unwrap()
                .set_modified(when)
                .unwrap();
            file
        };
        let app_old = put(r"local\OpenAI\Codex\bin\aaaa\codex.exe", 600);
        let app_new = put(r"local\OpenAI\Codex\bin\bbbb\codex.exe", 60);
        let code = put(
            r"home\.vscode\extensions\openai.chatgpt-26.903.71938-win32-x64\bin\windows-x86_64\codex.exe",
            0,
        );
        put(
            r"home\.vscode\extensions\ms-python.python-1\bin\windows-x86_64\codex.exe",
            0,
        );
        let on = put(r"tools\codex.exe", 0);
        let root = |name: &str| match name {
            "LOCALAPPDATA" => Some(dir.join("local")),
            "USERPROFILE" => Some(dir.join("home")),
            _ => None,
        };
        let path = std::env::join_paths([
            PathBuf::from(r"relative\tools"),
            dir.join("empty"),
            dir.join("tools"),
        ])
        .unwrap();
        assert_eq!(places::typed(Some(path), "codex", from_npm), Some(on));
        assert_eq!(
            places::typed(Some(r"tools".into()), "codex", from_npm),
            None
        );
        assert_eq!(places::typed(None, "codex", from_npm), None);
        // The app comes before the extension, even when the extension's copy
        // is newer, and its newest build wins.
        assert_eq!(places::in_places(&PLACES, root), Some(app_new));
        assert_ne!(app_old, code);
        std::fs::remove_dir_all(dir.join(r"local\OpenAI")).unwrap();
        assert_eq!(places::in_places(&PLACES, root), Some(code));
        std::fs::remove_dir_all(dir.join(r"home\.vscode")).unwrap();
        assert_eq!(places::in_places(&PLACES, root), None);
        assert_eq!(
            places::in_places(&PLACES, |_| Some(PathBuf::from("home"))),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(places::like(
            "openai.chatgpt-*-win32-x64",
            "OpenAI.ChatGPT-26.9-win32-x64"
        ));
        assert!(!places::like(
            "openai.chatgpt-*-win32-x64",
            "openai.chatgpt-win32-x64"
        ));
        assert!(!places::like(
            "openai.chatgpt-*-win32-x64",
            "openai.chatgpt-26-win32-arm64"
        ));
    }

    #[test]
    fn npm_s_codex_is_the_program_in_its_platform_package() {
        // From the script npm's shim starts to the program: the platform
        // package's, else the package's own vendor folder. npm's own folder
        // comes after Codex's installers and before the editor's extension.
        let dir =
            std::env::temp_dir().join(format!("limitpeeker-codex-npm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let put = |path: &str| {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, b"").unwrap();
            file
        };
        let package = r"npm\node_modules\@openai\codex";
        let script = put(&format!(r"{package}\bin\codex.js"));
        assert_eq!(from_npm(&script), None);
        let own = put(&format!(
            r"{package}\vendor\x86_64-pc-windows-msvc\bin\codex.exe"
        ));
        assert_eq!(from_npm(&script), Some(own.clone()));
        let platform = put(&format!(
            r"{package}\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin\codex.exe"
        ));
        assert_eq!(from_npm(&script), Some(platform.clone()));
        let extension = put(
            r"home\.vscode\extensions\openai.chatgpt-26.9-win32-x64\bin\windows-x86_64\codex.exe",
        );
        let root = |name: &str| match name {
            "APPDATA" => Some(dir.clone()),
            "USERPROFILE" => Some(dir.join("home")),
            _ => None,
        };
        assert_eq!(places::in_places(&PLACES, root), Some(platform));
        std::fs::remove_dir_all(dir.join(format!(r"{package}\node_modules"))).unwrap();
        assert_eq!(places::in_places(&PLACES, root), Some(own));
        std::fs::remove_dir_all(dir.join("npm")).unwrap();
        assert_eq!(places::in_places(&PLACES, root), Some(extension));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn answers_are_matched_to_their_request() {
        // Codex may say other things first; only the answer to the request
        // counts, and an error is told apart.
        let (send, replies) = mpsc::channel();
        for line in [
            r#"{"method":"account/rateLimits/updated","params":{}}"#,
            r#"{"id":1,"result":{"userAgent":"x"}}"#,
            r#"{"id":2,"method":"item/tool/call","params":{}}"#,
            r#"{"id":2,"result":{"rateLimits":{}}}"#,
            r#"{"id":3,"error":{"code":-32600,"message":"not signed in"}}"#,
        ] {
            send.send(Ok(line.as_bytes().to_vec())).unwrap();
        }
        let soon = Instant::now() + Duration::from_millis(200);
        assert!(
            answer(&replies, 2, soon)
                .unwrap()
                .get("rateLimits")
                .is_some()
        );
        assert_eq!(
            answer(&replies, 3, soon).unwrap_err(),
            "Codex refused: not signed in"
        );
        send.send(Ok(b"not json".to_vec())).unwrap();
        assert!(answer(&replies, 4, soon).is_err());
        assert_eq!(answer(&replies, 4, soon).unwrap_err(), "no answer in time");
        drop(send);
        assert_eq!(answer(&replies, 4, soon).unwrap_err(), "Codex ended");
    }
}
