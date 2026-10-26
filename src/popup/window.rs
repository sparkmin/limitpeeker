//! The popup window: Direct2D resources, drawing, the pointer and keys,
//! and opening and closing it.

use super::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::CoUninitialize;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PCWSTR, w};

/// The popup window's class.
pub(super) const CLASS: PCWSTR = w!("LimitPeekerPopup");
/// What the popup posts itself to close, out of the message that asked.
pub(super) const CLOSE: u32 = WM_APP + 1;
/// What the popup posts itself to open the color dialog, out of the press
/// that asked, so the dialog's own message loop runs with the popup free.
pub(super) const PICK_COLOR: u32 = WM_APP + 3;
/// The timer that counts Refresh now down while the popup is open.
pub(super) const TICK: usize = 1;
/// A click on the tray icon that closed the popup, by taking it out of the
/// foreground, does not open it again.
pub(super) const REOPEN: Duration = Duration::from_millis(500);

thread_local! {
    pub(super) static GRAPHICS: RefCell<Option<Graphics>> = const { RefCell::new(None) };
    pub(super) static OPEN: RefCell<Option<Popup>> = const { RefCell::new(None) };
    pub(super) static ACTIONS: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
    pub(super) static CLOSED: Cell<Option<Instant>> = const { Cell::new(None) };
    pub(super) static REGISTERED: Cell<bool> = const { Cell::new(false) };
    /// Whether the color dialog is open over the popup, which then keeps
    /// open without the foreground and is not updated under it; and the
    /// dialog's custom colors, kept for the run.
    pub(super) static DIALOG: Cell<bool> = const { Cell::new(false) };
    pub(super) static CUSTOM_COLORS: Cell<[COLORREF; 16]> = const { Cell::new([COLORREF(0xFFFFFF); 16]) };
}

/// Lays the view out with the tray thread's text measure.
pub(super) fn laid_out(view: &View) -> Layout {
    GRAPHICS.with(|graphics| {
        let graphics = graphics.borrow();
        layout(view, Instant::now(), &|text, font| {
            graphics
                .as_ref()
                .map_or(text.chars().count() as f32 * 7.0, |g| g.measure(text, font))
        })
    })
}

/// The open popup: its window and whom it tells, what it shows, and where the
/// pointer and keyboard focus are.
pub(super) struct Popup {
    pub(super) hwnd: HWND,
    pub(super) owner: HWND,
    pub(super) anchor: POINT,
    pub(super) dpi: u32,
    pub(super) theme: Theme,
    pub(super) acrylic: bool,
    /// Where the window is, in pixels.
    pub(super) frame: RECT,
    pub(super) target: Option<ID2D1HwndRenderTarget>,
    pub(super) brush: Option<ID2D1SolidColorBrush>,
    pub(super) icons: HashMap<&'static str, Option<ID2D1Bitmap>>,
    pub(super) view: View,
    pub(super) layout: Layout,
    /// The window's height in DIPs, less than the layout's when it scrolls.
    pub(super) shown: f32,
    pub(super) scroll: f32,
    pub(super) hover: Option<usize>,
    pub(super) pressed: Option<usize>,
    pub(super) focus: Option<usize>,
    /// Focus shows once the keyboard moves it, as in Windows' own controls.
    pub(super) keyboard: bool,
    pub(super) tracking: bool,
    /// Where the left button went down and the drag under it; the drag under
    /// way and the place it would go; whether the pointer is over a line that
    /// only drags; and the control to focus once a move from the keyboard
    /// shows.
    pub(super) down: Option<(f32, f32)>,
    pub(super) grabbed: Option<usize>,
    pub(super) dragging: Option<(usize, usize)>,
    pub(super) over_drag: bool,
    pub(super) refocus: Option<Action>,
    /// A box's name being typed on its tab.
    pub(super) renaming: Option<Renaming>,
    /// The color under the eyedropper while it takes one, with the box's
    /// opacity.
    pub(super) picking: Option<u32>,
}

/// The work area of the monitor at a point, in pixels.
pub(super) fn work_area(point: POINT) -> RECT {
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            info.rcWork
        } else {
            RECT {
                left: point.x - 1000,
                top: point.y - 1000,
                right: point.x,
                bottom: point.y,
            }
        }
    }
}

impl Popup {
    pub(super) fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    /// The window's place and size in pixels, as tall as its content up to
    /// the work area; sets how tall it shows and how far it scrolls.
    pub(super) fn fit(&mut self) -> RECT {
        let scale = self.scale();
        let work = work_area(self.anchor);
        let most = ((work.bottom - work.top) as f32 / scale - 2.0 * MARGIN).max(120.0);
        self.shown = self.layout.height.min(most);
        self.scroll = self.scroll.clamp(0.0, self.layout.height - self.shown);
        let size = SIZE {
            cx: (WIDTH * scale).round() as i32,
            cy: (self.shown * scale).round() as i32,
        };
        let at = place(self.anchor, size, work, (MARGIN * scale).round() as i32);
        RECT {
            left: at.x,
            top: at.y,
            right: at.x + size.cx,
            bottom: at.y + size.cy,
        }
    }

    /// A point in the window, from a mouse message, in the layout's DIPs.
    pub(super) fn at(&self, lparam: LPARAM) -> (f32, f32) {
        let x = (lparam.0 & 0xFFFF) as i16 as f32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
        (x / self.scale(), y / self.scale() + self.scroll)
    }

    pub(super) fn hit(&self, (x, y): (f32, f32)) -> Option<usize> {
        self.layout
            .controls
            .iter()
            .position(|c| c.at.contains(x, y))
    }

    /// Hands the control's action to the owner, and closes for Settings and
    /// Exit. Chips leave the popup open: the owner applies them and sends
    /// the popup what changed. The box's color is the popup's own to pick.
    pub(super) fn press(&mut self, index: usize) {
        let Some(control) = self.layout.controls.get(index).filter(|c| c.enabled) else {
            return;
        };
        let action = control.action.clone();
        match action {
            Action::PickColor => unsafe {
                let _ = PostMessageW(Some(self.hwnd), PICK_COLOR, WPARAM(0), LPARAM(0));
                return;
            },
            Action::Eyedrop => {
                self.start_eyedrop();
                return;
            }
            _ => {}
        }
        let closes = matches!(
            action,
            Action::Settings | Action::Exit | Action::RestartExplorer
        );
        self.tell(action);
        if closes {
            unsafe {
                let _ = PostMessageW(Some(self.hwnd), CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// The box's color as the swatch shows it, with the opacity a color
    /// picked keeps; a box with no background takes the color opaque, so the
    /// pick shows.
    pub(super) fn swatch_color(&self) -> Option<u32> {
        self.layout.controls.iter().find_map(|c| match c.kind {
            Kind::Swatch { color } if color >> 24 == 0 => Some(0xFF00_0000 | color),
            Kind::Swatch { color } => Some(color),
            _ => None,
        })
    }

    /// Takes the pointer to pick a color from the screen: the swatch shows
    /// what is under it until a press takes that, or Esc, the right button
    /// or losing the pointer gives up.
    pub(super) fn start_eyedrop(&mut self) {
        let Some(color) = self.swatch_color() else {
            return;
        };
        self.finish_rename(true);
        self.let_go();
        self.pressed = None;
        self.picking = Some(color);
        unsafe {
            SetCapture(self.hwnd);
            let _ = SetCursor(LoadCursorW(None, IDC_CROSS).ok());
        }
        self.invalidate();
    }
    pub(super) fn eyedrop_moved(&mut self) {
        if let (Some(picking), Some(pixel)) = (self.picking, pixel_under_pointer()) {
            self.picking = Some(picking & 0xFF00_0000 | pixel & 0x00FF_FFFF);
            unsafe {
                let _ = SetCursor(LoadCursorW(None, IDC_CROSS).ok());
            }
            self.invalidate();
        }
    }
    pub(super) fn end_eyedrop(&mut self, keep: bool) {
        let Some(color) = self.picking.take() else {
            return;
        };
        unsafe {
            let _ = ReleaseCapture();
        }
        if keep {
            self.tell(Action::Apply(format!(
                "color\t{}",
                crate::style::argb_text(color)
            )));
        }
        self.invalidate();
    }

    /// Hands an action to the owner.
    pub(super) fn tell(&self, action: Action) {
        ACTIONS.with(|actions| actions.borrow_mut().push(action));
        unsafe {
            let _ = PostMessageW(Some(self.owner), ACTION, WPARAM(0), LPARAM(0));
        }
    }

    /// Moves the focused chip one place earlier or later among those on, and
    /// keeps the focus on it once the popup shows the move.
    pub(super) fn move_focused(&mut self, later: bool) {
        let Some(control) = self.focus.and_then(|i| self.layout.controls.get(i)) else {
            return;
        };
        let Some(drag) = control.drag.and_then(|d| self.layout.drags.get(d)) else {
            return;
        };
        let count = self
            .layout
            .drags
            .iter()
            .filter(|d| d.group == drag.group)
            .count();
        let to = if later {
            Some(drag.index + 1).filter(|&to| to < count)
        } else {
            drag.index.checked_sub(1)
        };
        if let Some(to) = to {
            self.refocus = Some(control.action.clone());
            self.tell(Action::Apply(format!("{}{to}", drag.command)));
        }
    }

    /// Drops what was dragged where it would go.
    pub(super) fn drop_dragged(&self, (from, to): (usize, usize)) {
        if let Some(drag) = self.layout.drags.get(from)
            && to != drag.index
        {
            self.tell(Action::Apply(format!("{}{to}", drag.command)));
        }
    }

    /// Lets go of a drag, as when the popup shows something new under it.
    pub(super) fn let_go(&mut self) {
        self.down = None;
        self.grabbed = None;
        self.dragging = None;
    }

    /// Scrolls so the focused control shows whole.
    pub(super) fn reveal(&mut self, index: usize) {
        if let Some(control) = self.layout.controls.get(index) {
            let (top, bottom) = (control.at.y - 4.0, control.at.y + control.at.h + 4.0);
            if top < self.scroll {
                self.scroll = top.max(0.0);
            } else if bottom > self.scroll + self.shown {
                self.scroll = (bottom - self.shown).min(self.layout.height - self.shown);
            }
        }
    }

    /// Opens the name of the box whose tab is `control` for typing, over
    /// the tab and at least RENAME_W wide.
    pub(super) fn rename(&mut self, control: usize) {
        if self.renaming.is_some() {
            return;
        }
        let Some(tab) = self
            .layout
            .controls
            .get(control)
            .filter(|c| matches!(c.kind, Kind::Tab { .. }))
        else {
            return;
        };
        let Action::Apply(command) = &tab.action else {
            return;
        };
        let Some(index) = command
            .strip_prefix("select\t")
            .and_then(|i| i.parse::<usize>().ok())
        else {
            return;
        };
        let Some(name) = self.view.tabs.get(index).map(|t| t.text.clone()) else {
            return;
        };
        let scale = self.scale();
        let at = Rect {
            x: tab.at.x,
            y: tab.at.y + 2.0,
            w: tab.at.w.max(rename::RENAME_W).min(RIGHT - tab.at.x),
            h: tab.at.h - 4.0,
        };
        let px = |v: f32| (v * scale).round() as i32;
        let frame = RECT {
            left: self.frame.left + px(at.x),
            top: self.frame.top + px(at.y - self.scroll),
            right: self.frame.left + px(at.x + at.w),
            bottom: self.frame.top + px(at.y - self.scroll + at.h),
        };
        let palette = self.theme.palette();
        self.let_go();
        self.renaming =
            unsafe { rename::open(self.hwnd, frame, scale, &palette, index, &name) }.ok();
    }

    /// Ends the typing of a name: a changed one is sent, as the window would
    /// send it, and the popup has the keyboard back.
    pub(super) fn finish_rename(&mut self, keep: bool) {
        let Some(renaming) = self.renaming.take() else {
            return;
        };
        let name = rename::text(&renaming);
        if keep && name.trim() != renaming.original {
            self.tell(Action::Apply(format!("name\t{}\t{name}", renaming.index)));
        }
        unsafe { rename::close(renaming, self.hwnd) };
        self.invalidate();
    }

    pub(super) fn invalidate(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    /// Handles a message the popup takes; None for the default.
    pub(super) fn handle(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            // The eyedropper has the pointer: the color under it, taken by
            // the left button, given up by the right or Esc.
            WM_MOUSEMOVE if self.picking.is_some() => {
                self.eyedrop_moved();
                Some(LRESULT(0))
            }
            WM_SETCURSOR if self.picking.is_some() => {
                unsafe {
                    let _ = SetCursor(LoadCursorW(None, IDC_CROSS).ok());
                }
                Some(LRESULT(1))
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK if self.picking.is_some() => {
                self.end_eyedrop(true);
                Some(LRESULT(0))
            }
            WM_RBUTTONDOWN if self.picking.is_some() => {
                self.end_eyedrop(false);
                Some(LRESULT(0))
            }
            WM_KEYDOWN if self.picking.is_some() => {
                if wparam.0 == usize::from(VK_ESCAPE.0) {
                    self.end_eyedrop(false);
                }
                Some(LRESULT(0))
            }
            WM_PAINT => {
                unsafe {
                    let mut paint = PAINTSTRUCT::default();
                    BeginPaint(self.hwnd, &mut paint);
                    self.draw();
                    let _ = EndPaint(self.hwnd, &paint);
                }
                Some(LRESULT(0))
            }
            WM_SIZE => {
                if let Some(target) = &self.target {
                    let size = D2D_SIZE_U {
                        width: (lparam.0 & 0xFFFF) as u32,
                        height: ((lparam.0 >> 16) & 0xFFFF) as u32,
                    };
                    unsafe {
                        let _ = target.Resize(&size);
                    }
                }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                if !self.tracking {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };
                    self.tracking = unsafe { TrackMouseEvent(&mut track) }.is_ok();
                }
                let point = self.at(lparam);
                if let (Some(from), Some(down)) = (self.grabbed, self.down) {
                    let far = (point.0 - down.0).hypot(point.1 - down.1) > DRAG_START;
                    if self.dragging.is_none() && far {
                        // No longer a press.
                        self.dragging = self.layout.drags.get(from).map(|d| (from, d.index));
                        self.pressed = None;
                    }
                    if let Some((from, _)) = self.dragging {
                        let to = target(&self.layout.drags, from, point);
                        self.dragging = to.map(|to| (from, to));
                        unsafe {
                            let _ = SetCursor(LoadCursorW(None, IDC_SIZEALL).ok());
                        }
                        self.invalidate();
                        return Some(LRESULT(0));
                    }
                }
                let hover = self.hit(point);
                // A line of the summary does nothing but drag, so the pointer
                // says it moves there.
                self.over_drag = hover.is_none()
                    && self
                        .layout
                        .drags
                        .iter()
                        .any(|d| d.at.contains(point.0, point.1));
                if hover != self.hover {
                    self.hover = hover;
                    self.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_SETCURSOR if (lparam.0 & 0xFFFF) as u32 == HTCLIENT && self.over_drag => {
                unsafe {
                    let _ = SetCursor(LoadCursorW(None, IDC_SIZEALL).ok());
                }
                Some(LRESULT(1))
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                self.hover = None;
                self.over_drag = false;
                self.invalidate();
                Some(LRESULT(0))
            }
            // Two presses on a tab open its name; elsewhere the second is a
            // press too.
            WM_LBUTTONDBLCLK => {
                let point = self.at(lparam);
                match self.hit(point) {
                    Some(index) if matches!(self.layout.controls[index].kind, Kind::Tab { .. }) => {
                        self.rename(index);
                        Some(LRESULT(0))
                    }
                    _ => self.handle(WM_LBUTTONDOWN, wparam, lparam),
                }
            }
            rename::RENAMED => {
                self.finish_rename(wparam.0 == 1);
                // A click outside the popup closes it, as without the name.
                if lparam.0 == 1 {
                    CLOSED.set(Some(Instant::now()));
                    unsafe {
                        let _ = PostMessageW(Some(self.hwnd), CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                self.finish_rename(true);
                let point = self.at(lparam);
                self.pressed = self.hit(point);
                self.down = Some(point);
                self.grabbed = self
                    .layout
                    .drags
                    .iter()
                    .position(|d| d.at.contains(point.0, point.1));
                self.keyboard = false;
                unsafe {
                    SetCapture(self.hwnd);
                }
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                let pressed = self.pressed.take();
                let dragged = self.dragging;
                self.let_go();
                unsafe {
                    let _ = ReleaseCapture();
                }
                if let Some(dragged) = dragged {
                    self.drop_dragged(dragged);
                } else if let Some(index) = pressed
                    && self.hit(self.at(lparam)) == Some(index)
                {
                    self.focus = Some(index);
                    self.press(index);
                }
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_CAPTURECHANGED => {
                self.let_go();
                self.pressed = None;
                // The pointer taken away ends the eyedropper.
                self.picking = None;
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => {
                self.finish_rename(true);
                let notches = f32::from((wparam.0 >> 16) as u16 as i16) / 120.0;
                let most = self.layout.height - self.shown;
                self.scroll = (self.scroll - notches * 48.0).clamp(0.0, most.max(0.0));
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let key = VIRTUAL_KEY(wparam.0 as u16);
                let shift = unsafe { GetKeyState(i32::from(VK_SHIFT.0)) } < 0;
                let control = unsafe { GetKeyState(i32::from(VK_CONTROL.0)) } < 0;
                if control && matches!(key, VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN) {
                    self.keyboard = true;
                    self.move_focused(matches!(key, VK_RIGHT | VK_DOWN));
                    self.invalidate();
                    return Some(LRESULT(0));
                }
                let moved = match key {
                    // A tab's name, from the keyboard.
                    VK_F2 => {
                        if let Some(index) = self.focus {
                            self.rename(index);
                        }
                        return Some(LRESULT(0));
                    }
                    VK_ESCAPE => {
                        unsafe {
                            let _ = PostMessageW(Some(self.hwnd), CLOSE, WPARAM(0), LPARAM(0));
                        }
                        return Some(LRESULT(0));
                    }
                    VK_RETURN | VK_SPACE => {
                        if let Some(index) = self.focus {
                            self.press(index);
                        }
                        None
                    }
                    VK_TAB => step(&self.layout.controls, self.focus, !shift),
                    VK_DOWN | VK_RIGHT => step(&self.layout.controls, self.focus, true),
                    VK_UP | VK_LEFT => step(&self.layout.controls, self.focus, false),
                    VK_HOME => step(&self.layout.controls, None, true),
                    VK_END => step(&self.layout.controls, None, false),
                    _ => return None,
                };
                self.keyboard = true;
                if let Some(index) = moved {
                    self.focus = Some(index);
                    self.reveal(index);
                }
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_TIMER if wparam.0 == TICK => {
                // Only Refresh now changes with time alone.
                let fresh = laid_out(&self.view);
                if fresh.controls != self.layout.controls {
                    self.focus = follow(&self.layout.controls, self.focus, &fresh.controls);
                    self.layout = fresh;
                    self.fit();
                    self.invalidate();
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

pub(super) unsafe extern "system" fn popup_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            // Direct2D paints every pixel.
            WM_ERASEBKGND => return LRESULT(1),
            CLOSE | WM_CLOSE => {
                if current(hwnd) {
                    close();
                }
                return LRESULT(0);
            }
            // The color dialog, out of the press that asked for it, so the
            // popup is free while the dialog's loop runs.
            PICK_COLOR => {
                if current(hwnd) {
                    pick_color(hwnd);
                }
                return LRESULT(0);
            }
            // The field for a box's name, or the color dialog, taking the
            // foreground leaves the popup open.
            WM_ACTIVATE
                if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE
                    && current(hwnd)
                    && !rename::is_host(HWND(lparam.0 as *mut c_void))
                    && !DIALOG.get() =>
            {
                // A click outside, or on the tray icon, took it away. Closed
                // out of this message, and not opened again by that click.
                CLOSED.set(Some(Instant::now()));
                let _ = PostMessageW(Some(hwnd), CLOSE, WPARAM(0), LPARAM(0));
            }
            _ => {}
        }
        // A message sent while the popup is being handled, as capture's end
        // is, takes the default.
        let handled = OPEN.with(|open| {
            let mut open = open.try_borrow_mut().ok()?;
            let popup = open.as_mut().filter(|p| p.hwnd == hwnd)?;
            popup.handle(msg, wparam, lparam)
        });
        handled.unwrap_or_else(|| DefWindowProcW(hwnd, msg, wparam, lparam))
    }
}

/// Opens the popup at the point, or closes it when it is open, as the tray
/// icon does in Windows' own flyouts. `owner` gets ACTION.
/// Whether it could be opened, for the log.
pub fn toggle(owner: HWND, anchor: POINT, view: View) -> windows::core::Result<()> {
    if is_open() {
        close();
        return Ok(());
    }
    if CLOSED.get().is_some_and(|at| at.elapsed() < REOPEN) {
        return Ok(());
    }
    unsafe { open(owner, anchor, view) }
}

pub fn is_open() -> bool {
    OPEN.with(|open| open.try_borrow().is_ok_and(|open| open.is_some()))
}

/// Shows what changed in the open popup, growing or shrinking it to fit.
/// Not under the color dialog: a popup that changed size would be a new
/// window, and the dialog would go with its owner.
pub fn update(view: View) {
    if DIALOG.get() {
        return;
    }
    let change = OPEN.with(|open| {
        let mut open = open.try_borrow_mut().ok()?;
        let popup = open.as_mut()?;
        if popup.view == view {
            return None;
        }
        popup.view = view;
        let fresh = laid_out(&popup.view);
        // What was dragged may have moved under the pointer.
        if fresh.drags != popup.layout.drags {
            popup.let_go();
        }
        let was_controls = std::mem::replace(&mut popup.layout, fresh).controls;
        // A popup that changes size is a new window; a name being typed on
        // the old one is kept first.
        let size = popup.fit();
        if (size.right - size.left, size.bottom - size.top)
            != (
                popup.frame.right - popup.frame.left,
                popup.frame.bottom - popup.frame.top,
            )
        {
            popup.finish_rename(true);
        }
        let hover = popup.hover.filter(|&i| i < popup.layout.controls.len());
        popup.hover = hover;
        popup.focus = follow(&was_controls, popup.focus, &popup.layout.controls);
        if let Some(action) = popup.refocus.take() {
            popup.focus = popup
                .layout
                .controls
                .iter()
                .position(|c| c.action == action)
                .or(popup.focus);
        }
        let was = popup.frame;
        popup.frame = popup.fit();
        let resized = (
            popup.frame.right - popup.frame.left,
            popup.frame.bottom - popup.frame.top,
        ) != (was.right - was.left, was.bottom - was.top);
        Some((popup.hwnd, popup.frame, resized))
    });
    match change {
        Some((hwnd, frame, false)) => unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                frame.left,
                frame.top,
                0,
                0,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSIZE,
            );
            let _ = InvalidateRect(Some(hwnd), None, false);
        },
        Some((hwnd, frame, true)) => unsafe { replace(hwnd, frame) },
        None => {}
    }
}

/// Puts a window of the new size in place of the popup's. DWM does not carry
/// the backdrop through a resize: a popup that grew or shrank went darker
/// until it was opened again, whatever was set again. A new window has it.
pub(super) unsafe fn replace(old: HWND, frame: RECT) {
    unsafe {
        let Ok(hwnd) = create(frame) else {
            return;
        };
        let moved = OPEN.with(|open| {
            let mut open = open.try_borrow_mut().ok()?;
            let popup = open.as_mut()?;
            popup.hwnd = hwnd;
            popup.acrylic = backdrop(hwnd, popup.theme);
            popup.target = None;
            popup.brush = None;
            popup.icons.clear();
            popup.pressed = None;
            popup.tracking = false;
            // The old window had the pointer.
            popup.picking = None;
            Some(())
        });
        if moved.is_none() {
            let _ = DestroyWindow(hwnd);
            return;
        }
        SetTimer(Some(hwnd), TICK, 250, None);
        // The new window takes the foreground first, so the old one's
        // going is not taken for a click outside.
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = UpdateWindow(hwnd);
        let _ = DestroyWindow(old);
    }
}

/// Whether the window is the open popup's, and not one it replaced.
pub(super) fn current(hwnd: HWND) -> bool {
    OPEN.with(|open| {
        open.try_borrow()
            .is_ok_and(|open| open.as_ref().is_some_and(|p| p.hwnd == hwnd))
    })
}

/// The actions pressed since the last call, in order.
pub fn take() -> Vec<Action> {
    ACTIONS.with(|actions| std::mem::take(&mut *actions.borrow_mut()))
}

/// Closes the popup if it is open, keeping a name being typed.
pub fn close() {
    OPEN.with(|open| {
        if let Ok(mut open) = open.try_borrow_mut()
            && let Some(popup) = open.as_mut()
        {
            popup.finish_rename(true);
        }
    });
    let popup = OPEN.with(|open| open.try_borrow_mut().ok().and_then(|mut o| o.take()));
    if let Some(popup) = popup {
        let hwnd = popup.hwnd;
        drop(popup);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// Closes the popup and lets go of what it draws with, before the program
/// ends. Left to the thread's end, the factories were released after
/// Direct2D and DirectWrite had gone, and the exit crashed.
pub fn shut() {
    close();
    let graphics =
        GRAPHICS.with(|graphics| graphics.try_borrow_mut().ok().and_then(|mut g| g.take()));
    if let Some(graphics) = graphics {
        let com = graphics.com;
        drop(graphics);
        if com {
            unsafe { CoUninitialize() };
        }
    }
}

/// The pixel under the pointer, on whichever monitor, as an opaque ARGB;
/// none when it cannot be read.
fn pixel_under_pointer() -> Option<u32> {
    unsafe {
        let mut point = POINT::default();
        GetCursorPos(&mut point).ok()?;
        let hdc = GetDC(None);
        if hdc.is_invalid() {
            return None;
        }
        let pixel = GetPixel(hdc, point.x, point.y);
        ReleaseDC(None, hdc);
        if pixel.0 == CLR_INVALID {
            return None;
        }
        Some(argb_of(pixel))
    }
}

/// A GDI color as ARGB, opaque.
fn argb_of(color: COLORREF) -> u32 {
    let c = color.0;
    0xFF00_0000 | (c & 0xFF) << 16 | c & 0xFF00 | (c >> 16) & 0xFF
}

/// Windows' color dialog over the popup, started at the box's color; the
/// color picked keeps the box's opacity and goes to the owner as the
/// settings window's command. The dialog's custom colors are kept for the
/// run.
unsafe fn pick_color(hwnd: HWND) {
    let current = OPEN.with(|open| {
        open.try_borrow()
            .ok()?
            .as_ref()
            .filter(|p| p.hwnd == hwnd)?
            .swatch_color()
    });
    let Some(current) = current else {
        return;
    };
    let mut custom = CUSTOM_COLORS.get();
    let mut choose = CHOOSECOLORW {
        lStructSize: size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: hwnd,
        rgbResult: rename::colorref(current),
        lpCustColors: custom.as_mut_ptr(),
        Flags: CC_RGBINIT | CC_FULLOPEN | CC_ANYCOLOR,
        ..Default::default()
    };
    DIALOG.set(true);
    let picked = unsafe { ChooseColorW(&mut choose) }.as_bool();
    DIALOG.set(false);
    CUSTOM_COLORS.set(custom);
    if !picked {
        return;
    }
    let color = current & 0xFF00_0000 | argb_of(choose.rgbResult) & 0x00FF_FFFF;
    OPEN.with(|open| {
        if let Ok(open) = open.try_borrow()
            && let Some(popup) = open.as_ref().filter(|p| p.hwnd == hwnd)
        {
            popup.tell(Action::Apply(format!(
                "color\t{}",
                crate::style::argb_text(color)
            )));
        }
    });
}

/// Windows 11's rounded corners, and the translucent backdrop of its flyouts
/// behind a client area drawn clear; whether the backdrop took. Without it
/// the popup paints its own background.
pub(super) unsafe fn backdrop(hwnd: HWND, theme: Theme) -> bool {
    unsafe {
        let dark = BOOL::from(!theme.light);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const BOOL as *const c_void,
            size_of::<BOOL>() as u32,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const DWM_WINDOW_CORNER_PREFERENCE as *const c_void,
            size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
        let backdrop = DWMSBT_TRANSIENTWINDOW;
        DwmExtendFrameIntoClientArea(
            hwnd,
            &MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            },
        )
        .is_ok()
            && DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const DWM_SYSTEMBACKDROP_TYPE as *const c_void,
                size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
            )
            .is_ok()
    }
}

/// The popup's window, hidden, at its place and size in pixels.
pub(super) unsafe fn create(frame: RECT) -> windows::core::Result<HWND> {
    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            CLASS,
            w!("LimitPeeker"),
            WS_POPUP,
            frame.left,
            frame.top,
            frame.right - frame.left,
            frame.bottom - frame.top,
            None,
            None,
            Some(HINSTANCE(GetModuleHandleW(None)?.0)),
            None,
        )
    }
}

pub(super) unsafe fn open(owner: HWND, anchor: POINT, view: View) -> windows::core::Result<()> {
    unsafe {
        GRAPHICS.with(|graphics| -> windows::core::Result<()> {
            let mut graphics = graphics.borrow_mut();
            if graphics.is_none() {
                *graphics = Some(Graphics::new()?);
            }
            Ok(())
        })?;
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        if !REGISTERED.get() {
            let registered = RegisterClassW(&WNDCLASSW {
                style: CS_DROPSHADOW | CS_DBLCLKS,
                lpfnWndProc: Some(popup_proc),
                hInstance: instance,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: CLASS,
                ..Default::default()
            });
            if registered == 0 {
                return Err(windows::core::Error::from_thread());
            }
            REGISTERED.set(true);
        }
        let monitor = MonitorFromPoint(anchor, MONITOR_DEFAULTTONEAREST);
        let (mut dpi, mut unused) = (96, 96);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused).is_err() {
            dpi = 96;
        }
        let theme = theme();
        let mut popup = Popup {
            hwnd: HWND::default(),
            owner,
            anchor,
            dpi,
            theme,
            acrylic: false,
            frame: RECT::default(),
            target: None,
            brush: None,
            icons: HashMap::new(),
            view,
            layout: Layout::default(),
            shown: 0.0,
            scroll: 0.0,
            hover: None,
            pressed: None,
            focus: None,
            keyboard: false,
            tracking: false,
            down: None,
            grabbed: None,
            dragging: None,
            over_drag: false,
            refocus: None,
            renaming: None,
            picking: None,
        };
        popup.layout = laid_out(&popup.view);
        let frame = popup.fit();
        popup.frame = frame;
        let hwnd = create(frame)?;
        popup.hwnd = hwnd;
        popup.acrylic = backdrop(hwnd, theme);
        OPEN.with(|open| *open.borrow_mut() = Some(popup));
        SetTimer(Some(hwnd), TICK, 250, None);
        let _ = ShowWindow(hwnd, SW_SHOW);
        // With the foreground the shell or the hook handed this process.
        let _ = SetForegroundWindow(hwnd);
        Ok(())
    }
}
