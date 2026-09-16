use super::*;
use neomacs_display_protocol::{ImageFrameIndex, ImageRotation, ImageSizeSpec};
use std::io::Cursor;
#[cfg(not(target_family = "wasm"))]
use std::num::NonZeroUsize;

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

    let ready = WorkerDecodeOutcome::Ready(DecodedImage {
        load: ready,
        geometry: ImageRealization::default().resolve_geometry(
            ImageSizeSpec::default(),
            ImageNativeExtent::new(1, 1),
            ImageRotation::None,
        ),
        data: vec![0, 0, 0, 255],
        metadata: ImageMetadata {
            layout: ImageLayoutExtent::new(1, 1),
            reported: neomacs_display_protocol::ImageReportedExtent::new(1, 1),
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
        loads.take_current(WorkerDecodeOutcome::Failed(failed)),
        Some(WorkerDecodeOutcome::Failed(_))
    ));
    assert!(loads.active.is_empty());
    assert!(
        loads
            .take_current(WorkerDecodeOutcome::Failed(failed))
            .is_none()
    );
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn decoder_worker_survives_a_failed_request() {
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
            source: ImageSource::Data {
                data: Vec::new(),
                resources: crate::SvgResourceContext::Isolated,
                sequence: ImageSequenceId::new(61).unwrap(),
            },
            size: Default::default(),
            rotation: Default::default(),
            realization: ImageRealization::with_device_scale(1.0, 1.0),
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
        })
        .unwrap();
    request_tx
        .send(DecodeRequest {
            load: following,
            source: ImageSource::Data {
                data: png_bytes(vec![0x12, 0x34, 0x56, 0xff], 1, 1),
                resources: crate::SvgResourceContext::Isolated,
                sequence: ImageSequenceId::new(62).expect("non-zero sequence"),
            },
            size: Default::default(),
            rotation: Default::default(),
            realization: ImageRealization::with_device_scale(1.0, 1.0),
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
        })
        .unwrap();
    drop(request_tx);

    assert!(matches!(
        outcome_rx.recv().unwrap(),
        WorkerDecodeOutcome::Failed(load) if load == panicking
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
