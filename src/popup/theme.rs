//! The taskbar's light or dark mode and the accent color the popup follows,
//! kept legible.

use std::ffi::c_void;
use std::mem::size_of;
use windows::Win32::System::Registry::*;
use windows::core::{PCWSTR, w};

/// The taskbar's light or dark mode and the accent color the popup follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Theme {
    pub(super) light: bool,
    pub(super) accent: u32,
}

/// The popup's colors, ARGB, as Windows 11's own controls use them; what
/// can be chosen lights up more under the pointer than theirs, with an
/// edge, to show on the acrylic.
pub(super) struct Palette {
    pub(super) background: u32,
    pub(super) primary: u32,
    pub(super) secondary: u32,
    pub(super) tertiary: u32,
    pub(super) fill: u32,
    pub(super) fill_hover: u32,
    pub(super) fill_pressed: u32,
    pub(super) stroke: u32,
    pub(super) subtle_hover: u32,
    pub(super) subtle_pressed: u32,
    /// The edge of what can be chosen, under the pointer.
    pub(super) hover_stroke: u32,
    pub(super) divider: u32,
    pub(super) track: u32,
    pub(super) accent: u32,
    pub(super) on_accent: u32,
    /// What is on but cannot be pressed: a grey fill and quiet text, as
    /// Fluent draws a disabled accent, not the accent faded.
    pub(super) accent_disabled: u32,
    pub(super) on_accent_disabled: u32,
    /// A field taking text, as Fluent's text box with the keyboard.
    pub(super) field: u32,
    /// A card's fill and edge, as Fluent's, for the summary's filter line.
    pub(super) panel: u32,
    pub(super) panel_stroke: u32,
}

impl Theme {
    pub(super) fn palette(self) -> Palette {
        if self.light {
            Palette {
                background: 0xFFF3_F3F3,
                primary: 0xE400_0000,
                secondary: 0x9E00_0000,
                tertiary: 0x7200_0000,
                fill: 0xB3FF_FFFF,
                fill_hover: 0x1400_0000,
                fill_pressed: 0x4DF9_F9F9,
                stroke: 0x1400_0000,
                subtle_hover: 0x1200_0000,
                subtle_pressed: 0x0600_0000,
                hover_stroke: 0x5C00_0000,
                divider: 0x1400_0000,
                track: 0x1F00_0000,
                accent: self.accent,
                on_accent: 0xFFFF_FFFF,
                accent_disabled: 0x3700_0000,
                on_accent_disabled: 0xFFFF_FFFF,
                field: 0xFFFF_FFFF,
                panel: 0xB3FF_FFFF,
                panel_stroke: 0x0F00_0000,
            }
        } else {
            Palette {
                background: 0xFF2C_2C2C,
                primary: 0xFFFF_FFFF,
                secondary: 0xC5FF_FFFF,
                tertiary: 0x87FF_FFFF,
                fill: 0x0FFF_FFFF,
                fill_hover: 0x24FF_FFFF,
                fill_pressed: 0x08FF_FFFF,
                stroke: 0x12FF_FFFF,
                subtle_hover: 0x1FFF_FFFF,
                subtle_pressed: 0x0AFF_FFFF,
                hover_stroke: 0x6BFF_FFFF,
                divider: 0x15FF_FFFF,
                track: 0x26FF_FFFF,
                accent: self.accent,
                on_accent: 0xFF00_0000,
                accent_disabled: 0x28FF_FFFF,
                on_accent_disabled: 0x87FF_FFFF,
                field: 0xFF1E_1E1E,
                panel: 0x0BFF_FFFF,
                panel_stroke: 0x10FF_FFFF,
            }
        }
    }
}

/// The taskbar's mode, and the accent color's lighter shade on a dark one or
/// darker shade on a light one, as Windows 11 draws its own; dark and the
/// default blue when they cannot be read.
pub(super) fn theme() -> Theme {
    let light = light_taskbar();
    // Eight colors, lightest first, as red, green, blue and a spare byte.
    let shade = if light { 4 } else { 1 };
    let accent = accent_palette()
        .map(|p| {
            0xFF00_0000
                | u32::from(p[shade * 4]) << 16
                | u32::from(p[shade * 4 + 1]) << 8
                | u32::from(p[shade * 4 + 2])
        })
        .unwrap_or(if light { 0xFF00_67C0 } else { 0xFF4C_C2FF });
    Theme { light, accent }
}

/// Whether the taskbar is in light mode; dark when it cannot be read. The
/// tray icon follows it too.
pub fn light_taskbar() -> bool {
    dword(
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
        w!("SystemUsesLightTheme"),
    )
    .is_some_and(|value| value != 0)
}

pub(super) fn dword(key: PCWSTR, value: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    Some(data)
}

pub(super) fn accent_palette() -> Option<[u8; 32]> {
    let mut data = [0u8; 32];
    let mut size = data.len() as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent"),
            w!("AccentPalette"),
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    (size as usize == data.len()).then_some(data)
}

#[cfg(test)]
pub(super) use crate::style::contrast;
/// The luminance, contrast and legibility the popup and the boxes share.
pub(super) use crate::style::legible;

/// `argb` with its opacity scaled by `part`.
pub(super) fn faded(argb: u32, part: f32) -> u32 {
    let alpha = ((argb >> 24) as f32 * part).round() as u32;
    alpha.min(255) << 24 | argb & 0x00FF_FFFF
}
