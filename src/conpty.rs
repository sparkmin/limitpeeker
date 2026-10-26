//! A program run in a pseudo console with no window. What it writes is read
//! all the time and the end of it kept; what is typed is written to it; the
//! few things a terminal is asked are answered. The program is started
//! suspended and put in a job before it runs, so it and whatever it starts
//! end with this process.

use crate::job::{Cost, Job};
use std::ffi::{OsStr, OsString, c_void};
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::time::Duration;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_TIMEOUT};
use windows::Win32::System::Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, ResumeThread, STARTUPINFOEXW,
    TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR};

/// How much of what the program wrote is kept, from the end.
const KEPT: usize = 64 * 1024;
/// How far back a question to the terminal is looked for, so one split
/// across two reads is still seen.
const OVERLAP: usize = 16;
/// How long ending waits for the program, and then for the last output.
const ENDING: Duration = Duration::from_secs(3);

/// What a terminal is asked, and the answer given: where the cursor is, what
/// kind of terminal it is, and the text and background colors. A program
/// that asks may wait for the answer.
const ANSWERS: [(&[u8], &[u8]); 5] = [
    (b"\x1b[6n", b"\x1b[1;1R"),
    (b"\x1b[c", b"\x1b[?1;2c"),
    (b"\x1b[0c", b"\x1b[?1;2c"),
    (b"\x1b]10;?", b"\x1b]10;rgb:cccc/cccc/cccc\x1b\\"),
    (b"\x1b]11;?", b"\x1b]11;rgb:0c0c/0c0c/0c0c\x1b\\"),
];

#[derive(Default)]
struct Screen {
    kept: Vec<u8>,
}

pub struct Console {
    console: HPCON,
    process: HANDLE,
    input: Arc<Mutex<File>>,
    screen: Arc<Mutex<Screen>>,
    /// Told when the reader has read the last of the output.
    read_all: mpsc::Receiver<()>,
    job: Job,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Console {
    /// Starts `exe` with `args` in `folder`, in a console `size` columns by
    /// rows, with the environment of this process less `removed` and with
    /// `added`.
    pub fn start(
        exe: &Path,
        args: &[String],
        folder: &Path,
        removed: &[&str],
        added: &[(&str, &str)],
        size: (i16, i16),
    ) -> Result<Self, String> {
        unsafe {
            let (mut their_input, mut our_input) = (HANDLE::default(), HANDLE::default());
            let (mut our_output, mut their_output) = (HANDLE::default(), HANDLE::default());
            CreatePipe(&mut their_input, &mut our_input, None, 0)
                .map_err(|e| format!("could not make a pipe: {e}"))?;
            let input = File::from_raw_handle(our_input.0 as RawHandle);
            if let Err(e) = CreatePipe(&mut our_output, &mut their_output, None, 0) {
                let _ = CloseHandle(their_input);
                return Err(format!("could not make a pipe: {e}"));
            }
            let output = File::from_raw_handle(our_output.0 as RawHandle);
            let console = CreatePseudoConsole(
                COORD {
                    X: size.0,
                    Y: size.1,
                },
                their_input,
                their_output,
                0,
            );
            // The console holds its own copies of these; with ours closed,
            // the output ends when the console does.
            let _ = CloseHandle(their_input);
            let _ = CloseHandle(their_output);
            let console = console.map_err(|e| format!("could not make a console: {e}"))?;
            let input = Arc::new(Mutex::new(input));
            let screen = Arc::new(Mutex::new(Screen::default()));
            let read_all = read(output, Arc::clone(&input), Arc::clone(&screen));
            match spawn(console, exe, args, folder, removed, added) {
                Ok((process, job)) => Ok(Self {
                    console,
                    process,
                    input,
                    screen,
                    read_all,
                    job,
                }),
                Err(why) => {
                    ClosePseudoConsole(console);
                    let _ = read_all.recv_timeout(ENDING);
                    Err(why)
                }
            }
        }
    }
    /// Writes to the program as typed keys: text, `\r` for Enter, `\x1b`
    /// for Esc.
    pub fn type_keys(&self, keys: &[u8]) -> Result<(), String> {
        let mut input = locked(&self.input);
        input
            .write_all(keys)
            .and_then(|()| input.flush())
            .map_err(|e| format!("could not type: {e}"))
    }
    pub fn running(&self) -> bool {
        unsafe { WaitForSingleObject(self.process, 0) == WAIT_TIMEOUT }
    }
    /// The end of what the program wrote, as it came.
    pub fn kept(&self) -> Vec<u8> {
        locked(&self.screen).kept.clone()
    }
    /// Ends the program and all it started, and tells what they used.
    pub fn end(self) -> Cost {
        self.job.end();
        unsafe {
            WaitForSingleObject(self.process, ENDING.as_millis() as u32);
        }
        self.job.cost()
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        self.job.end();
        unsafe {
            ClosePseudoConsole(self.console);
            let _ = self.read_all.recv_timeout(ENDING);
            let _ = CloseHandle(self.process);
        }
    }
}

/// Reads the output until it ends, keeping its end and answering what the
/// terminal is asked. Output left unread fills the pipe and stops the
/// program.
fn read(
    mut output: File,
    input: Arc<Mutex<File>>,
    screen: Arc<Mutex<Screen>>,
) -> mpsc::Receiver<()> {
    let (done, read_all) = mpsc::channel();
    std::thread::spawn(move || {
        let mut chunk = vec![0u8; 8192];
        loop {
            let n = match output.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let replies = {
                let mut screen = locked(&screen);
                let from = screen.kept.len().saturating_sub(OVERLAP);
                let replies = answers(&screen.kept[from..], &chunk[..n]);
                screen.kept.extend_from_slice(&chunk[..n]);
                if screen.kept.len() > KEPT {
                    let cut = screen.kept.len() - KEPT;
                    screen.kept.drain(..cut);
                }
                replies
            };
            if !replies.is_empty() {
                let mut input = locked(&input);
                for reply in replies {
                    let _ = input.write_all(reply);
                }
                let _ = input.flush();
            }
        }
        let _ = done.send(());
    });
    read_all
}

/// The answers to the questions that end in `chunk`, in the order asked.
/// `before` is what came just before it, so a question split across two
/// reads is answered once.
fn answers(before: &[u8], chunk: &[u8]) -> Vec<&'static [u8]> {
    let joined = [before, chunk].concat();
    let mut found: Vec<(usize, &'static [u8])> = Vec::new();
    for (question, answer) in ANSWERS {
        for end in question.len()..=joined.len() {
            if end > before.len() && joined[end - question.len()..end] == *question {
                found.push((end, answer));
            }
        }
    }
    found.sort_by_key(|&(end, _)| end);
    found.into_iter().map(|(_, answer)| answer).collect()
}

/// Starts the program suspended on the console, puts it in a job and lets
/// it run. It is not run at all without the job.
unsafe fn spawn(
    console: HPCON,
    exe: &Path,
    args: &[String],
    folder: &Path,
    removed: &[&str],
    added: &[(&str, &str)],
) -> Result<(HANDLE, Job), String> {
    unsafe {
        let mut size = 0usize;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        let mut list = vec![0usize; size.div_ceil(size_of::<usize>())];
        let attributes = LPPROC_THREAD_ATTRIBUTE_LIST(list.as_mut_ptr().cast());
        InitializeProcThreadAttributeList(Some(attributes), 1, None, &mut size)
            .map_err(|e| format!("could not list attributes: {e}"))?;
        let started: Result<PROCESS_INFORMATION, String> = (|| {
            UpdateProcThreadAttribute(
                attributes,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                Some(console.0 as *const c_void),
                size_of::<HPCON>(),
                None,
                None,
            )
            .map_err(|e| format!("could not attach the console: {e}"))?;
            let mut info = STARTUPINFOEXW::default();
            info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
            info.lpAttributeList = attributes;
            let wide = |text: &OsStr| -> Vec<u16> { text.encode_wide().chain([0]).collect() };
            let application = wide(exe.as_os_str());
            let mut line = crate::message::wide(&command_line(exe, args));
            let environment = environment(std::env::vars_os(), removed, added);
            let folder = wide(folder.as_os_str());
            let mut process = PROCESS_INFORMATION::default();
            CreateProcessW(
                PCWSTR(application.as_ptr()),
                Some(PWSTR(line.as_mut_ptr())),
                None,
                None,
                false,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED,
                Some(environment.as_ptr().cast()),
                PCWSTR(folder.as_ptr()),
                &info.StartupInfo,
                &mut process,
            )
            .map_err(|e| format!("could not start: {e}"))?;
            Ok(process)
        })();
        DeleteProcThreadAttributeList(attributes);
        let process = started?;
        let job = Job::around(process.hProcess);
        if !job.held() {
            let _ = TerminateProcess(process.hProcess, 1);
            let _ = CloseHandle(process.hThread);
            let _ = CloseHandle(process.hProcess);
            return Err("could not put it in a job".into());
        }
        ResumeThread(process.hThread);
        let _ = CloseHandle(process.hThread);
        Ok((process.hProcess, job))
    }
}

/// The command line for `exe` and `args`, quoted as programs split it.
fn command_line(exe: &Path, args: &[String]) -> String {
    std::iter::once(exe.to_string_lossy().into_owned())
        .chain(args.iter().cloned())
        .map(|arg| quoted(&arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// One argument, quoted when it must be: backslashes are doubled only
/// before a quote, which is itself escaped.
fn quoted(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from('"');
    let mut slashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        let doubled = if c == '"' { slashes * 2 + 1 } else { slashes };
        out.push_str(&"\\".repeat(doubled));
        out.push(c);
        slashes = 0;
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

/// An environment block: `vars` less `removed` and any also in `added`,
/// then `added`, sorted by name without case, each `name=value` and a
/// null, and one more null at the end.
fn environment(
    vars: impl Iterator<Item = (OsString, OsString)>,
    removed: &[&str],
    added: &[(&str, &str)],
) -> Vec<u16> {
    let named = |name: &OsStr, other: &str| name.eq_ignore_ascii_case(other);
    let mut all: Vec<(OsString, OsString)> = vars
        .filter(|(name, _)| {
            !removed.iter().any(|r| named(name, r)) && !added.iter().any(|(a, _)| named(name, a))
        })
        .chain(added.iter().map(|(n, v)| (n.into(), v.into())))
        .collect();
    all.sort_by_key(|(name, _)| name.to_string_lossy().to_uppercase());
    let mut block = Vec::new();
    for (name, value) in all {
        block.extend(name.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_to_the_terminal_are_answered_once_in_order() {
        // A program that asks may wait; one split across reads is still
        // answered, and one answered before is not again.
        assert_eq!(
            answers(b"", b"a\x1b[6nb\x1b]11;?\x07c\x1b[c"),
            [
                &b"\x1b[1;1R"[..],
                b"\x1b]11;rgb:0c0c/0c0c/0c0c\x1b\\",
                b"\x1b[?1;2c"
            ]
        );
        assert_eq!(answers(b"xx\x1b[", b"6n"), [&b"\x1b[1;1R"[..]]);
        assert!(answers(b"\x1b[6n", b"plain").is_empty());
        assert!(answers(b"", b"\x1b[1;2H\x1b[?25l").is_empty());
    }

    #[test]
    fn arguments_are_quoted_as_programs_split_them() {
        assert_eq!(quoted("plain"), "plain");
        assert_eq!(quoted(""), "\"\"");
        assert_eq!(quoted(r"C:\a b\c.json"), r#""C:\a b\c.json""#);
        assert_eq!(quoted(r"C:\a b\"), r#""C:\a b\\""#);
        assert_eq!(quoted(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quoted(r#"a\"b"#), r#""a\\\"b""#);
        assert_eq!(
            command_line(
                Path::new(r"C:\Program Files\c.exe"),
                &["--x".into(), "y z".into()]
            ),
            r#""C:\Program Files\c.exe" --x "y z""#
        );
    }

    #[test]
    fn the_environment_is_changed_and_sorted() {
        let vars = [
            ("Path", r"C:\x"),
            ("CLAUDECODE", "1"),
            ("b", "2"),
            ("KEEP", "old"),
        ]
        .into_iter()
        .map(|(n, v)| (OsString::from(n), OsString::from(v)));
        let block = environment(vars, &["claudecode"], &[("keep", "new"), ("A", "1")]);
        let text = String::from_utf16(&block).unwrap();
        assert_eq!(text, "A=1\0b=2\0keep=new\0Path=C:\\x\0\0");
    }
}
