use super::*;
use neovm_core::emacs_core::image_catalog::{
    ImageLoadAttempt, ImageLoadToken, ResolvedImageMetadata,
};
use std::sync::Arc;

#[test]
fn free_or_reload_clears_shared_image_terminal() {
    let shared: super::super::SharedImageRenderState =
        Arc::new(super::super::ImageRenderState::default());
    let image = ImageId::new(9);
    let first = ImageLoadToken::new(image, ImageLoadAttempt::new(1).unwrap());
    let second = ImageLoadToken::new(image, ImageLoadAttempt::new(2).unwrap());
    shared.publish_terminal(
        first,
        super::super::ImageDecodeTerminal::Failed(
            neomacs_display_protocol::image_diagnostic::ImageDiagnostic::NotDrawable,
        ),
    );

    clear_image_terminals(&shared, image);
    assert!(shared.terminal(first).is_none());

    shared.publish_terminal(
        second,
        super::super::ImageDecodeTerminal::Ready(ResolvedImageMetadata::layout_is_image_pixels(
            2,
            3,
            0,
            true,
            neomacs_display_protocol::ImageMaskKind::Clipping,
        )),
    );
    clear_image_terminals(&shared, image);
    assert!(shared.terminal(second).is_none());

    shared.publish_terminal(
        second,
        super::super::ImageDecodeTerminal::Ready(ResolvedImageMetadata::layout_is_image_pixels(
            5,
            8,
            0x12_34_56,
            false,
            neomacs_display_protocol::ImageMaskKind::None,
        )),
    );
    assert!(matches!(
        shared.terminal(second),
        Some(super::super::ImageDecodeTerminal::Ready(_))
    ));
}

/// A band is published, not terminal: a reader asking how a load ended keeps
/// waiting past any number of bands, and the terminal state that follows
/// supersedes the last band rather than joining it.
#[test]
fn bands_do_not_answer_a_terminal_query() {
    let shared: super::super::SharedImageRenderState =
        Arc::new(super::super::ImageRenderState::default());
    let image = ImageId::new(11);
    let load = ImageLoadToken::new(image, ImageLoadAttempt::new(1).unwrap());
    let band = |start| super::super::RowRange::new(start, std::num::NonZeroU32::new(4).unwrap());

    shared.publish_band(load, band(0));
    assert!(
        shared.terminal(load).is_none(),
        "bands are not an answer to how the load ended"
    );
    assert_eq!(
        shared.band(load).map(|rows| rows.start()),
        Some(0),
        "the band is published where the display side can read it"
    );
    assert!(matches!(
        shared.wait_for_terminal(load, std::time::Duration::from_millis(1)),
        None
    ));

    shared.publish_band(load, band(4));
    assert_eq!(
        shared.band(load).map(|rows| rows.start()),
        Some(4),
        "a band supersedes the one before it"
    );

    shared.publish_terminal(
        load,
        super::super::ImageDecodeTerminal::Ready(ResolvedImageMetadata::layout_is_image_pixels(
            2,
            8,
            0,
            false,
            neomacs_display_protocol::ImageMaskKind::None,
        )),
    );
    assert!(matches!(
        shared.terminal(load),
        Some(super::super::ImageDecodeTerminal::Ready(_))
    ));
    assert!(
        shared.band(load).is_none(),
        "the terminal state supersedes the bands"
    );
}
