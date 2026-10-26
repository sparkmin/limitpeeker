//! The tray icon, drawn in this process at the size the taskbar shows it: a
//! dial while the box is on the taskbar, the tightest limit's value while it
//! is not, and a badge in the corner for what is wrong.

use crate::choice::Choice;
use crate::icons;
use crate::style::Fill;
use crate::usage;
use crate::xaml::Reason;
use std::mem::size_of;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, FindWindowW, HICON, ICONINFO, SM_CXICON, SM_CXSMICON,
};
use windows::core::{PCWSTR, w};
use windows_numerics::Vector2;

/// The ink of the dial and the number on a dark taskbar and on a light one,
/// one color as Windows 11 draws its own tray icons.
const ON_DARK: u32 = 0xFFFF_FFFF;
const ON_LIGHT: u32 = 0xFF1A_1A1A;
/// The badges' colors and their marks'.
const AMBER: u32 = 0xFFEF_9F27;
const ON_AMBER: u32 = 0xFF41_2402;
const RED: u32 = 0xFFE2_4B4A;
const ON_RED: u32 = 0xFFFF_FFFF;
/// Where the badge sits on the 16 by 16 grid, how large it is, and the ring
/// cleared around it so it stands apart from what is under it.
const BADGE: (f32, f32) = (12.3, 12.3);
const BADGE_RADIUS: f32 = 3.9;
const CLEARED_RADIUS: f32 = 4.5;

/// What the middle of the icon shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Face {
    /// The box on the taskbar shows the figures; the icon is a handle.
    Dial,
    /// The tightest limit's value, as the box would show it.
    Number(u8),
    /// A value of 100 or more, which two digits do not hold.
    Full,
}

/// What is wrong, shown in the corner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Badge {
    /// A shown service's last read failed, so the figures may be old.
    Failed,
    /// Restarting Explorer brings the box back.
    Restart,
    /// The box cannot show for another reason, which the popup gives.
    Off,
}

/// Everything the icon is drawn from; the same look draws the same icon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Look {
    pub face: Face,
    pub badge: Option<Badge>,
    /// The taskbar is light, so the ink is dark.
    pub light: bool,
    /// Pixels a side.
    pub size: u32,
}

/// The dial while the box is on the taskbar, or before any value; else the
/// tightest value, or a full disc from 100.
pub fn face(on_taskbar: bool, tightest: Option<u8>) -> Face {
    match tightest {
        _ if on_taskbar => Face::Dial,
        None => Face::Dial,
        Some(value) if value >= 100 => Face::Full,
        Some(value) => Face::Number(value),
    }
}

/// A notification's dial: its value as the box shows it, a full disc, the
/// failed read's badge, or the dial alone, drawn large.
pub fn alert_look(face: crate::notify::Face, light: bool) -> Look {
    use crate::notify::Face as Alert;
    let (face, badge) = match face {
        Alert::Value(value) if value >= 100 => (Face::Full, None),
        Alert::Value(value) => (Face::Number(value), None),
        Alert::Full => (Face::Full, None),
        Alert::Failed => (Face::Dial, Some(Badge::Failed)),
        Alert::Plain => (Face::Dial, None),
    };
    Look {
        face,
        badge,
        light,
        size: large_side(),
    }
}

/// The most pressing badge: a failed read, as the figures may be old; then
/// what restarting Explorer mends; then any other reason the box is off.
pub fn badge(failed: bool, snap_wait: bool, reason: Option<Reason>) -> Option<Badge> {
    if failed {
        Some(Badge::Failed)
    } else if snap_wait || reason.is_some_and(|r| r.restart) {
        Some(Badge::Restart)
    } else {
        reason.map(|_| Badge::Off)
    }
}

/// Of the limits shown, the tightest as they are shown: the most used, or
/// the least left. With a box on the taskbar those are the boxes' limits;
/// without one, the popup's summary is what is seen, so its services and
/// lines count instead. A value kept from before a failed read counts.
/// Also whether a shown service's last read failed.
pub fn figures(choice: &Choice, on_taskbar: bool) -> (Option<u8>, bool) {
    let offers = choice.offers();
    let read = choice.read();
    let fill = choice.fill();
    let mut values = Vec::new();
    let mut failed = false;
    // The services any box shows, or the summary's.
    let ids = if on_taskbar {
        choice.all_services(&offers)
    } else {
        choice.summary(&offers)
    };
    for id in ids {
        // This computer's meters are not limits, and would redraw the icon
        // each second.
        let (Some(offer), Some(service)) = (
            offers.iter().find(|o| o.id == id),
            read.iter()
                .find(|s| s.id == id && s.meaning == usage::Meaning::Limit),
        ) else {
            continue;
        };
        failed |= service.trouble.is_some();
        // Each box's own limits, not the chosen box's; or the summary's
        // lines.
        let labels = if on_taskbar {
            choice.all_limits(offer)
        } else {
            choice.summary_limits(offer)
        };
        for label in labels {
            if let Some(limit) = service.limits.iter().flatten().find(|l| l.label == label)
                && let Some((share, _)) = usage::shown(limit.reading, fill, service.meaning)
            {
                values.push(share);
            }
        }
    }
    let tightest = match fill {
        Fill::Used => values.into_iter().max(),
        Fill::Left => values.into_iter().min(),
    };
    (tightest, failed)
}

/// The notification area's icon size on the taskbar's monitor now.
pub fn side() -> u32 {
    side_at(taskbar_dpi())
}

/// The large icon size on the taskbar's monitor now, for a notification's
/// picture: 32 at 100%, as large as an icon is drawn.
pub fn large_side() -> u32 {
    let px = unsafe { GetSystemMetricsForDpi(SM_CXICON, taskbar_dpi()) };
    (px.max(0) as u32).clamp(32, icons::MAX_PX)
}

/// The DPI of the taskbar's monitor, or else the system's.
fn taskbar_dpi() -> u32 {
    unsafe {
        FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null())
            .ok()
            .map(|taskbar| GetDpiForWindow(taskbar))
            .filter(|&dpi| dpi != 0)
            .unwrap_or_else(|| GetDpiForSystem())
    }
}

/// The small icon size at `dpi`: 16 at 100%, 20 at 125%, 24 at 150%, 32
/// at 200%.
fn side_at(dpi: u32) -> u32 {
    let px = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) };
    (px.max(0) as u32).clamp(16, icons::MAX_PX)
}

/// The icon as a handle for the shell; the caller destroys it.
pub fn icon(look: Look) -> windows::core::Result<HICON> {
    let size = look.size.clamp(16, icons::MAX_PX);
    let pixels: Vec<u32> = draw(Look { size, ..look })?
        .into_iter()
        .map(straight)
        .collect();
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                // Top row first.
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), pixels.len());
        // The alpha decides; the mask, word-aligned rows of zeros, is unused.
        let zeros = vec![0u8; (size.div_ceil(16) * 2 * size) as usize];
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, Some(zeros.as_ptr().cast()));
        let made = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        });
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        made
    }
}

/// A premultiplied pixel with its colors undone, as an icon takes them.
pub(crate) fn straight(pixel: u32) -> u32 {
    let alpha = pixel >> 24;
    if alpha == 0 {
        return 0;
    }
    let channel = |shift: u32| ((((pixel >> shift) & 0xff) * 255 + alpha / 2) / alpha).min(255);
    alpha << 24 | channel(16) << 16 | channel(8) << 8 | channel(0)
}

pub(crate) fn color(argb: u32) -> D2D1_COLOR_F {
    let channel = |shift: u32| ((argb >> shift) & 0xff) as f32 / 255.0;
    D2D1_COLOR_F {
        r: channel(16),
        g: channel(8),
        b: channel(0),
        a: channel(24),
    }
}

/// Draws the icon, premultiplied BGRA row by row, `look.size` a side. The
/// shapes are laid out on a 16 by 16 grid and scaled to the size.
pub fn draw(look: Look) -> windows::core::Result<Vec<u32>> {
    let size = look.size.clamp(16, icons::MAX_PX);
    let s = size as f32 / 16.0;
    let at = |x: f32, y: f32| Vector2 { X: x * s, Y: y * s };
    let disc = |x: f32, y: f32, radius: f32| D2D1_ELLIPSE {
        point: at(x, y),
        radiusX: radius * s,
        radiusY: radius * s,
    };
    let arc = |x: f32, y: f32, radius: f32, sweep, arc_size| D2D1_ARC_SEGMENT {
        point: at(x, y),
        size: D2D_SIZE_F {
            width: radius * s,
            height: radius * s,
        },
        rotationAngle: 0.0,
        sweepDirection: sweep,
        arcSize: arc_size,
    };
    let text = match look.face {
        Face::Number(value) => Some(number(value, look.badge.is_some(), s)?),
        _ => None,
    };
    icons::paint(size, |d2d, context| unsafe {
        let brush = |argb: u32| context.CreateSolidColorBrush(&color(argb), None);
        let ink = brush(if look.light { ON_LIGHT } else { ON_DARK })?;
        let round = d2d.CreateStrokeStyle(
            &D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
            },
            None,
        )?;
        let path =
            |figures: &dyn Fn(&ID2D1GeometrySink)| -> windows::core::Result<ID2D1PathGeometry> {
                let geometry = d2d.CreatePathGeometry()?;
                let sink = geometry.Open()?;
                figures(&sink);
                sink.Close()?;
                Ok(geometry)
            };
        let scale = path(&|sink| {
            // From the left over the top to the right, in two quarters so
            // the half turn is not ambiguous.
            sink.BeginFigure(at(2.5, 11.5), D2D1_FIGURE_BEGIN_HOLLOW);
            for (x, y) in [(8.0, 6.0), (13.5, 11.5)] {
                sink.AddArc(&arc(
                    x,
                    y,
                    5.5,
                    D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    D2D1_ARC_SIZE_SMALL,
                ));
            }
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
        })?;
        context.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        match (look.face, &text) {
            (Face::Number(_), Some((layout, origin))) => {
                context.DrawTextLayout(*origin, layout, &ink, D2D1_DRAW_TEXT_OPTIONS_NONE);
            }
            (Face::Full, _) => context.FillEllipse(&disc(8.0, 8.0, 5.5), &ink),
            _ => {
                context.DrawGeometry(&scale, &ink, 1.8 * s, &round);
                context.DrawLine(at(8.0, 11.5), at(11.0, 7.3), &ink, 1.6 * s, &round);
                context.FillEllipse(&disc(8.0, 11.5, 1.4), &ink);
            }
        }
        if let Some(badge) = look.badge {
            let (fill, on) = match badge {
                Badge::Failed => (RED, ON_RED),
                Badge::Restart | Badge::Off => (AMBER, ON_AMBER),
            };
            let (x, y) = BADGE;
            // Copying a clear disc clears what is under it.
            context.SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_COPY);
            context.FillEllipse(&disc(x, y, CLEARED_RADIUS), &brush(0)?);
            context.SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_SOURCE_OVER);
            context.FillEllipse(&disc(x, y, BADGE_RADIUS), &brush(fill)?);
            let mark = brush(on)?;
            match badge {
                Badge::Restart => {
                    // A clockwise turn open at the top right, and a solid
                    // head at its end pointing on round the turn.
                    let turn = path(&|sink| {
                        sink.BeginFigure(at(13.6, 11.21), D2D1_FIGURE_BEGIN_HOLLOW);
                        sink.AddArc(&arc(
                            11.72,
                            10.7,
                            1.7,
                            D2D1_SWEEP_DIRECTION_CLOCKWISE,
                            D2D1_ARC_SIZE_LARGE,
                        ));
                        sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    })?;
                    let head = path(&|sink| {
                        sink.BeginFigure(at(13.32, 10.12), D2D1_FIGURE_BEGIN_FILLED);
                        sink.AddLine(at(11.1, 9.6));
                        sink.AddLine(at(11.96, 11.95));
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    })?;
                    context.DrawGeometry(&turn, &mark, 1.0 * s, &round);
                    context.FillGeometry(&head, &mark, None::<&ID2D1Brush>);
                }
                Badge::Failed | Badge::Off => {
                    context.DrawLine(at(x, 10.2), at(x, 12.5), &mark, 1.1 * s, &round);
                    context.FillEllipse(&disc(x, 14.1, 0.6), &mark);
                }
            }
        }
        Ok(())
    })
}

/// A value laid out to draw: centered across, or a little left of a badge,
/// with the digits' baseline where they sit in the middle.
fn number(value: u8, badged: bool, s: f32) -> windows::core::Result<(IDWriteTextLayout, Vector2)> {
    unsafe {
        let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let format = write.CreateTextFormat(
            w!("Segoe UI"),
            None::<&IDWriteFontCollection>,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            11.0 * s,
            w!("en-us"),
        )?;
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        let units: Vec<u16> = value.to_string().encode_utf16().collect();
        let layout = write.CreateTextLayout(&units, &format, 16.0 * s, 16.0 * s)?;
        let mut line = [DWRITE_LINE_METRICS::default()];
        let mut lines = 0;
        layout.GetLineMetrics(Some(&mut line), &mut lines)?;
        let left = if badged { -0.8 } else { 0.0 };
        let origin = Vector2 {
            X: left * s,
            Y: 11.8 * s - line[0].baseline,
        };
        Ok((layout, origin))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{self, Background};
    use crate::usage::{Trouble, mock_at};
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;

    const UNIX: i64 = 1_790_000_000;

    fn drawn(face: Face, badge: Option<Badge>, light: bool, size: u32) -> Vec<u32> {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        draw(Look {
            face,
            badge,
            light,
            size,
        })
        .unwrap()
    }
    /// Whether two colors' red, green and blue are near.
    fn near(a: u32, b: u32) -> bool {
        [16, 8, 0]
            .iter()
            .all(|shift| ((a >> shift) & 0xff).abs_diff((b >> shift) & 0xff) <= 24)
    }

    #[test]
    fn the_face_shows_where_the_figures_are() {
        // The box has them, or none came yet: the dial. Else the value, and
        // a full disc for what two digits do not hold.
        assert_eq!(face(true, Some(84)), Face::Dial);
        assert_eq!(face(false, None), Face::Dial);
        assert_eq!(face(false, Some(84)), Face::Number(84));
        assert_eq!(face(false, Some(0)), Face::Number(0));
        assert_eq!(face(false, Some(100)), Face::Full);
        assert_eq!(face(false, Some(130)), Face::Full);
    }

    #[test]
    fn the_most_pressing_badge_shows() {
        let restart = Reason::new("restart", true);
        let off = Reason::new("off", false);
        assert_eq!(badge(false, false, None), None);
        assert_eq!(badge(false, false, Some(off)), Some(Badge::Off));
        assert_eq!(badge(false, false, Some(restart)), Some(Badge::Restart));
        assert_eq!(badge(false, true, None), Some(Badge::Restart));
        assert_eq!(badge(false, true, Some(off)), Some(Badge::Restart));
        assert_eq!(badge(true, true, Some(restart)), Some(Badge::Failed));
    }

    #[test]
    fn the_tightest_shown_limit_is_taken_as_the_box_shows_it() {
        let choice = Choice::new(&style::LOOKS[0], Fill::Used, Background::Box);
        assert_eq!(figures(&choice, true), (None, false));
        // Three services and two limits each show, so Opus 7d does not
        // count: the most used is Claude's 7d at 90, the least left 10.
        choice.offer(&mock_at(0, UNIX));
        assert_eq!(figures(&choice, true), (Some(90), false));
        choice.set_fill(Fill::Left);
        assert_eq!(figures(&choice, true), (Some(10), false));
        choice.set_fill(Fill::Used);
        choice.offer(&mock_at(1, UNIX));
        assert_eq!(figures(&choice, true), (Some(100), false));
        // A value kept from before a failed read counts.
        let mut services = mock_at(2, UNIX);
        choice.offer(&services);
        assert_eq!(figures(&choice, true), (Some(42), false));
        // Only a shown service's failed read is told.
        let trouble = || {
            Some(Trouble {
                why: "offline".into(),
                retry_at: UNIX,
            })
        };
        services[3].trouble = trouble();
        choice.offer(&services);
        assert_eq!(figures(&choice, true), (Some(42), false));
        services[1].trouble = trouble();
        choice.offer(&services);
        assert_eq!(figures(&choice, true), (Some(42), true));
    }

    #[test]
    fn the_summary_counts_while_no_box_is_on_the_taskbar() {
        // Claude is taken out of the box but kept in the summary. On the
        // taskbar the boxes decide; off it the summary does, with every
        // line of Claude's.
        let choice = Choice::new(&style::LOOKS[0], Fill::Used, Background::Box);
        choice.offer(&mock_at(0, UNIX));
        let offers = choice.offers();
        choice.toggle_summary_service(&offers, "claude");
        choice.toggle_summary_service(&offers, "claude");
        choice.toggle_service(&offers, "claude");
        assert!(!choice.all_services(&offers).contains(&"claude"));
        assert!(choice.summary(&offers).contains(&"claude"));
        assert_ne!(figures(&choice, true).0, Some(90));
        assert_eq!(figures(&choice, false), (Some(90), false));
    }

    #[test]
    fn every_box_counts_for_the_tightest_limit() {
        // The first box shows Claude's 5h only; a second box shows its Opus
        // 7d too. With the first box chosen, the second's limit still
        // counts.
        let choice = Choice::new(&style::LOOKS[0], Fill::Used, Background::Box);
        choice.offer(&mock_at(0, UNIX));
        let claude = choice
            .offers()
            .into_iter()
            .find(|o| o.id == "claude")
            .expect("claude is offered");
        choice.toggle(&claude, "7d");
        assert_eq!(choice.shown(&claude), vec!["5h".to_string()]);
        assert!(choice.add_tile());
        choice.toggle(&claude, "Opus 7d");
        assert_eq!(
            choice.shown(&claude),
            vec!["5h".to_string(), "Opus 7d".to_string()]
        );
        assert!(choice.select(0));
        assert_eq!(choice.shown(&claude), vec!["5h".to_string()]);
        assert_eq!(figures(&choice, true), (Some(60), false));
    }

    #[test]
    fn the_icon_is_as_large_as_the_taskbar_shows_it() {
        assert_eq!(side_at(96), 16);
        assert_eq!(side_at(120), 20);
        assert_eq!(side_at(144), 24);
        assert_eq!(side_at(192), 32);
    }

    #[test]
    fn every_look_is_drawn_in_its_ink_and_badge_color() {
        use Badge::*;
        for size in [16, 20, 24, 32] {
            for light in [false, true] {
                let ink = if light { ON_LIGHT } else { ON_DARK };
                for face in [Face::Dial, Face::Number(7), Face::Number(84), Face::Full] {
                    for badge in [None, Some(Failed), Some(Restart), Some(Off)] {
                        let pixels = drawn(face, badge, light, size);
                        let what = format!("{face:?} {badge:?} light={light} {size}px");
                        assert_eq!(pixels.len(), (size * size) as usize, "{what}");
                        let has = |argb: u32| {
                            pixels
                                .iter()
                                .any(|&p| p >> 24 >= 200 && near(straight(p), argb))
                        };
                        assert!(has(ink), "{what}");
                        match badge {
                            Some(Failed) => assert!(has(RED), "{what}"),
                            Some(_) => assert!(has(AMBER), "{what}"),
                            None => assert!(!has(RED) && !has(AMBER), "{what}"),
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_ring_round_the_badge_is_cleared() {
        // At 64 pixels the ring is wider than a pixel: one inside it, on the
        // full disc, is clear with a badge and solid without.
        let at = |pixels: &[u32]| pixels[37 * 64 + 37] >> 24;
        assert_eq!(at(&drawn(Face::Full, None, false, 64)), 255);
        assert!(at(&drawn(Face::Full, Some(Badge::Off), false, 64)) <= 16);
    }

    #[test]
    fn a_number_is_centered_and_moves_aside_for_a_badge() {
        // Where the ink lies across, weighted by its opacity.
        let middle = |pixels: &[u32]| {
            let (mut sum, mut weight) = (0.0, 0.0);
            for (at, &p) in pixels.iter().enumerate() {
                if near(straight(p), ON_DARK) {
                    let alpha = f64::from(p >> 24);
                    sum += ((at % 32) as f64 + 0.5) * alpha;
                    weight += alpha;
                }
            }
            sum / weight
        };
        let alone = middle(&drawn(Face::Number(84), None, false, 32));
        let badged = middle(&drawn(Face::Number(84), Some(Badge::Off), false, 32));
        assert!((alone - 16.0).abs() < 1.5, "{alone}");
        assert!(badged < alone - 0.5, "{badged} {alone}");
        let one = middle(&drawn(Face::Number(7), None, false, 32));
        assert!((one - 16.0).abs() < 1.5, "{one}");
    }

    #[test]
    fn the_icon_is_a_handle_the_shell_can_take() {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let made = icon(Look {
            face: Face::Dial,
            badge: Some(Badge::Restart),
            light: false,
            size: 24,
        })
        .unwrap();
        assert!(!made.is_invalid());
        unsafe { DestroyIcon(made) }.unwrap();
    }

    #[test]
    fn premultiplied_colors_are_undone() {
        assert_eq!(straight(0), 0);
        assert_eq!(straight(0xFFFF_FFFF), 0xFFFF_FFFF);
        assert_eq!(straight(0x8040_4040), 0x8080_8080);
        assert_eq!(straight(0x401A_0000), 0x4068_0000);
    }
}
