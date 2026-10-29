use super::*;
use cosmic_text::{FontSystem, SubpixelBin};

fn monaco() -> (std::sync::Arc<Font>, String) {
    let mut system = FontSystem::new();
    let id = system
        .db()
        .query(&fontdb::Query {
            families: &[fontdb::Family::Name("Monaco")],
            ..Default::default()
        })
        .expect("macOS supplies Monaco");
    let name = system.db().face(id).unwrap().post_script_name.clone();
    (system.get_font(id, fontdb::Weight::NORMAL).unwrap(), name)
}

fn key(font: &Font, ch: char, size: f32) -> CacheKey {
    CacheKey {
        font_id: font.id(),
        glyph_id: font.as_swash().charmap().map(ch),
        font_size_bits: size.to_bits(),
        x_bin: SubpixelBin::Zero,
        y_bin: SubpixelBin::Zero,
        font_weight: fontdb::Weight::NORMAL,
        flags: CacheKeyFlags::empty(),
    }
}

#[test]
fn macos_native_masks_preserve_baseline_scale_and_grayscale() {
    let (font, name) = monaco();
    let mut native = NativeRasterizer::default();
    for size in [16.0, 32.0] {
        for ch in ['t', 'H', 'm', 'i', '0', '9', 'g'] {
            let mask = native
                .rasterize(&font, &name, key(&font, ch, size))
                .unwrap();
            assert_eq!(mask.content, RasterContent::Mask);
            assert_eq!(mask.data.len(), (mask.width * mask.height) as usize);
            assert!(mask.data.iter().any(|&a| a > 0));
            assert!(mask.data.iter().any(|&a| a > 0 && a < 255));
            assert!(mask.top > 0);
            assert!(mask.width < (size * 2.0) as u32);
            assert!(mask.height < (size * 2.0) as u32);
            // Bounds must describe ink, not transparent padding that would
            // make adjacent rows falsely report overlapping glyphs.
            let width = mask.width as usize;
            assert!(mask.data[..width].iter().any(|&a| a != 0));
            assert!(mask.data[mask.data.len() - width..].iter().any(|&a| a != 0));
            assert!(mask.data.chunks_exact(width).any(|row| row[0] != 0));
            assert!(mask.data.chunks_exact(width).any(|row| row[width - 1] != 0));
        }
    }
    assert_eq!(native.fonts.len(), 2, "reuse native fonts across glyphs");
    let small = native
        .rasterize(&font, &name, key(&font, 'H', 16.0))
        .unwrap();
    let large = native
        .rasterize(&font, &name, key(&font, 'H', 32.0))
        .unwrap();
    assert!((large.top - 2 * small.top).abs() <= 3);
    native.clear();
    assert!(native.fonts.is_empty());
}

#[test]
fn macos_native_replay_never_substitutes_a_different_font() {
    let (font, name) = monaco();
    assert!(open_exact_font(font.data(), "Not-The-Selected-Font", 16.0).is_none());
    let opened = open_exact_font(font.data(), &name, 16.0).unwrap();
    assert_eq!(unsafe { opened.post_script_name() }.to_string(), name);
    let mut native = NativeRasterizer::default();
    let space = native
        .rasterize(&font, &name, key(&font, ' ', 16.0))
        .unwrap();
    assert!(space.data.is_empty());
    let mut synthetic = key(&font, 't', 16.0);
    synthetic.flags = CacheKeyFlags::FAKE_ITALIC;
    assert!(native.rasterize(&font, &name, synthetic).is_none());
}

#[test]
fn macos_default_rasterization_does_not_assume_an_rgb_lcd() {
    assert!(
        !neomacs_layout_engine::font::subpixel::default_subpixel_order()
            .allows_horizontal_subpixel()
    );
}

/// Inspect the same exact Monaco glyphs with Swash (upper row) and Core
/// Graphics (lower row), on dark and light backgrounds. This isolates native
/// smoothing from WGPU, font selection and layout. No screenshot permissions
/// or running editor are needed.
#[test]
#[ignore = "manual raster comparison; set NEOMACS_FONT_PROBE_PNG"]
fn macos_native_visual_probe() {
    let path = std::env::var("NEOMACS_FONT_PROBE_PNG").expect("output PNG path");
    let (font, name) = monaco();
    let mut native = NativeRasterizer::default();
    let mut swash = swash::scale::ScaleContext::new();
    let mut sheet = image::RgbImage::new(1120, 240);
    for (i, pixel) in sheet.pixels_mut().enumerate() {
        *pixel = image::Rgb(if i / 1120 < 120 {
            [40, 44, 52]
        } else {
            [250, 250, 250]
        });
    }
    for (row, baseline) in [45, 100, 165, 220].into_iter().enumerate() {
        let mut pen = 20.0f32;
        let mut ink = 0u64;
        for ch in "Monaco Regular 16: t H m i 0123456789 gyp".chars() {
            let request = key(&font, ch, 32.0);
            let mask = if row % 2 == 1 {
                native.rasterize(&font, &name, request).unwrap()
            } else {
                let mut scaler = swash.builder(font.as_swash()).size(32.0).hint(true).build();
                let Some(mask) = swash::scale::Render::new(&[swash::scale::Source::Outline])
                    .format(swash::zeno::Format::Alpha)
                    .render(&mut scaler, request.glyph_id)
                else {
                    pen += font
                        .as_swash()
                        .glyph_metrics(&[])
                        .scale(32.0)
                        .advance_width(request.glyph_id);
                    continue;
                };
                RasterGlyphImage::from_swash(mask)
            };
            for y in 0..mask.height {
                for x in 0..mask.width {
                    let coverage = mask.data[(y * mask.width + x) as usize];
                    ink += u64::from(coverage);
                    let dx = pen.round() as i32 + mask.left + x as i32;
                    let dy = baseline - mask.top + y as i32;
                    if dx < 0 || dy < 0 || dx >= sheet.width() as i32 || dy >= sheet.height() as i32
                    {
                        continue;
                    }
                    let pixel = sheet.get_pixel_mut(dx as u32, dy as u32);
                    let fg = if row < 2 {
                        [190u16, 195, 205]
                    } else {
                        [40, 44, 52]
                    };
                    for (out, fg) in pixel.0.iter_mut().zip(fg) {
                        *out = ((u16::from(*out) * (255 - u16::from(coverage))
                            + fg * u16::from(coverage))
                            / 255) as u8;
                    }
                }
            }
            pen += font
                .as_swash()
                .glyph_metrics(&[])
                .scale(32.0)
                .advance_width(request.glyph_id);
        }
        eprintln!("row {row}: total coverage {ink}");
    }
    sheet.save(path).unwrap();
}
