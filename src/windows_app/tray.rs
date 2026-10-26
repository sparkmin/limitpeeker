//! The tray icon: putting it in, its tooltip and its drawing, and what the
//! popup opened from it shows.

use super::*;
use crate::{choice::Choice, layout::DisplayMode, popup, tray_icon, xaml::Controller};
use std::mem::size_of;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::core::PCWSTR;

/// How putting the tray icon in went.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Added {
    New,
    /// NIM_ADD said it failed, yet the icon was there.
    There,
    Missing,
}
impl Added {
    pub(super) fn note(self) -> &'static str {
        match self {
            Added::New => "tray icon added",
            Added::There => "tray icon was there already; taken as added",
            Added::Missing => "tray icon could not be added; trying each second",
        }
    }
}
/// Puts the tray icon in. Just after the taskbar comes up, NIM_ADD may say
/// it failed and put the icon in after all, and every NIM_ADD after it fails
/// as the icon is there: NIM_MODIFY then takes, and it counts as added. With
/// no taskbar both fail.
pub(super) unsafe fn add_icon(data: &NOTIFYICONDATAW) -> Added {
    unsafe {
        if Shell_NotifyIconW(NIM_ADD, data).as_bool() {
            Added::New
        } else if Shell_NotifyIconW(NIM_MODIFY, data).as_bool() {
            Added::There
        } else {
            Added::Missing
        }
    }
}
pub(super) fn mode(value: u32) -> DisplayMode {
    DisplayMode::from_word(value)
}
/// `text` in at most `most` UTF-16 units: whole when it fits, or else cut
/// after the last service that fits, with an ellipsis.
pub(super) fn tip(text: &str, most: usize) -> Vec<u16> {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() <= most {
        return units;
    }
    let room = most - 1;
    let cut = text
        .match_indices("  |  ")
        .map(|(at, _)| text[..at].encode_utf16().count())
        .filter(|&at| at <= room)
        .last()
        .unwrap_or(room);
    let mut out = units[..cut].to_vec();
    if out.last().is_some_and(|u| (0xD800..0xDC00).contains(u)) {
        out.pop();
    }
    out.push('\u{2026}' as u16);
    out
}
pub(super) fn tray(
    hwnd: HWND,
    current: DisplayMode,
    label: &str,
    mock: bool,
    icon: HICON,
) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: CALLBACK,
        ..Default::default()
    };
    data.hIcon = icon;
    // Where the box is, then the figures.
    let place = if current == DisplayMode::Full {
        "On the taskbar"
    } else {
        "Tray only"
    };
    let mock = if mock { " · mock figures" } else { "" };
    let text = tip(&format!("LimitPeeker · {place}{mock}\n{label}"), 127);
    for (slot, ch) in data.szTip.iter_mut().zip(text) {
        *slot = ch;
    }
    data
}
/// A notification from the tray icon. Windows shows it as a toast named for
/// this program and keeps it in its notification center; `picture`, the
/// dial drawn large, stays the caller's to destroy. An empty text would
/// take a notification away, so an empty body is a space.
pub(super) fn balloon(
    hwnd: HWND,
    alert: &crate::notify::Alert,
    picture: Option<HICON>,
) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_INFO,
        ..Default::default()
    };
    let body = if alert.body.is_empty() {
        " "
    } else {
        alert.body.as_str()
    };
    for (slot, ch) in data.szInfoTitle.iter_mut().zip(tip(&alert.title, 63)) {
        *slot = ch;
    }
    for (slot, ch) in data.szInfo.iter_mut().zip(tip(body, 255)) {
        *slot = ch;
    }
    if let Some(picture) = picture {
        data.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
        data.hBalloonIcon = picture;
    }
    data
}
/// What the tray icon shows now: where the figures are and what is wrong.
pub(super) fn tray_look(
    choice: &Choice,
    controller: &Controller,
    mode: DisplayMode,
) -> tray_icon::Look {
    let on_taskbar = mode == DisplayMode::Full;
    let (tightest, failed) = tray_icon::figures(choice, on_taskbar);
    tray_icon::Look {
        face: tray_icon::face(on_taskbar, tightest),
        badge: tray_icon::badge(
            failed,
            controller.snap_wait.load(Ordering::SeqCst),
            controller.reason(),
        ),
        light: popup::light_taskbar(),
        size: tray_icon::side(),
    }
}
/// How long an icon that could not be drawn waits to be tried again.
pub(super) const DRAW_AGAIN: Duration = Duration::from_secs(60);
/// The tray icon drawn now, and what it was drawn for.
#[derive(Default)]
pub(super) struct TrayIcon {
    pub(super) look: Option<tray_icon::Look>,
    /// Ours to destroy; none shows Windows' application icon.
    pub(super) handle: Option<HICON>,
    /// When drawing last failed, while it fails.
    pub(super) failed: Option<Instant>,
}
impl TrayIcon {
    /// The icon drawn now, or else the program's own, or Windows'
    /// application icon.
    pub(super) fn handle(&self) -> HICON {
        self.handle.unwrap_or_else(|| unsafe {
            GetModuleHandleW(None)
                .and_then(|module| LoadIconW(Some(HINSTANCE(module.0)), PCWSTR(1 as _)))
                .or_else(|_| LoadIconW(None, IDI_APPLICATION))
                .unwrap_or_default()
        })
    }
    /// Draws the icon again when what it shows changed, or a while after it
    /// could not be drawn. Gives back the handle it replaced, to destroy
    /// once the shell holds the new one; none when nothing changed.
    pub(super) fn update(
        &mut self,
        look: tray_icon::Look,
        controller: &Controller,
    ) -> Option<Option<HICON>> {
        let again = self.failed.is_some_and(|at| at.elapsed() >= DRAW_AGAIN);
        if self.look == Some(look) && !again {
            return None;
        }
        self.look = Some(look);
        match tray_icon::icon(look) {
            Ok(icon) => {
                self.failed = None;
                controller.note(format!(
                    "tray icon: {:?} {:?} {} {}px",
                    look.face,
                    look.badge,
                    if look.light { "light" } else { "dark" },
                    look.size
                ));
                Some(self.handle.replace(icon))
            }
            Err(error) => {
                let first = self.failed.is_none();
                self.failed = Some(Instant::now());
                // Said once, and the application icon stands in once.
                first.then(|| {
                    controller.note(format!(
                        "tray icon could not be drawn ({error}); the application icon shows"
                    ));
                    self.handle.take()
                })
            }
        }
    }
}
impl Drop for TrayIcon {
    fn drop(&mut self) {
        destroy(Some(self.handle.take()));
    }
}
/// Destroys an icon the shell no longer holds.
pub(super) fn destroy(replaced: Option<Option<HICON>>) {
    if let Some(Some(icon)) = replaced {
        let _ = unsafe { DestroyIcon(icon) };
    }
}

/// What the popup shows now: where the box stands, the choice and the
/// services, whether Refresh now and Settings can be pressed, and whether the
/// boxes' settings are unfolded.
pub(super) fn popup_view(
    choice: &Choice,
    controller: &Controller,
    unsaved: bool,
    current: DisplayMode,
    mock: bool,
    unfolded: bool,
) -> popup::View {
    popup::view(
        choice,
        popup::Status {
            mock,
            on_taskbar: current == DisplayMode::Full,
            snap_wait: controller.snap_wait.load(Ordering::SeqCst),
            restarting: controller.restarting(),
            unfolded,
            reason: controller.reason(),
            notice: if unsaved {
                Some("Could not save the settings; see the log.")
            } else {
                controller.notice()
            },
        },
        controller.read_now_ready_at(),
        settings_exe().is_some(),
    )
}
