use super::*;
use crate::display_row::face_state::{DisplayRowMeasurementMode, DisplayRowMeasurementPolicy};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::neovm_bridge::{FaceResolver, LayoutBufferSnapshot};
use neomacs_display_protocol::types::FaceId;
use neovm_core::emacs_core::Context;
use neovm_core::face::FaceTable;

fn capture(text: &str, chars: usize) -> Result<CapturedPhysicalLine, RowProgramError> {
    capture_with_properties(text, chars, None)
}

fn capture_with_properties(
    text: &str,
    chars: usize,
    property_start: Option<usize>,
) -> Result<CapturedPhysicalLine, RowProgramError> {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager_mut().current_buffer_mut().unwrap();
    buffer.insert(text);
    if let Some(start) = property_start {
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(text.len())),
            Value::symbol("display"),
            Value::string("replacement"),
        );
    }
    let id = buffer.id();
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let resolver = FaceResolver::new(&FaceTable::new(), 0xffffff, 0, 14.0, None);
    let metrics = DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0);
    let context = BufferSourceFaceResolutionContext::new(
        &snapshot,
        &resolver,
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::ConcreteFont),
        resolver.default_face(),
        FaceId::new(1),
        metrics,
        metrics,
        Default::default(),
    );
    capture_physical_line(
        id,
        1,
        CharPos0::ZERO,
        chars,
        16,
        context,
        &mut FrameFaceAttempt::for_test_with_next_id(2),
        || false,
    )
}

#[test]
fn later_line_replacement_does_not_reject_complete_plain_line() {
    let line = capture_with_properties("好a\nreplaced\n", 32, Some(5)).unwrap();
    assert_eq!(line.end, CharPos0::new(3));
    assert!(matches!(
        capture_with_properties("好a\nreplaced\n", 32, Some(3)),
        Err(RowProgramError::Unsupported)
    ));
}

#[test]
fn acquisition_stops_before_copying_a_huge_physical_line() {
    assert!(matches!(
        capture(&"a".repeat(1_000_000), 32),
        Err(RowProgramError::Incomplete)
    ));
}

#[test]
fn complete_line_capture_keeps_canonical_source_positions() {
    let line = capture("a好b\nnext\n", 16).unwrap();
    assert_eq!(line.end, CharPos0::new(4));
    assert!(matches!(
        line.items.last().unwrap().kind,
        DisplayItemKind::RowBreak(_)
    ));
    let first = &line.items[0];
    assert_eq!(
        first.span.start,
        crate::display_item::DisplaySourcePosition::buffer(
            match first.span.start {
                crate::display_item::DisplaySourcePosition::Buffer { buffer_id, .. } => buffer_id,
                _ => panic!("buffer provenance"),
            },
            CharPos0::ZERO,
            neovm_core::buffer::EmacsBytePos::ZERO
        )
    );
}
#[test]
fn nobreak_text_requires_buffer_special_character_policy() {
    for ch in ['\u{00a0}', '\u{00ad}', '\u{2011}'] {
        let text = format!("before{ch}after\n");
        assert!(
            matches!(capture(&text, 32), Err(RowProgramError::Unsupported)),
            "{ch:?}"
        );
    }
}
