//! A box's name typed on its tab: a small window of its own over the tab,
//! holding Windows' edit control, so the keyboard's input methods, the
//! clipboard and the caret work as everywhere. A child of the popup would be
//! drawn wrong on its translucent backdrop, which GDI does not draw on.

use super::*;
use std::cell::Cell;
use std::ffi::c_void;
use std::mem::size_of;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{EM_LIMITTEXT, EM_SETSEL};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

/// The window's class.
const RENAME_CLASS: PCWSTR = w!("LimitPeekerRename");
/// What the window posts the popup when the name is done: `wparam` 1 to keep
/// it, `lparam` 1 when the click that ended it was outside the popup.
pub(super) const RENAMED: u32 = WM_APP + 2;
/// How wide the field is at least, its text's size, and the room inside it,
/// in DIPs.
pub(super) const RENAME_W: f32 = 160.0;
const RENAME_FONT: f32 = 13.0;
const RENAME_INSET: f32 = 8.0;

thread_local! {
    static EDIT_PROC: Cell<isize> = const { Cell::new(0) };
    /// The text, field and edge colors, as COLORREFs, and the field's brush.
    static COLORS: Cell<(u32, u32, isize)> = const { Cell::new((0, 0, 0)) };
    static HOST: Cell<isize> = const { Cell::new(0) };
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
}

/// A name being typed: the window and its edit control, what they draw
/// with, the box it names, and the name it had.
pub(super) struct Renaming {
    pub(super) host: HWND,
    pub(super) edit: HWND,
    font: HFONT,
    brush: HBRUSH,
    pub(super) index: usize,
    pub(super) original: String,
}

/// Whether the window is the one a name is being typed in.
pub(super) fn is_host(hwnd: HWND) -> bool {
    let host = HOST.get();
    host != 0 && hwnd.0 as isize == host
}

/// An ARGB color as GDI takes it.
pub(super) fn colorref(argb: u32) -> COLORREF {
    COLORREF(((argb >> 16) & 0xFF) | (argb & 0xFF00) | ((argb & 0xFF) << 16))
}

/// Opens the field at `frame`, in pixels on the screen, over the popup that
/// owns it, with `name` in it chosen whole.
pub(super) unsafe fn open(
    popup: HWND,
    frame: RECT,
    scale: f32,
    palette: &Palette,
    index: usize,
    name: &str,
) -> windows::core::Result<Renaming> {
    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        if !REGISTERED.get() {
            let registered = RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(host_proc),
                hInstance: instance,
                hCursor: LoadCursorW(None, IDC_IBEAM)?,
                lpszClassName: RENAME_CLASS,
                ..Default::default()
            });
            if registered == 0 {
                return Err(windows::core::Error::from_thread());
            }
            REGISTERED.set(true);
        }
        let brush = CreateSolidBrush(colorref(palette.field));
        COLORS.set((
            colorref(palette.primary).0,
            colorref(palette.field).0,
            brush.0 as isize,
        ));
        let (width, height) = (frame.right - frame.left, frame.bottom - frame.top);
        let host = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            RENAME_CLASS,
            w!(""),
            WS_POPUP,
            frame.left,
            frame.top,
            width,
            height,
            Some(popup),
            None,
            Some(instance),
            None,
        )?;
        // Small rounded corners and an edge in the accent, as a field that
        // has the keyboard.
        let corner = DWMWCP_ROUNDSMALL;
        let _ = DwmSetWindowAttribute(
            host,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const DWM_WINDOW_CORNER_PREFERENCE as *const c_void,
            size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
        let edge = colorref(palette.accent);
        let _ = DwmSetWindowAttribute(
            host,
            DWMWA_BORDER_COLOR,
            &edge as *const COLORREF as *const c_void,
            size_of::<COLORREF>() as u32,
        );
        let line = (RENAME_FONT * 1.4 * scale).round() as i32;
        let inset = (RENAME_INSET * scale).round() as i32;
        let wide = crate::message::wide(name);
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            PCWSTR(wide.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            inset,
            (height - line) / 2,
            width - 2 * inset,
            line,
            Some(host),
            None,
            Some(instance),
            None,
        )?;
        let font = CreateFontW(
            -(RENAME_FONT * scale).round() as i32,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            u32::from(DEFAULT_PITCH.0 | FF_DONTCARE.0),
            w!("Segoe UI"),
        );
        SendMessageW(
            edit,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        SendMessageW(
            edit,
            EM_LIMITTEXT,
            Some(WPARAM(crate::choice::MAX_NAME)),
            None,
        );
        SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
        EDIT_PROC.set(SetWindowLongPtrW(
            edit,
            GWLP_WNDPROC,
            edit_proc as *const () as isize,
        ));
        // Known before it takes the foreground, so the popup it takes it
        // from does not close.
        HOST.set(host.0 as isize);
        let _ = ShowWindow(host, SW_SHOW);
        let _ = SetForegroundWindow(host);
        let _ = SetFocus(Some(edit));
        Ok(Renaming {
            host,
            edit,
            font,
            brush,
            index,
            original: name.to_string(),
        })
    }
}

/// The name in the field.
pub(super) fn text(renaming: &Renaming) -> String {
    let mut buffer = [0u16; 2 * crate::choice::MAX_NAME + 1];
    let length = unsafe { GetWindowTextW(renaming.edit, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

/// Takes the field away, the popup having the foreground back first.
pub(super) unsafe fn close(renaming: Renaming, popup: HWND) {
    unsafe {
        HOST.set(0);
        let _ = SetForegroundWindow(popup);
        let _ = DestroyWindow(renaming.host);
        let _ = DeleteObject(HGDIOBJ(renaming.font.0));
        let _ = DeleteObject(HGDIOBJ(renaming.brush.0));
    }
}

/// Tells the popup the name is done.
unsafe fn done(host: HWND, keep: bool, outside: bool) {
    unsafe {
        if let Ok(popup) = GetWindow(host, GW_OWNER) {
            let _ = PostMessageW(
                Some(popup),
                RENAMED,
                WPARAM(usize::from(keep)),
                LPARAM(isize::from(outside)),
            );
        }
    }
}

/// The field's window: the field's colors, the keyboard to the edit
/// control, and a click elsewhere keeps the name; one outside the popup
/// also closes it, as it would have.
unsafe extern "system" fn host_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_CTLCOLOREDIT => {
                let (text, field, brush) = COLORS.get();
                let hdc = HDC(wparam.0 as *mut c_void);
                SetTextColor(hdc, COLORREF(text));
                SetBkColor(hdc, COLORREF(field));
                return LRESULT(brush);
            }
            WM_ERASEBKGND => {
                let mut client = RECT::default();
                let _ = GetClientRect(hwnd, &mut client);
                let (_, _, brush) = COLORS.get();
                FillRect(
                    HDC(wparam.0 as *mut c_void),
                    &client,
                    HBRUSH(brush as *mut c_void),
                );
                return LRESULT(1);
            }
            WM_SETFOCUS => {
                if let Ok(edit) = GetWindow(hwnd, GW_CHILD) {
                    let _ = SetFocus(Some(edit));
                }
                return LRESULT(0);
            }
            WM_ACTIVATE if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE && is_host(hwnd) => {
                let to = HWND(lparam.0 as *mut c_void);
                let popup = GetWindow(hwnd, GW_OWNER).ok();
                done(hwnd, true, popup != Some(to));
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// The edit control: Enter keeps the name and Esc drops it, without the
/// beep a single line gives them.
unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let host = GetParent(hwnd).unwrap_or_default();
        match msg {
            WM_KEYDOWN if wparam.0 == usize::from(VK_RETURN.0) => {
                done(host, true, false);
                return LRESULT(0);
            }
            WM_KEYDOWN if wparam.0 == usize::from(VK_ESCAPE.0) => {
                done(host, false, false);
                return LRESULT(0);
            }
            WM_CHAR if matches!(wparam.0, 0x0D | 0x1B) => return LRESULT(0),
            _ => {}
        }
        let original: WNDPROC = std::mem::transmute(EDIT_PROC.get());
        CallWindowProcW(original, hwnd, msg, wparam, lparam)
    }
}
