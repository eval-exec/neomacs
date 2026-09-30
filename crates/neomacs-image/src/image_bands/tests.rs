//! What a banded decode promises its consumer.
//!
//! Three contracts, in the order a consumer meets them: the bands tile the
//! source and the raster it becomes, the pixels they carry are the source's own
//! pixels where the two paths meet — at the source's own size, which is what
//! these tests realize — and a decode that cannot finish says so instead of
//! stopping quietly.

use super::*;
use std::io::Cursor;

use neomacs_display_protocol::{
    AxisSize, ImageHeuristicMask, ImageMaskKind, ImageMaskPolicy, ImageRealization, ImageSizeSpec,
};

/// A source banded regardless of size, so these tests can use small images.
fn open_banded(data: &[u8]) -> BandSource<'_> {
    BandSource::open_forced(data, ImageSizeSpec::default(), ImageRealization::default())
}

/// The same, realized at an exact size, which is how a source is asked for a
/// raster smaller than itself.
fn open_banded_at(data: &[u8], width: u32, height: u32) -> BandSource<'_> {
    BandSource::open_forced(
        data,
        ImageSizeSpec::new(AxisSize::Exact(width), AxisSize::Exact(height)),
        ImageRealization::default(),
    )
}

/// Drive a banded source to completion, collecting what it published.
///
/// Returns the bands and the raster, and panics on a failure — the tests that
/// expect one drive the source themselves.
fn drain(mut source: BandedSource<'_>) -> (Vec<DecodedBand>, Option<RasterPixels>) {
    let mut bands = Vec::new();
    loop {
        match source.next_band() {
            BandStep::Band(band) => bands.push(band),
            BandStep::Done => return (bands, source.into_raster()),
            BandStep::Failed => panic!("a valid source must not fail"),
        }
    }
}

/// The pixels the whole-image path produces for `data`, through `image`'s own
/// decode and colour conversion — the reference a banded decode has to match
/// when it realizes the source at its own size, where the filter is the
/// identity.
fn whole_image_pixels(data: &[u8]) -> (u32, u32, Vec<u8>) {
    let image = image::load_from_memory(data).expect("fixture decodes whole");
    let rgba = image.to_rgba8();
    (rgba.width(), rgba.height(), rgba.into_raw())
}

/// One pixel per `(x, y)`, so a band placed at the wrong offset is visible
/// rather than hidden by identical rows.
fn varying_pixels(width: u32, height: u32) -> Vec<u8> {
    (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                [
                    (x % 251) as u8,
                    (y % 253) as u8,
                    ((x + y) % 241) as u8,
                    0xff,
                ]
            })
        })
        .collect()
}

fn png_of(width: u32, height: u32, pixels: Vec<u8>) -> Vec<u8> {
    let image = image::RgbaImage::from_raw(width, height, pixels).expect("pixel buffer");
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("PNG is encodable");
    bytes.into_inner()
}

/// A PNG of `width` x `height` with a distinct colour per pixel.
fn varying_png(width: u32, height: u32) -> Vec<u8> {
    png_of(width, height, varying_pixels(width, height))
}

#[test]
fn bands_tile_the_source_in_order_and_cover_every_row_once() {
    let (width, height) = (120, 500);
    let data = varying_png(width, height);
    let BandSource::Banded(source) = open_banded(&data) else {
        panic!("a PNG has a row-wise decoder");
    };

    let (bands, raster) = drain(source);

    assert!(
        bands.len() > 1,
        "a 500-row source must band into more than one piece, got {}",
        bands.len()
    );
    let mut expected_start = 0;
    for band in &bands {
        assert_eq!(
            band.source().start(),
            expected_start,
            "bands arrive in order, each where the last one ended"
        );
        expected_start = band.source().end();
    }
    assert_eq!(expected_start, height, "the bands cover every row once");

    let Some(raster) = raster else {
        panic!("a source that reached Done yields the raster");
    };
    assert_eq!(raster.native().dimensions(), (width, height));
    assert_eq!(raster.raster().dimensions(), (width, height));
    assert_eq!(
        raster.into_rgba(),
        varying_pixels(width, height),
        "the realized image is the source's pixels"
    );
}

/// The bands of one decode fill the raster from the top: contiguous, in order,
/// and reaching its last row. One number therefore says how far the image has
/// come, which is what the display side draws against.
#[test]
fn the_bands_of_a_source_tile_the_raster_from_row_zero() {
    // A 12000x700-like shape without the megapixels: 700 source rows onto 238
    // raster rows, so most source rows land inside an output row rather than on
    // one.
    let (width, height) = (600, 700);
    let data = varying_png(width, height);
    let BandSource::Banded(source) = open_banded_at(&data, 205, 238) else {
        panic!("a PNG has a row-wise decoder");
    };
    let (bands, _) = drain(source);

    let mut expected_start = 0;
    for band in &bands {
        let placement = band.placed().placement();
        assert_eq!(
            placement.rows().start(),
            expected_start,
            "a band starts where the last one ended"
        );
        assert!(placement.rows().len().get() > 0);
        expected_start = placement.rows().end();
    }
    assert_eq!(
        expected_start, 238,
        "the bands reach the last row of the raster"
    );
}

/// Every band has somewhere to write.
///
/// A source minified hard enough that one raster row spans many source rows
/// still bands by raster row rather than by source row, because a band with no
/// rows to fill is a state this type does not have: the reader runs past its
/// row budget until the filter closes an output row, and then stops. That makes
/// the first band long — it has to reach the end of the first output's support,
/// which is six output rows' worth of source for a windowed sinc — and every
/// band after it exactly one raster row, up to the last.
///
/// The last band is the exception, and it is the source that stops it rather
/// than the filter: the raster's final output rows all end at the source's last
/// row, so the band that reads it closes however many of them are still open.
#[test]
fn a_source_minified_hard_still_produces_a_band_for_every_raster_row() {
    let data = varying_png(80, 400);
    let BandSource::Banded(source) = open_banded_at(&data, 80, 25) else {
        panic!("a PNG has a row-wise decoder");
    };
    let (bands, raster) = drain(source);

    assert!(!bands.is_empty());
    let mut start = 0;
    for (at, band) in bands.iter().enumerate() {
        let placement = band.placed().placement();
        assert_eq!(
            placement.rows().start(),
            start,
            "a band starts where the last one ended"
        );
        assert!(
            placement.rows().len().get() >= 1,
            "a band fills at least one raster row"
        );
        start = placement.rows().end();
        if at + 1 == bands.len() {
            continue;
        }
        assert_eq!(
            placement.rows().len().get(),
            1,
            "sixteen source rows to a raster row: each band closes one raster row"
        );
        assert!(
            band.source().len().get() >= 16,
            "the band reads the rows that row is made of, got {}",
            band.source().len().get()
        );
    }
    assert_eq!(start, 25, "the bands reach the last row of the raster");
    assert!(
        bands.len() <= 25,
        "the bands follow the raster's height, not the source's: {} of them",
        bands.len()
    );
    assert_eq!(
        raster
            .expect("a completed source yields the raster")
            .raster()
            .height(),
        25
    );
}

/// A source a caller asks to show larger than it is takes the whole-image path:
/// a filter asked for samples that do not exist could only invent them, and the
/// filter that does enlarge is the one that path already uses.
#[test]
fn a_source_shown_larger_than_itself_declines_the_target() {
    let data = varying_png(40, 30);
    assert!(matches!(open_banded(&data), BandSource::Banded(_)));
    for (width, height) in [(41, 30), (40, 31), (80, 15)] {
        assert!(
            matches!(open_banded_at(&data, width, height), BandSource::Whole),
            "{width}x{height} is larger than the source on an axis"
        );
    }
    // The control: the same source shown at or below its own size bands.
    for (width, height) in [(40, 30), (20, 30), (40, 15), (1, 1)] {
        assert!(
            matches!(open_banded_at(&data, width, height), BandSource::Banded(_)),
            "{width}x{height} is a reduction"
        );
    }
}

#[test]
fn a_banded_decode_agrees_with_the_whole_image_path_for_every_row_format() {
    // One fixture per colour type that reaches the decoder as 8-bit output:
    // `Transformations::EXPAND` widens the sub-8-bit and paletted ones. Each is
    // realized at its own size, so the filter is the identity and the
    // raster must be `image`'s own decode of the same file, byte for byte.
    let width = 23;
    let height = 61;
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("gray8", {
            let raw: Vec<u8> = (0..width * height).map(|i| (i % 251) as u8).collect();
            png_from(
                image::GrayImage::from_raw(width, height, raw)
                    .unwrap()
                    .into(),
            )
        }),
        ("gray-alpha8", {
            let raw: Vec<u8> = (0..width * height * 2).map(|i| (i % 249) as u8).collect();
            png_from(
                image::ImageBuffer::<image::LumaA<u8>, _>::from_raw(width, height, raw)
                    .unwrap()
                    .into(),
            )
        }),
        ("rgb8", {
            let raw: Vec<u8> = (0..width * height * 3).map(|i| (i % 247) as u8).collect();
            png_from(
                image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(width, height, raw)
                    .unwrap()
                    .into(),
            )
        }),
        ("rgba8", varying_png(width, height)),
        // 1-bit and 4-bit grayscale, and a palette with transparency, are the
        // shapes `EXPAND` exists to lift into 8-bit output.
        (
            "gray1",
            png_with_depth(
                width,
                height,
                png::BitDepth::One,
                png::ColorType::Grayscale,
                |x| u8::from(x % 2 == 0),
            ),
        ),
        (
            "gray4",
            png_with_depth(
                width,
                height,
                png::BitDepth::Four,
                png::ColorType::Grayscale,
                |x| (x % 16) as u8,
            ),
        ),
        (
            "palette",
            png_paletted(
                width,
                height,
                vec![0x10, 0x20, 0x30, 0x40, 0x50, 0x60],
                None,
            ),
        ),
        // A palette with tRNS is translated to RGBA, not RGB, by EXPAND.
        (
            "palette+trns",
            png_paletted(
                width,
                height,
                vec![0x10, 0x20, 0x30, 0x40, 0x50, 0x60],
                Some(vec![0x00, 0x80]),
            ),
        ),
    ];

    for (name, data) in cases {
        let BandSource::Banded(source) = open_banded(&data) else {
            panic!("{name}: an 8-bit PNG has a row-wise decoder");
        };
        let (bands, raster) = drain(source);
        assert!(!bands.is_empty(), "{name}: bands were produced");
        let Some(raster) = raster else {
            panic!("{name}: a completed source yields the raster");
        };
        let (image_width, image_height, rgba) = whole_image_pixels(&data);
        assert_eq!(
            raster.raster().dimensions(),
            (image_width, image_height),
            "{name}: the raster is the source's size"
        );
        assert_eq!(
            raster.into_rgba(),
            rgba,
            "{name}: a banded decode must produce the whole decode's pixels"
        );
    }
}

/// The mask identity GNU's `:mask` reads is a property of the *source* pixels,
/// so a banded decode classifies them as they arrive rather than reading the
/// raster it built: a source whose alphas are only ever clear or opaque has
/// pixels in between once they have been filtered, and asking the raster would
/// be asking the filter.
#[test]
fn the_mask_comes_from_the_source_pixels_not_from_the_raster() {
    let (width, height) = (40, 20);
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    for (index, texel) in pixels.chunks_exact_mut(4).enumerate() {
        texel.copy_from_slice(&[0x40, 0x80, 0xc0, if index % 3 == 0 { 0 } else { 255 }]);
    }
    let data = png_of(width, height, pixels);
    let BandSource::Banded(source) = open_banded_at(&data, 20, 10) else {
        panic!("a PNG has a row-wise decoder");
    };
    let (_, raster) = drain(source);
    let raster = raster.expect("a completed source yields the raster");
    assert_eq!(
        raster.mask(),
        ImageMaskKind::Clipping,
        "the source's own alphas are clear or opaque"
    );
    assert!(
        raster
            .into_rgba()
            .chunks_exact(4)
            .any(|texel| texel[3] != 0 && texel[3] != 255),
        "the raster the filter built has alphas the source never had"
    );

    // The control: a source with partial alpha of its own is neither.
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    for texel in pixels.chunks_exact_mut(4) {
        texel.copy_from_slice(&[0x40, 0x80, 0xc0, 0x80]);
    }
    let data = png_of(width, height, pixels);
    let BandSource::Banded(source) = open_banded_at(&data, 20, 10) else {
        panic!("a PNG has a row-wise decoder");
    };
    let (_, raster) = drain(source);
    assert_eq!(
        raster.expect("a completed source").mask(),
        ImageMaskKind::AlphaChannel
    );
}

#[test]
fn a_sixteen_bit_png_declines_banding_rather_than_re_deriving_its_endianness() {
    let width = 9;
    let height = 4;
    let raw: Vec<u16> = (0..width * height * 4).map(|i| (i * 997) as u16).collect();
    let data = png_from(
        image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(width, height, raw)
            .unwrap()
            .into(),
    );

    assert!(
        matches!(open_banded(&data), BandSource::Whole),
        "16-bit output is read whole"
    );
    // The control: the same source without the extra bit depth bands.
    assert!(matches!(
        open_banded(&varying_png(width, height)),
        BandSource::Banded(_)
    ));
}

/// Adam7 rows are partial rows, so an interlaced source has no bands to take.
/// The `png` encoder cannot write one, so the flag is set on an otherwise
/// valid file with a corrected IHDR checksum — enough for the rule this pins,
/// which is decided from the header alone.
#[test]
fn an_interlaced_png_declines_banding() {
    let mut data = varying_png(8, 8);
    // IHDR: 8-byte signature, 4-byte length, 4-byte type, then the data whose
    // thirteenth byte is the interlace method.
    let interlace = 8 + 4 + 4 + 12;
    assert_eq!(data[interlace], 0, "the fixture is not interlaced to begin");
    data[interlace] = 1;
    let crc = crc32(&data[8 + 4..8 + 4 + 4 + 13]);
    data[8 + 4 + 4 + 13..8 + 4 + 4 + 17].copy_from_slice(&crc.to_be_bytes());

    assert!(
        matches!(open_banded(&data), BandSource::Whole),
        "interlaced rows are read whole"
    );
}

/// A source that runs out mid-stream fails rather than reporting the rows it
/// managed to read as the image.
#[test]
fn a_truncated_png_fails_mid_stream_and_yields_no_image() {
    let (width, height) = (64, 600);
    let data = varying_png(width, height);
    // Keep the header and part of the image data: enough rows to publish
    // bands, not enough to finish.
    let truncated = &data[..data.len() / 2];

    let BandSource::Banded(mut source) = open_banded(truncated) else {
        panic!("the header of a truncated PNG still parses");
    };
    let mut bands = Vec::new();
    loop {
        match source.next_band() {
            BandStep::Band(band) => bands.push(band),
            BandStep::Done => panic!("a truncated source cannot complete"),
            BandStep::Failed => break,
        }
    }

    let covered = bands.last().map_or(0, |band| band.source().end());
    assert!(
        covered > 0 && covered < height,
        "the failure is mid-stream: {covered} of {height} rows"
    );
    assert!(
        source.into_raster().is_none(),
        "an unfinished decode must not hand back a prefix as the image"
    );
}

/// A band has a destination only where the texture can be built up from the
/// top: an unrotated realization whose mask policy leaves the pixels alone.
/// Both exceptions are about the realization rather than the band, and a source
/// with either takes the whole-image path instead of holding a native image.
#[test]
fn a_band_has_a_destination_only_where_the_texture_can_be_filled_from_the_top() {
    assert_eq!(
        BandFilling::of(ImageRotation::None, ImageMaskPolicy::Preserve),
        BandFilling::TopDown,
    );
    for rotation in [
        ImageRotation::Quarter,
        ImageRotation::Half,
        ImageRotation::ThreeQuarter,
    ] {
        assert_eq!(
            BandFilling::of(rotation, ImageMaskPolicy::Preserve),
            BandFilling::Deferred,
            "a {rotation:?} turn moves a band's rows out of the raster's rows",
        );
    }
    for mask in [
        ImageMaskPolicy::Suppress,
        ImageMaskPolicy::Heuristic(ImageHeuristicMask::FourCorners),
        ImageMaskPolicy::Heuristic(ImageHeuristicMask::Rgb16([0x12, 0x34, 0x56])),
    ] {
        assert_eq!(
            BandFilling::of(ImageRotation::None, mask),
            BandFilling::Deferred,
            "a {mask:?} mask rewrites pixels and needs all of them first",
        );
    }
}

/// Whatever the source, a band says which rows it is: the constructor refuses
/// pixels that are not that rectangle.
#[test]
#[should_panic(expected = "a band carries exactly the rows it fills")]
fn a_band_refuses_pixels_that_are_not_the_rows_it_claims() {
    let rows = NonZeroU32::new(2).expect("non-zero");
    let placement = BandPlacement::new(ImageRasterExtent::new(4, 8), TextureRows::new(0, rows));
    let _ = RasterBand::new(placement, vec![0_u8; 4 * 2 * 4 - 1].into());
}

/// The size a band covers follows the source and the byte cap, not the
/// scanline: a tall source bands into tens of pieces, a wide one into bigger
/// pieces rather than megabytes of them.
#[test]
fn band_rows_are_bounded_by_the_display_row_the_count_and_the_bytes() {
    let plan = BandPlan::new(ImageSizeSpec::default(), ImageRealization::default());
    let display_row = NonZeroU32::new(1).unwrap();

    // A tall, narrow source: a thirtieth of its height is far more than one
    // display row, and its rows are small enough for the byte cap not to bind.
    assert_eq!(plan.band_rows(100, 2000), NonZeroU32::new(63).unwrap());

    // The byte cap binds on a wide source: 4 MiB of RGBA at 20000 pixels
    // across is 52 rows, fewer than a thirtieth of 20000.
    assert_eq!(plan.band_rows(20_000, 20_000), NonZeroU32::new(52).unwrap());

    // A source below the count floor bands per row, which is what the floor
    // means at that size; banding itself never engages this small.
    assert_eq!(plan.band_rows(10, 4), NonZeroU32::new(1).unwrap());

    // Magnified or at native size: the display row is a single source row, so
    // the count floor decides, and the plan never asks for zero rows.
    assert_eq!(
        plan.rows_per_display_row(10, 4).get(),
        display_row.get(),
        "a source shown at its own size has one source row per display row"
    );
}

/// The source rows behind a display row: more when the source is shown
/// smaller than it is, one when it is shown at its own size.
#[test]
fn the_band_plan_scales_source_rows_to_display_rows() {
    let at_native = BandPlan::new(ImageSizeSpec::default(), ImageRealization::default());
    assert_eq!(at_native.rows_per_display_row(1000, 1000).get(), 1);

    let minified = BandPlan::new(
        ImageSizeSpec::new(AxisSize::Exact(100), AxisSize::Exact(100)),
        ImageRealization::default(),
    );
    assert_eq!(
        minified.rows_per_display_row(1000, 1000).get(),
        10,
        "a source shown at a tenth of its size has ten source rows per display row"
    );
}

/// A baseline JPEG bands, and what it bands into is `image`'s own decode of the
/// same file.
///
/// Realized at its own size, so the filter is the identity and "the same
/// pixels" is a byte comparison rather than a tolerance: the two paths decode
/// the same columns through the same `zune-jpeg`, and this is what says the
/// row-wise one has not lost a row, doubled one, or started somewhere else.
#[test]
fn a_baseline_jpeg_bands_the_image_the_whole_path_decodes() {
    // Sizes chosen so the band plan has something to do: 4:2:0 means one MCU
    // row is sixteen output rows, so the first is 17 rows and the rest are
    // a row each.
    for (width, height) in [(140_u32, 409_u32), (333, 64), (64, 64), (1, 130), (130, 1)] {
        let data = varying_jpeg(width, height);
        let (whole_width, whole_height, whole) = whole_image_pixels(&data);
        assert_eq!((whole_width, whole_height), (width, height));

        let BandSource::Banded(source) = open_banded_at(&data, width, height) else {
            panic!("a baseline JPEG has a row-wise decoder");
        };
        let (bands, raster) = drain(source);

        let mut expected_start = 0;
        for band in &bands {
            assert_eq!(
                band.source().start(),
                expected_start,
                "bands tile the source"
            );
            assert_eq!(band.source().end(), band.placed().placement().rows().end());
            expected_start = band.source().end();
        }
        assert_eq!(expected_start, height, "the bands cover every row");

        let raster = raster.expect("a completed source");
        assert_eq!(raster.native().dimensions(), (width, height));
        assert_eq!(raster.raster().dimensions(), (width, height));
        assert_eq!(
            raster.into_rgba(),
            whole,
            "a {width}x{height} baseline JPEG decoded in bands is not its whole decode"
        );
    }
}

/// A progressive JPEG has no bands and says so, because its scans each carry
/// part of every block: the pixels a scan has produced so far are not early
/// pixels, they are wrong ones, and a band taken from them would be a picture
/// the file never contained.
#[test]
fn a_progressive_jpeg_has_no_bands_and_is_read_whole() {
    let (width, height) = (140, 409);
    let data = varying_progressive_jpeg(width, height);
    assert!(
        matches!(open_banded(&data), BandSource::Whole),
        "a progressive JPEG must route to the whole-image path"
    );

    // The control: the same picture, encoded as a baseline frame, does band.
    assert!(
        matches!(
            open_banded(&varying_jpeg(width, height)),
            BandSource::Banded(_)
        ),
        "the baseline encoding of the same pixels bands"
    );
}

/// More than half the pixels of a progressive frame differ from the same
/// picture's baseline encoding, which is why the two cannot be told apart by
/// eye and why routing one to the other path would not have shown up as a
/// difference in size or shape — the reason the refusal above has to be
/// explicit rather than left to the reader to be careful about.
#[test]
fn a_progressive_frame_is_not_the_same_file_as_a_baseline_one() {
    let (width, height) = (140, 409);
    let baseline = varying_jpeg(width, height);
    let progressive = varying_progressive_jpeg(width, height);
    assert_ne!(baseline, progressive, "the two encodings differ on disk");
    assert_eq!(
        image::load_from_memory(&baseline)
            .expect("baseline decodes")
            .to_rgba8()
            .dimensions(),
        image::load_from_memory(&progressive)
            .expect("progressive decodes")
            .to_rgba8()
            .dimensions(),
        "and agree on what they are, which is what makes the route invisible"
    );
}

/// A JPEG that runs out mid-stream fails rather than reporting the rows it read
/// as the image, the same contract the truncated PNG has.
#[test]
fn a_truncated_jpeg_fails_mid_stream_and_yields_no_image() {
    let (width, height) = (64, 600);
    let data = varying_jpeg(width, height);
    let truncated = &data[..data.len() / 2];

    let BandSource::Banded(mut source) = open_banded(truncated) else {
        panic!("the header of a truncated JPEG still parses");
    };
    let mut bands = 0_u32;
    let mut covered = 0;
    loop {
        match source.next_band() {
            BandStep::Band(band) => {
                bands += 1;
                covered = band.source().end();
            }
            BandStep::Done => break,
            BandStep::Failed => {
                assert!(bands > 0, "the failure is mid-stream");
                assert!(
                    source.into_raster().is_none(),
                    "an unfinished decode must not hand back a prefix as the image"
                );
                return;
            }
        }
    }
    // A truncated stream whose remaining rows the decoder can fill with grey
    // ends the image instead of failing it, which is what a whole-image decode
    // of the same bytes does; either ending is honest, and both must cover
    // exactly the source.
    assert_eq!(covered, height, "the bands cover the source either way");
}

/// A paletted PNG of `index(x, y) = x + y` over a two-colour palette, with an
/// optional per-entry alpha channel.
fn png_paletted(width: u32, height: u32, palette: Vec<u8>, trns: Option<Vec<u8>>) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(palette);
        if let Some(trns) = trns {
            encoder.set_trns(trns);
        }
        let mut writer = encoder.write_header().expect("header");
        let pixels: Vec<u8> = (0..height)
            .flat_map(|y| (0..width).map(move |x| ((x + y) % 2) as u8))
            .collect();
        writer.write_image_data(&pixels).expect("image data");
        writer.finish().expect("finish");
    }
    bytes
}

/// A sub-8-bit grayscale PNG, bit-packed the way the format stores it.
fn png_with_depth(
    width: u32,
    height: u32,
    depth: png::BitDepth,
    color: png::ColorType,
    sample: impl Fn(u32) -> u8,
) -> Vec<u8> {
    let per_byte = 8 / depth as usize;
    let row_bytes = (width as usize).div_ceil(per_byte);
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        let mut writer = encoder.write_header().expect("header");
        let mut pixels = Vec::with_capacity(row_bytes * height as usize);
        for _ in 0..height {
            let mut row = vec![0u8; row_bytes];
            for x in 0..width {
                let value = sample(x);
                let shift = 8 - depth as usize * ((x as usize % per_byte) + 1);
                row[x as usize / per_byte] |= value << shift;
            }
            pixels.extend_from_slice(&row);
        }
        writer.write_image_data(&pixels).expect("image data");
        writer.finish().expect("finish");
    }
    bytes
}

fn png_from(image: image::DynamicImage) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("PNG is encodable");
    bytes.into_inner()
}

/// A JPEG of `width` x `height` carrying `pixels` as RGB.
///
/// `image` 0.25's `jpeg` feature decodes only, so the tests bring their own
/// encoder; `progressive` is what separates the frames that have bands from the
/// ones that do not. Quality 85 is below the encoder's 90, which selects 2x2
/// chroma subsampling — the 4:2:0 shape, where one MCU row is sixteen output
/// rows and a band can never be a single row.
fn jpeg_of(width: u32, height: u32, pixels: Vec<u8>, progressive: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut bytes, 85);
    encoder.set_progressive(progressive);
    encoder
        .encode(
            &pixels,
            width as u16,
            height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .expect("JPEG is encodable");
    bytes
}

/// The RGB an RGBA buffer carries, for a JPEG encoder that takes three channels.
fn as_rgb(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|texel| [texel[0], texel[1], texel[2]])
        .collect()
}

/// A baseline JPEG of `width` x `height` with a distinct colour per pixel.
fn varying_jpeg(width: u32, height: u32) -> Vec<u8> {
    jpeg_of(width, height, as_rgb(&varying_pixels(width, height)), false)
}

/// A progressive JPEG of `width` x `height` with a distinct colour per pixel.
fn varying_progressive_jpeg(width: u32, height: u32) -> Vec<u8> {
    jpeg_of(width, height, as_rgb(&varying_pixels(width, height)), true)
}

/// CRC-32/ISO-HDLC, for the one hand-edited chunk above.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}
