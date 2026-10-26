//! The tray's popup, in place of the tray menu: what each service in the box
//! has used and when each limit starts over, above every choice the menu had,
//! drawn by this process with Direct2D in a borderless window of its own. A
//! chip sends the settings window's command, so the menu's rules hold, and the
//! popup stays open to show what changed. It follows the taskbar's light or
//! dark mode and the accent color.
//!
//! It is split by layer: `view` takes the choice, `layout` places it,
//! `theme` colors it, `paint` draws it, `window` is the window and its
//! input, and `rename` the field a box's name is typed in.

mod layout;
mod paint;
mod rename;
#[cfg(test)]
mod tests;
mod theme;
mod view;
mod window;

use layout::*;
use paint::*;
use rename::Renaming;
use theme::*;
use view::*;
use window::*;

pub use layout::WIDTH;
pub use theme::light_taskbar;
pub use view::{Status, View, until, view};
pub use window::{is_open, shut, take, toggle, update};

use windows::Win32::UI::WindowsAndMessaging::WM_APP;

/// What the popup posts its owner when a control was pressed; the owner
/// takes the actions with `take`.
pub const ACTION: u32 = WM_APP + 7;

/// What pressing a control does.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    /// A command in the settings window's words, applied by its rules.
    Apply(String),
    Refresh,
    Settings,
    Exit,
    /// Restarts the taskbar's Explorer, whose Snap layouts thread holds the
    /// box back.
    RestartExplorer,
    /// Opens the settings window's editor on the box shown now.
    Customize,
    /// Unfolds the boxes' settings, first choosing the box whose folded tab
    /// was pressed; and folds them again.
    Unfold(Option<String>),
    Fold,
    /// The box's color: Windows' color dialog over the popup, and the
    /// eyedropper that takes a color from the screen. The popup handles
    /// both itself and sends the color chosen as a command.
    PickColor,
    Eyedrop,
}
