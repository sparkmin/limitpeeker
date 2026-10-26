//! Attaching to Explorer: the hook loaded through XAML diagnostics, and what
//! holds an attach back, the Snap layouts thread and administrator rights,
//! or lets it skip, a resting hook.

use super::*;
use crate::log::{Log, log};
use crate::shell::Shell;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;

use windows::Win32::Security::*;
use windows::Win32::System::Diagnostics::ToolHelp::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::*;

use windows::core::{GUID, PCSTR, PCWSTR, w};

/// The name Windows gives the Explorer thread that hosts the Snap layouts
/// flyout. Explorer starts it the first time a maximize button is hovered,
/// and keeps it.
pub(super) const SNAP_FLYOUT_THREAD: &str = "SnapFlyoutHost Thread";

/// Whether the process has the Snap layouts thread; None when its threads
/// cannot be read.
pub(super) fn snap_flyout(pid: u32) -> Option<bool> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).ok()?;
        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut found = false;
        let mut more = Thread32First(snapshot, &mut entry).is_ok();
        while more && !found {
            if entry.th32OwnerProcessID == pid
                && let Ok(thread) =
                    OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, entry.th32ThreadID)
            {
                if let Ok(name) = GetThreadDescription(thread) {
                    found = name.to_string().is_ok_and(|n| n == SNAP_FLYOUT_THREAD);
                    let _ = LocalFree(Some(HLOCAL(name.0.cast())));
                }
                let _ = CloseHandle(thread);
            }
            more = Thread32Next(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        Some(found)
    }
}

/// Whether the hook of an earlier run rests in this Explorer, of this
/// version: its box off, still subscribed, waiting for a controller. Taking it
/// back subscribes nothing, so the Snap layouts thread does not matter.
pub(super) fn resting(pid: u32) -> bool {
    unsafe {
        let name = wide(&format!("Local\\LimitPeekerXaml_v1_{pid}"));
        let Ok(mapping) = OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(name.as_ptr())) else {
            return false;
        };
        let view = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, WORDS * 4);
        let found = !view.Value.is_null() && {
            let word =
                |index: usize| (*view.Value.cast::<AtomicU32>().add(index)).load(Ordering::SeqCst);
            word(0) == MAGIC && word(1) == VERSION && word(HOOK_STATUS) == RESTING
        };
        if !view.Value.is_null() {
            let _ = UnmapViewOfFile(view);
        }
        let _ = CloseHandle(mapping);
        found
    }
}

/// Sends the hook resting in this Explorer away, as `--remove` does, so
/// nothing of LimitPeeker stays subscribed there: the same compare-exchange a
/// claim is, 7 to 4, on which the hook unsubscribes and leaves. A hook still
/// taking its box off is waited for, up to `within`. Whether one was sent
/// away.
pub(crate) fn release_resting(pid: u32, within: Duration) -> bool {
    unsafe {
        let name = wide(&format!("Local\\LimitPeekerXaml_v1_{pid}"));
        let access = FILE_MAP_READ | FILE_MAP_WRITE;
        let Ok(mapping) = OpenFileMappingW(access.0, false, PCWSTR(name.as_ptr())) else {
            return false;
        };
        let view = MapViewOfFile(mapping, access, 0, 0, WORDS * 4);
        let mut released = false;
        if !view.Value.is_null() {
            let word = |index: usize| &*view.Value.cast::<AtomicU32>().add(index);
            let start = Instant::now();
            while word(0).load(Ordering::SeqCst) == MAGIC
                && word(1).load(Ordering::SeqCst) == VERSION
            {
                match word(HOOK_STATUS).compare_exchange(
                    RESTING,
                    4,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => {
                        released = true;
                        break;
                    }
                    // Starting, shown, hidden, or just taken off: on its way
                    // to rest once its controller has gone.
                    Err(1 | 2 | 3 | 6) if start.elapsed() < within => {
                        std::thread::sleep(Duration::from_millis(100))
                    }
                    Err(_) => break,
                }
            }
            let _ = UnmapViewOfFile(view);
        }
        let _ = CloseHandle(mapping);
        released
    }
}

/// Holds an attach back while Explorer has the Snap layouts thread:
/// attaching then crashed Explorer twice, on that thread (0x328f23). Looked
/// at before every attempt, and every 5 s while it holds, until the thread
/// goes or Explorer restarts.
#[derive(Default)]
pub(super) struct SnapWait {
    pub(super) shell: Shell,
    pub(super) next: u64,
    pub(super) blocked: bool,
}
impl SnapWait {
    pub(super) fn blocks(
        &mut self,
        shell: Shell,
        now: u64,
        file: &Log,
        waiting: &AtomicBool,
    ) -> bool {
        if !self.shell.is(&shell) {
            *self = Self {
                shell,
                ..Self::default()
            };
        }
        let pid = shell.pid;
        if self.blocked && now < self.next {
            return true;
        }
        self.next = now.saturating_add(5000);
        let blocked = snap_flyout(pid).unwrap_or(false);
        if blocked != self.blocked {
            log(
                file,
                if blocked {
                    format!(
                        "attach deferred: explorer pid={pid} has \"{SNAP_FLYOUT_THREAD}\"; attaching then crashed Explorer. Waiting for it to go or Explorer to restart"
                    )
                } else {
                    format!(
                        "attach resumed: explorer pid={pid} no longer has \"{SNAP_FLYOUT_THREAD}\""
                    )
                },
            );
        }
        self.blocked = blocked;
        waiting.store(blocked, Ordering::SeqCst);
        blocked
    }
}

pub(super) fn elevated(process: HANDLE) -> windows::core::Result<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token)?;
        let mut value = TOKEN_ELEVATION::default();
        let mut bytes = 0;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut value as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut bytes,
        );
        let _ = CloseHandle(token);
        result?;
        Ok(value.TokenIsElevated != 0)
    }
}

/// Whether Explorer has the hook's DLL loaded; None when its modules cannot
/// be read. Of an attach whose hook never took the session, it tells one whose
/// DLL never came from one whose DLL did not start.
pub(super) fn loaded(pid: u32, dll: &str) -> Option<bool> {
    let name = std::path::Path::new(dll)
        .file_name()?
        .to_string_lossy()
        .to_lowercase();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid).ok()?;
        let mut entry = MODULEENTRY32W {
            dwSize: size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        let mut more = Module32FirstW(snapshot, &mut entry).is_ok();
        while more && !found {
            let module = &entry.szModule;
            let end = module.iter().position(|&c| c == 0).unwrap_or(module.len());
            found = String::from_utf16_lossy(&module[..end]).to_lowercase() == name;
            more = Module32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        Some(found)
    }
}

pub(super) type Initialize =
    unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, GUID, PCWSTR) -> windows::core::HRESULT;
/// Puts the hook's DLL into Explorer. `token` names this attach in the
/// initialization data, as in ATTACH_TOKEN; the hook takes the session only
/// when they match.
pub(super) fn attach(pid: u32, dll: &str, token: u32, file: &Log) -> windows::core::Result<()> {
    unsafe {
        let library = LoadLibraryExW(
            w!("Windows.UI.Xaml.dll"),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )?;
        let raw = GetProcAddress(
            library,
            PCSTR(c"InitializeXamlDiagnosticsEx".as_ptr().cast()),
        );
        let Some(raw) = raw else {
            let error = windows::core::Error::from_thread();
            let _ = FreeLibrary(library);
            return Err(error);
        };
        let initialize: Initialize = std::mem::transmute(raw);
        let path = wide(dll);
        let data = wide(&token.to_string());
        let mut hr = E_FAIL;
        let mut connection = 0;
        // Only names the shell itself opened exist; a made-up name is
        // ERROR_NOT_FOUND, so a reused session cannot pick a fresh connection.
        // Bounded discovery. Never request elevation or patch remote memory.
        for index in 1..=64 {
            connection = index;
            let name = wide(&format!("VisualDiagConnection{index}"));
            hr = initialize(
                PCWSTR(name.as_ptr()),
                pid,
                w!(""),
                PCWSTR(path.as_ptr()),
                CLSID,
                PCWSTR(data.as_ptr()),
            );
            if hr != windows::core::HRESULT::from_win32(ERROR_NOT_FOUND.0) {
                break;
            }
        }
        // Which connection a reused session lands on.
        log(
            file,
            format!(
                "attach pid={pid} connection={connection} HRESULT={:#010x}",
                hr.0
            ),
        );
        // Keep a successful diagnostics session loaded, but release failed attempts.
        if hr.is_err() {
            let _ = FreeLibrary(library);
        }
        hr.ok()
    }
}
