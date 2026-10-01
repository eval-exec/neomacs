use super::*;
use neomacs_display_protocol::{
    ImageEmbeddedMetadata, ImageFrameDelay, ImageId, ImageLoadAttempt, ImageLoadToken,
    ImageStateEvent,
};
use std::sync::Arc;

#[test]
fn renderer_eviction_removes_published_residency_metadata() {
    let shared: super::super::SharedImageRenderState =
        Arc::new(super::super::ImageRenderState::default());
    let metadata = neomacs_renderer_wgpu::ImageMetadata {
        layout: neomacs_display_protocol::ImageLayoutExtent::new(48, 48),
        reported: neomacs_display_protocol::ImageReportedExtent::new(48, 48),
        background: 0,
        background_transparent: false,
        mask: neomacs_display_protocol::ImageMaskKind::None,
        embedded: ImageEmbeddedMetadata::animation(
            3,
            ImageFrameDelay::milliseconds(75, 1).expect("valid delay"),
        ),
    };
    let expected_embedded = metadata.embedded.clone();

    let image = ImageId::new(91);
    let load = ImageLoadToken::new(image, ImageLoadAttempt::new(1).unwrap());
    let event = publish_image_cache_event(
        &shared,
        neomacs_renderer_wgpu::ImageCacheEvent::Ready { load, metadata },
    );
    assert_eq!(event, Some(ImageStateEvent::DecodeCompleted(load)));
    let super::super::ImageDecodeTerminal::Ready(ready) =
        shared.terminal(load).expect("published decoder metadata")
    else {
        panic!("ready renderer event must publish ready metadata");
    };
    assert_eq!(ready.embedded, expected_embedded);

    let event = publish_image_cache_event(
        &shared,
        neomacs_renderer_wgpu::ImageCacheEvent::Evicted { image },
    );
    assert_eq!(event, Some(ImageStateEvent::Evicted(image)));
    assert!(shared.terminal(load).is_none());
}

/// A band advances its load without ending it: it is published for the display
/// side, and the evaluator is not woken to reconcile layout that has not
/// changed.
#[test]
fn a_band_publishes_without_ending_the_load() {
    let shared: super::super::SharedImageRenderState =
        Arc::new(super::super::ImageRenderState::default());
    let image = ImageId::new(92);
    let load = ImageLoadToken::new(image, ImageLoadAttempt::new(1).unwrap());
    let rows = neomacs_renderer_wgpu::RowRange::new(
        3,
        std::num::NonZeroU32::new(2).expect("non-zero rows"),
    );

    let event = publish_image_cache_event(
        &shared,
        neomacs_renderer_wgpu::ImageCacheEvent::Band { load, rows },
    );
    assert!(
        event.is_none(),
        "a band is not an event the evaluator reconciles against"
    );
    assert!(
        shared.terminal(load).is_none(),
        "a band must not answer a terminal query"
    );
    assert_eq!(shared.band(load).map(|rows| rows.start()), Some(3),);
}
