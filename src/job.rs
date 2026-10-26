//! A job for the programs this one starts: it ends them when it is closed or
//! when this process ends first, and it counts what they used, running or
//! ended. Codex and the hidden Claude Code each run in one.
//!
//! Also the job this run itself was started in, for the log.

use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc;
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE};
use windows::Win32::Storage::Packaging::Appx::GetPackageFamilyName;
use windows::Win32::System::JobObjects::*;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::PWSTR;

/// No console window for a program started here.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// How many of the processes in this run's own job are looked at, at most.
const MOST_AROUND: usize = 1024;

/// The ids of a job's processes, at most `N` of them, and how many it has.
#[repr(C)]
struct Listed<const N: usize> {
    assigned: u32,
    listed: u32,
    ids: [usize; N],
}
impl<const N: usize> Listed<N> {
    /// The ids listed of `job`, or of this run's own job with None. A job of
    /// more processes than `N` lists the first `N`.
    fn of(job: Option<HANDLE>) -> Option<Box<Self>> {
        let mut list = Box::new(Self {
            assigned: 0,
            listed: 0,
            ids: [0; N],
        });
        let read = unsafe {
            QueryInformationJobObject(
                job,
                JobObjectBasicProcessIdList,
                (&raw mut *list).cast(),
                size_of::<Self>() as u32,
                None,
            )
        };
        (read.is_ok() || list.listed > 0).then_some(list)
    }
    fn ids(&self) -> &[usize] {
        &self.ids[..(self.listed as usize).min(N)]
    }
}

/// A packaged app's process in this run's job: its package's family name,
/// its file name and its id.
#[derive(Debug, PartialEq)]
struct Packaged {
    family: String,
    image: String,
    pid: u32,
}

/// The job this run was started in, for the log's line at the start. A
/// program started from inside a packaged app, such as a terminal that app
/// hosts, is in that app's job, and Windows ends every process in it, with
/// no word to them, when the package is updated.
pub fn around() -> String {
    unsafe {
        let mut inside = windows::core::BOOL(0);
        if IsProcessInJob(GetCurrentProcess(), None, &mut inside).is_err() || !inside.as_bool() {
            return told_around(None, None, false);
        }
        let own = GetCurrentProcessId() as usize;
        let list = Listed::<MOST_AROUND>::of(None);
        let packaged = list.as_ref().and_then(|list| {
            list.ids()
                .iter()
                .filter(|&&pid| pid != own)
                .find_map(|&pid| packaged(pid as u32))
        });
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let ends_with_job = QueryInformationJobObject(
            None,
            JobObjectExtendedLimitInformation,
            (&raw mut limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            None,
        )
        .is_ok()
            && limits
                .BasicLimitInformation
                .LimitFlags
                .contains(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
        told_around(
            Some(list.map_or(0, |list| list.assigned)),
            packaged,
            ends_with_job,
        )
    }
}

/// The package and file name of process `pid`, when it is a packaged app's.
fn packaged(pid: u32) -> Option<Packaged> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut family = [0u16; 256];
        let mut length = family.len() as u32;
        let found = GetPackageFamilyName(process, &mut length, Some(PWSTR(family.as_mut_ptr())))
            == ERROR_SUCCESS;
        let mut image = [0u16; 1024];
        let mut image_length = image.len() as u32;
        let named = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(image.as_mut_ptr()),
            &mut image_length,
        )
        .is_ok();
        let _ = CloseHandle(process);
        if !found {
            return None;
        }
        // The length counts the closing zero.
        let family = String::from_utf16_lossy(&family[..(length as usize).saturating_sub(1)]);
        let image = if named {
            let path = String::from_utf16_lossy(&image[..image_length as usize]);
            path.rsplit('\\').next().unwrap_or_default().to_string()
        } else {
            "?".into()
        };
        Some(Packaged { family, image, pid })
    }
}

/// The log's line for this run's job: none, or how many processes it has,
/// the packaged app's whose update ends this run, and whether closing the
/// job ends it.
fn told_around(processes: Option<u32>, packaged: Option<Packaged>, ends_with_job: bool) -> String {
    let Some(processes) = processes else {
        return "job: none".into();
    };
    let mut told = if processes > 0 {
        format!("job: in a job of {processes} processes")
    } else {
        "job: in a job".to_string()
    };
    if let Some(p) = packaged {
        told += &format!(
            " with package {} ({} {}); Windows ends this run when that package is updated",
            p.family, p.image, p.pid
        );
    }
    if ends_with_job {
        told += "; it ends with the job";
    }
    told
}

/// What a program cost: how long it took, and the CPU time and most memory
/// that it and anything it started used.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cost {
    pub wall_ms: f64,
    pub user_ms: f64,
    pub kernel_ms: f64,
    pub peak_mb: f64,
}
impl Cost {
    pub fn cpu_ms(&self) -> f64 {
        self.user_ms + self.kernel_ms
    }
}

/// A job that ends the processes in it when it is closed.
pub struct Job(Option<HANDLE>);
impl Job {
    /// A job around `process`. None inside when the job could not be made;
    /// the process then runs as it would have.
    pub fn around(process: HANDLE) -> Self {
        unsafe {
            let Ok(job) = CreateJobObjectW(None, windows::core::PCWSTR::null()) else {
                return Self(None);
            };
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if set.is_err() || AssignProcessToJobObject(job, process).is_err() {
                let _ = CloseHandle(job);
                return Self(None);
            }
            Self(Some(job))
        }
    }
    /// Whether the process is in a job at all.
    pub fn held(&self) -> bool {
        self.0.is_some()
    }
    /// The CPU time and the most committed memory of every process that has
    /// been in the job, running or ended.
    pub fn cost(&self) -> Cost {
        let Some(job) = self.0 else {
            return Cost::default();
        };
        unsafe {
            let mut times = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            let timed = QueryInformationJobObject(
                Some(job),
                JobObjectBasicAccountingInformation,
                (&raw mut times).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                None,
            );
            let sized = QueryInformationJobObject(
                Some(job),
                JobObjectExtendedLimitInformation,
                (&raw mut limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                None,
            );
            // Times are in units of 100 ns.
            let ms = |t: i64| t as f64 / 10_000.0;
            Cost {
                wall_ms: 0.0,
                user_ms: if timed.is_ok() {
                    ms(times.TotalUserTime)
                } else {
                    0.0
                },
                kernel_ms: if timed.is_ok() {
                    ms(times.TotalKernelTime)
                } else {
                    0.0
                },
                peak_mb: if sized.is_ok() {
                    limits.PeakJobMemoryUsed as f64 / (1024.0 * 1024.0)
                } else {
                    0.0
                },
            }
        }
    }
    /// Ends every process in the job now.
    pub fn end(&self) {
        if let Some(job) = self.0 {
            unsafe {
                let _ = TerminateJobObject(job, 1);
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if let Some(job) = self.0.take() {
            unsafe {
                let _ = CloseHandle(job);
            }
        }
    }
}

/// What a started program writes, line by line, as it comes, read on a
/// thread of its own: each line with its `\n`, then an error once a line
/// runs past `longest` bytes. The channel closes when the program's output
/// does.
pub fn lines(
    output: impl Read + Send + 'static,
    longest: u64,
) -> mpsc::Receiver<Result<Vec<u8>, String>> {
    let (lines, replies) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(output);
        loop {
            let mut line = Vec::new();
            match (&mut reader).take(longest).read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) if !line.ends_with(b"\n") && line.len() as u64 >= longest => {
                    let _ = lines.send(Err("a line too long".to_string()));
                    break;
                }
                Ok(_) => {
                    if lines.send(Ok(line)).is_err() {
                        break;
                    }
                }
            }
        }
    });
    replies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_job_a_run_is_in_is_told_with_the_package_that_ends_it() {
        // None, a job alone, and a packaged app's job, whose update ends
        // the run; a job that ends its processes when closed says so too.
        assert_eq!(told_around(None, None, false), "job: none");
        assert_eq!(
            told_around(Some(3), None, false),
            "job: in a job of 3 processes"
        );
        assert_eq!(
            told_around(Some(0), None, true),
            "job: in a job; it ends with the job"
        );
        let app = Packaged {
            family: "Contoso.Terminal_8wekyb3d8bbwe".into(),
            image: "terminal.exe".into(),
            pid: 4321,
        };
        assert_eq!(
            told_around(Some(25), Some(app), false),
            "job: in a job of 25 processes with package Contoso.Terminal_8wekyb3d8bbwe \
             (terminal.exe 4321); Windows ends this run when that package is updated"
        );
    }

    #[test]
    fn the_test_run_reads_its_own_job() {
        // Whatever starts the tests, the line is one of the three kinds.
        let told = around();
        assert!(
            told == "job: none" || told.starts_with("job: in a job"),
            "{told}"
        );
    }
}
