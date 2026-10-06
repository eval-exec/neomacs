use super::*;
use crate::image_bands::{BandPlacement, DecodedBand, RasterBand, RowRange};
use std::io::Cursor;

/// A diagnostic for tests that only exercise scheduling, not wording.
fn test_diagnostic() -> neomacs_display_protocol::image_diagnostic::ImageDiagnostic {
    neomacs_display_protocol::image_diagnostic::ImageDiagnostic::InvalidSize
}

fn test_load_identity() -> neomacs_display_protocol::image_diagnostic::ImageLoadIdentity {
    use neomacs_display_protocol::image_diagnostic::{ImageDiagnosticSubject, ImageFormatName};
    neomacs_display_protocol::image_diagnostic::ImageLoadIdentity::new(
        ImageFormatName::Png,
        ImageDiagnosticSubject::File(String::new()),
    )
}
#[test]
fn image_decoder_pool_is_nonempty_and_bounded_on_large_hosts() {
    let one = NonZeroUsize::new(1).unwrap();
    let large_host = NonZeroUsize::new(256).unwrap();

    assert_eq!(
        ImageDecoderPoolSize::from_available_parallelism(Some(one)).get(),
        1
    );
    assert_eq!(
        ImageDecoderPoolSize::from_available_parallelism(Some(large_host)).get(),
        MAX_IMAGE_DECODER_THREADS
    );
    assert_eq!(
        ImageDecoderPoolSize::from_available_parallelism(None).get(),
        MAX_IMAGE_DECODER_THREADS
    );
}

#[test]
fn freed_or_replaced_image_loads_reject_late_decode_outcomes() {
    let mut loads = ImageLoadLifecycle::default();

    let freed = loads.begin_generated(ImageId::new(41));
    loads.free(ImageId::new(41));
    assert!(!loads.accept(freed));

    let old = loads.begin_generated(ImageId::new(42));
    let current = loads.begin_generated(ImageId::new(42));
    assert!(!loads.accept(old));
    assert!(loads.accept(current));
    assert!(!loads.accept(current), "a duplicate terminal is stale");
    let replacement = loads.begin_generated(ImageId::new(42));
    assert!(loads.accept(replacement), "a new generation remains valid");
    assert!(loads.active.is_empty());
}

#[test]
fn ready_and_failed_terminals_consume_their_active_generations() {
    let mut loads = ImageLoadLifecycle::default();
    let ready = loads.begin_generated(ImageId::new(51));
    let failed = loads.begin_generated(ImageId::new(52));

    let geometry = ImageRealization::default().resolve_geometry(
        ImageSizeSpec::default(),
        ImageNativeExtent::new(1, 1),
        ImageRotation::None,
    );
    let ready = WorkerDecodeOutcome::Ready(DecodedImage {
        load: ready,
        geometry,
        data: vec![0, 0, 0, 255],
        metadata: ImageMetadata {
            layout: geometry.layout(),
            reported: geometry.reported(),
            background: 0,
            background_transparent: false,
            mask: ImageMaskKind::None,
            embedded: ImageEmbeddedMetadata::default(),
        },
    });
    assert!(matches!(
        loads.take_current(ready),
        Some(WorkerDecodeOutcome::Ready(_))
    ));
    assert_eq!(loads.active.len(), 1);

    assert!(matches!(
        loads.take_current(WorkerDecodeOutcome::Failed {
            load: failed,
            diagnostic: test_diagnostic()
        }),
        Some(WorkerDecodeOutcome::Failed { .. })
    ));
    assert!(loads.active.is_empty());
    assert!(
        loads
            .take_current(WorkerDecodeOutcome::Failed {
                load: failed,
                diagnostic: test_diagnostic()
            })
            .is_none()
    );
}

#[test]
fn lru_victim_prefers_least_recent_stamp_over_smallest_id() {
    // Insert order 1, 2, 3 (stamps 1, 2, 3), then id 1 is accessed again
    // (stamp 4). FIFO-by-smallest-id would evict 1; LRU must evict 2.
    let entries = [
        (ImageId::new(1), 4u64),
        (ImageId::new(2), 2),
        (ImageId::new(3), 3),
    ];
    assert_eq!(
        lru_unpresented_victim(entries.iter().copied(), &Default::default()),
        Some(ImageId::new(2))
    );
}

#[test]
fn lru_victim_repeated_touches_protect_hot_entries() {
    // 3 was inserted last but 1 and 3 were both re-read afterwards; the
    // coldest entry is 2 regardless of insertion order.
    let entries = [
        (ImageId::new(1), 5u64),
        (ImageId::new(2), 2),
        (ImageId::new(3), 6),
    ];
    assert_eq!(
        lru_unpresented_victim(entries.iter().copied(), &Default::default()),
        Some(ImageId::new(2))
    );
}

#[test]
fn lru_victim_matches_insert_order_when_never_touched() {
    let entries = [
        (ImageId::new(1), 1u64),
        (ImageId::new(2), 2),
        (ImageId::new(3), 3),
    ];
    assert_eq!(
        lru_unpresented_victim(entries.iter().copied(), &Default::default()),
        Some(ImageId::new(1))
    );
}

#[test]
fn lru_victim_of_no_entries_is_none() {
    assert_eq!(
        lru_unpresented_victim(std::iter::empty(), &Default::default()),
        None
    );
}

#[test]
fn retiring_image_is_released_only_after_its_presentation_stops_referencing_it() {
    let image = ImageId::new(41);
    let mut lifecycle = ImageResidencyLifecycle::default();
    lifecycle.request_retirement(image);

    let retained = [image].into_iter().collect::<RetainedImageSet>();
    assert!(lifecycle.take_releasable(&retained).is_empty());
    assert_eq!(lifecycle.take_releasable(&Default::default()), [image]);
}

#[test]
fn lru_never_selects_an_image_referenced_by_an_active_presentation() {
    let retained = [ImageId::new(1)].into_iter().collect::<RetainedImageSet>();
    let entries = [(ImageId::new(1), 1u64), (ImageId::new(2), 2)];

    assert_eq!(
        lru_unpresented_victim(entries.into_iter(), &retained),
        Some(ImageId::new(2))
    );
}

#[test]
fn bands_do_not_consume_the_load_attempt() {
    let mut loads = ImageLoadLifecycle::default();
    let load = loads.begin_generated(ImageId::new(71));

    let band = || WorkerDecodeOutcome::Band {
        load,
        decoded: test_band(),
    };
    assert!(matches!(loads.take_current(band()), Some(_)));
    assert!(matches!(loads.take_current(band()), Some(_)));
    assert!(loads.is_current(load), "bands leave the attempt alone");

    // The attempt is superseded while its bands are still arriving.
    let replacement = loads.begin_generated(ImageId::new(71));
    assert_ne!(replacement, load);
    assert!(
        loads.take_current(band()).is_none(),
        "a superseded decode's bands are dropped"
    );
    assert!(loads.is_current(replacement));
}

#[test]
fn a_textures_filled_prefix_advances_only_by_bands_that_continue_it() {
    let raster = ImageRasterExtent::new(4, 8);
    let rows_of = |start: u32, len: u32| {
        TextureRows::new(
            start,
            std::num::NonZeroU32::new(len).expect("non-zero test rows"),
        )
    };

    let empty = FilledRows::empty(raster);
    assert_eq!(empty.filled(), 0);
    assert_eq!(empty.filled_fraction(), 0.0);
    assert!(!empty.is_complete());

    let first = rows_of(0, 3);
    let filled = empty
        .extend(first)
        .expect("the first band continues row zero");
    assert_eq!(filled.filled(), first.end());
    assert!(!filled.is_complete());

    assert!(
        filled.extend(first).is_none(),
        "rows already written are not written twice"
    );
    assert!(
        filled.extend(rows_of(4, 2)).is_none(),
        "a band that would leave a hole is refused"
    );
    assert!(
        filled.extend(rows_of(0, 2)).is_none(),
        "a band from before the prefix is refused"
    );

    let filled = filled
        .extend(rows_of(3, 5))
        .expect("the last band continues the prefix");
    assert!(filled.is_complete());
    assert_eq!(filled.filled(), raster.height());
    assert_eq!(filled.filled_fraction(), 1.0);

    // A texture shorter than the rows a band names cannot take them, however
    // contiguous they are: the prefix may not outgrow the texture.
    let shorter = FilledRows::empty(ImageRasterExtent::new(4, 2));
    assert!(shorter.extend(rows_of(0, 3)).is_none());
}

#[test]
fn a_quad_is_drawn_only_over_the_rows_that_hold_pixels() {
    /// The trimmed span, as `(v1, height)`, to within a pixel.
    fn trimmed(span: Option<(f32, f32)>) -> Option<(f32, f32)> {
        span.map(|(v1, height)| (v1, (height * 1000.0).round() / 1000.0))
    }

    let complete = FilledRows::complete(ImageRasterExtent::new(4, 8));
    assert_eq!(
        trimmed(complete.clip_span(0.0, 1.0, 80.0)),
        Some((1.0, 80.0)),
        "a whole texture draws the whole quad"
    );

    let half = FilledRows::empty(ImageRasterExtent::new(4, 8))
        .extend(TextureRows::new(
            0,
            std::num::NonZeroU32::new(4).expect("non-zero"),
        ))
        .expect("the first band continues row zero");
    assert_eq!(half.filled_fraction(), 0.5);

    // The quad is drawn over the top half of the texture and no further.
    assert_eq!(trimmed(half.clip_span(0.0, 1.0, 80.0)), Some((0.5, 40.0)));
    // A span already inside the filled part is untouched.
    assert_eq!(trimmed(half.clip_span(0.0, 0.25, 20.0)), Some((0.25, 20.0)));
    // A span that crosses the boundary keeps its start and loses the rest.
    assert_eq!(trimmed(half.clip_span(0.25, 1.0, 60.0)), Some((0.5, 20.0)));
    // A span wholly below the boundary has nothing to draw.
    assert_eq!(half.clip_span(0.5, 1.0, 40.0), None);
    assert_eq!(half.clip_span(0.75, 1.0, 20.0), None);
}

fn test_band() -> DecodedBand {
    let rows = RowRange::new(0, std::num::NonZeroU32::new(1).expect("one row"));
    let placement = BandPlacement::new(
        ImageRasterExtent::new(1, 1),
        TextureRows::new(0, std::num::NonZeroU32::new(1).expect("one row")),
    );
    DecodedBand::new(rows, RasterBand::new(placement, vec![0u8; 4].into()))
}

#[test]
fn decoder_worker_survives_a_panicking_request() {
    let (request_tx, request_rx) = mpsc::channel();
    let (outcome_tx, outcome_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        ImageCache::decoder_thread_pooled(
            0,
            Arc::new(Mutex::new(request_rx)),
            outcome_tx,
            Arc::new(ImageSequenceCache::new()),
        )
    });
    let mut loads = ImageLoadLifecycle::default();
    let panicking = loads.begin_generated(ImageId::new(61));
    let following = loads.begin_generated(ImageId::new(62));

    request_tx
        .send(DecodeRequest {
            load: panicking,
            source: ImageSource::Panic,
            size: Default::default(),
            rotation: Default::default(),
            realization: ImageRealization::with_device_scale(1.0, 1.0),
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
            identity: test_load_identity(),
        })
        .unwrap();
    request_tx
        .send(DecodeRequest {
            load: following,
            source: ImageSource::Data {
                data: EncodedBytes::new(png_bytes(vec![0x12, 0x34, 0x56, 0xff], 1, 1)),
                resources: neomacs_image::SvgResourceContext::Isolated,
                sequence: ImageSequenceId::new(62).expect("non-zero sequence"),
            },
            size: Default::default(),
            rotation: Default::default(),
            realization: ImageRealization::with_device_scale(1.0, 1.0),
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
            identity: test_load_identity(),
        })
        .unwrap();
    drop(request_tx);

    assert!(matches!(
        outcome_rx.recv().unwrap(),
        WorkerDecodeOutcome::Failed { load, .. } if load == panicking
    ));
    assert!(matches!(
        outcome_rx.recv().unwrap(),
        WorkerDecodeOutcome::Ready(decoded) if decoded.load == following
    ));
    worker.join().unwrap();
}

fn png_bytes(pixels: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_raw(width, height, pixels).unwrap();
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

fn encoded_image_bytes(format: image::ImageFormat) -> Vec<u8> {
    let image = image::RgbaImage::from_raw(1, 1, vec![0x12, 0x34, 0x56, 0xff]).unwrap();
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, format)
        .unwrap();
    bytes.into_inner()
}

fn animated_gif_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        let frames = [
            image::Frame::from_parts(
                image::RgbaImage::from_pixel(2, 1, image::Rgba([0xff, 0, 0, 0xff])),
                0,
                0,
                image::Delay::from_numer_denom_ms(20, 1),
            ),
            image::Frame::from_parts(
                image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0xff, 0, 0xff])),
                0,
                0,
                image::Delay::from_numer_denom_ms(40, 1),
            ),
        ];
        encoder.encode_frames(frames).unwrap();
    }
    bytes
}
