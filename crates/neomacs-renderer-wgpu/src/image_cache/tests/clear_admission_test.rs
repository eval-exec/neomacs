//! The shared decoder boundary between a delayed whole-cache clear and an
//! eager decode requested after it.
//!
//! GNU `Fclear_image_cache` (`src/image.c:2436-2478`) frees what exists when it
//! runs.  Neomacs' evaluator eagerly submits CPU decodes, and the renderer can
//! be stalled with a `ClearImageCache` command already queued.  A load
//! requested after that clear must not lose its eager decode when the older
//! command finally executes: its CPU job was admitted after the clear, so the
//! physical clear may free only jobs admitted up to its cutoff.  This row
//! drives the real `ImageCache` clear and adoption paths on a real device.

use super::*;
use neomacs_display_protocol::ImageSizeLimit;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

fn token(image: u32, attempt: u64) -> ImageLoadToken {
    ImageLoadToken::new(ImageId::new(image), ImageLoadAttempt::new(attempt).unwrap())
}

fn png() -> EncodedBytes {
    let pixels = image::RgbaImage::from_raw(2, 1, vec![18, 52, 86, 255, 0, 0, 0, 0]).unwrap();
    let mut encoded = Cursor::new(Vec::new());
    pixels
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    EncodedBytes::new(encoded.into_inner())
}

fn sequence(load: ImageLoadToken) -> ImageSequenceId {
    ImageSequenceId::new(u64::from(load.image().get())).unwrap()
}

/// The evaluator's eager submission for a load it just admitted.  It goes
/// straight to the shared decoder; the renderer has not seen this token yet.
fn eager_load(client: &ImageDecodeClient, load: ImageLoadToken, data: EncodedBytes) {
    client.load_data_with_id(
        load,
        data,
        ImageSizeSpec::default(),
        ImageRotation::None,
        ImageRealization::default(),
        ImageColorContext::default(),
        ImageMaskPolicy::Preserve,
        ImageAnimationPolicy::disabled(),
        ImageFrameIndex::default(),
        sequence(load),
        crate::SvgResourceContext::Isolated,
        ImageLoadIdentity::unspecified(),
        ImageSizeLimit::UNLIMITED,
    );
}

/// The same load's render command, arriving after the physical clear.
fn admitted_load(cache: &mut ImageCache, load: ImageLoadToken, data: EncodedBytes) {
    cache.load_data_with_id(
        load,
        data,
        ImageSizeSpec::default(),
        ImageRotation::None,
        ImageRealization::default(),
        ImageColorContext::default(),
        ImageMaskPolicy::Preserve,
        ImageAnimationPolicy::disabled(),
        ImageFrameIndex::default(),
        sequence(load),
        crate::SvgResourceContext::Isolated,
        ImageLoadIdentity::unspecified(),
        ImageSizeLimit::UNLIMITED,
    );
}

fn real_device() -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("the clear/adoption regression requires a real wgpu adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("image-clear-admission-test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    }))
    .expect("image-clear-admission device");
    (Arc::new(device), Arc::new(queue))
}

#[test]
fn a_delayed_clear_does_not_retire_the_eager_decode_requested_after_it() {
    let (device, queue) = real_device();
    let decoder = ImageDecoder::new();
    let client = decoder.client();
    let mut cache = ImageCache::with_decoder(&device, decoder);

    // The old image is resident before the invalidation.
    let old = token(701, 1);
    admitted_load(&mut cache, old, png());
    assert!(
        client
            .wait_for_terminal(old, Duration::from_secs(5))
            .expect("the old decode reaches a terminal state")
            .is_ok()
    );
    let events = cache.process_pending(&device, &queue);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ImageCacheEvent::Ready { load, .. } if *load == old)),
        "the old image becomes resident"
    );
    assert!(cache.is_ready(old.image()));

    // The catalog's flush retires the old identity; its `ImageFlush` command
    // is exactly this evaluator-side retirement before the delayed clear.
    client.retire_image(old.image());

    // The clear is issued now, so its cutoff is the last admission.  The load
    // requested afterwards gets a later ticket from the same shared decoder.
    let through = client.admission_ticket();
    let fresh = token(702, 1);
    eager_load(&client, fresh, png());
    assert!(
        client
            .wait_for_terminal(fresh, Duration::from_secs(5))
            .expect("the eager decode requested after the clear completes")
            .is_ok(),
        "the post-clear load's eager decode is not retired by the older clear"
    );

    // The renderer finally services the delayed whole-cache clear.
    cache.clear(through);
    assert!(
        !cache.is_ready(old.image()),
        "the physical clear frees the old texture"
    );
    assert!(!client.accepts(old), "the clear retires the old decode");
    assert!(
        matches!(client.wait_for_terminal(fresh, Duration::ZERO), Some(Ok(_))),
        "the post-clear decode keeps its synchronous metadata across the clear"
    );

    // The matching load command arrives after the clear and must adopt the
    // decode that survived it -- and only once.
    admitted_load(&mut cache, fresh, png());
    let events = cache.process_pending(&device, &queue);
    let adopted: Vec<_> = events
        .iter()
        .filter(|event| matches!(event, ImageCacheEvent::Ready { load, .. } if *load == fresh))
        .collect();
    assert_eq!(
        adopted.len(),
        1,
        "the post-clear load adopts its eager decode exactly once"
    );
    assert!(cache.is_ready(fresh.image()));
    assert!(
        cache.process_pending(&device, &queue).is_empty(),
        "a repeated poll does not adopt the same decode twice"
    );
}

/// A load the renderer starts itself -- a toolbar icon or an ID-less
/// `load_image_*` call -- is admitted after the evaluator's cutoff but before
/// the clear command executes. Its CPU submission carries a newer ticket, yet
/// the render command already preceded the clear, so the physical clear must
/// retire it: otherwise `ImageCache::clear` drops the load lifecycle that
/// alone could adopt the outcome, and the decoded pixels leak.
#[test]
fn a_clear_retires_a_renderer_originated_load_admitted_after_the_cutoff() {
    let (device, queue) = real_device();
    let decoder = ImageDecoder::new();
    let client = decoder.client();
    let mut cache = ImageCache::with_decoder(&device, decoder);

    // The evaluator issues the clear now. No decode has been admitted.
    let through = client.admission_ticket();

    // A renderer-originated load is taken in before the clear command runs:
    // `ImageCache::load_*` admits it to the load lifecycle and submits its
    // CPU decode with a ticket above the cutoff.
    let icon = token(703, 1);
    admitted_load(&mut cache, icon, png());
    assert!(
        client
            .wait_for_terminal(icon, Duration::from_secs(5))
            .expect("the renderer-originated decode reaches a terminal state")
            .is_ok()
    );
    assert!(client.accepts(icon), "the CPU decode is still live");

    cache.clear(through);

    assert!(
        !client.accepts(icon),
        "a renderer-admitted load predates the clear command and must be retired"
    );
    assert!(
        client.wait_for_terminal(icon, Duration::ZERO).is_none(),
        "the clear must not leave an outcome waiting for an admission it can never get"
    );
    assert!(
        cache.process_pending(&device, &queue).is_empty(),
        "no orphan outcome is adopted after the clear"
    );
    assert!(!cache.has_pending());
}
