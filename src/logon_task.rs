//! Task Scheduler tasks a person made to start LimitPeeker at sign-in. The
//! product does not make them; it finds them, so the settings window and
//! the log can say how it starts.

use std::path::{Path, PathBuf};
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::TaskScheduler::*;
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, HSTRING, Interface};

/// The program's file name, which a task that starts it runs.
const PROGRAM: &str = "limitpeeker.exe";
/// The folder of Windows' own tasks, hundreds of them, not looked in.
const WINDOWS_OWN: &str = "Microsoft";

/// A task that starts a `limitpeeker.exe` when a person signs in.
#[derive(Clone, Debug, PartialEq)]
pub struct LogonTask {
    /// Where it is in Task Scheduler's library, as `\LimitPeeker`.
    pub path: String,
    /// The program it starts, as written in it.
    pub program: String,
    pub enabled: bool,
    /// Whether that program is this EXE.
    pub this_exe: bool,
    pub problem: Problem,
}

/// What keeps a task from starting LimitPeeker where it can be seen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Problem {
    None,
    /// It runs with highest privileges, as administrator, and the box
    /// does not attach then.
    Admin,
    /// It runs whether the person is signed in or not, in a session
    /// nobody sees, with no tray and no box.
    Hidden,
}

impl Problem {
    /// The word the settings window is sent.
    pub fn word(self) -> &'static str {
        match self {
            Problem::None => "",
            Problem::Admin => "admin",
            Problem::Hidden => "hidden",
        }
    }
}

/// The tasks in Task Scheduler's library, outside Windows' own folder,
/// that start a `limitpeeker.exe` at sign-in. Tasks and folders it may not
/// read are left out, and so is everything when Task Scheduler cannot be
/// asked.
pub fn find(exe: &Path) -> Vec<LogonTask> {
    find_starting(exe, PROGRAM)
}

/// The same, for tasks that start a program of the file name `name`.
fn find_starting(exe: &Path, name: &str) -> Vec<LogonTask> {
    unsafe {
        // Each one made balances one ended, whether it was made here or
        // was there already.
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let mut found = Vec::new();
        if let Ok(service) =
            CoCreateInstance::<_, ITaskService>(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
        {
            let none = VARIANT::default();
            if service.Connect(&none, &none, &none, &none).is_ok()
                && let Ok(root) = service.GetFolder(&BSTR::from("\\"))
            {
                look_in(&root, exe, name, true, &mut found);
            }
        }
        if com {
            CoUninitialize();
        }
        found
    }
}

unsafe fn look_in(
    folder: &ITaskFolder,
    exe: &Path,
    name: &str,
    top: bool,
    found: &mut Vec<LogonTask>,
) {
    unsafe {
        if let Ok(tasks) = folder.GetTasks(TASK_ENUM_HIDDEN.0) {
            for at in 1..=tasks.Count().unwrap_or(0) {
                if let Ok(task) = tasks.get_Item(&VARIANT::from(at))
                    && let Some(task) = read(&task, exe, name)
                {
                    found.push(task);
                }
            }
        }
        if let Ok(folders) = folder.GetFolders(0) {
            for at in 1..=folders.Count().unwrap_or(0) {
                if let Ok(inner) = folders.get_Item(&VARIANT::from(at))
                    && !(top && inner.Name().is_ok_and(|n| n.to_string() == WINDOWS_OWN))
                {
                    look_in(&inner, exe, name, false, found);
                }
            }
        }
    }
}

/// The task, if it starts a program of the file name `name` at sign-in.
unsafe fn read(task: &IRegisteredTask, exe: &Path, name: &str) -> Option<LogonTask> {
    unsafe {
        let definition = task.Definition().ok()?;
        let actions = definition.Actions().ok()?;
        let mut count = 0;
        actions.Count(&mut count).ok()?;
        let program = (1..=count).find_map(|at| {
            let action = actions.get_Item(at).ok()?;
            let mut kind = TASK_ACTION_TYPE::default();
            action.Type(&mut kind).ok()?;
            if kind != TASK_ACTION_EXEC {
                return None;
            }
            let mut path = BSTR::new();
            action.cast::<IExecAction>().ok()?.Path(&mut path).ok()?;
            let path = path.to_string();
            starts(&path, name).then_some(path)
        })?;
        let triggers = definition.Triggers().ok()?;
        let mut count = 0;
        triggers.Count(&mut count).ok()?;
        let at_sign_in = (1..=count).any(|at| {
            let mut kind = TASK_TRIGGER_TYPE2::default();
            triggers
                .get_Item(at)
                .and_then(|trigger| trigger.Type(&mut kind))
                .is_ok()
                && kind == TASK_TRIGGER_LOGON
        });
        if !at_sign_in {
            return None;
        }
        let (mut level, mut logon) = (TASK_RUNLEVEL_TYPE::default(), TASK_LOGON_TYPE::default());
        if let Ok(principal) = definition.Principal() {
            let _ = principal.RunLevel(&mut level);
            let _ = principal.LogonType(&mut logon);
        }
        Some(LogonTask {
            path: task.Path().map(|p| p.to_string()).unwrap_or_default(),
            this_exe: same_file(&expanded(&program), exe),
            program,
            enabled: task.Enabled().is_ok_and(|on| on != VARIANT_BOOL(0)),
            problem: problem(level, logon),
        })
    }
}

/// Whether a task's program has the file name `name`, wherever it is.
fn starts(path: &str, name: &str) -> bool {
    unquoted(path)
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|file| file.eq_ignore_ascii_case(name))
}

fn unquoted(path: &str) -> &str {
    path.trim().trim_matches('"')
}

/// A task's program as a path, with its environment variables filled in.
fn expanded(path: &str) -> PathBuf {
    let path = unquoted(path);
    let source = HSTRING::from(path);
    let mut text = vec![0u16; 1024];
    let length = unsafe { ExpandEnvironmentStringsW(&source, Some(&mut text)) } as usize;
    if length == 0 || length > text.len() {
        return PathBuf::from(path);
    }
    PathBuf::from(String::from_utf16_lossy(&text[..length - 1]))
}

/// Windows paths name the same file in any case.
fn same_file(a: &Path, b: &Path) -> bool {
    a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
}

/// What a task's principal keeps it from: highest privileges first, as it
/// keeps the box off even where it can be seen.
fn problem(level: TASK_RUNLEVEL_TYPE, logon: TASK_LOGON_TYPE) -> Problem {
    if level == TASK_RUNLEVEL_HIGHEST {
        Problem::Admin
    } else if matches!(logon, TASK_LOGON_PASSWORD | TASK_LOGON_S4U) {
        Problem::Hidden
    } else {
        Problem::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_is_known_by_the_program_it_starts() {
        for path in [
            r"D:\Tools\LimitPeeker\limitpeeker.exe",
            r#""C:\My Tools\LimitPeeker.EXE""#,
            "%LOCALAPPDATA%/LimitPeeker/limitpeeker.exe",
            "limitpeeker.exe",
        ] {
            assert!(starts(path, PROGRAM), "{path}");
        }
        for path in [
            r"C:\Tools\limitpeeker.exe.bak",
            r"C:\Tools\notlimitpeeker.exe",
            r"C:\limitpeeker.exe\other.exe",
            "",
        ] {
            assert!(!starts(path, PROGRAM), "{path}");
        }
    }

    #[test]
    fn its_program_is_compared_with_this_exe_as_windows_would() {
        let exe = Path::new(r"C:\Tools\LimitPeeker\limitpeeker.exe");
        assert!(same_file(
            &expanded(r#""C:\TOOLS\LimitPeeker\LIMITPEEKER.exe""#),
            exe
        ));
        assert!(!same_file(&expanded(r"E:\limitpeeker.exe"), exe));
        // Its environment variables are filled in first.
        let windows = std::env::var("SystemRoot").unwrap();
        assert_eq!(
            expanded(r"%SystemRoot%\limitpeeker.exe"),
            Path::new(&windows).join("limitpeeker.exe")
        );
    }

    #[test]
    fn a_task_that_keeps_the_box_away_says_why() {
        use windows::Win32::System::TaskScheduler::TASK_RUNLEVEL_LUA;
        assert_eq!(
            problem(TASK_RUNLEVEL_LUA, TASK_LOGON_INTERACTIVE_TOKEN),
            Problem::None
        );
        assert_eq!(
            problem(TASK_RUNLEVEL_HIGHEST, TASK_LOGON_INTERACTIVE_TOKEN),
            Problem::Admin
        );
        assert_eq!(
            problem(TASK_RUNLEVEL_LUA, TASK_LOGON_PASSWORD),
            Problem::Hidden
        );
        assert_eq!(problem(TASK_RUNLEVEL_LUA, TASK_LOGON_S4U), Problem::Hidden);
        assert_eq!(
            problem(TASK_RUNLEVEL_HIGHEST, TASK_LOGON_S4U),
            Problem::Admin
        );
        assert_eq!(
            [Problem::None, Problem::Admin, Problem::Hidden].map(Problem::word),
            ["", "admin", "hidden"]
        );
    }

    #[test]
    #[ignore = "reads this PC's Task Scheduler library, and changes nothing"]
    fn this_pcs_tasks_are_read() {
        let start = std::time::Instant::now();
        let exe = std::env::current_exe().unwrap();
        let found = find(&exe);
        println!(
            "{} task(s) in {} ms: {found:?}",
            found.len(),
            start.elapsed().as_millis()
        );
        // Another program's sign-in tasks, to see the library walked.
        if let Ok(name) = std::env::var("LIMITPEEKER_TASK_PROGRAM") {
            println!("{name}: {:?}", find_starting(&exe, &name));
        }
    }
}
