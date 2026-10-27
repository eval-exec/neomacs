use super::*;
use std::io::Cursor;

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

fn submit(
    client: &ImageDecodeClient,
    load: ImageLoadToken,
    data: EncodedBytes,
    limit: ImageSizeLimit,
) {
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
        ImageSequenceId::new(u64::from(load.image().get())).unwrap(),
        crate::SvgResourceContext::Isolated,
        ImageLoadIdentity::unspecified(),
        limit,
    );
}

#[test]
fn metadata_before_gpu_adopts_the_same_decoded_pixels_once() {
    let mut decoder = ImageDecoder::new();
    let client = decoder.client();
    let load = token(91, 1);
    submit(&client, load, png(), ImageSizeLimit::UNLIMITED);
    let metadata = client
        .wait_for_terminal(load, Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(metadata.layout, ImageLayoutExtent::new(2, 1));
    assert_eq!(metadata.mask, ImageMaskKind::Clipping);
    // A later renderer command must adopt this job, even after it completed.
    submit(
        &client,
        load,
        EncodedBytes::copy_of(b"not another image"),
        ImageSizeLimit::UNLIMITED,
    );
    let mut loads = ImageLoadLifecycle::default();
    assert!(
        decoder.take_pending(&loads).is_empty(),
        "hold pixels until the renderer admits this token"
    );
    loads.begin(load);
    let outcomes = decoder.take_pending(&loads);
    let ready: Vec<_> = outcomes
        .into_iter()
        .filter_map(|outcome| match outcome {
            WorkerDecodeOutcome::Ready(decoded) => Some(decoded),
            WorkerDecodeOutcome::Band { .. } => None,
            WorkerDecodeOutcome::Failed { .. } => panic!("the completed token was decoded twice"),
        })
        .collect();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].data, [18, 52, 86, 255, 0, 0, 0, 0]);
    assert!(decoder.take_pending(&loads).is_empty());
    assert!(
        client
            .wait_for_terminal(load, Duration::ZERO)
            .unwrap()
            .is_ok()
    );
}

#[test]
fn newer_decode_and_retirement_reject_delayed_generations_and_release_pixels() {
    let mut decoder = ImageDecoder::new();
    let client = decoder.client();
    let old = token(92, 1);
    let current = token(92, 2);
    submit(&client, current, png(), ImageSizeLimit::UNLIMITED);
    submit(
        &client,
        old,
        EncodedBytes::copy_of(b"stale"),
        ImageSizeLimit::UNLIMITED,
    );
    assert!(
        client
            .wait_for_terminal(current, Duration::from_secs(5))
            .unwrap()
            .is_ok()
    );
    assert!(client.wait_for_terminal(old, Duration::ZERO).is_none());
    client.retire_image(current.image());
    let mut loads = ImageLoadLifecycle::default();
    loads.begin(current);
    assert!(
        decoder.take_pending(&loads).is_empty(),
        "retirement releases queued pixels"
    );
    // An in-flight worker completing after retirement cannot resurrect metadata.
    DecodeOutputSink::Shared(Arc::clone(&client.shared)).publish(WorkerDecodeOutcome::Failed {
        load: current,
        diagnostic: ImageDiagnostic::NotDrawable,
    });
    assert!(client.wait_for_terminal(current, Duration::ZERO).is_none());
    submit(&client, current, png(), ImageSizeLimit::UNLIMITED);
    assert!(client.wait_for_terminal(current, Duration::ZERO).is_none());
    let replacement = token(92, 3);
    submit(&client, replacement, png(), ImageSizeLimit::UNLIMITED);
    assert!(
        client
            .wait_for_terminal(replacement, Duration::from_secs(5))
            .unwrap()
            .is_ok()
    );
}

#[test]
fn cpu_metadata_requires_full_decode_and_preserves_size_admission() {
    let decoder = ImageDecoder::new();
    let client = decoder.client();
    let invalid = token(93, 1);
    // Keep a valid IHDR while corrupting the compressed pixel stream. A header
    // measurement alone would incorrectly report this as a successful image.
    let mut data = png().as_slice().to_vec();
    let idat = data.windows(4).position(|bytes| bytes == b"IDAT").unwrap();
    data[idat + 4] ^= 0xff;
    let truncated = EncodedBytes::new(data);
    assert!(
        crate::image_probe::probe_intrinsic_extent(crate::ImageProbeSource::Data(
            truncated.as_slice()
        ))
        .is_some()
    );
    submit(&client, invalid, truncated, ImageSizeLimit::UNLIMITED);
    assert!(
        client
            .wait_for_terminal(invalid, Duration::from_secs(5))
            .unwrap()
            .is_err()
    );
    let oversized = token(94, 1);
    submit(
        &client,
        oversized,
        png(),
        ImageSizeLimit::from_axis_pixels(1),
    );
    assert!(matches!(
        client.wait_for_terminal(oversized, Duration::from_secs(5)),
        Some(Err(ImageDiagnostic::InvalidSize))
    ));
}

#[test]
fn terminal_wait_allows_deferred_submission_and_outlives_upload_owner() {
    let decoder = ImageDecoder::new();
    let client = decoder.client();
    let load = token(95, 1);
    let started = Instant::now();
    assert!(
        client
            .wait_for_terminal(load, Duration::from_millis(5))
            .is_none()
    );
    assert!(started.elapsed() >= Duration::from_millis(5));
    let observer = client.clone();
    let waiting = thread::spawn(move || observer.wait_for_terminal(load, Duration::from_secs(5)));
    // Metadata clients may outlive the upload owner, e.g. isolated catalog tests.
    drop(decoder);
    submit(&client, load, png(), ImageSizeLimit::UNLIMITED);
    assert!(waiting.join().unwrap().unwrap().is_ok());
    let newer = token(95, 2);
    let started = Instant::now();
    assert!(
        client
            .wait_for_terminal(newer, Duration::from_millis(5))
            .is_none()
    );
    assert!(
        started.elapsed() >= Duration::from_millis(5),
        "an older terminal does not cancel a newer deferred request"
    );
    let observer = client.clone();
    let waiting = thread::spawn(move || observer.wait_for_terminal(newer, Duration::from_secs(5)));
    submit(&client, newer, png(), ImageSizeLimit::UNLIMITED);
    assert!(waiting.join().unwrap().unwrap().is_ok());
    assert!(client.shared.state.lock().unwrap().outcomes.is_empty());
    assert!(
        client
            .wait_for_terminal(token(96, 1), Duration::ZERO)
            .is_none()
    );
}

#[test]
fn sequence_retirement_fences_old_jobs_but_allows_new_sequence_identity() {
    let mut decoder = ImageDecoder::new();
    let client = decoder.client();
    let old = token(97, 1);
    submit(&client, old, png(), ImageSizeLimit::UNLIMITED);
    assert!(
        client
            .wait_for_terminal(old, Duration::from_secs(5))
            .unwrap()
            .is_ok()
    );
    client.retire_sequence(ImageSequenceRetirement::AllocatedThrough(
        ImageSequenceId::new(97).unwrap(),
    ));
    assert!(client.wait_for_terminal(old, Duration::ZERO).is_none());
    let mut loads = ImageLoadLifecycle::default();
    loads.begin(old);
    assert!(decoder.take_pending(&loads).is_empty());
    let replacement = token(98, 2);
    submit(&client, replacement, png(), ImageSizeLimit::UNLIMITED);
    assert!(
        client
            .wait_for_terminal(replacement, Duration::from_secs(5))
            .unwrap()
            .is_ok()
    );
}

#[test]
fn retirement_before_deferred_submission_cannot_resurrect_identity() {
    let mut decoder = ImageDecoder::new();
    let client = decoder.client();
    let retired = token(99, 1);
    client.retire_image(retired.image());
    submit(&client, retired, png(), ImageSizeLimit::UNLIMITED);
    assert!(
        client
            .wait_for_terminal(retired, Duration::from_secs(5))
            .is_none()
    );
    assert!(
        !client.accepts(retired),
        "a delayed render command cannot adopt a retired identity"
    );
    let mut loads = ImageLoadLifecycle::default();
    loads.begin(retired);
    assert!(decoder.take_pending(&loads).is_empty());
    let fresh = token(100, 2);
    submit(&client, fresh, png(), ImageSizeLimit::UNLIMITED);
    assert!(
        client
            .wait_for_terminal(fresh, Duration::from_secs(5))
            .unwrap()
            .is_ok()
    );
}
