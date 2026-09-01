//! Stage 5 unit tests: truncation / continuation fringe arrows.

use super::*;
use crate::output::row_request::DisplayWindowRowMutation;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;

fn bitmaps() -> FringeArrowBitmaps {
    FringeArrowBitmaps {
        truncation_left: Some(10),
        truncation_right: Some(11),
        continuation_left: Some(12),
        continuation_right: Some(13),
    }
}

fn enabled_row() -> GlyphRow {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row
}

#[test]
fn truncated_right_selects_right_arrow() {
    let state = FringeArrowRowState {
        truncated_right: true,
        ..Default::default()
    };
    let (left, right) = select_fringe_bitmaps(state, true, true, &bitmaps());
    assert_eq!(left, None);
    assert_eq!(right, Some(11), "right truncation => right-arrow");
}

#[test]
fn truncated_left_selects_left_arrow() {
    let state = FringeArrowRowState {
        truncated_left: true,
        ..Default::default()
    };
    let (left, right) = select_fringe_bitmaps(state, true, true, &bitmaps());
    assert_eq!(left, Some(10), "left truncation => left-arrow");
    assert_eq!(right, None);
}

#[test]
fn continued_selects_right_curly_arrow() {
    let state = FringeArrowRowState {
        continued: true,
        ..Default::default()
    };
    let (left, right) = select_fringe_bitmaps(state, true, true, &bitmaps());
    assert_eq!(left, None);
    assert_eq!(right, Some(13), "continued => right-curly-arrow");
}

#[test]
fn continuation_selects_left_curly_arrow() {
    let state = FringeArrowRowState {
        continuation: true,
        ..Default::default()
    };
    let (left, right) = select_fringe_bitmaps(state, true, true, &bitmaps());
    assert_eq!(left, Some(12), "continuation line => left-curly-arrow");
    assert_eq!(right, None);
}

#[test]
fn truncation_takes_precedence_over_continuation_on_right() {
    // A row both truncated-right and continued: GNU draws truncation first.
    let state = FringeArrowRowState {
        truncated_right: true,
        continued: true,
        ..Default::default()
    };
    let (_, right) = select_fringe_bitmaps(state, true, true, &bitmaps());
    assert_eq!(right, Some(11), "truncation precedes continuation");
}

#[test]
fn reversed_row_mirrors_left_and_right() {
    // R2L: truncated_right shows on the LEFT fringe, truncated_left on the RIGHT.
    let state = FringeArrowRowState {
        truncated_right: true,
        reversed: true,
        ..Default::default()
    };
    let (left, right) = select_fringe_bitmaps(state, true, true, &bitmaps());
    assert_eq!(
        left,
        Some(10),
        "R2L: right-truncation draws in the left fringe"
    );
    assert_eq!(right, None);

    let state = FringeArrowRowState {
        continued: true,
        reversed: true,
        ..Default::default()
    };
    let (left, right) = select_fringe_bitmaps(state, true, true, &bitmaps());
    assert_eq!(left, Some(12), "R2L: continued draws the left curly arrow");
    assert_eq!(right, None);
}

#[test]
fn zero_width_fringe_suppresses_that_side() {
    let state = FringeArrowRowState {
        truncated_left: true,
        truncated_right: true,
        ..Default::default()
    };
    // No left fringe width: left side suppressed.
    let (left, right) = select_fringe_bitmaps(state, false, true, &bitmaps());
    assert_eq!(left, None);
    assert_eq!(right, Some(11));
    // No right fringe width: right side suppressed.
    let (left, right) = select_fringe_bitmaps(state, true, false, &bitmaps());
    assert_eq!(left, Some(10));
    assert_eq!(right, None);
}

#[test]
fn mutation_sets_right_arrow_on_truncated_row() {
    let mut row = enabled_row();
    let mutation = FringeArrowRowMutation {
        continued: false,
        continuation: false,
        truncated_right: true,
        has_left_fringe: true,
        has_right_fringe: true,
        bitmaps: bitmaps(),
        face_id: FaceId::new(7),
    };
    mutation.apply(&mut row, 80);
    assert_eq!(
        row.right_fringe_bitmap.map(|i| i.bitmap_index),
        Some(11),
        "truncated row gets right-arrow in the right fringe",
    );
    assert_eq!(
        row.right_fringe_bitmap.map(|i| i.face_id),
        Some(FaceId::new(7))
    );
    assert!(row.left_fringe_bitmap.is_none());
}

#[test]
fn mutation_sets_curly_arrow_on_continued_row() {
    let mut row = enabled_row();
    let mutation = FringeArrowRowMutation {
        continued: true,
        continuation: false,
        truncated_right: false,
        has_left_fringe: true,
        has_right_fringe: true,
        bitmaps: bitmaps(),
        face_id: FaceId::new(7),
    };
    mutation.apply(&mut row, 80);
    assert_eq!(
        row.right_fringe_bitmap.map(|i| i.bitmap_index),
        Some(13),
        "continued row gets right-curly-arrow",
    );
}

#[test]
fn mutation_does_not_clobber_existing_left_fringe_spec() {
    // An explicit `(left-fringe …)` spec already set the left slot; the arrow
    // installer must not overwrite it.
    let mut row = enabled_row();
    row.truncated_left = true;
    row.left_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 99,
        face_id: FaceId::new(3),
    });
    let mutation = FringeArrowRowMutation {
        continued: false,
        continuation: false,
        truncated_right: false,
        has_left_fringe: true,
        has_right_fringe: true,
        bitmaps: bitmaps(),
        face_id: FaceId::new(7),
    };
    mutation.apply(&mut row, 80);
    assert_eq!(
        row.left_fringe_bitmap.map(|i| i.bitmap_index),
        Some(99),
        "explicit left-fringe spec is preserved",
    );
}

#[test]
fn mutation_reads_truncated_left_from_row() {
    // `truncated_left` lives on the GlyphRow; the mutation must read it.
    let mut row = enabled_row();
    row.truncated_left = true;
    let mutation = FringeArrowRowMutation {
        continued: false,
        continuation: false,
        truncated_right: false,
        has_left_fringe: true,
        has_right_fringe: true,
        bitmaps: bitmaps(),
        face_id: FaceId::new(7),
    };
    mutation.apply(&mut row, 80);
    assert_eq!(
        row.left_fringe_bitmap.map(|i| i.bitmap_index),
        Some(10),
        "row.truncated_left => left-arrow",
    );
}

#[test]
fn disabled_row_is_skipped() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = false;
    let mutation = FringeArrowRowMutation {
        continued: true,
        continuation: false,
        truncated_right: false,
        has_left_fringe: true,
        has_right_fringe: true,
        bitmaps: bitmaps(),
        face_id: FaceId::new(7),
    };
    mutation.apply(&mut row, 80);
    assert!(row.right_fringe_bitmap.is_none());
    assert!(row.left_fringe_bitmap.is_none());
}

#[test]
fn resolve_bitmaps_from_standard_indicator_alist() {
    // With Doom-style / standard `fringe-indicator-alist`, the resolver should
    // yield the four standard arrow bitmaps for truncation/continuation.
    let mut ctx = neovm_core::emacs_core::Context::new();
    let alist = ctx
        .eval_str(
            "'((truncation left-arrow right-arrow) \
               (continuation left-curly-arrow right-curly-arrow))",
        )
        .expect("alist");
    let mut buf = neovm_core::buffer::Buffer::new_standalone(
        neovm_core::buffer::BufferId(42),
        Value::string("*f*"),
    );
    buf.set_buffer_local("fringe-indicator-alist", alist);

    let resolved = FringeArrowBitmaps::resolve(&buf, &ctx);
    let idx = |name: &str| {
        let sym = neovm_core::emacs_core::intern::intern(name);
        u16::try_from(ctx.fringe_bitmap_registry().index_of(sym).unwrap()).unwrap()
    };
    assert_eq!(resolved.truncation_left, Some(idx("left-arrow")));
    assert_eq!(resolved.truncation_right, Some(idx("right-arrow")));
    assert_eq!(resolved.continuation_left, Some(idx("left-curly-arrow")));
    assert_eq!(resolved.continuation_right, Some(idx("right-curly-arrow")));
}
