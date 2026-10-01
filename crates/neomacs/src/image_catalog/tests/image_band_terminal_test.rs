//! What the catalog makes of an image load's intermediate publications.
//!
//! Step 1's contract is that geometry resolves from the header while the decode
//! is pending and that the decode's own answer replaces it at the end. Bands
//! are rows of a decode that is *still* running, so they must not answer for it:
//! the slot keeps the geometry the header resolved, and only the terminal state
//! moves it.

use super::*;
use neomacs_display_protocol::image_diagnostic::{ImageDiagnostic, ImageFormatName};
use neomacs_display_runtime::render_thread::{ImageDecodeTerminal, RowRange};
use neovm_core::emacs_core::image_catalog::{
    ImageLayoutExtent, ImageLoadAttempt, ImageLookup, ImagePlacement, PendingImage,
    ResolvedImageMetadata,
};

fn load() -> ImageLoadToken {
    ImageLoadToken::new(
        ImageId::new(7),
        ImageLoadAttempt::new(1).expect("non-zero attempt"),
    )
}

fn band(rows: u32) -> ImageDecodeTerminal {
    ImageDecodeTerminal::Band(RowRange::new(
        0,
        std::num::NonZeroU32::new(rows).expect("non-zero rows"),
    ))
}

/// The invariant, from the catalog's side: bands leave a pending image pending,
/// on the geometry the header gave it.
#[test]
fn a_band_leaves_the_slot_pending_on_its_header_geometry() {
    let load = load();
    let pending = PendingImage::new(load, ImageLayoutExtent::new(50, 100));

    let lookup = image_lookup_from_terminal(pending.clone(), band(64));

    let ImageLookup::Pending(still) = lookup else {
        panic!("rows of a decode still running must not resolve the slot");
    };
    assert_eq!(still.load(), load);
    assert_eq!(still.placement(), pending.placement());
    assert_eq!(still.placement().dimensions(), (50, 100));
}

/// The terminal state still resolves it, and it is the same slot: the bands in
/// between changed nothing about identity or placement.
#[test]
fn the_terminal_state_after_bands_resolves_the_same_slot() {
    let load = load();
    let pending = PendingImage::new(load, ImageLayoutExtent::new(50, 50));

    let _ = image_lookup_from_terminal(pending.clone(), band(8));
    let lookup = image_lookup_from_terminal(
        pending.clone(),
        ImageDecodeTerminal::Ready(ResolvedImageMetadata::layout_is_image_pixels(
            50,
            100,
            0,
            false,
            Default::default(),
        )),
    );

    let ImageLookup::Ready(ready) = lookup else {
        panic!("the decode's own answer resolves the slot");
    };
    assert_eq!(ready.image_id(), pending.placement().image_id());
    assert_eq!(ready.image_id(), load.image());
    assert_eq!(ready.metadata.layout.dimensions(), (50, 100));
}

/// A decode that failed after publishing bands fails as a whole: the slot does
/// not keep the rows that were drawn before the failure.
#[test]
fn a_failure_after_bands_fails_the_slot() {
    let load = load();
    let pending = PendingImage::new(load, ImageLayoutExtent::new(50, 50));

    let _ = image_lookup_from_terminal(pending.clone(), band(8));
    let lookup = image_lookup_from_terminal(
        pending.clone(),
        ImageDecodeTerminal::Failed(ImageDiagnostic::FormatError {
            format: ImageFormatName::Png,
            detail: "Read error".to_owned(),
        }),
    );

    let ImageLookup::Failed(failed) = lookup else {
        panic!("a failed load is a failed slot");
    };
    assert_eq!(failed.placement(), pending.placement());
}

/// A band is not terminal. The distinction the whole seam rests on: `Ready` and
/// `Failed` end a load, a band only advances one.
#[test]
fn only_ready_and_failed_end_a_load() {
    assert!(!band(1).is_terminal());
    assert!(
        ImageDecodeTerminal::Ready(ResolvedImageMetadata::layout_is_image_pixels(
            1,
            1,
            0,
            false,
            Default::default(),
        ))
        .is_terminal()
    );
    assert!(ImageDecodeTerminal::Failed(ImageDiagnostic::InvalidSize).is_terminal());
}

/// The placement a band publishes is the placement the load published, so a
/// consumer reacting to a band cannot invent an identity for the image.
#[test]
fn a_bands_placement_is_the_pending_slots() {
    let load = load();
    let placement = ImagePlacement::new(load.image(), ImageLayoutExtent::new(300, 200));
    let pending = PendingImage::new(load, ImageLayoutExtent::new(300, 200));

    assert_eq!(pending.placement(), placement);
    let ImageLookup::Pending(still) = image_lookup_from_terminal(pending, band(2)) else {
        panic!("still pending");
    };
    assert_eq!(still.placement().image_id(), placement.image_id());
    assert_eq!(still.placement().layout(), placement.layout());
}
