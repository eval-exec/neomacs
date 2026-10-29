use super::*;
use cosmic_text::{FontSystem, SubpixelBin};
use objc2_core_foundation::CFData;

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
    assert!(native.sources.is_empty());
    assert!(native.backing.is_empty());
}

/// The ink centroid in pen-relative pixels, with y increasing upwards from the
/// baseline (the same frame `RasterGlyphImage::left/top` describe).
fn ink_centroid(mask: &RasterGlyphImage) -> (f32, f32) {
    let width = mask.width as usize;
    let mut total = 0u64;
    let mut sum_x = 0u64;
    let mut sum_y = 0u64;
    for (row, pixels) in mask.data.chunks_exact(width).enumerate() {
        for (col, &coverage) in pixels.iter().enumerate() {
            let coverage = u64::from(coverage);
            total += coverage;
            sum_x += coverage * col as u64;
            sum_y += coverage * row as u64;
        }
    }
    assert!(total > 0, "centroid of an empty mask");
    let x = mask.left as f32 + sum_x as f32 / total as f32 + 0.5;
    let y = mask.top as f32 - (sum_y as f32 / total as f32 + 0.5);
    (x, y)
}

/// A y subpixel bin must move native ink in the same direction, and by the
/// same distance, as the Swash placement path it replaces. The two rasterizers
/// otherwise disagree by 2*y_bin; production currently snaps bins to Zero, so
/// only a direct comparison catches a regression here.
#[test]
fn macos_native_subpixel_placement_matches_swash() {
    let (font, name) = monaco();
    let mut native = NativeRasterizer::default();
    let mut scale_context = swash::scale::ScaleContext::new();
    let mut zero_native = None;
    let mut zero_swash = None;
    for bin in [
        SubpixelBin::Zero,
        SubpixelBin::One,
        SubpixelBin::Two,
        SubpixelBin::Three,
    ] {
        let mut request = key(&font, 'H', 32.0);
        request.y_bin = bin;
        let native_image = native.rasterize(&font, &name, request).unwrap();
        let mut scaler = scale_context
            .builder(font.as_swash())
            .size(32.0)
            .hint(true)
            .build();
        let swash_image = swash::scale::Render::new(&[swash::scale::Source::Outline])
            .format(swash::zeno::Format::Alpha)
            .offset(swash::zeno::Vector::new(0.0, bin.as_float()))
            .render(&mut scaler, request.glyph_id)
            .map(RasterGlyphImage::from_swash)
            .expect("Monaco H has an outline");
        let (_, native_y) = ink_centroid(&native_image);
        let (_, swash_y) = ink_centroid(&swash_image);
        if bin == SubpixelBin::Zero {
            zero_native = Some(native_y);
            zero_swash = Some(swash_y);
            continue;
        }
        let native_shift = native_y - zero_native.expect("zero bin runs first");
        let swash_shift = swash_y - zero_swash.expect("zero bin runs first");
        assert!(
            native_shift.abs() > 0.05,
            "{bin:?}: native ink did not move"
        );
        assert!(
            (native_shift - swash_shift).abs() < 0.35,
            "{bin:?}: native shift {native_shift} vs Swash shift {swash_shift}"
        );
    }
    let (zero_native, zero_swash) = (zero_native.unwrap(), zero_swash.unwrap());
    assert!(
        (zero_native - zero_swash).abs() < 1.0,
        "zero-bin baseline differs: native {zero_native} vs Swash {zero_swash}"
    );
}

/// One backing-file copy serves every raster size, and the atlas keeps one
/// exact face per font id.
#[test]
fn macos_native_shares_one_backing_copy_across_sizes() {
    let (font, name) = monaco();
    let file = backing_hash(font.data());
    let mut native = NativeRasterizer::default();
    let small = native
        .rasterize(&font, &name, key(&font, 'H', 16.0))
        .unwrap();
    assert!(!small.data.is_empty());
    let first = &*native.backing[&file][0] as *const CFData;
    let large = native
        .rasterize(&font, &name, key(&font, 'H', 32.0))
        .unwrap();
    assert!(!large.data.is_empty());
    assert_eq!(native.sources.len(), 1, "one exact face per font id");
    assert_eq!(native.fonts.len(), 2, "one physical-size font per size");
    assert_eq!(native.backing.len(), 1, "one copy of the backing file");
    assert_eq!(native.backing[&file].len(), 1);
    assert_eq!(
        &*native.backing[&file][0] as *const CFData, first,
        "the same CFData is reused across sizes"
    );
}

#[test]
fn macos_native_replay_never_substitutes_a_different_font() {
    let (font, name) = monaco();
    let data = CFData::from_bytes(font.data());
    assert!(open_exact_font(&data, "Not-The-Selected-Font", 16.0).is_none());
    let opened = open_exact_font(&data, &name, 16.0).unwrap();
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
fn macos_native_shares_backing_across_separately_pinned_collection_faces() {
    let mut system = FontSystem::new();
    let mut files = neomacs_font_materializer::FontFileCache::new();
    let mut native = NativeRasterizer::default();
    let mut fonts = Vec::new();
    // Load the first face before registering the second, as redisplay does
    // when another style from the same collection is first encountered.
    for index in [0, 1] {
        let pin = files
            .pin_exact_face(&mut system, "/System/Library/Fonts/Menlo.ttc", index)
            .unwrap();
        let id = pin.fontdb_id();
        let name = system.db().face(id).unwrap().post_script_name.clone();
        let font = system.get_font(id, fontdb::Weight::NORMAL).unwrap();
        let mask = native
            .rasterize(&font, &name, key(&font, 'H', 16.0))
            .unwrap();
        assert!(mask.data.iter().any(|&p| p > 0));
        assert_eq!(
            unsafe { native.sources[&id].as_ref().unwrap().post_script_name() }.to_string(),
            name,
            "backing reuse must preserve the selected collection member"
        );
        fonts.push(font);
    }
    assert_ne!(fonts[0].data().as_ptr(), fonts[1].data().as_ptr());
    assert_eq!(fonts[0].data(), fonts[1].data());
    assert_eq!(native.sources.len(), 2);
    assert_eq!(
        native.backing.len(),
        1,
        "one copy across independent mappings"
    );
    assert_eq!(native.backing.values().next().unwrap().len(), 1);
}

#[test]
fn macos_native_backing_compares_bytes_on_hash_collision() {
    let mut backing = FxHashMap::default();
    // Force a collision with different, equally sized contents. A hash is
    // only an accelerator, never the authority for selecting font bytes.
    backing.insert(backing_hash(b"new"), vec![CFData::from_bytes(b"old")]);
    let data = intern_backing(&mut backing, b"new");
    assert_eq!(data.to_vec(), b"new");
    assert_eq!(backing[&backing_hash(b"new")].len(), 2);
    let again = intern_backing(&mut backing, &b"new".to_vec());
    assert!(std::ptr::eq(&*data, &*again));
    assert_eq!(backing[&backing_hash(b"new")].len(), 2);
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
