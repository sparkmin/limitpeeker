//! Drawing the popup: the Direct2D and DirectWrite resources made once
//! for the tray thread, and the popup painted from its layout and theme.

use super::*;
use crate::icons;
use crate::usage;
use std::collections::HashMap;
use std::ffi::c_void;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PCWSTR, w};

/// What the popup draws with, made once for the tray thread.
pub(super) struct Graphics {
    pub(super) d2d: ID2D1Factory,
    pub(super) write: IDWriteFactory,
    pub(super) formats: HashMap<(Font, Align), IDWriteTextFormat>,
    /// Whether COM was started here, so `shut` ends it.
    pub(super) com: bool,
}

impl Graphics {
    pub(super) fn new() -> windows::core::Result<Self> {
        unsafe {
            // The service icons are read with WIC.
            let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let mut fonts: Option<IDWriteFontCollection> = None;
            write.GetSystemFontCollection(&mut fonts, false)?;
            let has = |name: PCWSTR| {
                fonts.as_ref().is_some_and(|fonts: &IDWriteFontCollection| {
                    let (mut index, mut exists) = (0, BOOL(0));
                    fonts.FindFamilyName(name, &mut index, &mut exists).is_ok() && exists.as_bool()
                })
            };
            let text = if has(w!("Segoe UI Variable Text")) {
                w!("Segoe UI Variable Text")
            } else {
                w!("Segoe UI")
            };
            let glyphs = if has(w!("Segoe Fluent Icons")) {
                w!("Segoe Fluent Icons")
            } else {
                w!("Segoe MDL2 Assets")
            };
            let mut formats = HashMap::new();
            for font in [
                Font::Title,
                Font::Strong,
                Font::Body,
                Font::Caption,
                Font::Picked,
                Font::Glyph,
                Font::Small,
            ] {
                let (family, weight, size) = match font {
                    Font::Title => (text, DWRITE_FONT_WEIGHT_SEMI_BOLD, 14.0),
                    Font::Strong => (text, DWRITE_FONT_WEIGHT_SEMI_BOLD, 13.0),
                    Font::Body => (text, DWRITE_FONT_WEIGHT_NORMAL, 13.0),
                    Font::Caption => (text, DWRITE_FONT_WEIGHT_NORMAL, 12.0),
                    Font::Picked => (text, DWRITE_FONT_WEIGHT_SEMI_BOLD, 12.0),
                    Font::Glyph => (glyphs, DWRITE_FONT_WEIGHT_NORMAL, 14.0),
                    Font::Small => (glyphs, DWRITE_FONT_WEIGHT_NORMAL, 10.0),
                };
                for align in [Align::Start, Align::End, Align::Center] {
                    let format = write.CreateTextFormat(
                        family,
                        None::<&IDWriteFontCollection>,
                        weight,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        size,
                        w!("en-us"),
                    )?;
                    format.SetTextAlignment(match align {
                        Align::Start => DWRITE_TEXT_ALIGNMENT_LEADING,
                        Align::End => DWRITE_TEXT_ALIGNMENT_TRAILING,
                        Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                    })?;
                    format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                    format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                    let sign = write.CreateEllipsisTrimmingSign(&format)?;
                    format.SetTrimming(
                        &DWRITE_TRIMMING {
                            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                            delimiter: 0,
                            delimiterCount: 0,
                        },
                        &sign,
                    )?;
                    formats.insert((font, align), format);
                }
            }
            Ok(Self {
                d2d,
                write,
                formats,
                com,
            })
        }
    }

    /// A text's width in DIPs.
    pub(super) fn measure(&self, text: &str, font: Font) -> f32 {
        let units: Vec<u16> = text.encode_utf16().collect();
        let width = self
            .formats
            .get(&(font, Align::Start))
            .and_then(|format| unsafe {
                let layout = self
                    .write
                    .CreateTextLayout(&units, format, 10_000.0, 100.0)
                    .ok()?;
                let mut metrics = DWRITE_TEXT_METRICS::default();
                layout.GetMetrics(&mut metrics).ok()?;
                Some(metrics.widthIncludingTrailingWhitespace)
            });
        width.unwrap_or(text.chars().count() as f32 * 7.0)
    }
}

impl Popup {
    /// Draws the popup; a lost device is made again on the next paint.
    pub(super) fn draw(&mut self) {
        if self.target.is_none() && self.make_target().is_err() {
            return;
        }
        let (Some(target), Some(brush)) = (self.target.clone(), self.brush.clone()) else {
            return;
        };
        let palette = self.theme.palette();
        let scale = self.scale();
        let scroll = self.scroll;
        // DIPs from the layout onto whole pixels, scrolled.
        let snap = |v: f32| (v * scale).round() / scale;
        let place = |at: Rect| D2D_RECT_F {
            left: snap(at.x),
            top: snap(at.y - scroll),
            right: snap(at.x + at.w),
            bottom: snap(at.y - scroll + at.h),
        };
        let paint = |argb: u32| {
            let color = D2D1_COLOR_F {
                r: ((argb >> 16) & 0xff) as f32 / 255.0,
                g: ((argb >> 8) & 0xff) as f32 / 255.0,
                b: (argb & 0xff) as f32 / 255.0,
                a: (argb >> 24) as f32 / 255.0,
            };
            unsafe { brush.SetColor(&color) };
        };
        let round = |at: Rect, radius: f32| D2D1_ROUNDED_RECT {
            rect: place(at),
            radiusX: radius,
            radiusY: radius,
        };
        // The bright edge of what can be chosen, under the pointer, a hair
        // inside it.
        let edge = |at: Rect, radius: f32| {
            paint(palette.hover_stroke);
            let inner = Rect {
                x: at.x + 0.5,
                y: at.y + 0.5,
                w: at.w - 1.0,
                h: at.h - 1.0,
            };
            unsafe {
                target.DrawRoundedRectangle(
                    &round(inner, radius),
                    &brush,
                    1.0,
                    None::<&ID2D1StrokeStyle>,
                )
            };
        };
        let ink = |ink: Ink| match ink {
            Ink::Primary => palette.primary,
            Ink::Secondary => palette.secondary,
            Ink::Tertiary => palette.tertiary,
            Ink::Warning => legible(usage::WARNING, palette.background),
            Ink::Caution => legible(usage::CAUTION, palette.background),
        };
        let made = GRAPHICS.with(|g| {
            g.borrow()
                .as_ref()
                .map(|g| (g.formats.clone(), g.d2d.clone()))
        });
        let Some((formats, d2d)) = made else {
            return;
        };
        // A point of the layout, scrolled and on whole pixels.
        let point = |x: f32, y: f32| windows_numerics::Vector2 {
            X: snap(x),
            Y: snap(y - scroll),
        };
        let write = |at: Rect, text: &str, font: Font, align: Align, argb: u32| {
            if text.is_empty() {
                return;
            }
            let Some(format) = formats.get(&(font, align)) else {
                return;
            };
            let units: Vec<u16> = text.encode_utf16().collect();
            paint(argb);
            unsafe {
                target.DrawText(
                    &units,
                    format,
                    &place(at),
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        };
        unsafe {
            target.BeginDraw();
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let clear = if self.acrylic {
                D2D1_COLOR_F::default()
            } else {
                let b = palette.background;
                D2D1_COLOR_F {
                    r: ((b >> 16) & 0xff) as f32 / 255.0,
                    g: ((b >> 8) & 0xff) as f32 / 255.0,
                    b: (b & 0xff) as f32 / 255.0,
                    a: 1.0,
                }
            };
            target.Clear(Some(&clear));
        }
        let parts = self.layout.parts.clone();
        for part in &parts {
            match part {
                Part::Text {
                    at,
                    text,
                    font,
                    align,
                    ink: tone,
                    faded: dim,
                } => {
                    let argb = ink(*tone);
                    write(
                        *at,
                        text,
                        *font,
                        *align,
                        if *dim { faded(argb, FADED) } else { argb },
                    );
                }
                Part::Bar {
                    at,
                    share,
                    color,
                    warn,
                    faded: dim,
                } => {
                    paint(palette.track);
                    unsafe { target.FillRoundedRectangle(&round(*at, 2.0), &brush) };
                    if let Some(share) = share.filter(|&s| s > 0) {
                        let base = warn.color().unwrap_or(*color);
                        let argb = legible(base, palette.background);
                        paint(if *dim { faded(argb, FADED) } else { argb });
                        let colored = Rect {
                            w: at.w * f32::from(share) / 100.0,
                            ..*at
                        };
                        unsafe { target.FillRoundedRectangle(&round(colored, 2.0), &brush) };
                    }
                }
                Part::Icon { at, id, color } => {
                    if let Some(bitmap) = self.icon(&target, id, *color, &palette) {
                        unsafe {
                            target.DrawBitmap(
                                &bitmap,
                                Some(&place(*at)),
                                1.0,
                                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                                None,
                            );
                        }
                    }
                }
                Part::Rule { at } => {
                    paint(palette.divider);
                    let line = Rect {
                        h: 1.0 / scale,
                        ..*at
                    };
                    unsafe { target.FillRectangle(&place(line), &brush) };
                }
                // The chosen tab and its card as one shape, so the tab runs
                // into the card with no edge between, filled and edged as a
                // card.
                Part::TabCard { tab, card } => {
                    let r = CARD_RADIUS;
                    let arc = |x: f32, y: f32| D2D1_ARC_SEGMENT {
                        point: point(x, y),
                        size: D2D_SIZE_F {
                            width: r,
                            height: r,
                        },
                        rotationAngle: 0.0,
                        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                        arcSize: D2D1_ARC_SIZE_SMALL,
                    };
                    let shape = || -> windows::core::Result<ID2D1PathGeometry> {
                        let path = unsafe { d2d.CreatePathGeometry()? };
                        let sink = unsafe { path.Open()? };
                        let (left, top, right, bottom) =
                            (card.x, card.y, card.x + card.w, card.y + card.h);
                        let (start, roof, end) = (tab.x, tab.y, tab.x + tab.w);
                        let joined = tab.w > 0.0;
                        unsafe {
                            sink.BeginFigure(point(left, bottom - r), D2D1_FIGURE_BEGIN_FILLED);
                            if joined && start <= left + 0.5 {
                                sink.AddLine(point(left, roof + r));
                                sink.AddArc(&arc(left + r, roof));
                            } else {
                                sink.AddLine(point(left, top + r));
                                sink.AddArc(&arc(left + r, top));
                                if joined {
                                    sink.AddLine(point(start, top));
                                    sink.AddLine(point(start, roof + r));
                                    sink.AddArc(&arc(start + r, roof));
                                }
                            }
                            if joined {
                                sink.AddLine(point(end - r, roof));
                                sink.AddArc(&arc(end, roof + r));
                                sink.AddLine(point(end, top));
                            }
                            sink.AddLine(point(right - r, top));
                            sink.AddArc(&arc(right, top + r));
                            sink.AddLine(point(right, bottom - r));
                            sink.AddArc(&arc(right - r, bottom));
                            sink.AddLine(point(left + r, bottom));
                            sink.AddArc(&arc(left, bottom - r));
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            sink.Close()?;
                        }
                        Ok(path)
                    };
                    if let Ok(path) = shape() {
                        paint(palette.panel);
                        unsafe { target.FillGeometry(&path, &brush, None::<&ID2D1Brush>) };
                        paint(palette.panel_stroke);
                        unsafe {
                            target.DrawGeometry(
                                &path,
                                &brush,
                                1.0 / scale,
                                None::<&ID2D1StrokeStyle>,
                            )
                        };
                    }
                }
                // A row of segments: one frame, as an unchosen chip's.
                Part::Segments { at } => {
                    paint(palette.fill);
                    unsafe { target.FillRoundedRectangle(&round(*at, 4.0), &brush) };
                    paint(palette.stroke);
                    let half = 0.5 / scale;
                    let edge = Rect {
                        x: at.x + half,
                        y: at.y + half,
                        w: at.w - 2.0 * half,
                        h: at.h - 2.0 * half,
                    };
                    unsafe {
                        target.DrawRoundedRectangle(
                            &round(edge, 4.0),
                            &brush,
                            1.0 / scale,
                            None::<&ID2D1StrokeStyle>,
                        )
                    };
                }
                // A Fluent card: a faint fill and a fainter one-pixel edge.
                Part::Panel { at } => {
                    paint(palette.panel);
                    unsafe { target.FillRoundedRectangle(&round(*at, CARD_RADIUS), &brush) };
                    paint(palette.panel_stroke);
                    let half = 0.5 / scale;
                    let edge = Rect {
                        x: at.x + half,
                        y: at.y + half,
                        w: at.w - 2.0 * half,
                        h: at.h - 2.0 * half,
                    };
                    unsafe {
                        target.DrawRoundedRectangle(
                            &round(edge, CARD_RADIUS),
                            &brush,
                            1.0 / scale,
                            None::<&ID2D1StrokeStyle>,
                        )
                    };
                }
            }
        }
        let dragged = self.dragging.map(|(from, _)| from);
        // The filter line's icons, taken before the controls are walked, as
        // taking one may load it.
        let kinds: Vec<Kind> = self
            .layout
            .controls
            .iter()
            .map(|c| c.kind.clone())
            .collect();
        let filter_icons: Vec<Option<ID2D1Bitmap>> = kinds
            .iter()
            .map(|kind| match *kind {
                Kind::Filter { id, color, .. } => self.icon(&target, id, color, &palette),
                _ => None,
            })
            .collect();
        for (index, control) in self.layout.controls.iter().enumerate() {
            let hover = control.enabled && self.hover == Some(index) && dragged.is_none();
            let pressed = hover && self.pressed == Some(index);
            // The chip dragged fades where it was.
            let alpha = if !control.enabled || (dragged.is_some() && control.drag == dragged) {
                FADED
            } else {
                1.0
            };
            match control.kind {
                Kind::Chip { on } | Kind::Toggle { on } => {
                    let fill = match (on, pressed, hover) {
                        _ if on && !control.enabled => palette.accent_disabled,
                        (true, true, _) => faded(palette.accent, 0.8),
                        (true, false, true) => faded(palette.accent, 0.9),
                        (true, false, false) => palette.accent,
                        (false, true, _) => faded(palette.fill_pressed, alpha),
                        (false, false, true) => faded(palette.fill_hover, alpha),
                        (false, false, false) => faded(palette.fill, alpha),
                    };
                    paint(fill);
                    unsafe { target.FillRoundedRectangle(&round(control.at, 4.0), &brush) };
                    // What can be chosen shows its edge under the pointer.
                    if !on {
                        paint(faded(
                            if hover {
                                palette.hover_stroke
                            } else {
                                palette.stroke
                            },
                            alpha,
                        ));
                        let inner = Rect {
                            x: control.at.x + 0.5,
                            y: control.at.y + 0.5,
                            w: control.at.w - 1.0,
                            h: control.at.h - 1.0,
                        };
                        unsafe {
                            target.DrawRoundedRectangle(
                                &round(inner, 4.0),
                                &brush,
                                1.0,
                                None::<&ID2D1StrokeStyle>,
                            )
                        };
                    }
                    let text = match (on, control.enabled) {
                        (true, false) => palette.on_accent_disabled,
                        (true, true) => palette.on_accent,
                        (false, _) => faded(palette.primary, alpha),
                    };
                    let font = if matches!(control.kind, Kind::Toggle { .. }) {
                        Font::Caption
                    } else {
                        Font::Body
                    };
                    write(control.at, &control.text, font, Align::Center, text);
                }
                // The chosen tab is its card's; the others a quiet name
                // that lights up under the pointer; a hidden box's name is
                // faint.
                Kind::Tab { on, hidden } => {
                    if hover && !on {
                        paint(if pressed {
                            palette.subtle_pressed
                        } else {
                            palette.subtle_hover
                        });
                        let inner = Rect {
                            y: control.at.y + 2.0,
                            h: control.at.h - 4.0,
                            ..control.at
                        };
                        unsafe { target.FillRoundedRectangle(&round(inner, 4.0), &brush) };
                    }
                    write(
                        control.at,
                        &control.text,
                        Font::Body,
                        Align::Center,
                        faded(
                            match (hidden, on) {
                                (true, _) => palette.tertiary,
                                (false, true) => palette.primary,
                                (false, false) => palette.secondary,
                            },
                            alpha,
                        ),
                    );
                }
                Kind::Glyph { glyph } => {
                    if hover {
                        paint(if pressed {
                            palette.subtle_pressed
                        } else {
                            palette.subtle_hover
                        });
                        unsafe { target.FillRoundedRectangle(&round(control.at, 4.0), &brush) };
                    }
                    write(
                        control.at,
                        &glyph.to_string(),
                        Font::Small,
                        Align::Center,
                        faded(palette.secondary, alpha),
                    );
                }
                // Edit or Done: the words and a chevron in the accent, on a
                // wash of it that deepens under the pointer.
                Kind::Disclose { glyph } => {
                    let wash = match (pressed, hover) {
                        (true, _) => 0.32,
                        (false, true) => 0.24,
                        (false, false) => 0.14,
                    };
                    paint(faded(palette.accent, wash));
                    unsafe { target.FillRoundedRectangle(&round(control.at, 4.0), &brush) };
                    let Rect { x, y, w, h } = control.at;
                    write(
                        Rect {
                            x: x + DISCLOSE_PAD,
                            y,
                            w: w - 2.0 * DISCLOSE_PAD - DISCLOSE_GLYPH_W,
                            h,
                        },
                        &control.text,
                        Font::Body,
                        Align::Start,
                        palette.accent,
                    );
                    write(
                        Rect {
                            x: x + w - DISCLOSE_PAD - DISCLOSE_GLYPH_W,
                            y,
                            w: DISCLOSE_GLYPH_W,
                            h,
                        },
                        &glyph.to_string(),
                        Font::Small,
                        Align::Center,
                        palette.accent,
                    );
                }
                // In its row's frame: the chosen one filled with the accent,
                // inset so the frame shows round it.
                Kind::Segment { on } => {
                    let inner = Rect {
                        x: control.at.x + 2.0,
                        y: control.at.y + 2.0,
                        w: control.at.w - 4.0,
                        h: control.at.h - 4.0,
                    };
                    let fill = match (on, pressed, hover) {
                        _ if on && !control.enabled => Some(palette.accent_disabled),
                        (true, true, _) => Some(faded(palette.accent, 0.8)),
                        (true, false, true) => Some(faded(palette.accent, 0.9)),
                        (true, false, false) => Some(palette.accent),
                        (false, true, _) => Some(palette.subtle_pressed),
                        (false, false, true) => Some(palette.subtle_hover),
                        (false, false, false) => None,
                    };
                    if let Some(fill) = fill {
                        paint(fill);
                        unsafe { target.FillRoundedRectangle(&round(inner, 3.0), &brush) };
                    }
                    if hover && !on {
                        edge(inner, 3.0);
                    }
                    let text = match (on, control.enabled) {
                        (true, false) => palette.on_accent_disabled,
                        (true, true) => palette.on_accent,
                        (false, _) => faded(palette.primary, alpha),
                    };
                    write(control.at, &control.text, Font::Body, Align::Center, text);
                }
                Kind::Item { glyph } => {
                    if hover {
                        paint(if pressed {
                            palette.subtle_pressed
                        } else {
                            palette.subtle_hover
                        });
                        unsafe { target.FillRoundedRectangle(&round(control.at, 4.0), &brush) };
                    }
                    let Rect { x, y, w, h } = control.at;
                    write(
                        Rect {
                            x: x + 6.0,
                            y,
                            w: 24.0,
                            h,
                        },
                        &glyph.to_string(),
                        Font::Glyph,
                        Align::Center,
                        faded(palette.primary, alpha),
                    );
                    write(
                        Rect {
                            x: x + 40.0,
                            y,
                            w: w - 48.0,
                            h,
                        },
                        &control.text,
                        Font::Body,
                        Align::Start,
                        faded(palette.primary, alpha),
                    );
                }
                // No border or fill of a chip: the summary's icon and a
                // small name, bold when on and faint when left out; the last
                // one on is drawn on, though it cannot be pressed.
                Kind::Filter { on, .. } => {
                    if hover {
                        paint(if pressed {
                            palette.subtle_pressed
                        } else {
                            palette.subtle_hover
                        });
                        unsafe { target.FillRoundedRectangle(&round(control.at, 4.0), &brush) };
                        edge(control.at, 4.0);
                    }
                    let Rect { x, y, w, h } = control.at;
                    if let Some(bitmap) = &filter_icons[index] {
                        let icon = Rect {
                            x: x + 6.0,
                            y: y + (h - 16.0) / 2.0,
                            w: 16.0,
                            h: 16.0,
                        };
                        unsafe {
                            target.DrawBitmap(
                                bitmap,
                                Some(&place(icon)),
                                if on { 1.0 } else { FILTER_OFF },
                                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                                None,
                            );
                        }
                    }
                    write(
                        Rect {
                            x: x + 26.0,
                            y,
                            w: w - 26.0,
                            h,
                        },
                        &control.text,
                        if on { Font::Picked } else { Font::Caption },
                        Align::Start,
                        if on {
                            palette.primary
                        } else {
                            palette.tertiary
                        },
                    );
                }
                // The box's color, or the one under the eyedropper, on the
                // popup's own backdrop so a translucent one shows as such,
                // with an edge that brightens under the pointer.
                Kind::Swatch { color } => {
                    paint(self.picking.unwrap_or(color));
                    unsafe { target.FillRoundedRectangle(&round(control.at, 4.0), &brush) };
                    paint(if hover {
                        palette.hover_stroke
                    } else {
                        palette.stroke
                    });
                    let inner = Rect {
                        x: control.at.x + 0.5,
                        y: control.at.y + 0.5,
                        w: control.at.w - 1.0,
                        h: control.at.h - 1.0,
                    };
                    unsafe {
                        target.DrawRoundedRectangle(
                            &round(inner, 4.0),
                            &brush,
                            1.0,
                            None::<&ID2D1StrokeStyle>,
                        )
                    };
                }
                // The eyedropper, in the accent on a wash of it while it
                // takes a color.
                Kind::Dropper => {
                    let picking = self.picking.is_some();
                    if picking || hover {
                        paint(match (picking, pressed) {
                            (true, _) => faded(palette.accent, 0.24),
                            (false, true) => palette.subtle_pressed,
                            (false, false) => palette.subtle_hover,
                        });
                        unsafe { target.FillRoundedRectangle(&round(control.at, 4.0), &brush) };
                    }
                    write(
                        control.at,
                        &EYEDROPPER_GLYPH.to_string(),
                        Font::Glyph,
                        Align::Center,
                        if picking {
                            palette.accent
                        } else {
                            faded(palette.primary, alpha)
                        },
                    );
                }
            }
            if self.keyboard && self.focus == Some(index) {
                paint(palette.primary);
                let ring = Rect {
                    x: control.at.x - 2.0,
                    y: control.at.y - 2.0,
                    w: control.at.w + 4.0,
                    h: control.at.h + 4.0,
                };
                unsafe {
                    target.DrawRoundedRectangle(
                        &round(ring, 6.0),
                        &brush,
                        2.0,
                        None::<&ID2D1StrokeStyle>,
                    )
                };
            }
        }
        // A line of the summary dragged fades where it was, and the accent
        // marks where it would go.
        if let Some((from, to)) = self.dragging {
            if let Some(drag) = self.layout.drags.get(from).filter(|d| !d.across) {
                paint(faded(palette.background, 0.6));
                unsafe { target.FillRectangle(&place(drag.at), &brush) };
            }
            if let Some(mark) = drop_mark(&self.layout.drags, from, to) {
                paint(palette.accent);
                unsafe { target.FillRoundedRectangle(&round(mark, 1.0), &brush) };
            }
        }
        // Where the view stands when it scrolls.
        let most = self.layout.height - self.shown;
        if most > 0.5 {
            let length = self.shown * self.shown / self.layout.height;
            let thumb = Rect {
                x: WIDTH - 5.0,
                y: scroll + 4.0 + (self.shown - length - 8.0) * scroll / most,
                w: 3.0,
                h: length,
            };
            paint(palette.tertiary);
            unsafe { target.FillRoundedRectangle(&round(thumb, 1.5), &brush) };
        }
        if let Err(error) = unsafe { target.EndDraw(None, None) }
            && error.code() == D2DERR_RECREATE_TARGET
        {
            self.target = None;
            self.brush = None;
            self.icons.clear();
        }
    }

    pub(super) fn make_target(&mut self) -> windows::core::Result<()> {
        let mut client = RECT::default();
        unsafe { GetClientRect(self.hwnd, &mut client)? };
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: self.dpi as f32,
            dpiY: self.dpi as f32,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let window = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd: self.hwnd,
            pixelSize: D2D_SIZE_U {
                width: (client.right - client.left).max(1) as u32,
                height: (client.bottom - client.top).max(1) as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        // The target makes its Direct3D device here, never while the
        // icons make one.
        let target = GRAPHICS.with(|g| {
            g.borrow().as_ref().map(|g| {
                icons::one_device_at_a_time(|| unsafe {
                    g.d2d.CreateHwndRenderTarget(&properties, &window)
                })
            })
        });
        let target = target.ok_or_else(|| windows::core::Error::from(E_FAIL))??;
        let brush = unsafe { target.CreateSolidColorBrush(&D2D1_COLOR_F::default(), None)? };
        self.target = Some(target);
        self.brush = Some(brush);
        self.icons.clear();
        Ok(())
    }

    /// A service's icon, from the same file as the box's, at the monitor's
    /// scale; a one-color icon takes the service color as the popup draws
    /// it.
    pub(super) fn icon(
        &mut self,
        target: &ID2D1HwndRenderTarget,
        id: &'static str,
        color: u32,
        palette: &Palette,
    ) -> Option<ID2D1Bitmap> {
        let dpi = self.dpi;
        self.icons
            .entry(id)
            .or_insert_with(|| {
                let side = (16.0 * dpi as f32 / 96.0).round() as u32;
                let dir = icons::dir()?;
                let (icon, _) = icons::load(&dir, id, side, legible(color, palette.background));
                let properties = D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: dpi as f32,
                    dpiY: dpi as f32,
                };
                unsafe {
                    target.CreateBitmap(
                        D2D_SIZE_U {
                            width: icon.size,
                            height: icon.size,
                        },
                        Some(icon.pixels.as_ptr() as *const c_void),
                        icon.size * 4,
                        &properties,
                    )
                }
                .ok()
            })
            .clone()
    }
}
