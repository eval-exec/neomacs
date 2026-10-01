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

fn request_for_rows(display_text_row_base: usize) -> TruncationContinuationFringeRequest {
    TruncationContinuationFringeRequest {
        display_text_row_base,
        has_left_fringe: true,
        has_right_fringe: true,
        bitmaps: bitmaps(),
        face_id: FaceId::new(7),
    }
}

#[test]
fn installer_preserves_shared_rows_when_no_fringe_slot_changes() {
    use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, MatrixRow};
    use neomacs_display_protocol::types::Rect;

    let explicit = FringeBitmapInfo {
        bitmap_index: 99,
        face_id: FaceId::new(3),
    };
    let mut cases = Vec::new();
    cases.push(("plain", enabled_row(), Vec::new(), request_for_rows(1)));
    let mut disabled = enabled_row();
    disabled.enabled = false;
    disabled.truncated_left = true;
    cases.push((
        "disabled",
        disabled,
        vec![DisplayRowFlagKind::Truncated],
        request_for_rows(1),
    ));
    let mut occupied = enabled_row();
    occupied.truncated_left = true;
    occupied.left_fringe_bitmap = Some(explicit);
    occupied.right_fringe_bitmap = Some(explicit);
    cases.push((
        "explicit slots",
        occupied,
        vec![DisplayRowFlagKind::Truncated],
        request_for_rows(1),
    ));
    let mut no_width = request_for_rows(1);
    no_width.has_left_fringe = false;
    no_width.has_right_fringe = false;
    cases.push((
        "zero width",
        enabled_row(),
        vec![
            DisplayRowFlagKind::Continued,
            DisplayRowFlagKind::Continuation,
        ],
        no_width,
    ));
    let mut no_bitmap = request_for_rows(1);
    no_bitmap.bitmaps = FringeArrowBitmaps::default();
    cases.push((
        "missing indicators",
        enabled_row(),
        vec![
            DisplayRowFlagKind::Continued,
            DisplayRowFlagKind::Continuation,
        ],
        no_bitmap,
    ));

    for (name, mut row, flags, request) in cases {
        row.glyphs[GlyphArea::Text.index()].push(Glyph::char('x', FaceId::new(1), 10));
        let retained = MatrixRow::new(row);
        let mut output = DisplayOutputBuilder::new();
        output.begin_window(1, 2, 80, Rect::new(0.0, 0.0, 800.0, 32.0), true);
        output.install_finalized_output_row(1, retained.clone());
        let mut row_flags = DisplayRowFlags::new(1);
        for flag in flags {
            row_flags.mark(0, flag);
        }
        request.install(&mut output, &row_flags);
        assert!(
            std::ptr::eq(output.current_window_row(1).unwrap(), retained.as_ref()),
            "{name}: a no-op must retain the shared row allocation and glyph storage"
        );
    }
}

#[test]
fn installer_copies_only_changed_rows_and_preserves_rtl_fringe_precedence() {
    use neomacs_display_protocol::glyph_matrix::MatrixRow;
    use neomacs_display_protocol::types::Rect;

    let explicit = FringeBitmapInfo {
        bitmap_index: 99,
        face_id: FaceId::new(3),
    };
    let mut row = enabled_row();
    row.reversed_p = true;
    row.truncated_left = true;
    row.left_fringe_bitmap = Some(explicit);
    let retained = MatrixRow::new(row);
    let mut output = DisplayOutputBuilder::new();
    output.begin_window(1, 2, 80, Rect::new(0.0, 0.0, 800.0, 32.0), true);
    output.install_finalized_output_row(1, retained.clone());
    let mut flags = DisplayRowFlags::new(1);
    flags.mark(0, DisplayRowFlagKind::Continued);
    flags.mark(0, DisplayRowFlagKind::Continuation);
    let request = request_for_rows(1);
    request.clone().install(&mut output, &flags);
    let changed = output.current_window_row(1).unwrap();
    assert!(
        !std::ptr::eq(changed, retained.as_ref()),
        "a real addition must copy a shared row"
    );
    assert_eq!(
        changed.left_fringe_bitmap,
        Some(explicit),
        "explicit left bitmap beats the RTL continued arrow"
    );
    assert_eq!(
        changed.right_fringe_bitmap,
        Some(FringeBitmapInfo {
            bitmap_index: 11,
            face_id: FaceId::new(7)
        }),
        "RTL left truncation beats the RTL continuation arrow on the right"
    );
    assert!(
        retained.right_fringe_bitmap.is_none(),
        "published source rows remain immutable"
    );
    let allocation = changed as *const GlyphRow;
    request.install(&mut output, &flags);
    assert_eq!(
        output.current_window_row(1).unwrap() as *const GlyphRow,
        allocation,
        "installing the same decoration twice leaves the row allocation unchanged"
    );
}
