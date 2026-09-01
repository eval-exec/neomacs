#![cfg(target_os = "linux")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use neomacs_display_protocol::types::VideoId;

use super::{BackendEvent, DecodedFrame, DecodedFrameTransfer, backend_bridge};
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
    publisher.frame(id, frame(1));
    publisher.frame(id, frame(2));

    let events = inbox.drain();
    assert_eq!(events.len(), 3);
    assert!(matches!(events[0], BackendEvent::Ended { id: event_id } if event_id == id));
    assert!(matches!(
        &events[1],
        BackendEvent::Frame { id: event_id, frame } if *event_id == id && frame.lease == 2
    ));
    assert!(matches!(
        events[2],
        BackendEvent::FramesReplaced {
            id: event_id,
            count: 1
        } if event_id == id
    ));
    assert_eq!(wakes.load(Ordering::Relaxed), 3);
}

fn frame(lease: u64) -> DecodedFrame<u64> {
    DecodedFrame {
        lease,
        timing: FrameTiming {
            pts: MediaTime::ZERO,
            duration: MediaTime::from_nanos(1),
            epoch: PlaybackEpoch::INITIAL,
        },
        geometry: VideoGeometry::packed(1, 1),
        format: VideoFrameFormat::Packed(PackedVideoFormat::Rgba8),
        colorimetry: VideoColorimetry::SRGB,
        decoder_transfer: DecodedFrameTransfer::Deferred,
    }
}
