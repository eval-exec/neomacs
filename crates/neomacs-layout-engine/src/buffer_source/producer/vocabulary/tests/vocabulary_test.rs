use super::ProducedGlyphProvenance as GlyphProvenance;
use super::*;
use crate::display_item::{
    DisplayGlyphless, DisplayItem, DisplayItemKind, DisplayLength, DisplayRowBreak,
    DisplaySourceMappedText, DisplayStretch, DisplayTextRun, GlyphlessMethod, RenderFaceRef,
    SourceSpan,
};
use crate::display_source::DisplaySourceItem;
use neomacs_display_protocol::glyph_matrix::NO_BUFFER_POSITION_CHARPOS;
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::{BufferId, CharPos0, EmacsBytePos};

const BUFFER: BufferId = BufferId(1);

fn buffer_position(charpos: usize) -> DisplaySourcePosition {
    DisplaySourcePosition::buffer(
        BUFFER,
        CharPos0::new(charpos),
        EmacsBytePos::new(charpos + 1),
    )
}

fn buffer_span(start_char: usize, end_char: usize) -> SourceSpan {
    SourceSpan::new(buffer_position(start_char), buffer_position(end_char))
}

fn item(span: SourceSpan, kind: DisplayItemKind) -> DisplayItem {
    DisplayItem::new(span, RenderFaceRef::FaceId(FaceId::new(2)), kind)
}

/// A pipeline step item, which requires a step CHAR: the covered-run case
/// carries the first covered buffer char the way the replacement session does.
fn step_item(
    span: SourceSpan,
    kind: DisplayItemKind,
    start_charpos: i64,
    source_char: Option<char>,
) -> DisplaySourceStepItem {
    let source_item = DisplaySourceItem::new(
        item(span, kind),
        start_charpos as usize,
        start_charpos,
        source_char,
    );
    DisplaySourceStepItem::new(source_item, 0).expect("step item")
}

// ---- GlyphProvenance: the (charpos, object) pair as one value.

#[test]
fn buffer_provenance_carries_the_buffer_charpos_through_to_the_glyph_stamp() {
    let provenance = GlyphProvenance::buffer(17);

    assert_eq!(provenance.buffer_charpos(), Some(17));
    assert_eq!(provenance.legacy_charpos(), 17);
}

#[test]
fn covered_replacement_glyphs_map_to_buffer_provenance_at_the_covered_start() {
    // A generic `SourceMappedText` without a Lisp-string source freezes every
    // glyph at the covered START. String-backed replacements instead use the
    // separate string-index plus row-occurrence path tested below.
    let covered_start = buffer_position(5);

    for offset in 0..4 {
        assert_eq!(
            covered_text_glyph(&covered_start),
            GlyphProvenance::buffer(5),
            "covered glyph {offset} must stamp the covered start"
        );
        assert_eq!(
            natural_text_glyph(&covered_start, offset),
            GlyphProvenance::buffer(5 + offset),
        );
    }
}

#[test]
fn appended_newline_space_maps_to_the_line_end_sentinel() {
    // GNU charpos 0 (`append_space_for_newline`): the appended glyph is
    // redisplay's own, not buffer text. The engine stamps
    // NO_BUFFER_POSITION_CHARPOS; the pin that the EMITTER agrees lives beside
    // the mutation in display_row/line_end.rs.
    let provenance = GlyphProvenance::line_end();

    assert_eq!(provenance.legacy_charpos(), NO_BUFFER_POSITION_CHARPOS);
    assert_eq!(provenance.buffer_charpos(), None);
}

#[test]
fn truncation_marks_map_to_the_mark_sentinel() {
    // GNU charpos -1. Truncation / continuation markers are emitted from
    // synthetic item sources (display_row/special_glyphs.rs,
    // row_lifecycle.rs hscroll marker), whose span start is Synthetic.
    let marker_start = DisplaySourcePosition::synthetic(0x7265_6467, 0);

    assert_eq!(
        provenance_from_source_position(&marker_start),
        GlyphProvenance::mark()
    );
    assert_eq!(
        GlyphProvenance::mark().legacy_charpos(),
        NO_BUFFER_POSITION_CHARPOS
    );
    assert_eq!(GlyphProvenance::mark().buffer_charpos(), None);
}

#[test]
fn line_end_and_mark_are_distinct_provenances_behind_one_glyph_sentinel() {
    // The whole point of the typed vocabulary: the glyph field cannot tell a
    // line-end space from a truncation mark (both are the untyped sentinel),
    // the provenance can.
    assert_ne!(GlyphProvenance::line_end(), GlyphProvenance::mark());
    assert_eq!(
        GlyphProvenance::line_end().legacy_charpos(),
        GlyphProvenance::mark().legacy_charpos()
    );
}

#[test]
fn empty_line_newline_maps_to_the_real_buffer_position_of_its_newline() {
    // The e772f82ed behavior, pinned end-to-end by
    // engine_test.rs::empty_line_row_carries_real_buffer_position: an empty
    // line reports its own newline's charpos, NOT the (0,0) / no-position
    // sentinel that caused the scroll and edit-reuse corruption class.
    let provenance = GlyphProvenance::empty_line_newline(6);

    assert_eq!(provenance.legacy_charpos(), 6);
    assert_ne!(provenance.legacy_charpos(), NO_BUFFER_POSITION_CHARPOS);
    assert_ne!(provenance, GlyphProvenance::line_end());
    // It is redisplay's own glyph, so it is not buffer-text provenance even
    // though it carries a real buffer charpos.
    assert_eq!(provenance.buffer_charpos(), None);
}

#[test]
fn a_lisp_string_position_maps_to_the_str_arm() {
    let position = DisplaySourcePosition::lisp_string(42, 3, 3);

    assert_eq!(
        provenance_from_source_position(&position),
        GlyphProvenance::Str {
            string: ProducedStringId::new(42),
            index: 3,
            covered_buffer: None,
        }
    );
}

// ---- ProducerPosition: the two tracks that must not drift.

#[test]
fn producer_position_defaults_its_stamp_to_the_scan_position() {
    let scan = BufferScanPos::new(4, 9);
    let position = ProducerPosition::buffer_at(scan);

    assert_eq!(position.scan(), scan);
    assert_eq!(position.stamp(), GlyphProvenance::buffer(9));
}

#[test]
fn producer_position_keeps_scan_on_buffer_text_while_the_stamp_differs() {
    // GNU it->current.pos vs it->position: inside a replacement the scan track
    // stays at the covered buffer position while the stamp is what lands on
    // glyphs. One struct, so they cannot be advanced independently.
    let scan = BufferScanPos::new(4, 9);
    let position = ProducerPosition::with_stamp(scan, GlyphProvenance::line_end());

    assert_eq!(position.scan(), scan);
    assert_eq!(position.stamp(), GlyphProvenance::line_end());
}

#[test]
fn producer_position_reads_a_step_items_scan_and_stamp() {
    let item = step_item(
        buffer_span(9, 12),
        DisplayItemKind::TextRun(DisplayTextRun::new("abc")),
        9,
        None,
    );

    let position = ProducerPosition::from_step_item(&item);

    assert_eq!(position.scan(), BufferScanPos::new(9, 9));
    assert_eq!(position.stamp(), GlyphProvenance::buffer(9));
}

// ---- ProducedElement: typed it_method output, bridged from today's items.

#[test]
fn a_text_run_step_item_becomes_a_run_with_natural_buffer_provenance() {
    let item = step_item(
        buffer_span(9, 12),
        DisplayItemKind::TextRun(DisplayTextRun::new("abc")),
        9,
        None,
    );

    let ProducedElement::Run(run) = ProducedElement::from_step_item(&item).expect("run") else {
        panic!("a TextRun item must produce a Run element");
    };

    assert_eq!(run.text(), "abc");
    assert_eq!(run.face(), RenderFaceRef::FaceId(FaceId::new(2)));
    assert_eq!(run.position().stamp(), GlyphProvenance::buffer(9));
    assert!(!run.is_covered_provenance());
    // A run is batching, not a different meaning: glyph N is what a Char at
    // start + N would have carried.
    for offset in 0..3 {
        assert_eq!(
            run.glyph_provenance(offset),
            GlyphProvenance::buffer(9 + offset)
        );
    }
}

#[test]
fn a_source_mapped_step_item_becomes_a_run_with_covered_provenance() {
    // Generic source-mapped text has no Lisp-string coordinate space, so it is
    // frozen at the covered buffer anchor.
    let item = step_item(
        buffer_span(5, 7),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("STR")),
        5,
        Some('x'),
    );

    let ProducedElement::Run(run) = ProducedElement::from_step_item(&item).expect("run") else {
        panic!("a SourceMappedText item must produce a Run element");
    };

    assert_eq!(run.text(), "STR");
    assert!(run.is_covered_provenance());
    for offset in 0..run.text().chars().count() {
        assert_eq!(run.glyph_provenance(offset), GlyphProvenance::buffer(5));
    }
}

#[test]
fn a_replacement_string_run_advances_string_indices_and_keeps_buffer_coverage() {
    let string_start = DisplaySourcePosition::lisp_string(42, 3, 3);
    let item = step_item(
        buffer_span(5, 7),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::from_string_run(
            "STR",
            string_start,
        )),
        5,
        Some('x'),
    );

    let ProducedElement::Run(run) = ProducedElement::from_step_item(&item).expect("run") else {
        panic!("a SourceMappedText item must produce a Run element");
    };

    assert!(!run.is_covered_provenance());
    for offset in 0..3 {
        assert_eq!(
            run.glyph_provenance(offset),
            GlyphProvenance::string_replacement(
                ProducedStringId::new(42),
                3 + offset,
                GlyphStringBufferRange::new(5, 7),
            )
        );
    }
}

#[test]
fn a_row_break_step_item_becomes_a_row_break_element() {
    let item = step_item(
        buffer_span(12, 13),
        DisplayItemKind::RowBreak(DisplayRowBreak {
            line_height: Default::default(),
            line_spacing: Default::default(),
        }),
        12,
        None,
    );

    let ProducedElement::RowBreak(row_break) =
        ProducedElement::from_step_item(&item).expect("row break")
    else {
        panic!("a RowBreak item must produce a RowBreak element");
    };

    assert_eq!(row_break.position().stamp(), GlyphProvenance::buffer(12));
}

#[test]
fn a_stretch_step_item_becomes_a_buffer_stamped_stretch_element() {
    // Design section 4.3: a `(space ...)` spec on buffer text is the ONE
    // replacement that keeps Buffer provenance.
    // A stretch item has no step CHAR of its own, so it bridges through
    // from_item at the scan position the walk holds.
    let stretch_item = item(
        buffer_span(3, 4),
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(16.0)),
            height: None,
            ascent: None,
        }),
    );

    let ProducedElement::Stretch(stretch) =
        ProducedElement::from_item(&stretch_item, BufferScanPos::new(3, 3)).expect("stretch")
    else {
        panic!("a Stretch item must produce a Stretch element");
    };

    assert_eq!(stretch.position().stamp(), GlyphProvenance::buffer(3));
    assert_eq!(
        stretch.width(),
        &DisplayStretchWidth::Length(DisplayLength::Pixels(16.0))
    );
    assert_eq!(stretch.face(), RenderFaceRef::FaceId(FaceId::new(2)));
    assert!(!stretch.avoid_cursor());
}

#[test]
fn a_control_char_step_item_becomes_a_char_element() {
    let item = step_item(
        buffer_span(3, 4),
        DisplayItemKind::ControlChar { ch: '\u{1}' },
        3,
        None,
    );

    let ProducedElement::Char(produced) = ProducedElement::from_step_item(&item).expect("char")
    else {
        panic!("a ControlChar item must produce a Char element");
    };

    assert_eq!(produced.ch(), '\u{1}');
    assert_eq!(produced.face(), RenderFaceRef::FaceId(FaceId::new(2)));
    assert_eq!(produced.position().stamp(), GlyphProvenance::buffer(3));
    assert!(!produced.avoid_cursor());
}

#[test]
fn item_kinds_the_vocabulary_does_not_model_yet_refuse_conversion() {
    // Glyphless and media replacements keep going through the legacy item path
    // until their rungs; the bridge says so rather than inventing an element.
    let glyphless = item(
        buffer_span(3, 4),
        DisplayItemKind::Glyphless(DisplayGlyphless {
            ch: '\u{200b}',
            method: GlyphlessMethod::ZeroWidth,
        }),
    );

    assert!(ProducedElement::from_item(&glyphless, BufferScanPos::new(3, 3)).is_none());
}

#[test]
fn end_of_text_is_the_producers_own_terminator_never_a_bridged_item() {
    // EndOfText marks the end of the visible window, which no single item can
    // express -- the bridge must never invent one.
    for kind in [
        DisplayItemKind::TextRun(DisplayTextRun::new("a")),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("a")),
        DisplayItemKind::ControlChar { ch: '\u{1}' },
        DisplayItemKind::RowBreak(DisplayRowBreak {
            line_height: Default::default(),
            line_spacing: Default::default(),
        }),
    ] {
        let bridged =
            ProducedElement::from_item(&item(buffer_span(0, 1), kind), BufferScanPos::new(0, 0));
        assert_ne!(bridged, Some(ProducedElement::EndOfText));
    }
}
