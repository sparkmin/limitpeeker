//! Service icons, drawn in this process at the exact size the box shows them
//! and sent to the hook as pixels. Explorer never opens or parses an image
//! file; a bad one can stop only this process.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::SHCreateStreamOnFileEx;
use windows::core::{HSTRING, Interface, w};

/// The largest icon, in pixels a side. Must match IconMaxPx.
pub const MAX_PX: u32 = 64;

/// Premultiplied BGRA, a pixel a word, row by row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Icon {
    pub size: u32,
    pub pixels: Vec<u32>,
}

/// The side in pixels for an icon of `hundredths` of a DIP on a taskbar at
/// `dpi`, so it maps 1:1 onto the screen.
pub fn side(hundredths: u32, dpi: u32) -> u32 {
    let px = (f64::from(hundredths) * f64::from(dpi) / 9600.0).round();
    px.clamp(1.0, f64::from(MAX_PX)) as u32
}

/// The icon for a service from `dir/{id}.svg` or `.png`, or, when there is
/// none, PC's own chip or a dot in the service's color, which also stands
/// for a file that cannot be drawn. Also says where it came from, for the
/// log.
pub fn load(dir: &Path, id: &str, size: u32, color: u32) -> (Icon, String) {
    for (extension, draw) in [
        (
            "svg",
            svg as fn(&Path, u32) -> windows::core::Result<Vec<u32>>,
        ),
        ("png", png),
    ] {
        let path = dir.join(format!("{id}.{extension}"));
        if path.is_file() {
            return match draw(&path, size) {
                Ok(pixels) => {
                    let (pixels, tinted) = tint(pixels, color);
                    let icon = Icon { size, pixels };
                    (
                        icon,
                        format!("{extension}{}", if tinted { "+tint" } else { "" }),
                    )
                }
                Err(error) => (
                    dot(size, color),
                    format!("dot({extension}: {:#x})", error.code().0),
                ),
            };
        }
    }
    if id == crate::resources::ID {
        return (chip(size, color), "chip".into());
    }
    (dot(size, color), "dot(missing)".into())
}

/// What the files a service's icon may come from are now, by their sizes and
/// times: a file put in, changed or taken away changes it, so the box draws
/// the icon again without a restart.
pub fn stamp(dir: &Path, id: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for extension in ["svg", "png"] {
        std::fs::metadata(dir.join(format!("{id}.{extension}")))
            .ok()
            .map(|meta| (meta.len(), meta.modified().ok()))
            .hash(&mut hasher);
    }
    hasher.finish()
}

/// The folder of the services' logos, beside the settings and the custom
/// boxes, so a new or moved program folder keeps them.
pub fn dir() -> Option<PathBuf> {
    crate::settings::data_dir().map(|dir| dir.join("icons"))
}

/// Moves the logos a person put in `old`, the program's own icons folder
/// that 0.1.0 read, into `new`, but not one `new` has already; then takes
/// `old` away once only 0.1.0's note is left in it. Copied and then
/// deleted, as the two may be on different drives. Says what it did, for
/// the log.
pub fn move_old(old: &Path, new: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(old) else {
        return Vec::new();
    };
    let mut said = Vec::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        let logo = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("svg") || extension.eq_ignore_ascii_case("png")
            });
        let Some(name) = path.file_name().filter(|_| logo && path.is_file()) else {
            continue;
        };
        let to = new.join(name);
        let name = name.to_string_lossy();
        if to.exists() {
            said.push(format!("{name} left there: the new folder has one"));
            continue;
        }
        said.push(
            match std::fs::create_dir_all(new).and_then(|()| std::fs::copy(&path, &to)) {
                Err(error) => format!("{name} not copied: {error}"),
                Ok(_) => match std::fs::remove_file(&path) {
                    Ok(()) => format!("{name} moved to {}", new.display()),
                    Err(error) => format!("{name} copied, but left there too: {error}"),
                },
            },
        );
    }
    // 0.1.0's note says to put the logos there, so it goes, and the folder
    // with it, but only when nothing else is left.
    let left: Vec<PathBuf> = std::fs::read_dir(old)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    let note_only = left.iter().all(|path| {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("README.txt"))
    });
    if note_only {
        let gone = left
            .iter()
            .try_for_each(std::fs::remove_file)
            .and_then(|()| std::fs::remove_dir(old));
        said.push(match gone {
            Ok(()) => "taken away".into(),
            Err(error) => format!("kept: {error}"),
        });
    }
    said
}

/// Runs `make`, which makes a Direct2D render target and so a Direct3D
/// device, while no other thread does. Some security software hooks
/// Direct3D in every process. With such a hook, when two threads make the
/// process's first device at once, the hook calls itself until the stack
/// overflows: the target fails with D2DERR_NO_HARDWARE_DEVICE (0x8899000B)
/// or the thread dies, and every later device in the process fails too. The
/// icons (controller thread) and the popup (tray thread) both make one.
pub fn one_device_at_a_time<T>(make: impl FnOnce() -> T) -> T {
    static DEVICE: Mutex<()> = Mutex::new(());
    let _held = DEVICE.lock().unwrap_or_else(PoisonError::into_inner);
    make()
}

pub(crate) fn factory() -> windows::core::Result<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
}

pub(crate) fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// Draws with Direct2D onto a clear size × size bitmap and gives back its
/// pixels, premultiplied BGRA row by row. `draw` gets the factory and the
/// context between BeginDraw and EndDraw.
pub(crate) fn paint(
    size: u32,
    draw: impl FnOnce(&ID2D1Factory, &ID2D1DeviceContext) -> windows::core::Result<()>,
) -> windows::core::Result<Vec<u32>> {
    unsafe {
        let bitmap = factory()?.CreateBitmap(
            size,
            size,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapCacheOnDemand,
        )?;
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target =
            one_device_at_a_time(|| d2d.CreateWicBitmapRenderTarget(&bitmap, &properties))?;
        let context: ID2D1DeviceContext = target.cast()?;
        context.BeginDraw();
        context.Clear(Some(&D2D1_COLOR_F::default()));
        let drawn = draw(&d2d, &context);
        let ended = context.EndDraw(None, None);
        drawn?;
        ended?;
        drop(context);
        drop(target);
        let mut bytes = vec![0u8; (size * size * 4) as usize];
        bitmap.CopyPixels(std::ptr::null(), size * 4, &mut bytes)?;
        Ok(words(&bytes))
    }
}

/// Draws an SVG with Direct2D onto a size × size bitmap. A file that sets a
/// viewBox is fitted to the icon whatever width and height it declares.
fn svg(path: &Path, size: u32) -> windows::core::Result<Vec<u32>> {
    unsafe {
        let bitmap = factory()?.CreateBitmap(
            size,
            size,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapCacheOnDemand,
        )?;
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target =
            one_device_at_a_time(|| d2d.CreateWicBitmapRenderTarget(&bitmap, &properties))?;
        let context: ID2D1DeviceContext5 = target.cast()?;
        let stream = SHCreateStreamOnFileEx(
            &HSTRING::from(path.as_os_str()),
            STGM_READ.0,
            0,
            false,
            None,
        )?;
        let viewport = D2D_SIZE_F {
            width: size as f32,
            height: size as f32,
        };
        let document = context.CreateSvgDocument(&stream, viewport)?;
        let root = document.GetRoot()?;
        if root.IsAttributeSpecified(w!("viewBox"), None).as_bool() {
            root.SetAttributeValue3(w!("width"), D2D1_SVG_ATTRIBUTE_STRING_TYPE_SVG, w!("100%"))?;
            root.SetAttributeValue3(w!("height"), D2D1_SVG_ATTRIBUTE_STRING_TYPE_SVG, w!("100%"))?;
        }
        context.BeginDraw();
        context.Clear(Some(&D2D1_COLOR_F::default()));
        context.DrawSvgDocument(&document);
        context.EndDraw(None, None)?;
        drop(context);
        drop(target);
        let mut bytes = vec![0u8; (size * size * 4) as usize];
        bitmap.CopyPixels(std::ptr::null(), size * 4, &mut bytes)?;
        Ok(words(&bytes))
    }
}

/// Reads a PNG with WIC, scaled to fit a size × size square and centered.
fn png(path: &Path, size: u32) -> windows::core::Result<Vec<u32>> {
    unsafe {
        let wic = factory()?;
        let decoder = wic.CreateDecoderFromFilename(
            &HSTRING::from(path.as_os_str()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;
        let (mut width, mut height) = (0, 0);
        frame.GetSize(&mut width, &mut height)?;
        let longest = f64::from(width.max(height).max(1));
        let fit = |side: u32| {
            ((f64::from(side) * f64::from(size) / longest).round() as u32).clamp(1, size)
        };
        let (fitted_width, fitted_height) = (fit(width), fit(height));
        let scaler = wic.CreateBitmapScaler()?;
        scaler.Initialize(
            &frame,
            fitted_width,
            fitted_height,
            WICBitmapInterpolationModeHighQualityCubic,
        )?;
        let converter = wic.CreateFormatConverter()?;
        converter.Initialize(
            &scaler,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        let mut bytes = vec![0u8; (fitted_width * fitted_height * 4) as usize];
        converter.CopyPixels(std::ptr::null(), fitted_width * 4, &mut bytes)?;
        let fitted = words(&bytes);
        let mut pixels = vec![0u32; (size * size) as usize];
        let (left, top) = ((size - fitted_width) / 2, (size - fitted_height) / 2);
        for row in 0..fitted_height {
            let from = (row * fitted_width) as usize;
            let to = ((top + row) * size + left) as usize;
            pixels[to..to + fitted_width as usize]
                .copy_from_slice(&fitted[from..from + fitted_width as usize]);
        }
        Ok(pixels)
    }
}

/// Paints a one-color icon in the service's color, keeping its edges. An
/// icon with more than one color keeps its own. Says whether it painted.
fn tint(pixels: Vec<u32>, color: u32) -> (Vec<u32>, bool) {
    // The color of a fairly opaque pixel, undoing the premultiplied alpha.
    let straight = |p: u32| {
        let alpha = p >> 24;
        [16, 8, 0].map(|shift| ((p >> shift) & 0xff) * 255 / alpha)
    };
    let mut solid = pixels.iter().copied().filter(|p| p >> 24 >= 160);
    let Some(first) = solid.next().map(straight) else {
        return (pixels, false);
    };
    let one_color = solid.all(|p| {
        straight(p)
            .iter()
            .zip(first)
            .all(|(a, b)| a.abs_diff(b) <= 24)
    });
    if !one_color {
        return (pixels, false);
    }
    let painted = pixels
        .into_iter()
        .map(|p| premultiply(color, p >> 24))
        .collect();
    (painted, true)
}

/// `color`'s RGB at `alpha`, premultiplied.
fn premultiply(color: u32, alpha: u32) -> u32 {
    let channel = |shift: u32| ((color >> shift) & 0xff) * alpha / 255;
    alpha << 24 | channel(16) << 16 | channel(8) << 8 | channel(0)
}

/// A dot in the service's color, 80% of the icon across, with smooth edges.
fn dot(size: u32, color: u32) -> Icon {
    let center = f64::from(size) / 2.0;
    let radius = f64::from(size) * 0.4;
    sampled(size, color, |x, y| {
        let (dx, dy) = (x - center, y - center);
        dx * dx + dy * dy <= radius * radius
    })
}

/// A chip in the service's color: a frame, a core, and three pins a side.
/// It is PC's, this computer's meters, which is no company's mark, so it
/// comes with the program; a file of that name still goes first.
fn chip(size: u32, color: u32) -> Icon {
    let side = f64::from(size);
    sampled(size, color, |x, y| {
        let (x, y) = (x / side, y / side);
        let square = |half: f64| (x - 0.5).abs() <= half && (y - 0.5).abs() <= half;
        let pins = |along: f64, across: f64| {
            [0.32, 0.5, 0.68].iter().any(|p| (along - p).abs() <= 0.035)
                && (0.06..=0.2).contains(&(0.5 - (across - 0.5).abs()))
        };
        (square(0.3) && !square(0.2)) || square(0.1) || pins(x, y) || pins(y, x)
    })
}

/// A size × size icon in `color` with smooth edges: each pixel as covered
/// as its share of 4 × 4 points, at (x, y) in pixels, that `inside` takes.
fn sampled(size: u32, color: u32, inside: impl Fn(f64, f64) -> bool) -> Icon {
    const SAMPLES: u32 = 4;
    let pixels = (0..size * size)
        .map(|at| {
            let (x, y) = (at % size, at / size);
            let covered = (0..SAMPLES * SAMPLES)
                .filter(|s| {
                    inside(
                        f64::from(x) + (f64::from(s % SAMPLES) + 0.5) / f64::from(SAMPLES),
                        f64::from(y) + (f64::from(s / SAMPLES) + 0.5) / f64::from(SAMPLES),
                    )
                })
                .count() as u32;
            premultiply(color, (color >> 24) * covered / (SAMPLES * SAMPLES))
        })
        .collect();
    Icon { size, pixels }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_map_one_to_one_onto_the_screen() {
        assert_eq!(side(1600, 96), 16);
        assert_eq!(side(1600, 120), 20);
        assert_eq!(side(1600, 144), 24);
        assert_eq!(side(1600, 168), 28);
        // Never larger than the hook's block.
        assert_eq!(side(3200, 480), MAX_PX);
    }

    #[test]
    fn a_dot_is_the_service_color_with_soft_edges() {
        let teal = 0xFF4F_D1C5;
        let icon = dot(16, teal);
        assert_eq!(icon.pixels.len(), 256);
        assert_eq!(icon.pixels[8 * 16 + 8], teal);
        assert_eq!(icon.pixels[0], 0);
        // Some edge pixel is partly covered.
        assert!(icon.pixels.iter().any(|p| (1..255).contains(&(p >> 24))));
    }

    #[test]
    fn pc_has_a_chip_of_its_own_and_a_file_still_goes_first() {
        // PC's icon comes with the program: a chip, framed with a core and
        // pins, in its color; others with no file are a dot.
        let none = std::env::temp_dir().join("limitpeeker-no-icons");
        let (chip, from) = load(&none, crate::resources::ID, 32, 0xFFA6_D96A);
        assert_eq!(from, "chip");
        let at = |x: u32, y: u32| chip.pixels[(y * 32 + x) as usize] >> 24;
        // The frame, the core and a pin are solid; the gap inside the frame
        // and the corner are clear.
        assert_eq!((at(16, 7), at(16, 16), at(15, 3)), (255, 255, 255));
        assert_eq!((at(11, 11), at(0, 0)), (0, 0));
        assert_eq!(chip.pixels[16 * 32 + 16], 0xFFA6_D96A);
        assert_eq!(load(&none, "gpt", 16, 0xFFFF_FFFF).1, "dot(missing)");
    }

    #[test]
    fn one_color_icons_take_the_service_color_and_others_keep_theirs() {
        let black = 0xFF00_0000;
        let half_black = 0x8000_0000;
        let (painted, tinted) = tint(vec![black, half_black, 0], 0xFFFF_0000);
        assert!(tinted);
        assert_eq!(painted, [0xFFFF_0000, 0x8080_0000, 0]);
        let (kept, tinted) = tint(vec![black, 0xFFFF_FFFF], 0xFFFF_0000);
        assert!(!tinted);
        assert_eq!(kept, [black, 0xFFFF_FFFF]);
    }

    /// A folder of its own for one test's icon file.
    fn folder(test: &str, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("limitpeeker-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), bytes).unwrap();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        dir
    }

    #[test]
    fn an_svg_is_fitted_to_the_icon_and_painted() {
        // A square over the middle half of its viewBox, in a file that
        // declares another size and carries a title, as downloaded icons do.
        let dir = folder(
            "svg",
            "gpt.svg",
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 24 24"><title>test</title><rect x="6" y="6" width="12" height="12"/></svg>"#,
        );
        let (icon, source) = load(&dir, "gpt", 16, 0xFF4F_D1C5);
        assert_eq!(source, "svg+tint");
        assert_eq!(icon.pixels[8 * 16 + 8], 0xFF4F_D1C5);
        assert_eq!(icon.pixels[0], 0);
        assert_eq!(icon.pixels[15 * 16 + 15], 0);
    }

    #[test]
    fn svgs_drawn_on_many_threads_at_once_are_all_drawn() {
        // Threads that make the process's first Direct3D device together
        // broke a security product's Direct3D hook on a quarter of runs.
        let dir = folder(
            "threads",
            "gpt.svg",
            br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12"/></svg>"#,
        );
        let start = std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (dir, start) = (dir.clone(), start.clone());
                std::thread::spawn(move || {
                    unsafe {
                        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    }
                    start.wait();
                    load(&dir, "gpt", 16, 0xFF4F_D1C5).1
                })
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap(), "svg+tint");
        }
    }

    #[test]
    fn a_png_keeps_its_shape_and_is_centered() {
        // Two red pixels side by side: twice as wide as tall.
        const RED_2X1: [u8; 71] = [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 244, 34, 127, 138, 0, 0, 0, 14, 73, 68, 65, 84, 120, 156, 99, 248, 207,
            192, 240, 31, 132, 1, 17, 247, 3, 253, 227, 197, 245, 239, 0, 0, 0, 0, 73, 69, 78, 68,
            174, 66, 96, 130,
        ];
        let dir = folder("png", "claude.png", &RED_2X1);
        let (icon, source) = load(&dir, "claude", 16, 0xFFF4_A27A);
        assert_eq!(source, "png+tint");
        // 16 wide and 8 tall, rows 4 to 11.
        assert_eq!(icon.pixels[8 * 16 + 8] >> 24, 0xFF);
        assert_eq!(icon.pixels[2 * 16 + 8], 0);
        assert_eq!(icon.pixels[13 * 16 + 8], 0);
    }

    #[test]
    fn a_file_that_cannot_be_drawn_becomes_a_dot() {
        let dir = folder("broken", "gemini.svg", b"not an svg");
        let (icon, source) = load(&dir, "gemini", 16, 0xFF7A_A7FF);
        assert!(source.starts_with("dot(svg: "), "{source}");
        assert_eq!(icon, dot(16, 0xFF7A_A7FF));
    }

    #[test]
    fn a_missing_file_becomes_a_dot() {
        let (icon, source) = load(Path::new("no such folder"), "gpt", 16, 0xFF4F_D1C5);
        assert_eq!(source, "dot(missing)");
        assert_eq!(icon, dot(16, 0xFF4F_D1C5));
    }

    #[test]
    fn an_icons_stamp_changes_as_its_file_comes_changes_and_goes() {
        // So the box draws a logo put in without a restart; another
        // service's file does not touch it.
        let dir = folder("stamp", "grok.png", b"other");
        let none = stamp(&dir, "claude");
        assert_eq!(stamp(&dir, "claude"), none);
        std::fs::write(dir.join("claude.svg"), b"<svg/>").unwrap();
        let put = stamp(&dir, "claude");
        assert_ne!(put, none);
        std::fs::write(dir.join("claude.svg"), b"<svg width='16'/>").unwrap();
        let changed = stamp(&dir, "claude");
        assert_ne!(changed, put);
        std::fs::write(dir.join("grok.png"), b"changed").unwrap();
        assert_eq!(stamp(&dir, "claude"), changed);
        std::fs::remove_file(dir.join("claude.svg")).unwrap();
        assert_eq!(stamp(&dir, "claude"), none);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn old_logos_move_beside_the_settings() {
        // 0.1.0's icons folder beside the program: two logos and its note.
        let root = std::env::temp_dir().join(format!("limitpeeker-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join("program").join("icons");
        let new = root.join("data").join("icons");
        std::fs::create_dir_all(&old).unwrap();
        for name in ["gpt.svg", "claude.png", "README.txt"] {
            std::fs::write(old.join(name), name).unwrap();
        }
        let said = move_old(&old, &new);
        let read = |path: PathBuf| std::fs::read_to_string(path).unwrap();
        assert_eq!(read(new.join("gpt.svg")), "gpt.svg");
        assert_eq!(read(new.join("claude.png")), "claude.png");
        assert!(!new.join("README.txt").exists());
        assert!(!old.exists(), "{said:?}");
        // With the old folder gone, there is nothing to do.
        assert!(move_old(&old, &new).is_empty());

        // A logo the new folder has already stays on both sides, and so
        // do the old folder and its note.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("gpt.svg"), "older").unwrap();
        std::fs::write(old.join("README.txt"), "note").unwrap();
        move_old(&old, &new);
        assert_eq!(read(new.join("gpt.svg")), "gpt.svg");
        assert_eq!(read(old.join("gpt.svg")), "older");
        assert!(old.join("README.txt").exists());

        // Another file of the person's keeps the note and the folder too.
        std::fs::remove_file(old.join("gpt.svg")).unwrap();
        std::fs::write(old.join("mine.txt"), "mine").unwrap();
        move_old(&old, &new);
        assert!(old.join("README.txt").exists());
        assert!(old.join("mine.txt").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
