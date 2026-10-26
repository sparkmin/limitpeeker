//! Text as Windows takes it, and the message boxes of runs that have no
//! window or console to say things in: an option not known, `--remove`, and
//! uninstall.exe, which builds this file in too.

use windows::Win32::UI::WindowsAndMessaging::{MESSAGEBOX_RESULT, MESSAGEBOX_STYLE, MessageBoxW};
use windows::core::PCWSTR;

/// `text` as Windows takes it: UTF-16, ended by a zero.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Shows `text` in a message box titled `title`, with the buttons and icon
/// of `style`, and gives the button pressed.
pub fn show(title: &str, text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let (title, text) = (wide(title), wide(text));
    unsafe { MessageBoxW(None, PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), style) }
}
