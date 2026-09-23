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
        super::super::ImageDecodeTerminal::Failed("old failure".to_owned()),
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
