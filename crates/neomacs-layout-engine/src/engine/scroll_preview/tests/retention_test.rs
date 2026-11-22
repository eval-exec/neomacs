use super::*;
use neomacs_display_protocol::scroll_coverage::{ScrollCoverage, ScrollCoverageRenewal};
use neomacs_display_protocol::{
    DisplayFrameId, FrameDisplayState, GlyphMatrix, ParentFrameRect, PresentationId,
    PresentedFramePlacement, PresentedHitIndex, Rect, SealedFramePresentation, WindowMatrixEntry,
};
use std::sync::Arc;

fn coverage(window: i64, epoch: u64) -> Arc<ScrollCoverage> {
    let viewport = Rect::new(0.0, 0.0, 80.0, 32.0);
    Arc::new(ScrollCoverage {
        epoch,
        predict_pixels: true,
        compositor_enabled: true,
        anchor_row: 0,
        viewport,
        origin: 0.0,
        content: WindowMatrixEntry {
            window_id: DisplayWindowId::new(window),
            matrix: GlyphMatrix::new(1, 10),
            pixel_bounds: viewport,
            text_pixel_bounds: viewport,
            text_clip_bounds: None,
            selected: true,
        },
        faces: Default::default(),
        fonts: Default::default(),
        char_fonts: Default::default(),
        shaped_clusters: Default::default(),
        hit_index: PresentedHitIndex::from_parts(PresentationId::new(1), vec![], vec![]).unwrap(),
        pointer_source: Default::default(),
    })
}

fn publication(
    revision: u64,
    fresh: Vec<Arc<ScrollCoverage>>,
    renewals: Vec<ScrollCoverageRenewal>,
) -> SealedFramePresentation {
    let mut frame = FrameDisplayState::new(10, 2, 8.0, 16.0);
    frame.presentation_id = PresentationId::new(revision);
    frame.frame_placement = PresentedFramePlacement::new(
        DisplayFrameId::new(7),
        frame.presentation_id,
        None,
        ParentFrameRect::new(0.0, 0.0, 80.0, 32.0).unwrap(),
        0,
    );
    frame.presented_hit_index =
        PresentedHitIndex::from_parts(frame.presentation_id, vec![], vec![]).unwrap();
    frame.scroll_coverage = fresh;
    frame.scroll_coverage_renewals = renewals;
    SealedFramePresentation::seal(frame).unwrap()
}

fn renewal(window: i64, epoch: u64, enabled: bool) -> ScrollCoverageRenewal {
    ScrollCoverageRenewal {
        window: DisplayWindowId::new(window),
        epoch,
        predict_pixels: false,
        compositor_enabled: enabled,
    }
}

#[test]
fn semantic_renewal_shares_rows_and_updates_only_accepted_policy() {
    let original = coverage(11, 3);
    let mut retained = ScrollPreviewCoverage::default();
    retained.refresh(&publication(1, vec![Arc::clone(&original)], vec![]));
    retained.refresh(&publication(2, vec![], vec![renewal(11, 3, false)]));
    let entry = retained.get(FrameId(7), DisplayWindowId::new(11)).unwrap();
    assert!(Arc::ptr_eq(&entry.coverage, &original));
    assert!(!entry.compositor_enabled);
    assert!(retained.get(FrameId(8), DisplayWindowId::new(11)).is_none());
    retained.refresh(&publication(3, vec![], vec![renewal(11, 3, true)]));
    assert!(
        retained
            .get(FrameId(7), DisplayWindowId::new(11))
            .unwrap()
            .compositor_enabled
    );
    assert!(
        original.compositor_enabled,
        "renewal never mutates exported rows"
    );
}

#[test]
fn semantic_silence_expires_only_windows_not_certified_by_the_publication() {
    let mut retained = ScrollPreviewCoverage::default();
    retained.refresh(&publication(
        1,
        vec![coverage(11, 3), coverage(12, 5)],
        vec![],
    ));
    retained.refresh(&publication(2, vec![], vec![renewal(12, 5, true)]));
    assert!(retained.get(FrameId(7), DisplayWindowId::new(11)).is_none());
    assert!(retained.get(FrameId(7), DisplayWindowId::new(12)).is_some());
    retained.refresh(&publication(3, vec![], vec![]));
    assert!(retained.get(FrameId(7), DisplayWindowId::new(12)).is_none());
}

#[test]
fn semantic_rekey_does_not_resurrect_an_epoch_without_a_fresh_export() {
    let mut retained = ScrollPreviewCoverage::default();
    retained.refresh(&publication(1, vec![coverage(11, 3)], vec![]));
    retained.refresh(&publication(2, vec![], vec![renewal(11, 4, true)]));
    retained.refresh(&publication(3, vec![], vec![renewal(11, 3, true)]));
    assert!(retained.get(FrameId(7), DisplayWindowId::new(11)).is_none());
    // A contradictory renewal cannot certify even a fresh different epoch.
    retained.refresh(&publication(
        4,
        vec![coverage(11, 5)],
        vec![renewal(11, 3, true)],
    ));
    assert!(retained.get(FrameId(7), DisplayWindowId::new(11)).is_none());
    let replacement = coverage(11, 6);
    retained.refresh(&publication(5, vec![Arc::clone(&replacement)], vec![]));
    assert!(Arc::ptr_eq(
        &retained
            .get(FrameId(7), DisplayWindowId::new(11))
            .unwrap()
            .coverage,
        &replacement
    ));
}
