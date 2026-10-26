//! The program icon, which the tests draw into assets/limitpeeker.ico: a dark
//! tile, the taskbar up close, and a lens over two gauges. build.rs puts the
//! file into the EXE, and the settings window uses it too. Nothing here is in
//! the EXE itself.

use crate::icons;
use crate::tray_icon::{color, straight};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::StructuredStorage::IPropertyBag2;
use windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET,
};
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows_numerics::Vector2;

/// The file, beside the sources.
const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/limitpeeker.ico");
/// The sizes Windows 11 asks of a program icon.
const SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 96, 256];
/// Up to this size the icon is its own simpler drawing on a 16 by 16 grid;
/// above it, the drawing on a 64 by 64 grid made smaller.
const SMALL: u32 = 20;
const TILE: u32 = 0xFF3A_3A38;
const GLASS: u32 = 0xFF1F_1F1D;
const RIM: u32 = 0xFFFF_FFFF;
const TRACK: u32 = 0x33FF_FFFF;
const AMBER: u32 = 0xFFEF_9F27;
const TEAL: u32 = 0xFF5D_CAA5;

/// The icon at `size`, premultiplied BGRA row by row.
fn draw(size: u32) -> windows::core::Result<Vec<u32>> {
    let small = size <= SMALL;
    let s = size as f32 / if small { 16.0 } else { 64.0 };
    let at = |x: f32, y: f32| Vector2 { X: x * s, Y: y * s };
    let rect = |left: f32, top: f32, right: f32, bottom: f32| D2D_RECT_F {
        left: left * s,
        top: top * s,
        right: right * s,
        bottom: bottom * s,
    };
    let rounded = |left, top, right, bottom, radius: f32| D2D1_ROUNDED_RECT {
        rect: rect(left, top, right, bottom),
        radiusX: radius * s,
        radiusY: radius * s,
    };
    let disc = |x: f32, y: f32, radius: f32| D2D1_ELLIPSE {
        point: at(x, y),
        radiusX: radius * s,
        radiusY: radius * s,
    };
    icons::paint(size, |d2d, context| unsafe {
        let brush = |argb: u32| context.CreateSolidColorBrush(&color(argb), None);
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
        let (tile, glass, rim) = (brush(TILE)?, brush(GLASS)?, brush(RIM)?);
        if small {
            // Edge to edge, and each gauge two whole pixels high.
            context.FillRoundedRectangle(&rounded(0.0, 0.0, 16.0, 16.0, 3.5), &tile);
            context.DrawLine(at(6.0, 6.0), at(2.8, 2.8), &rim, 2.0 * s, &round);
            let lens = disc(9.5, 9.5, 4.6);
            context.FillEllipse(&lens, &glass);
            context.DrawEllipse(&lens, &rim, 1.5 * s, &round);
            for (top, fill, end) in [(7.0, AMBER, 10.0), (10.0, TEAL, 9.0)] {
                context.FillRectangle(&rect(7.0, top, 12.0, top + 2.0), &brush(TRACK)?);
                context.FillRectangle(&rect(7.0, top, end, top + 2.0), &brush(fill)?);
            }
        } else {
            context.FillRoundedRectangle(&rounded(4.0, 4.0, 60.0, 60.0, 13.0), &tile);
            context.DrawLine(at(25.0, 25.0), at(13.5, 13.5), &rim, 6.0 * s, &round);
            let lens = disc(36.0, 36.0, 15.0);
            context.FillEllipse(&lens, &glass);
            context.DrawEllipse(&lens, &rim, 4.0 * s, &round);
            for (top, fill, end) in [(29.0, AMBER, 40.0), (37.5, TEAL, 33.0)] {
                context.FillRoundedRectangle(
                    &rounded(26.0, top, 46.0, top + 5.5, 2.75),
                    &brush(TRACK)?,
                );
                context
                    .FillRoundedRectangle(&rounded(26.0, top, end, top + 5.5, 2.75), &brush(fill)?);
            }
        }
        Ok(())
    })
}

/// An icon file's image as a bitmap: its header, the colors bottom row first
/// and not premultiplied, and a mask of zeros, which the alpha overrides.
fn bitmap(size: u32, pixels: &[u32]) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    for word in [40, size, size * 2] {
        out.extend(word.to_le_bytes());
    }
    out.extend(1u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    for word in [0, size * size * 4 + mask_row * size, 0, 0, 0, 0] {
        out.extend(word.to_le_bytes());
    }
    for row in (0..size).rev() {
        for &pixel in &pixels[(row * size) as usize..((row + 1) * size) as usize] {
            out.extend(straight(pixel).to_le_bytes());
        }
    }
    out.extend(vec![0u8; (mask_row * size) as usize]);
    out
}

/// An icon file's image as a PNG, for the largest size.
fn png(size: u32, pixels: &[u32]) -> windows::core::Result<Vec<u8>> {
    unsafe {
        let memory = SHCreateMemStream(None).ok_or_else(windows::core::Error::from_thread)?;
        let encoder =
            icons::factory()?.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        encoder.Initialize(&memory, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        encoder.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
        let frame: IWICBitmapFrameEncode = frame.ok_or_else(windows::core::Error::empty)?;
        frame.Initialize(None::<&IPropertyBag2>)?;
        frame.SetSize(size, size)?;
        let mut format = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut format)?;
        let bytes: Vec<u8> = pixels
            .iter()
            .flat_map(|&pixel| straight(pixel).to_le_bytes())
            .collect();
        frame.WritePixels(size, size * 4, &bytes)?;
        frame.Commit()?;
        encoder.Commit()?;
        let mut stat = STATSTG::default();
        memory.Stat(&mut stat, STATFLAG_NONAME)?;
        memory.Seek(0, STREAM_SEEK_SET, None)?;
        let mut out = vec![0u8; stat.cbSize as usize];
        let mut read = 0;
        memory
            .Read(out.as_mut_ptr().cast(), out.len() as u32, Some(&mut read))
            .ok()?;
        out.truncate(read as usize);
        Ok(out)
    }
}

/// An icon file of these images, each with its size.
fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![0, 0, 1, 0];
    out.extend((images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (size, data) in images {
        // 256 is written as 0.
        let side = if *size >= 256 { 0 } else { *size as u8 };
        out.extend([side, side, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in images {
        out.extend(data);
    }
    out
}

/// The whole file, every size drawn now.
fn file() -> windows::core::Result<Vec<u8>> {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let mut images = Vec::new();
    for size in SIZES {
        let pixels = draw(size)?;
        let data = if size >= 256 {
            png(size, &pixels)?
        } else {
            bitmap(size, &pixels)
        };
        images.push((size, data));
    }
    Ok(ico(&images))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pixel's color at (x, y) on the grid the size is drawn on.
    fn on_grid(pixels: &[u32], size: u32, grid: f32, x: f32, y: f32) -> u32 {
        let scale = size as f32 / grid;
        let (px, py) = ((x * scale) as u32, (y * scale) as u32);
        straight(pixels[(py * size + px) as usize])
    }
    fn near(a: u32, b: u32) -> bool {
        [24, 16, 8, 0]
            .iter()
            .all(|shift| ((a >> shift) & 0xff).abs_diff((b >> shift) & 0xff) <= 12)
    }

    #[test]
    #[ignore]
    fn write_the_program_icon() {
        // Run with --ignored after changing the drawing, and keep the file.
        std::fs::write(FILE, file().unwrap()).unwrap();
    }

    #[test]
    fn the_icon_is_a_tile_a_lens_and_two_gauges() {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // The small drawing: the tile to the edge, the gauges on whole pixels.
        let small = draw(16).unwrap();
        assert!(near(on_grid(&small, 16, 16.0, 1.5, 8.5), TILE));
        assert!(near(on_grid(&small, 16, 16.0, 8.5, 7.5), AMBER));
        assert!(near(on_grid(&small, 16, 16.0, 7.5, 10.5), TEAL));
        assert!(near(on_grid(&small, 16, 16.0, 8.5, 12.8), GLASS));
        // The large one: a margin, the rim, the handle to the top left.
        let large = draw(256).unwrap();
        assert_eq!(large[0] >> 24, 0);
        assert!(near(on_grid(&large, 256, 64.0, 8.0, 50.0), TILE));
        assert!(near(on_grid(&large, 256, 64.0, 36.0, 21.0), RIM));
        assert!(near(on_grid(&large, 256, 64.0, 19.0, 19.0), RIM));
        assert!(near(on_grid(&large, 256, 64.0, 30.0, 31.5), AMBER));
        assert!(near(on_grid(&large, 256, 64.0, 28.0, 40.0), TEAL));
        assert!(near(on_grid(&large, 256, 64.0, 36.0, 47.0), GLASS));
    }

    #[test]
    fn the_icon_file_is_the_one_drawn_now() {
        // Every size is there, and each bitmap is what the drawing makes
        // now; after changing it, write the file again (see above).
        let file = std::fs::read(FILE).expect("assets/limitpeeker.ico");
        let u16_at = |at: usize| u16::from_le_bytes([file[at], file[at + 1]]);
        let u32_at = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap());
        assert_eq!(
            (u16_at(0), u16_at(2), u16_at(4)),
            (0, 1, SIZES.len() as u16)
        );
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        for (index, size) in SIZES.into_iter().enumerate() {
            let entry = 6 + 16 * index;
            let side = if size >= 256 { 0 } else { size as u8 };
            assert_eq!((file[entry], file[entry + 1]), (side, side), "{size}");
            let (length, offset) = (u32_at(entry + 8) as usize, u32_at(entry + 12) as usize);
            let data = &file[offset..offset + length];
            if size >= 256 {
                assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
                assert_eq!(&data[16..24], [0, 0, 1, 0, 0, 0, 1, 0]);
                continue;
            }
            assert_eq!(data, bitmap(size, &draw(size).unwrap()), "{size}");
        }
    }

    #[test]
    fn an_icon_file_lists_its_images_then_holds_them() {
        let file = ico(&[(16, vec![1, 2, 3]), (256, vec![4, 5])]);
        assert_eq!(&file[..6], [0, 0, 1, 0, 2, 0]);
        // 16 and 256, which is written as 0, each with its length and offset.
        assert_eq!(
            &file[6..22],
            [16, 16, 0, 0, 1, 0, 32, 0, 3, 0, 0, 0, 38, 0, 0, 0]
        );
        assert_eq!(
            &file[22..38],
            [0, 0, 0, 0, 1, 0, 32, 0, 2, 0, 0, 0, 41, 0, 0, 0]
        );
        assert_eq!(&file[38..], [1, 2, 3, 4, 5]);
        // A bitmap is twice as high for its mask, bottom row first.
        let image = bitmap(2, &[0xFF00_0001, 0xFF00_0002, 0xFF00_0003, 0xFF00_0004]);
        assert_eq!(u32::from_le_bytes(image[8..12].try_into().unwrap()), 4);
        assert_eq!(image[40], 3);
        assert_eq!(image.len(), 40 + 16 + 8);
    }
}
