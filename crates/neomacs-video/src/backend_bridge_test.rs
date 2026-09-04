#![cfg(target_os = "linux")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use neomacs_display_protocol::types::VideoId;

use super::{
    BackendEvent, DecodedFrame, DecodedFrameImport, DecoderOutputGeneration,
    MeasurementEpochDisposition, backend_bridge,
};
use crate::{
    FrameTiming, MediaTime, PackedVideoFormat, PlaybackEpoch, VideoColorimetry, VideoFrameFormat,
    VideoGeometry, VideoWake,
};

#[test]
fn decoder_bridge_bounds_frames_per_session_but_keeps_control_order() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let wake_count = Arc::clone(&wakes);
    let (publisher, inbox) = backend_bridge(VideoWake::new(move || {
        wake_count.fetch_add(1, Ordering::Relaxed);
    }));
    let id = VideoId::new(1);

    publisher.event(BackendEvent::Ended { id });
    let epoch = publisher.measurement_epoch();
    publisher.frame(epoch, id, frame(1));
    publisher.frame(epoch, id, frame(2));
    publisher.frame(epoch, id, frame(3));

    let events = inbox.drain();
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], BackendEvent::Ended { id: event_id } if event_id == id));
    assert!(matches!(
        &events[1],
        BackendEvent::Frame { id: event_id, frame } if *event_id == id && frame.lease == 1
    ));
    assert!(matches!(
        &events[2],
        BackendEvent::Frame { id: event_id, frame } if *event_id == id && frame.lease == 3
    ));
    assert!(matches!(
        events[3],
        BackendEvent::FramesReplaced {
            id: event_id,
            count: 1
        } if event_id == id
    ));
    assert_eq!(wakes.load(Ordering::Relaxed), 4);
}

#[test]
fn measurement_boundary_discards_prepublished_frames_but_keeps_lifecycle_events() {
    let (publisher, inbox) = backend_bridge(VideoWake::new(|| {}));
    let id = VideoId::new(1);
    publisher.event(BackendEvent::Ended { id });
    let warmup_epoch = publisher.measurement_epoch();
    publisher.frame(warmup_epoch, id, frame(1));
    publisher.frame(warmup_epoch, id, frame(2));

    inbox.begin_measurement_epoch();

    let events = inbox.drain();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], BackendEvent::Ended { id: event_id } if event_id == id));
}

#[test]
fn measurement_boundary_rejects_an_in_flight_old_generation_publication() {
    let (publisher, inbox) = backend_bridge(VideoWake::new(|| {}));
    let id = VideoId::new(1);
    let in_flight_warmup_epoch = publisher.measurement_epoch();

    inbox.begin_measurement_epoch();
    publisher.frame(in_flight_warmup_epoch, id, frame(1));
    let measured_epoch = publisher.measurement_epoch();
    publisher.frame(measured_epoch, id, frame(2));

    let events = inbox.drain();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        BackendEvent::Frame { frame, .. } if frame.lease == 2
    ));
}

#[test]
fn every_backend_event_has_an_explicit_measurement_epoch_disposition() {
    let id = VideoId::new(1);
    assert!(MeasurementEpochDisposition::Retain.retains_event());
    assert!(!MeasurementEpochDisposition::Discard.retains_event());
    assert_eq!(
        BackendEvent::Frame {
            id,
            frame: frame(1),
        }
        .measurement_epoch_disposition(),
        MeasurementEpochDisposition::Discard
    );
    assert_eq!(
        BackendEvent::<u64>::FramesReplaced { id, count: 1 }.measurement_epoch_disposition(),
        MeasurementEpochDisposition::Discard
    );
    assert_eq!(
        BackendEvent::<u64>::Ended { id }.measurement_epoch_disposition(),
        MeasurementEpochDisposition::Retain
    );
    assert_eq!(
        BackendEvent::<u64>::OutputReconfigured {
            id,
            generation: DecoderOutputGeneration::INITIAL.next(),
        }
        .measurement_epoch_disposition(),
        MeasurementEpochDisposition::Retain
    );
}

fn frame(lease: u64) -> DecodedFrame<u64> {
    DecodedFrame {
        lease,
        decode_residency: crate::VideoDecodeResidency::Unknown,
        timing: FrameTiming {
            pts: MediaTime::ZERO,
            duration: MediaTime::from_nanos(1),
            epoch: PlaybackEpoch::INITIAL,
        },
        geometry: VideoGeometry::packed(1, 1),
        format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
        colorimetry: VideoColorimetry::SRGB,
        output_generation: DecoderOutputGeneration::INITIAL,
        decoder_import: DecodedFrameImport::Deferred,
    }
}
