use super::*;
use crate::display_cursor::{
    CapturedTextWindowCursorPublishContext, CapturedTextWindowCursorPublishOutcome,
    CursorGeometryContext, CursorGeometrySource, VisualTextWindowCursorPublishContext,
    VisualTextWindowCursorPublishSummary, cursor_style_for_window,
};
use crate::display_face_id::FrameFaceIdAllocator;
use crate::display_face_policy::BaseFacePolicy;
use crate::display_item::RenderFaceRef;
use crate::display_origin::{DisplayOrigin, OverlayStringKind};
use crate::display_output_builder::DisplayOutputBuilder;
use crate::display_row::{
    DisplayRowActiveFaceState, DisplayRowFace, DisplayRowGeometry, DisplayRowGlyphMeasurer,
    DisplayRowMeasurementPolicy, DisplayRowRenderBounds, DisplayRowRenderer,
    DisplayRowSourceFragmentFrame,
};
use crate::display_row_builder::{
    DisplayGlyphMeasurer, DisplayRowGlyphCheckpoint, DisplayRowPosition, DisplayTabPolicy,
};
use crate::display_row_geometry::DisplayRowMaxX;
use crate::display_row_transition::{
    DisplayRowLineBreakTransitionPlan, DisplayRowTransitionRenderState,
};
use crate::display_row_walk_state::{
    DisplayRowTextOverflowDecision, SpecialTextRowOverflowDecision, TextRowTransitionStatePolicy,
    next_window_start_for_partially_visible_point_row,
    next_window_start_for_point_line_continuation, next_window_start_from_visible_rows,
};
use crate::display_source::DisplaySpaceGeometry;
use crate::glyph_advance::GlyphAdvanceQuantization;
use crate::neovm_bridge::{FaceResolver, LayoutBufferSnapshot, RustBufferAccess};
use crate::types::{VisualCursorSpec, WindowKind};
use crate::window_output::{TextWindowOutputTarget, WindowOutputEmitter};
use neomacs_display_protocol::cursor::CursorBarWidth;
use neomacs_display_protocol::frame_chrome::{FrameChromeContent, FrameChromeKind};
use neomacs_display_protocol::frame_glyphs::{CursorKind, DisplaySlotId, FrameGlyph, GlyphRowRole};
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphRow, GlyphType};
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::{
    BufferId, BufferTextBackendKind, CharPos0, EmacsBytePos, EmacsByteRange, LispCharPos1,
};
use neovm_core::emacs_core::eval::{
    DisplayHost, GuiFrameHostRequest, ResolvedSurface, ResolvedVideo, ResolvedWebKit,
    SurfaceChannelKind, SurfaceResolveRequest, VideoResolveRequest, WebKitResolveRequest,
};
use neovm_core::emacs_core::image_catalog::{
    ImageCatalog, ImageLookup, ImageResolveRequest, PendingImage, ReadyImage,
};
use neovm_core::emacs_core::load::{
    apply_runtime_startup_state, create_bootstrap_evaluator_cached_with_features,
};
use neovm_core::emacs_core::value::StringTextPropertyRun;
use neovm_core::emacs_core::{Context, Value};
use neovm_core::face::FaceTable;
use neovm_core::heap_types::LispString;
use neovm_core::window::{
    DisplayPointSnapshot, DisplayRowSnapshot, WindowCursorSnapshot, WindowDisplaySnapshot,
    WindowVisibleBufferSpan,
};
use std::sync::{Arc, Mutex};

trait BufferTextPropertyTestExt {
    fn put_text_property(&mut self, start: usize, end: usize, name: Value, value: Value) -> bool;
}

fn emacs_byte_range(start: usize, end: usize) -> EmacsByteRange {
    EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(end))
}

impl BufferTextPropertyTestExt for neovm_core::buffer::Buffer {
    fn put_text_property(&mut self, start: usize, end: usize, name: Value, value: Value) -> bool {
        self.text_props_put_property_in_emacs_byte_range(emacs_byte_range(start, end), name, value)
    }
}

fn activate_last_engine_presentation(
    evaluator: &mut Context,
    engine: &LayoutEngine,
    frame_id: neovm_core::window::FrameId,
) -> neovm_core::window::geometry::PresentationId {
    let presentation = neovm_core::window::geometry::PresentationId::new(
        engine
            .last_frame_display_state
            .as_ref()
            .expect("prepared renderer presentation")
            .presentation_id
            .get(),
    );
    evaluator
        .frame_manager_mut()
        .get_mut(frame_id)
        .expect("presentation frame")
        .activate_display_presentation(presentation)
        .expect("activate prepared renderer presentation");
    presentation
}

#[test]
fn resize_mini_windows_mode_parses_gnu_values() {
    assert_eq!(
        ResizeMiniWindowsMode::from_lisp_value(Some(&Value::NIL)),
        ResizeMiniWindowsMode::Disabled
    );
    assert_eq!(
        ResizeMiniWindowsMode::from_lisp_value(Some(&Value::symbol("grow-only"))),
        ResizeMiniWindowsMode::GrowOnly
    );
    assert_eq!(
        ResizeMiniWindowsMode::from_lisp_value(Some(&Value::T)),
        ResizeMiniWindowsMode::Exact
    );
    assert_eq!(
        ResizeMiniWindowsMode::from_lisp_value(Some(&Value::symbol("anything-else"))),
        ResizeMiniWindowsMode::Exact
    );
}

#[test]
fn minibuffer_growth_stops_at_maximum_achievable_rows() {
    assert_eq!(super::minibuffer_growth_target(8, 4, 10.0), Some(8));
    assert_eq!(super::minibuffer_growth_target(11, 9, 10.0), Some(10));
    assert_eq!(super::minibuffer_growth_target(11, 10, 10.0), None);
}

#[test]
fn grow_only_minibuffer_shrinks_only_when_visible_region_is_empty() {
    assert!(ResizeMiniWindowsMode::GrowOnly.should_grow());
    assert!(!ResizeMiniWindowsMode::Disabled.should_grow());
    // `t` (Exact) always shrinks, regardless of exact_p / emptiness.
    assert!(ResizeMiniWindowsMode::Exact.should_shrink(false, false));
    assert!(ResizeMiniWindowsMode::Exact.should_shrink(true, false));
    // `nil` (Disabled) never shrinks.
    assert!(!ResizeMiniWindowsMode::Disabled.should_shrink(false, true));
    assert!(!ResizeMiniWindowsMode::Disabled.should_shrink(true, true));
    // `grow-only` shrinks for an empty buffer (GNU `BEGV == ZV`)...
    assert!(ResizeMiniWindowsMode::GrowOnly.should_shrink(false, true));
    // ...or when an exact resize is requested (GNU `exact_p`, i.e. the
    // post-command `resize_echo_area_exactly` with `minibuf_level == 0`),
    // even for a non-empty shorter message.
    assert!(ResizeMiniWindowsMode::GrowOnly.should_shrink(true, false));
    // But never for a non-empty buffer with no exact request (normal
    // mid-redisplay grow-only behavior keeps the larger size).
    assert!(!ResizeMiniWindowsMode::GrowOnly.should_shrink(false, false));
}

#[test]
fn word_wrap_break_candidate_records_rewind_position_and_clears() {
    let mut candidate = WordWrapBreakCandidate::default();

    assert!(!candidate.is_available());

    candidate.record(
        7,
        42,
        3,
        (Some(LispCharPos1::new(9)), Some(LispCharPos1::new(13))),
        DisplayRowGlyphCheckpoint::default(),
    );

    assert!(candidate.is_available());
    assert_eq!(candidate.byte_idx(), 7);
    assert_eq!(candidate.charpos(), 42);
    assert_eq!(candidate.display_point_count(), 3);
    assert_eq!(
        candidate.row_display_positions(),
        (Some(LispCharPos1::new(9)), Some(LispCharPos1::new(13)))
    );
    assert_eq!(
        candidate.glyph_checkpoint(),
        DisplayRowGlyphCheckpoint::default()
    );

    candidate.clear();

    assert!(!candidate.is_available());
}

#[test]
fn word_wrap_render_state_records_candidates_only_when_wrap_is_allowed() {
    let mut state = WordWrapRenderState::new(true);

    assert!(!state.candidate().is_available());

    state.record_candidate(
        ' ',
        7,
        42,
        3,
        (Some(LispCharPos1::new(9)), Some(LispCharPos1::new(13))),
        DisplayRowGlyphCheckpoint::default(),
    );

    assert!(!state.candidate().is_available());

    state.allow_after_current_char(' ');
    state.record_candidate(
        'a',
        7,
        42,
        3,
        (Some(LispCharPos1::new(9)), Some(LispCharPos1::new(13))),
        DisplayRowGlyphCheckpoint::default(),
    );

    assert!(state.candidate().is_available());
    assert_eq!(state.candidate().byte_idx(), 7);
    assert_eq!(state.candidate().charpos(), 42);

    state.reset_after_row_transition();

    assert!(!state.candidate().is_available());

    let mut disabled = WordWrapRenderState::new(false);
    disabled.allow_after_current_char(' ');
    disabled.record_candidate(
        'a',
        1,
        2,
        3,
        (None, None),
        DisplayRowGlyphCheckpoint::default(),
    );

    assert!(!disabled.candidate().is_available());
}

#[test]
fn text_row_transition_state_policy_applies_line_break_state_updates() {
    let mut line_numbers = LineNumberRenderState::new(true, 3, 5);
    let mut hscroll = HorizontalScrollSkipState::new(LineWrapMode::Truncate, 4);
    hscroll.consume_columns(2);
    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');
    word_wrap.record_candidate(
        'a',
        7,
        11,
        2,
        (None, None),
        DisplayRowGlyphCheckpoint::default(),
    );
    let mut trailing = TrailingWhitespaceRenderState::new(true, 0x00112233);
    trailing.track_rendered_char(
        ' ',
        DisplayRowStartMarker::Active {
            row: DisplayRowMarker::Row(0),
            x: 24.0,
        },
    );

    let mut col = 8;
    let mut prefix_request = DisplayRowPrefixRequest::None;
    DisplayRowTransitionRenderState::new(
        &mut prefix_request,
        true,
        &mut line_numbers,
        &mut hscroll,
        &mut word_wrap,
        &mut trailing,
    )
    .apply_line_break_row_start(
        DisplayRowLineBreakTransitionPlan::hidden_line_break(),
        &mut col,
    );

    assert_eq!(col, 0);
    assert_eq!(prefix_request, DisplayRowPrefixRequest::Line);
    assert_eq!(line_numbers.current_line(), 4);
    assert_eq!(hscroll.consumed_columns(), 0);
    assert!(!word_wrap.candidate().is_available());
    assert_eq!(trailing.start_marker(), DisplayRowStartMarker::Inactive);
}

#[test]
fn text_row_transition_state_policy_applies_character_wrap_state_updates() {
    let mut line_numbers = LineNumberRenderState::new(true, 3, 5);
    let mut hscroll = HorizontalScrollSkipState::new(LineWrapMode::Truncate, 4);
    hscroll.consume_columns(2);
    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');
    word_wrap.record_candidate(
        'a',
        7,
        11,
        2,
        (None, None),
        DisplayRowGlyphCheckpoint::default(),
    );
    let mut trailing = TrailingWhitespaceRenderState::new(true, 0x00112233);
    trailing.track_rendered_char(
        '\t',
        DisplayRowStartMarker::Active {
            row: DisplayRowMarker::Row(0),
            x: 24.0,
        },
    );

    let prefix = TextRowTransitionStatePolicy::character_wrap().apply(
        &mut line_numbers,
        &mut hscroll,
        &mut word_wrap,
        &mut trailing,
    );

    assert_eq!(
        prefix,
        crate::display_row_walk_state::TextRowTransitionPrefixAction::Wrap
    );
    assert_eq!(line_numbers.current_line(), 3);
    assert_eq!(hscroll.consumed_columns(), 2);
    assert_eq!(word_wrap.candidate().byte_idx(), 7);
    word_wrap.record_candidate(
        'b',
        9,
        13,
        4,
        (None, None),
        DisplayRowGlyphCheckpoint::default(),
    );
    assert_eq!(word_wrap.candidate().byte_idx(), 7);
    assert_eq!(trailing.start_marker(), DisplayRowStartMarker::Inactive);
}

#[test]
fn special_text_row_overflow_decision_names_fit_truncate_and_wrap() {
    assert_eq!(
        SpecialTextRowOverflowDecision::for_width(4.0, 6.0, 10.0, LineWrapMode::Truncate),
        SpecialTextRowOverflowDecision::Fits
    );
    assert_eq!(
        SpecialTextRowOverflowDecision::for_width(5.0, 6.0, 10.0, LineWrapMode::Truncate),
        SpecialTextRowOverflowDecision::Truncate
    );
    assert_eq!(
        SpecialTextRowOverflowDecision::for_width(5.0, 6.0, 10.0, LineWrapMode::Wrap),
        SpecialTextRowOverflowDecision::Wrap
    );
}

#[test]
fn buffer_text_row_overflow_decision_names_main_text_wrap_policy() {
    let empty_wrap = WordWrapRenderState::new(true);

    assert_eq!(
        DisplayRowTextOverflowDecision::for_char(
            'x',
            4.0,
            6.0,
            10.0,
            LineWrapMode::Truncate,
            empty_wrap
        ),
        DisplayRowTextOverflowDecision::Fits
    );
    assert_eq!(
        DisplayRowTextOverflowDecision::for_char(
            '\t',
            12.0,
            16.0,
            10.0,
            LineWrapMode::Truncate,
            empty_wrap
        ),
        DisplayRowTextOverflowDecision::Fits
    );
    assert_eq!(
        DisplayRowTextOverflowDecision::for_char(
            'x',
            5.0,
            6.0,
            10.0,
            LineWrapMode::Truncate,
            empty_wrap
        ),
        DisplayRowTextOverflowDecision::Truncate
    );
    assert_eq!(
        DisplayRowTextOverflowDecision::for_char(
            'x',
            5.0,
            6.0,
            10.0,
            LineWrapMode::Wrap,
            empty_wrap
        ),
        DisplayRowTextOverflowDecision::CharacterWrap
    );

    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');
    word_wrap.record_candidate(
        'a',
        7,
        11,
        2,
        (Some(LispCharPos1::new(3)), None),
        DisplayRowGlyphCheckpoint::default(),
    );

    assert_eq!(
        DisplayRowTextOverflowDecision::for_char(
            'x',
            5.0,
            6.0,
            10.0,
            LineWrapMode::Wrap,
            word_wrap
        ),
        DisplayRowTextOverflowDecision::WordWrap {
            break_candidate: word_wrap.candidate(),
        }
    );
}

#[test]
fn invisible_text_scan_checkpoint_tracks_next_visibility_change() {
    let mut checkpoints = InvisibleTextScanCheckpoint::new(10);

    assert!(!checkpoints.should_check(9));
    assert!(checkpoints.should_check(10));

    checkpoints.record_next_visible(15);

    assert!(!checkpoints.should_check(14));
    assert!(checkpoints.should_check(15));
}

#[test]
fn trailing_whitespace_render_state_tracks_enabled_marker_and_background() {
    let marker = DisplayRowStartMarker::Active {
        row: DisplayRowMarker::Row(0),
        x: 24.0,
    };
    let later_marker = DisplayRowStartMarker::Active {
        row: DisplayRowMarker::Row(0),
        x: 48.0,
    };
    let mut state = TrailingWhitespaceRenderState::new(true, 0x00112233);

    assert_eq!(state.background(), Some(Color::from_pixel(0x00112233)));
    assert_eq!(state.start_marker(), DisplayRowStartMarker::Inactive);

    state.track_rendered_char(' ', marker);
    state.track_rendered_char('\t', later_marker);

    assert_eq!(state.start_marker(), marker);

    state.track_rendered_char('x', later_marker);

    assert_eq!(state.start_marker(), DisplayRowStartMarker::Inactive);

    state.track_rendered_char('\t', later_marker);
    state.reset_after_row_transition();

    assert_eq!(state.start_marker(), DisplayRowStartMarker::Inactive);

    let mut disabled = TrailingWhitespaceRenderState::new(false, 0x00ABCDEF);
    disabled.track_rendered_char(' ', marker);

    assert_eq!(disabled.background(), None);
    assert_eq!(disabled.start_marker(), DisplayRowStartMarker::Inactive);
}

#[test]
fn hit_row_range_tracker_builds_ranges_and_tracks_pending_finish() {
    let mut tracker = HitRowRangeTracker::new(10);

    assert_eq!(
        tracker.range_to(14),
        DisplayRowHitRange {
            charpos_start: 10,
            charpos_end: 14,
        }
    );
    assert!(!tracker.should_finish_current_row(10, false));
    assert!(tracker.should_finish_current_row(11, false));
    assert!(tracker.should_finish_current_row(10, true));

    tracker.advance_to(14);

    assert_eq!(
        tracker.range_to(20),
        DisplayRowHitRange {
            charpos_start: 14,
            charpos_end: 20,
        }
    );
    assert!(!tracker.should_finish_current_row(14, false));
}

#[test]
fn face_scan_checkpoint_tracks_resolution_boundaries_and_invalidation() {
    let mut checkpoint = FaceScanCheckpoint::initial();

    assert!(checkpoint.should_resolve_at(0));

    *checkpoint.next_check_mut() = 12;

    assert!(!checkpoint.should_resolve_at(11));
    assert!(checkpoint.should_resolve_at(12));

    checkpoint.invalidate();

    assert!(checkpoint.should_resolve_at(0));
    assert_eq!(*checkpoint.next_check_mut(), 0);
}

#[test]
fn cursor_capture_state_captures_once_and_refines_matching_main_char_width() {
    let mut state = CursorCaptureState::new();
    let first = CapturedCursorInfo {
        x: 1.0,
        y: 2.0,
        face_w: 7.0,
        face_h: 14.0,
        face_ascent: 10.0,
        bg: Color::from_pixel(0x00112233),
        byte_idx: 5,
        col: 3,
        display_row_offset: 2,
        slot_width: None,
        stretch_like: false,
        glyph_row_resolved: false,
        display_replacement_anchor_charpos: None,
    };
    let second = CapturedCursorInfo {
        x: 9.0,
        byte_idx: 8,
        ..first
    };

    assert!(state.is_missing());

    state.capture_once(first);
    state.capture_once(second);
    state.update_for_main_char(8, 44.0);
    state.update_for_main_char(5, 12.5);

    let captured = state.as_ref().expect("cursor should be captured");
    assert_eq!(captured.x, 1.0);
    assert_eq!(captured.byte_idx, 5);
    assert_eq!(captured.slot_width, Some(12.5));
}

#[test]
fn frame_face_id_allocator_clamps_to_sentinel_and_allocates_sequential_ids() {
    let mut allocator = FrameFaceIdAllocator::new(100);

    assert_eq!(allocator.allocate(), FaceId::new(100));
    assert_eq!(allocator.allocate(), FaceId::new(101));
    assert_eq!(allocator.finish(), 102);

    let mut clamped = FrameFaceIdAllocator::new(0);

    assert_eq!(clamped.allocate(), FaceId::new(BasicFaceId::SENTINEL));
    assert_eq!(clamped.finish(), BasicFaceId::SENTINEL + 1);

    let mut frame_counter = 0;
    FrameFaceIdAllocator::new(200).finish_into(&mut frame_counter);
    assert_eq!(frame_counter, 200);
}

#[test]
fn display_row_prefix_request_tracks_pending_prefix_mode() {
    let _eval = Context::new();
    let mut request = DisplayRowPrefixRequest::initial(true, true);

    assert!(request.is_requested());
    let line_source = request
        .source_from_values(
            DisplayRowPrefixValues::new(
                Some(Value::string("line-property")),
                Some(Value::string("wrap-property")),
                Some(Value::string("line-default")),
                Some(Value::string("wrap-default")),
            ),
            CharPos0::new(3),
        )
        .expect("line prefix source");
    assert_eq!(
        line_source.origin(),
        DisplayOrigin::LinePrefix {
            anchor_charpos: CharPos0::new(3),
        }
    );
    assert_eq!(
        line_source.value().as_runtime_string_owned(),
        Some("line-property".to_string())
    );
    let line_fallback_source = request
        .source_from_values(
            DisplayRowPrefixValues::new(
                Some(Value::fixnum(1)),
                None,
                Some(Value::string("line-default")),
                None,
            ),
            CharPos0::new(4),
        )
        .expect("line default source");
    assert_eq!(
        line_fallback_source.value().as_runtime_string_owned(),
        Some("line-default".to_string())
    );

    request.clear();

    assert!(!request.is_requested());

    request.request_wrap();

    assert!(request.is_requested());
    let wrap_source = request
        .source_from_values(
            DisplayRowPrefixValues::new(
                Some(Value::string("line-property")),
                None,
                Some(Value::string("line-default")),
                Some(Value::string("wrap-default")),
            ),
            CharPos0::new(5),
        )
        .expect("wrap prefix source");
    assert_eq!(
        wrap_source.origin(),
        DisplayOrigin::WrapPrefix {
            anchor_charpos: CharPos0::new(5),
        }
    );
    assert_eq!(
        wrap_source.value().as_runtime_string_owned(),
        Some("wrap-default".to_string())
    );

    // The line prefix is now requested unconditionally so the per-row
    // `line-prefix` TEXT PROPERTY is always consulted (the variable default is
    // only a fallback); the no-prefix case is gated downstream by
    // `source_from_values` returning None, not by skipping the request.
    assert_eq!(
        DisplayRowPrefixRequest::initial(false, true),
        DisplayRowPrefixRequest::Line
    );

    request.clear();
    request.apply_transition_prefix_action(
        true,
        crate::display_row_walk_state::TextRowTransitionPrefixAction::Wrap,
    );
    let transition_wrap_source = request
        .source_from_values(
            DisplayRowPrefixValues::new(None, Some(Value::string("transition-wrap")), None, None),
            CharPos0::new(6),
        )
        .expect("transition wrap prefix source");
    assert_eq!(
        transition_wrap_source.value().as_runtime_string_owned(),
        Some("transition-wrap".to_string())
    );

    request.clear();
    // A transition requests the prefix regardless of the variable default so the
    // per-row text property is consulted; the no-prefix case is handled by
    // `source_from_values` returning None, not by skipping the request.
    request.apply_transition_prefix_action(
        false,
        crate::display_row_walk_state::TextRowTransitionPrefixAction::Line,
    );
    assert!(request.is_requested());
    assert_eq!(request, DisplayRowPrefixRequest::Line);
}

#[test]
fn display_row_prefix_request_builds_typed_prefix_source() {
    let _eval = Context::new();
    let line_value = Value::string("line");
    let line_source = DisplayRowPrefixRequest::Line
        .source_for_value(line_value, CharPos0::new(4))
        .expect("line prefix source");
    assert_eq!(line_source.value(), line_value);
    assert_eq!(
        line_source.origin(),
        DisplayOrigin::LinePrefix {
            anchor_charpos: CharPos0::new(4),
        }
    );
    assert_eq!(line_source.base_face_policy(), BaseFacePolicy::DefaultFace);

    let wrap_value = Value::string("wrap");
    let wrap_source = DisplayRowPrefixRequest::Wrap
        .source_for_value(wrap_value, CharPos0::new(7))
        .expect("wrap prefix source");
    assert_eq!(wrap_source.value(), wrap_value);
    assert_eq!(
        wrap_source.origin(),
        DisplayOrigin::WrapPrefix {
            anchor_charpos: CharPos0::new(7),
        }
    );
    assert_eq!(wrap_source.base_face_policy(), BaseFacePolicy::DefaultFace);

    assert!(
        DisplayRowPrefixRequest::None
            .source_for_value(Value::string("none"), CharPos0::new(0))
            .is_none()
    );
}

#[test]
fn overlay_string_render_source_exposes_typed_render_inputs() {
    let _eval = Context::new();
    let text = Value::string("overlay");
    let overlay_id = Value::symbol("overlay-id");
    let source = OverlayStringRenderSource::new(
        crate::neovm_bridge::OverlayDisplayString {
            string: text,
            overlay_id,
            after_string_p: false,
            priority: 0,
        },
        CharPos0::new(9),
        OverlayStringKind::After,
    );

    assert_eq!(source.value(), text);
    assert_eq!(source.anchor_i64(), 9);
    assert_eq!(
        source.origin(),
        DisplayOrigin::OverlayString {
            overlay_id,
            anchor_charpos: CharPos0::new(9),
            kind: OverlayStringKind::After,
        }
    );
    assert_eq!(
        source.base_face_policy(),
        BaseFacePolicy::OverlayStringAtAnchor
    );
}

#[test]
fn horizontal_scroll_skip_state_consumes_and_resets_remaining_columns() {
    let mut state = HorizontalScrollSkipState::new(LineWrapMode::Truncate, 5);

    assert!(state.should_skip());
    assert!(state.should_show_left_truncation());
    assert_eq!(state.consumed_columns(), 0);

    state.consume_columns(2);

    assert!(state.should_skip());
    assert_eq!(state.consumed_columns(), 2);

    state.consume_columns(9);

    assert!(!state.should_skip());
    assert_eq!(state.consumed_columns(), 5);

    state.reset_line();

    assert!(state.should_skip());
    assert_eq!(state.consumed_columns(), 0);
    assert!(!HorizontalScrollSkipState::new(LineWrapMode::Wrap, 5).should_skip());
}

#[test]
fn box_face_row_state_tracks_active_row_and_start_x() {
    let mut state = BoxFaceRowState::inactive();

    assert!(!state.is_active());
    assert_eq!(state.start_x(), None);
    assert_eq!(state.row(), DisplayRowMarker::Inactive);

    state.activate(DisplayRowMarker::Row(2), 18.0);

    assert!(state.is_active());
    assert_eq!(state.start_x(), Some(18.0));
    assert_eq!(state.row(), DisplayRowMarker::Row(2));

    state.continue_on_row(DisplayRowMarker::Row(3), 4.0);

    assert!(state.is_active());
    assert_eq!(state.start_x(), Some(4.0));
    assert_eq!(state.row(), DisplayRowMarker::Row(3));

    state.clear();

    assert!(!state.is_active());
    assert_eq!(state.row(), DisplayRowMarker::Inactive);
}

#[test]
fn line_number_render_state_tracks_current_point_and_pending_render() {
    let mut state = LineNumberRenderState::new(true, 7, 9);

    assert!(state.should_render());
    assert_eq!(state.current_line(), 7);
    assert_eq!(state.point_line(), 9);
    assert!(!state.is_current_line());
    assert_eq!(state.display_number(3, false, 0), 2);
    let request = state
        .margin_render_request(3, false, 0, 0, 4)
        .expect("line number request");
    assert_eq!(request.text(), "2");
    assert_eq!(request.cols(), 4);
    assert_eq!(request.face().face_name(), "line-number");

    state.consume_render_request();

    assert!(!state.should_render());

    state.advance_line();
    state.advance_line();

    assert!(state.should_render());
    assert_eq!(state.current_line(), 9);
    assert!(state.is_current_line());
    assert_eq!(state.display_number(3, true, 10), 19);
    let request = state
        .margin_render_request(3, true, 10, 3, 5)
        .expect("current line request");
    assert_eq!(request.text(), "19");
    assert_eq!(request.cols(), 5);
    assert_eq!(request.face().face_name(), "line-number-current-line");

    state.consume_render_request();
    state.advance_hidden_line();

    assert!(!state.should_render());
    assert_eq!(state.current_line(), 10);
    assert!(!LineNumberRenderState::new(false, 7, 9).should_render());

    let major_tick = LineNumberRenderState::new(true, 12, 9)
        .margin_render_request(1, false, 0, 4, 3)
        .expect("major tick line number request");
    assert_eq!(major_tick.text(), "12");
    assert_eq!(major_tick.face().face_name(), "line-number-major-tick");
}

#[test]
fn line_number_render_state_renders_blank_gutter_on_continuation_rows() {
    // First row of a buffer line renders the absolute number with a non-blank
    // gutter (GNU `maybe_produce_line_number`).
    let mut state = LineNumberRenderState::new(true, 7, 9);
    let first = state
        .margin_render_request(1, false, 0, 0, 4)
        .expect("first-row line number request");
    assert!(!first.blank());
    assert_eq!(first.text(), "7");
    assert_eq!(first.cols(), 4);
    state.consume_render_request();
    assert!(!state.should_render());

    // A wrapped continuation row re-arms the pending render but renders a blank
    // (no-number), width-reserved gutter so its text aligns with the first row.
    state.mark_continuation_row();
    assert!(state.should_render());
    let continuation = state
        .margin_render_request(1, false, 0, 0, 4)
        .expect("continuation-row line number request");
    assert!(continuation.blank());
    assert_eq!(continuation.text(), "");
    assert_eq!(continuation.cols(), 4);
    assert_eq!(continuation.face().face_name(), first.face().face_name());
    state.consume_render_request();
    assert!(!state.should_render());

    // The next buffer line resets back to a non-blank numbered gutter.
    state.advance_line();
    let next_line = state
        .margin_render_request(1, false, 0, 0, 4)
        .expect("next-line line number request");
    assert!(!next_line.blank());
    assert_eq!(next_line.text(), "8");
}

#[test]
fn captured_cursor_info_builds_from_active_face_state() {
    let eval = Context::new();
    let resolver = crate::neovm_bridge::FaceResolver::new(
        eval.face_table(),
        0x00FFFFFF,
        0x00000000,
        14.0,
        None,
    );
    let mut face = resolver.default_face().clone();
    face.bg = 0x00445566;
    let mut font_metrics = None;
    let measured = DisplayRowMeasurementPolicy::for_frame(false).measured_face(
        FaceId::new(9),
        &face,
        None,
        7.5,
        crate::display_row_metrics::DisplayRowFallbackMetrics {
            char_width: 7.5,
            row_height: 18.0,
            ascent: 13.0,
        },
        &mut font_metrics,
    );
    let active_face = DisplayRowActiveFaceState::new(face, measured);

    let cursor = CapturedCursorInfo::from_active_face_state(
        &active_face,
        CapturedCursorPlacement {
            x: 21.0,
            y: 34.0,
            byte_idx: 5,
            col: 3,
            display_row_offset: 2,
            slot_width: CapturedCursorSlotWidth::FaceChar,
            stretch_like: false,
        },
    );

    assert_eq!(cursor.x, 21.0);
    assert_eq!(cursor.y, 34.0);
    assert_eq!(cursor.face_w, 7.5);
    assert_eq!(cursor.face_h, 18.0);
    assert_eq!(cursor.face_ascent, 13.0);
    assert_eq!(cursor.bg, Color::from_pixel(0x00445566));
    assert_eq!(cursor.byte_idx, 5);
    assert_eq!(cursor.col, 3);
    assert_eq!(cursor.display_row_offset, 2);
    assert_eq!(cursor.slot_width, Some(7.5));
    assert!(!cursor.stretch_like);
}

#[test]
fn captured_cursor_info_builds_display_box_from_active_face_state() {
    let eval = Context::new();
    let resolver = crate::neovm_bridge::FaceResolver::new(
        eval.face_table(),
        0x00FFFFFF,
        0x00000000,
        14.0,
        None,
    );
    let mut face = resolver.default_face().clone();
    face.bg = 0x00445566;
    let mut font_metrics = None;
    let measured = DisplayRowMeasurementPolicy::for_frame(false).measured_face(
        FaceId::new(9),
        &face,
        None,
        7.5,
        crate::display_row_metrics::DisplayRowFallbackMetrics {
            char_width: 7.5,
            row_height: 18.0,
            ascent: 13.0,
        },
        &mut font_metrics,
    );
    let active_face = DisplayRowActiveFaceState::new(face, measured);

    let cursor = CapturedCursorInfo::display_box_from_active_face_state(
        &active_face,
        CapturedCursorPlacement {
            x: 21.0,
            y: 34.0,
            byte_idx: 5,
            col: 3,
            display_row_offset: 2,
            slot_width: CapturedCursorSlotWidth::Explicit(42.0),
            stretch_like: true,
        },
        31.0,
        29.0,
    );

    assert_eq!(cursor.x, 21.0);
    assert_eq!(cursor.y, 34.0);
    assert_eq!(cursor.face_w, 7.5);
    assert_eq!(cursor.face_h, 31.0);
    assert_eq!(cursor.face_ascent, 29.0);
    assert_eq!(cursor.bg, Color::from_pixel(0x00445566));
    assert_eq!(cursor.byte_idx, 5);
    assert_eq!(cursor.col, 3);
    assert_eq!(cursor.display_row_offset, 2);
    assert_eq!(cursor.slot_width, Some(42.0));
    assert!(cursor.stretch_like);
}

#[test]
fn captured_cursor_info_builds_line_break_from_active_face_state() {
    let eval = Context::new();
    let resolver = crate::neovm_bridge::FaceResolver::new(
        eval.face_table(),
        0x00FFFFFF,
        0x00000000,
        14.0,
        None,
    );
    let mut face = resolver.default_face().clone();
    face.bg = 0x00445566;
    let mut font_metrics = None;
    let measured = DisplayRowMeasurementPolicy::for_frame(false).measured_face(
        FaceId::new(9),
        &face,
        None,
        7.5,
        crate::display_row_metrics::DisplayRowFallbackMetrics {
            char_width: 7.5,
            row_height: 18.0,
            ascent: 13.0,
        },
        &mut font_metrics,
    );
    let active_face = DisplayRowActiveFaceState::new(face, measured);

    let cursor = CapturedCursorInfo::line_break_from_active_face_state(
        &active_face,
        CapturedCursorPlacement {
            x: 21.0,
            y: 34.0,
            byte_idx: 5,
            col: 3,
            display_row_offset: 2,
            slot_width: CapturedCursorSlotWidth::FaceChar,
            stretch_like: false,
        },
        24.0,
    );

    assert_eq!(cursor.x, 21.0);
    assert_eq!(cursor.y, 34.0);
    assert_eq!(cursor.face_w, 7.5);
    assert_eq!(cursor.face_h, 24.0);
    assert_eq!(cursor.face_ascent, 13.0);
    assert_eq!(cursor.bg, Color::from_pixel(0x00445566));
    assert_eq!(cursor.byte_idx, 5);
    assert_eq!(cursor.col, 3);
    assert_eq!(cursor.display_row_offset, 2);
    assert_eq!(cursor.slot_width, Some(7.5));
    assert!(!cursor.stretch_like);
}

#[test]
fn captured_cursor_info_builds_from_visual_state() {
    let cursor = CapturedCursorInfo::from_visual_state(
        CapturedCursorVisualState {
            face_width: 9.0,
            face_height: 22.0,
            face_ascent: 17.0,
            background: Color::from_pixel(0x00112233),
        },
        CapturedCursorPlacement {
            x: 21.0,
            y: 34.0,
            byte_idx: 5,
            col: 3,
            display_row_offset: 2,
            slot_width: CapturedCursorSlotWidth::Explicit(18.0),
            stretch_like: true,
        },
    );

    assert_eq!(cursor.x, 21.0);
    assert_eq!(cursor.y, 34.0);
    assert_eq!(cursor.face_w, 9.0);
    assert_eq!(cursor.face_h, 22.0);
    assert_eq!(cursor.face_ascent, 17.0);
    assert_eq!(cursor.bg, Color::from_pixel(0x00112233));
    assert_eq!(cursor.byte_idx, 5);
    assert_eq!(cursor.col, 3);
    assert_eq!(cursor.display_row_offset, 2);
    assert_eq!(cursor.slot_width, Some(18.0));
    assert!(cursor.stretch_like);
}

#[test]
fn cursor_geometry_source_builds_from_captured_cursor_and_row_metrics() {
    let cursor = CapturedCursorInfo::from_visual_state(
        CapturedCursorVisualState {
            face_width: 9.0,
            face_height: 22.0,
            face_ascent: 17.0,
            background: Color::from_pixel(0x00112233),
        },
        CapturedCursorPlacement {
            x: 21.0,
            y: 34.0,
            byte_idx: 5,
            col: 3,
            display_row_offset: 2,
            slot_width: CapturedCursorSlotWidth::Explicit(18.0),
            stretch_like: true,
        },
    );
    let row_metric = RowMetricsSnapshot::new(9, 9, 32.0, 25.0, 19.0);

    let source = CursorGeometrySource::from_captured_cursor(
        &cursor,
        row_metric,
        CursorGeometryContext {
            window_id: 7,
            slot_width: 18.0,
            default_line_height: 16.0,
            ends_at_visible_eob: true,
        },
    );

    assert_eq!(
        source.slot_id,
        DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(7),
            row: 9,
            col: 3,
        }
    );
    assert_eq!(source.x, 21.0);
    assert_eq!(source.y, 34.0);
    assert_eq!(source.slot_width, 18.0);
    assert_eq!(source.face_height, 22.0);
    assert_eq!(source.face_ascent, 17.0);
    assert_eq!(source.row_height, 25.0);
    assert_eq!(source.row_ascent, 19.0);
    assert_eq!(source.default_line_height, 16.0);
    assert!(source.stretch_like);
    assert!(source.ends_at_visible_eob);
    assert_eq!(source.cursor_fg, Color::from_pixel(0x00112233));
}

#[test]
fn cursor_geometry_source_builds_from_display_point_snapshot() {
    let point = DisplayPointSnapshot {
        buffer_pos: LispCharPos1::from_one_based_usize(4),
        x: 11,
        y: 13,
        width: 17,
        height: 19,
        row: 3,
        col: 5,
    };

    let source = CursorGeometrySource::from_display_point(
        &point,
        VisualCursorGeometryContext {
            window_id: -10,
            text_area_left: 100.0,
            window_top: 7.0,
        },
    );

    assert_eq!(
        source.slot_id,
        DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(-10),
            row: 3,
            col: 5,
        }
    );
    assert_eq!(source.x, 111.0);
    assert_eq!(source.y, 20.0);
    assert_eq!(source.slot_width, 17.0);
    assert_eq!(source.face_height, 19.0);
    assert_eq!(source.face_ascent, 19.0);
    assert_eq!(source.row_height, 19.0);
    assert_eq!(source.row_ascent, 19.0);
    assert_eq!(source.default_line_height, 19.0);
    assert!(!source.stretch_like);
    assert!(!source.ends_at_visible_eob);
    assert_eq!(source.cursor_fg, Color::BLACK);
}

#[test]
fn captured_cursor_info_resolves_explicit_slot_width_before_style_width() {
    let cursor = CapturedCursorInfo::from_visual_state(
        CapturedCursorVisualState {
            face_width: 9.0,
            face_height: 22.0,
            face_ascent: 17.0,
            background: Color::from_pixel(0x00112233),
        },
        CapturedCursorPlacement {
            x: 21.0,
            y: 34.0,
            byte_idx: 0,
            col: 1,
            display_row_offset: 2,
            slot_width: CapturedCursorSlotWidth::Explicit(18.0),
            stretch_like: true,
        },
    );
    let mut params = test_window_params();
    params.x_stretch_cursor = true;

    let width = cursor.resolved_slot_width(CursorStyle::FilledBox, b"\t", &params);

    assert_eq!(width, 18.0);
}

#[test]
fn captured_cursor_info_resolves_missing_slot_width_from_style_width() {
    let mut cursor = CapturedCursorInfo::from_visual_state(
        CapturedCursorVisualState {
            face_width: 8.0,
            face_height: 22.0,
            face_ascent: 17.0,
            background: Color::from_pixel(0x00112233),
        },
        CapturedCursorPlacement {
            x: 21.0,
            y: 34.0,
            byte_idx: 0,
            col: 1,
            display_row_offset: 2,
            slot_width: CapturedCursorSlotWidth::Explicit(18.0),
            stretch_like: true,
        },
    );
    cursor.slot_width = None;
    let mut params = test_window_params();
    params.x_stretch_cursor = true;

    let width = cursor.resolved_slot_width(CursorStyle::FilledBox, b"\t", &params);

    assert_eq!(width, 56.0);
}

#[test]
fn captured_cursor_info_builds_logical_cursor_position() {
    let cursor = CapturedCursorInfo::from_visual_state(
        CapturedCursorVisualState {
            face_width: 9.0,
            face_height: 22.0,
            face_ascent: 17.0,
            background: Color::from_pixel(0x00112233),
        },
        CapturedCursorPlacement {
            x: 21.4,
            y: 34.0,
            byte_idx: 5,
            col: 3,
            display_row_offset: 2,
            slot_width: CapturedCursorSlotWidth::Explicit(18.0),
            stretch_like: true,
        },
    );
    let row_metric = RowMetricsSnapshot::new(9, 9, 32.6, 25.0, 19.0);

    let logical = cursor.logical_cursor_position(row_metric, 7, 10.0, 2.0);

    assert_eq!(logical.x, 11);
    assert_eq!(logical.y, 31);
    assert_eq!(logical.row, 9);
    assert_eq!(logical.col, 3);
}

#[test]
fn captured_text_window_cursor_publish_context_publishes_captured_cursor() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("captured-cursor-publish-context", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 10.0, 20.0);
    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(window_id.0, 1, 10, Rect::new(0.0, 0.0, 160.0, 64.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let cursor = CapturedCursorInfo::from_visual_state(
        CapturedCursorVisualState {
            face_width: 8.0,
            face_height: 16.0,
            face_ascent: 12.0,
            background: Color::from_pixel(0x00112233),
        },
        CapturedCursorPlacement {
            x: 24.0,
            y: 20.0,
            byte_idx: 0,
            col: 3,
            display_row_offset: 0,
            slot_width: CapturedCursorSlotWidth::Explicit(8.0),
            stretch_like: false,
        },
    );
    let mut params = test_window_params();
    params.window_id = window_id.0 as i64;
    params.selected = true;
    params.cursor_color = 0x00ffffff;

    let outcome = CapturedTextWindowCursorPublishContext::new(
        &params, b"abc", 0, 10.0, 20.0, 20.0, 64.0, 8.0, 16.0, 4, false,
    )
    .publish_captured_cursor(
        cursor,
        &[RowMetricsSnapshot::new(0, 0, 20.0, 16.0, 12.0)],
        RowMetricsSnapshot::new(0, 0, 20.0, 16.0, 12.0),
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut output_emitter,
    );

    assert_eq!(outcome, CapturedTextWindowCursorPublishOutcome::Published);
    let phys = builder.phys_cursor().expect("selected phys cursor");
    assert_eq!(
        phys.slot_id,
        DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(window_id.0 as i64),
            row: 0,
            col: 0,
        }
    );
    assert_eq!(phys.charpos, 4);
    assert_eq!(phys.width, 8.0);
    assert_eq!(phys.height, 16.0);
}

#[test]
fn visual_text_window_cursor_publish_context_publishes_decorative_cursor_from_display_point() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("visual-cursor-publish-context", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 10.0, 20.0);
    output_emitter.push_display_point(LispCharPos1::ONE, 34.0, 52.0, 11.0, 17.0, 2, 4);

    let mut params = test_window_params();
    params.x_stretch_cursor = false;
    params.visual_cursors = vec![VisualCursorSpec {
        id: -42,
        charpos: 0,
        cursor_kind: CursorKind::Bar,
        cursor_bar_width: CursorBarWidth::new(3),
        color: 0x00112233,
        effects: None,
    }];
    let mut builder = DisplayOutputBuilder::new();

    let summary = VisualTextWindowCursorPublishContext::new(&params, 10.0, 20.0, 20.0, 80.0, 8.0)
        .publish_visual_cursors(
            TextWindowOutputTarget::from_builder(&mut builder),
            &output_emitter,
        );

    assert_eq!(
        summary,
        VisualTextWindowCursorPublishSummary {
            requested: 1,
            published: 1,
            ..Default::default()
        }
    );
    let state = builder.finish(10, 1, 8.0, 16.0);
    assert_eq!(state.cursors.len(), 1);
    let cursor = &state.cursors[0];
    assert_eq!(cursor.window_id.get(), -42);
    assert_eq!(
        cursor.slot_id,
        DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(-42),
            row: 2,
            col: 4,
        }
    );
    assert_eq!(cursor.x, 34.0);
    assert_eq!(cursor.y, 52.0);
    assert_eq!(cursor.width, 3.0);
    assert_eq!(cursor.height, 17.0);
    assert_eq!(cursor.color, Color::from_pixel(0x00112233));
}

fn test_window_params() -> WindowParams {
    WindowParams {
        window_id: 1,
        buffer_id: 1,
        bounds: Rect::new(0.0, 0.0, 800.0, 600.0),
        text_bounds: Rect::new(0.0, 0.0, 800.0, 560.0),
        selected: true,
        kind: WindowKind::Main,
        left_col: 0,
        top_line: 0,
        window_start: 1,
        force_start: false,
        window_end: 0,
        point: 1,
        buffer_size: 1,
        buffer_begv: 1,
        hscroll: 0,
        vscroll: 0,
        wrap_mode: LineWrapMode::Wrap,
        word_wrap: false,
        tab_width: 8,
        scroll_conservatively: 0,
        scroll_margin: 0,
        tab_stop_list: vec![],
        default_fg: 0xFFFFFF,
        default_bg: 0x000000,
        char_width: 8.0,
        char_height: 16.0,
        window_system: true,
        font_pixel_size: 14.0,
        image_scale_environment: Default::default(),
        font_ascent: 12.0,
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        cursor_kind: neomacs_display_protocol::frame_glyphs::CursorKind::FilledBox,
        cursor_bar_width: CursorBarWidth::TWO,
        x_stretch_cursor: false,
        cursor_color: 0xFFFFFF,
        cursor_effects: None,
        visual_cursors: Vec::new(),
        left_fringe_width: 0.0,
        right_fringe_width: 0.0,
        fringes_outside_margins: false,
        indicate_empty_lines: 0,
        show_trailing_whitespace: false,
        trailing_ws_bg: 0,
        fill_column_indicator: -1,
        fill_column_indicator_char: '|',
        fill_column_indicator_fg: 0,
        extra_line_spacing: 0.0,
        selective_display: 0,
        escape_glyph_fg: 0,
        nobreak_char_display: 0,
        nobreak_char_fg: 0,
        glyphless_char_fg: 0,
        wrap_prefix: vec![],
        line_prefix: vec![],
        left_margin_width: 0.0,
        left_margin_columns: 0,
        right_margin_width: 0.0,
        right_margin_columns: 0,
        vertical_scroll_bar_side: None,
        horizontal_scroll_bar: false,
        scroll_bar_pixel_width: 0.0,
        scroll_bar_pixel_height: 0.0,
    }
}

fn realize_test_gui_frame(eval: &mut Context, frame_id: neovm_core::window::FrameId) {
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.install_gnu_gui_default_parameters();
    }
    assert!(eval.frame_manager_mut().select_frame(frame_id));
    let results = eval
        .eval_str_each("(internal-set-lisp-face-attribute 'default :height 100 (selected-frame))");
    assert!(
        results.iter().all(Result::is_ok),
        "test GUI frame should have a realized default face height, got {results:?}"
    );
}

#[derive(Default)]
struct RecordingImageDisplayHost {
    requests: Arc<Mutex<Vec<ImageResolveRequest>>>,
    video_requests: Arc<Mutex<Vec<VideoResolveRequest>>>,
    webkit_requests: Arc<Mutex<Vec<WebKitResolveRequest>>>,
    surface_requests: Arc<Mutex<Vec<SurfaceResolveRequest>>>,
}

impl DisplayHost for RecordingImageDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_image_sync(
        &self,
        _request: ImageResolveRequest,
    ) -> Result<Option<ReadyImage>, String> {
        panic!("layout must not use synchronous image resolution");
    }

    fn image_catalog(&self) -> Option<&dyn ImageCatalog> {
        Some(self)
    }

    fn request_video(&self, request: VideoResolveRequest) -> Result<Option<ResolvedVideo>, String> {
        self.video_requests
            .lock()
            .expect("video requests lock")
            .push(request);
        Ok(Some(ResolvedVideo { video_id: 88 }))
    }

    fn request_webkit(
        &self,
        request: WebKitResolveRequest,
    ) -> Result<Option<ResolvedWebKit>, String> {
        self.webkit_requests
            .lock()
            .expect("webkit requests lock")
            .push(request);
        Ok(Some(ResolvedWebKit { webkit_id: 99 }))
    }

    fn request_surface(
        &self,
        request: SurfaceResolveRequest,
    ) -> Result<Option<ResolvedSurface>, String> {
        self.surface_requests
            .lock()
            .expect("surface requests lock")
            .push(request);
        Ok(Some(ResolvedSurface { surface_id: 4343 }))
    }
}

impl ImageCatalog for RecordingImageDisplayHost {
    fn lookup(&self, request: ImageResolveRequest) -> ImageLookup {
        self.requests
            .lock()
            .expect("requests lock")
            .push(request.clone());
        ImageLookup::Pending(PendingImage::new(77, 32, 24))
    }
}

fn window_matrix_text(entry: &neomacs_display_protocol::glyph_matrix::WindowMatrixEntry) -> String {
    entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled)
        .flat_map(|row| row.glyphs[1].iter())
        .filter_map(|glyph| match &glyph.glyph_type {
            neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch } => Some(*ch),
            neomacs_display_protocol::glyph_matrix::GlyphType::Composite { text } => {
                text.chars().next()
            }
            _ => None,
        })
        .collect()
}

fn enabled_window_row_texts(
    entry: &neomacs_display_protocol::glyph_matrix::WindowMatrixEntry,
) -> Vec<String> {
    entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled)
        .map(|row| {
            row.glyphs[1]
                .iter()
                .filter_map(|glyph| match &glyph.glyph_type {
                    neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch } => Some(*ch),
                    neomacs_display_protocol::glyph_matrix::GlyphType::Composite { text } => {
                        text.chars().next()
                    }
                    _ => None,
                })
                .collect()
        })
        .collect()
}

/// Concatenated text of every enabled row's text area (`glyphs[1]`) in a
/// backend layout trace.  Char glyphs contribute their character; composites
/// their text.  Used to assert on rendered output (e.g. ellipsis runs).
fn backend_trace_text_area_text(trace: &BackendLayoutTrace) -> String {
    trace
        .matrix_rows
        .iter()
        .filter(|row| row.enabled)
        .flat_map(|row| row.glyph_areas[1].iter())
        .filter(|glyph| !glyph.padding)
        .filter_map(|glyph| match &glyph.kind {
            GlyphKindTrace::Char(ch) | GlyphKindTrace::Glyphless(ch) => Some(ch.to_string()),
            GlyphKindTrace::Composite(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn glyphs_logical_text(glyphs: &[Glyph]) -> String {
    glyphs
        .iter()
        .filter(|glyph| !glyph.padding)
        .map(|glyph| match &glyph.glyph_type {
            GlyphType::Char { ch } | GlyphType::Glyphless { ch } => ch.to_string(),
            GlyphType::Composite { text } => text.to_string(),
            GlyphType::Stretch { width_cols } => " ".repeat(usize::from(*width_cols)),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_buffer_text_source_shadow_row(
    buf_id: BufferId,
    snapshot: &LayoutBufferSnapshot,
    line_end: CharPos0,
    width_px: f32,
    height_px: f32,
    ascent_px: f32,
    char_width_px: f32,
) -> GlyphRow {
    let mut source = crate::display_buffer_text_source::BufferTextSourceCursor::new(
        buf_id,
        snapshot,
        CharPos0::ZERO,
        line_end,
        RenderFaceRef::FaceId(FaceId::new(0)),
    );
    let mut font_metrics = None;
    let mut renderer = DisplayRowRenderer::new(&mut font_metrics);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ff_ffff, 0x0000_0000, 14.0, None);
    let mut face_ids = FrameFaceIdAllocator::new(1);
    DisplayRowSourceFragmentFrame::new(
        DisplayRowGeometry::new(
            0.0,
            width_px,
            height_px,
            char_width_px,
            ascent_px,
            DisplayTabPolicy::every(8),
        ),
        GlyphRowRole::Text,
        FaceId::new(0),
        resolver.default_face(),
    )
    .render_request(DisplayRowRenderBounds::new(
        DisplayRowPosition::new(0.0, 0),
        DisplayRowMaxX::Bounded(width_px),
    ))
    .render(&mut renderer, &mut source, &resolver, &mut face_ids)
    .expect("typed buffer text source row")
    .into_row()
}

fn expected_gui_glyph_advance(
    metrics: &mut FontMetricsService,
    ch: char,
    family: &str,
    weight: u16,
    italic: bool,
    font_size: f32,
) -> f32 {
    let face_metrics = metrics.font_metrics(family, weight, italic, font_size);
    let columns = crate::composition::base_width_cols(ch);
    let minimum = f32::from(columns) * face_metrics.char_width.max(1.0);
    let measured = metrics.char_width(ch, family, weight, italic, font_size);

    GlyphAdvanceQuantization::PreserveLogicalPixels.resolve(Some(measured), minimum, minimum)
}

fn assert_point_width_matches_advance(
    point: &DisplayPointSnapshot,
    expected_advance: f32,
    label: &str,
    all_points: &[DisplayPointSnapshot],
) {
    let expected_width = expected_advance.round() as i64;
    assert!(
        (point.width - expected_width).abs() <= 1,
        "expected {label} width near {expected_width} ({expected_advance:.3}px), got {point:?}; points={all_points:?}"
    );
}

fn assert_point_delta_matches_advance(
    from: &DisplayPointSnapshot,
    to: &DisplayPointSnapshot,
    expected_advance: f32,
    label: &str,
    all_points: &[DisplayPointSnapshot],
) {
    let observed = (to.x - from.x) as f32;
    assert!(
        (observed - expected_advance).abs() <= 1.0,
        "expected {label} x delta near {expected_advance:.3}px, got {} -> {}; points={all_points:?}",
        from.x,
        to.x
    );
}

fn assert_replacement_slot_between_neighbors(
    eval: &Context,
    frame_id: neovm_core::window::FrameId,
    replacement_pos: usize,
    expected_width: i64,
) -> DisplayPointSnapshot {
    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(frame.selected_window)
        .expect("display snapshot");
    let before = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(
            replacement_pos.saturating_sub(1),
        ))
        .expect("previous point");
    let replacement = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(replacement_pos))
        .expect("replacement point");
    let after = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(replacement_pos + 1))
        .expect("following point");

    assert_eq!(replacement.x, before.x + before.width);
    assert_eq!(replacement.width, expected_width);
    assert_eq!(replacement.row, before.row);
    assert_eq!(replacement.row, after.row);
    assert!(
        replacement.x + replacement.width <= after.x,
        "replacement slot should own the covered source geometry before following text; before={before:?} replacement={replacement:?} after={after:?}"
    );
    replacement.clone()
}

#[test]
fn accepted_presentation_publishes_identical_evaluator_and_renderer_window_regions() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.set_buffer_local("header-line-format", Value::string("HEADER"));
        buffer.set_buffer_local("tab-line-format", Value::string("TAB"));
        buffer.set_buffer_local("mode-line-format", Value::string("MODE"));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("presented-regions", 1975, 1214, buffer_id);
    let selected = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let left_side = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            selected,
            neovm_core::window::SplitDirection::Horizontal,
            buffer_id,
            Some(144),
            neovm_core::window::SplitPlacement::BeforeTarget,
        )
        .expect("left side window");
    let _bottom_side = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            left_side,
            neovm_core::window::SplitDirection::Vertical,
            buffer_id,
            Some(100),
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("bottom side window");
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.eval_str(
        "(progn
           (insert \"hello\")
           (set-window-margins nil 2 3)
           (set-window-fringes nil 8 10 t)
           (set-window-scroll-bars nil 12 'left 8 'bottom)
           (modify-frame-parameters nil '((right-divider-width . 6)
                                          (bottom-divider-width . 5))))",
    )
    .expect("explicit main window geometry");
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
        let display = frame
            .find_window_mut(left_side)
            .and_then(|window| window.display_mut())
            .expect("left side display state");
        display.scroll_bar_width = 12;
        display.vertical_scroll_bar_type = Value::symbol("right");
        display.scroll_bar_height = 0;
        display.horizontal_scroll_bar_type = Value::NIL;
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let evaluator_presentation = activate_last_engine_presentation(&mut eval, &engine, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let first_publication = frame
        .active_presentation_geometry()
        .expect("first immutable publication")
        .clone();
    let renderer = engine
        .last_frame_display_state
        .as_ref()
        .expect("renderer presentation");
    assert_eq!(renderer.presentation_id.get(), evaluator_presentation.get());
    for window_id in [left_side, selected] {
        let presented_window = first_publication
            .resolve(neovm_core::window::geometry::WindowGeometryQuery::new(
                evaluator_presentation,
                window_id,
            ))
            .expect("evaluator presented window");
        let typed_regions = presented_window.regions();
        let info = renderer
            .window_infos
            .iter()
            .find(|info| info.window_id.get() == window_id.0 as i64)
            .expect("renderer window regions");
        let neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Complete {
            cell_origin,
            regions,
        } = info.geometry
        else {
            panic!("complete renderer geometry");
        };
        assert_eq!(
            cell_origin.column,
            presented_window.cell_origin().column().get()
        );
        assert_eq!(
            cell_origin.line,
            presented_window.cell_origin().line().get()
        );
        assert!(typed_regions.matches_transport(&regions));
    }
    let left = first_publication
        .resolve(neovm_core::window::geometry::WindowGeometryQuery::new(
            evaluator_presentation,
            left_side,
        ))
        .expect("left geometry")
        .regions();
    let main = first_publication
        .resolve(neovm_core::window::geometry::WindowGeometryQuery::new(
            evaluator_presentation,
            selected,
        ))
        .expect("main geometry")
        .regions();
    assert_eq!(left.outer().origin().x().get(), 0.0);
    assert_eq!(left.outer().width().get(), 144.0);
    assert_eq!(main.outer().origin().x().get(), 144.0);
    assert_eq!(main.left_margin_columns(), 2);
    assert_eq!(main.right_margin_columns(), 3);
    assert!(main.left_margin().is_some());
    assert!(main.right_margin().is_some());
    assert!(main.left_fringe().is_some());
    assert!(main.right_fringe().is_some());
    assert!(main.left_scroll_bar().is_some());
    assert!(main.horizontal_scroll_bar().is_some());
    assert!(main.tab_line().is_some());
    assert!(main.header_line().is_some());
    assert!(main.mode_line().is_some());
    assert!(left.right_divider().is_some());
    assert!(left.bottom_divider().is_some());
    assert!(left.right_scroll_bar().is_some());

    let protocol_presentation =
        neomacs_display_protocol::PresentationId::new(evaluator_presentation.get());
    let hit_index = &renderer.presented_hit_index;
    let selected_regions = match renderer
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == selected.0 as i64)
        .expect("selected renderer window")
        .geometry
    {
        neomacs_display_protocol::PresentedWindowGeometry::Complete { regions, .. } => regions,
        _ => panic!("selected complete geometry"),
    };
    let selected_matrix = renderer
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected.0 as i64)
        .expect("selected renderer matrix");
    assert_eq!(
        selected_matrix.text_area_clip_rect(),
        selected_regions.text_body,
        "the matrix clip and published body must come from one canonical window partition"
    );
    for (kind, bounds) in [
        (
            neomacs_display_protocol::PresentedRegionKind::TextBody,
            Some(selected_regions.text_body),
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::LeftMargin,
            selected_regions.left_margin,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::RightMargin,
            selected_regions.right_margin,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::LeftFringe,
            selected_regions.left_fringe,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::RightFringe,
            selected_regions.right_fringe,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::LeftScrollBar,
            selected_regions.left_scroll_bar,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::HorizontalScrollBar,
            selected_regions.horizontal_scroll_bar,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::TabLine,
            selected_regions.tab_line,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::HeaderLine,
            selected_regions.header_line,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::ModeLine,
            selected_regions.mode_line,
        ),
    ] {
        let bounds = bounds.expect("configured semantic region");
        let hit = hit_index
            .resolve(neomacs_display_protocol::PresentedHitQuery::new(
                protocol_presentation,
                bounds.x + bounds.width / 2.0,
                bounds.y + bounds.height / 2.0,
            ))
            .expect("current presentation")
            .expect("region hit");
        assert_eq!(hit.region().kind(), kind);
        assert_eq!(
            hit.region().window().map(|id| id.get()),
            Some(selected.0 as i64)
        );
    }
    let text_position = hit_index
        .text_positions()
        .iter()
        .find(|position| position.window().get() == selected.0 as i64)
        .copied()
        .expect("selected exact text position");
    let bounds = text_position.bounds();
    let exact_hit = hit_index
        .resolve(neomacs_display_protocol::PresentedHitQuery::new(
            protocol_presentation,
            bounds.x() + bounds.width() / 2.0,
            bounds.y() + bounds.height() / 2.0,
        ))
        .unwrap()
        .unwrap();
    assert_eq!(exact_hit.text_position(), Some(text_position));

    let mut snapshots = renderer
        .window_infos
        .iter()
        .filter_map(|info| {
            frame
                .redisplay_snapshot(neovm_core::window::WindowId(info.window_id.get() as u64))
                .cloned()
        })
        .collect::<Vec<_>>();
    let mut poisoned_transport = renderer.clone().into_state();
    let neomacs_display_protocol::PresentedWindowGeometry::Complete {
        regions: transported_regions,
        ..
    } = &mut poisoned_transport
        .window_infos
        .iter_mut()
        .find(|info| info.window_id.get() == selected.0 as i64)
        .expect("selected transported window")
        .geometry
    else {
        panic!("complete transported geometry");
    };
    transported_regions.text_body.x += 500.0;
    let poisoned = snapshots
        .iter_mut()
        .find(|snapshot| snapshot.window_id == selected)
        .expect("selected display snapshot");
    let poisoned_point = poisoned.points.first_mut().expect("visible point");
    poisoned_point.y = 777;
    poisoned_point.row = 999;
    let poisoned_x = poisoned_point.x;
    poisoned
        .body_rows
        .push(neovm_core::window::PresentedBodyRowSnapshot {
            output_row: 999,
            body_row: 7,
            body_y: 3,
        });
    assert!(
        selected_regions.text_body.y > selected_regions.outer.y,
        "fixture must contain top chrome"
    );
    let spatial = crate::presentation_spatial::PresentationSpatialPlan::compile(
        &poisoned_transport,
        &snapshots,
    )
    .unwrap();
    spatial.seal(&mut poisoned_transport).unwrap();
    let rebuilt = &poisoned_transport.presented_hit_index;
    let neomacs_display_protocol::PresentedWindowGeometry::Complete {
        regions: repaired_regions,
        ..
    } = poisoned_transport
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == selected.0 as i64)
        .expect("selected repaired transport window")
        .geometry
    else {
        panic!("complete repaired transport geometry");
    };
    assert_eq!(repaired_regions, selected_regions);
    let canonical = rebuilt
        .text_positions()
        .iter()
        .find(|position| position.window().get() == selected.0 as i64 && position.row() == 7)
        .copied()
        .expect("canonical body position");
    assert_eq!(
        canonical.bounds().x(),
        selected_regions.text_body.x + poisoned_x as f32
    );
    assert_eq!(canonical.row(), 7);
    assert_eq!(canonical.bounds().y(), selected_regions.text_body.y + 3.0);

    let mut invalid_snapshots = snapshots.clone();
    invalid_snapshots
        .iter_mut()
        .find(|snapshot| snapshot.window_id == selected)
        .expect("selected invalid snapshot")
        .regions
        .text_body
        .width = -1.0;
    assert_eq!(
        crate::presentation_spatial::PresentationSpatialPlan::compile(
            renderer,
            &invalid_snapshots,
        )
        .map(|plan| plan.hit_index().clone()),
        Err(neomacs_display_protocol::PresentedHitError::InvalidRegionGeometry)
    );
    let mut zero_snapshots = snapshots.clone();
    let zero = zero_snapshots
        .iter_mut()
        .find(|snapshot| snapshot.window_id == selected)
        .expect("selected zero-area snapshot");
    zero.points.clear();
    zero.regions.text_body.width = 0.0;
    let zero_index =
        crate::presentation_spatial::PresentationSpatialPlan::compile(renderer, &zero_snapshots)
            .unwrap();
    let zero_index = zero_index.hit_index();
    assert!(!zero_index.regions().iter().any(|region| {
        region.id()
            == neomacs_display_protocol::PresentedRegionId::new(
                Some(neomacs_display_protocol::DisplayWindowId::new(
                    selected.0 as i64,
                )),
                neomacs_display_protocol::PresentedRegionKind::TextBody,
            )
    }));

    let materialized = renderer.materialize();
    let left_transport = match renderer
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == left_side.0 as i64)
        .unwrap()
        .geometry
    {
        neomacs_display_protocol::PresentedWindowGeometry::Complete { regions, .. } => regions,
        _ => panic!("left complete geometry"),
    };
    for (kind, bounds, x) in [
        (
            neomacs_display_protocol::PresentedRegionKind::RightScrollBar,
            left_transport.right_scroll_bar.unwrap(),
            left_transport.right_scroll_bar.unwrap().x + 1.0,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::RightDivider,
            left_transport.right_divider.unwrap(),
            left_transport.right_divider.unwrap().x
                + left_transport.right_divider.unwrap().width / 2.0,
        ),
        (
            neomacs_display_protocol::PresentedRegionKind::BottomDivider,
            left_transport.bottom_divider.unwrap(),
            left_transport.bottom_divider.unwrap().x
                + left_transport.bottom_divider.unwrap().width / 2.0,
        ),
    ] {
        let hit = materialized
            .resolve_presented_hit(neomacs_display_protocol::PresentedHitQuery::new(
                protocol_presentation,
                x,
                bounds.y + bounds.height / 2.0,
            ))
            .unwrap()
            .unwrap();
        assert_eq!(hit.semantic().unwrap().region().kind(), kind);
    }

    let retained_main = first_publication
        .resolve(neovm_core::window::geometry::WindowGeometryQuery::new(
            evaluator_presentation,
            selected,
        ))
        .expect("retained main geometry")
        .regions();

    engine.layout_frame_rust(&mut eval, frame_id);
    let replacement = activate_last_engine_presentation(&mut eval, &engine, frame_id);
    let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
    assert_eq!(frame.active_presentation(), Some(replacement));
    assert_ne!(replacement, first_publication.presentation());
    frame.resize_pixelwise(1600, 900);
    assert_eq!(frame.active_presentation(), Some(replacement));
    assert_eq!(
        first_publication
            .resolve(neovm_core::window::geometry::WindowGeometryQuery::new(
                evaluator_presentation,
                selected,
            ))
            .expect("retained publication survives later layout and invalidation")
            .regions(),
        retained_main
    );
}

#[test]
fn presentation_spatial_maps_blank_body_rows_to_their_buffer_position() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.eval_str("(progn (erase-buffer) (insert \"a\\n\\nb\\n\") (goto-char 1))")
        .expect("blank-line fixture");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("blank-row-hit", 672, 720, buffer_id);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let selected = frame.selected_window;
    let snapshot = frame
        .redisplay_snapshot(selected)
        .expect("selected redisplay snapshot");
    let blank_position = LispCharPos1::from_one_based_usize(3);
    let blank_row = snapshot
        .rows
        .iter()
        .find(|row| row.start_buffer_pos == Some(blank_position))
        .expect("blank line has a semantic row anchor");
    let (body_row, body_y) = snapshot.text_body_position(blank_row.row, blank_row.y);
    let renderer = engine
        .last_frame_display_state
        .as_ref()
        .expect("renderer presentation");
    let regions = match renderer
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == selected.0 as i64)
        .expect("selected renderer window")
        .geometry
    {
        neomacs_display_protocol::PresentedWindowGeometry::Complete { regions, .. } => regions,
        other => panic!("expected complete geometry, got {other:?}"),
    };

    let hit = renderer
        .presented_hit_index
        .resolve(neomacs_display_protocol::PresentedHitQuery::new(
            renderer.presentation_id,
            regions.text_body.x + 1.0,
            regions.text_body.y + body_y as f32 + blank_row.height as f32 / 2.0,
        ))
        .expect("current presentation")
        .expect("text-body hit");
    let text = hit
        .text_position()
        .expect("every visible body row must publish a semantic source position");
    assert_eq!(text.buffer_position(), blank_position.as_i64());
    assert_eq!(text.row(), body_row);
}

#[test]
fn skipped_zero_body_window_still_publishes_known_regions_and_cell_origin() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("zero-body-regions", 800, 600, buffer_id);
    let selected = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame.find_window_mut(selected).expect("window");
        window.set_bounds(neovm_core::window::Rect::new(144.0, 24.0, 0.0, 100.0));
        window.set_left_col(18);
        window.set_top_line(2);
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    assert!(frame.active_presentation().is_some());
    let presentation = frame.active_presentation().expect("presentation");
    let presented_window = frame
        .active_presentation_geometry()
        .expect("geometry")
        .resolve(neovm_core::window::geometry::KnownWindowGeometryQuery::new(
            presentation,
            selected,
        ))
        .expect("skipped presented window");
    assert_eq!(presented_window.outer().origin().x().get(), 144.0);
    assert_eq!(presented_window.outer().origin().y().get(), 24.0);
    assert_eq!(presented_window.outer().width().get(), 0.0);
    assert_eq!(presented_window.outer().height().get(), 100.0);
    assert_eq!(presented_window.cell_origin().column().get(), 18);
    assert_eq!(presented_window.cell_origin().line().get(), 2);
    assert!(matches!(
        frame
            .active_presentation_geometry()
            .expect("geometry")
            .resolve(neovm_core::window::geometry::WindowGeometryQuery::new(
                presentation,
                selected,
            )),
        Err(neovm_core::window::geometry::GeometryQueryError::MissingMaterializedGeometry(id)) if id == selected
    ));

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let info = state
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == selected.0 as i64)
        .expect("renderer zero-body regions");
    let neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Skipped {
        cell_origin,
        outer,
    } = info.geometry
    else {
        panic!("skipped renderer geometry");
    };
    assert_eq!(cell_origin.column, 18);
    assert_eq!(cell_origin.line, 2);
    assert_eq!(
        outer,
        neomacs_display_protocol::types::Rect::new(144.0, 24.0, 0.0, 100.0)
    );
}

fn enabled_window_row_texts_expanding_stretches(
    entry: &neomacs_display_protocol::glyph_matrix::WindowMatrixEntry,
) -> Vec<String> {
    entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled)
        .map(|row| {
            row.glyphs[1]
                .iter()
                .flat_map(|glyph| match &glyph.glyph_type {
                    neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch } => {
                        std::iter::repeat_n(*ch, 1).collect::<Vec<_>>()
                    }
                    neomacs_display_protocol::glyph_matrix::GlyphType::Composite { text } => {
                        text.chars().collect::<Vec<_>>()
                    }
                    neomacs_display_protocol::glyph_matrix::GlyphType::Stretch { width_cols } => {
                        std::iter::repeat_n(' ', usize::from(*width_cols)).collect::<Vec<_>>()
                    }
                    _ => Vec::new(),
                })
                .collect()
        })
        .collect()
}

fn implemented_text_backends() -> impl Iterator<Item = BufferTextBackendKind> {
    BufferTextBackendKind::implemented_variants()
}

fn convert_current_buffer_text_backend(eval: &mut Context, kind: BufferTextBackendKind) {
    let form = format!("(neomacs-set-buffer-text-backend '{})", kind.symbol_name());
    let result = eval
        .eval_str(&form)
        .unwrap_or_else(|err| panic!("convert buffer text backend with {form}: {err}"));
    assert_eq!(result.as_symbol_name(), Some(kind.symbol_name()));
}

fn insert_fragmented_current_buffer_text(eval: &mut Context, text: &str) {
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer = eval
        .buffer_manager_mut()
        .get_mut(buffer_id)
        .expect("current buffer");
    buffer.insert(text);

    for marker in ["\n", "日本", "Ω"] {
        if let Some(pos) = text.find(marker) {
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(pos));
            buffer.insert("tmp");
            buffer.delete_emacs_byte_range(emacs_byte_range(pos, pos + "tmp".len()));
        }
    }

    assert_eq!(buffer.buffer_string(), text);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GlyphKindTrace {
    Char(char),
    Composite(String),
    Stretch(u16),
    Image(i32),
    Surface(i32),
    Glyphless(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GlyphTrace {
    kind: GlyphKindTrace,
    // The RESOLVED face content (id normalized away), not the opaque per-frame
    // face_id. The rendered output uses the face the id resolves to; the id is an
    // allocation artifact that legitimately differs between frames (e.g. a fast
    // path re-registers reused faces + reserves their id range, shifting the
    // walk's fresh ids). Resolving also catches a face_id MISSING from the frame
    // faces table (the face-id collision bug) as "UNREGISTERED".
    face: String,
    charpos: usize,
    bidi_level: u8,
    wide: bool,
    padding: bool,
    pixel_width_bits: u32,
    pixel_height_bits: u32,
    pixel_ascent_bits: u32,
}

impl GlyphTrace {
    fn from_glyph(
        glyph: &Glyph,
        faces: &std::collections::HashMap<FaceId, neomacs_display_protocol::face::Face>,
    ) -> Self {
        let kind = match &glyph.glyph_type {
            GlyphType::Char { ch } => GlyphKindTrace::Char(*ch),
            GlyphType::Composite { text } => GlyphKindTrace::Composite(text.to_string()),
            GlyphType::Stretch { width_cols } => GlyphKindTrace::Stretch(*width_cols),
            GlyphType::Image { image_id, .. } => GlyphKindTrace::Image(*image_id),
            GlyphType::Surface { surface_id, .. } => GlyphKindTrace::Surface(*surface_id),
            GlyphType::Video { video_id, .. } => GlyphKindTrace::Image(*video_id),
            GlyphType::Xwidget { xwidget_id, .. } => GlyphKindTrace::Image(*xwidget_id),
            GlyphType::Glyphless { ch } => GlyphKindTrace::Glyphless(*ch),
        };
        Self {
            kind,
            face: faces
                .get(&glyph.face_id)
                .map(|f| {
                    // Compare CONTENT — normalize the allocation-dependent Face.id.
                    let mut f = f.clone();
                    f.id = FaceId::new(0);
                    format!("{f:?}")
                })
                .unwrap_or_else(|| format!("UNREGISTERED#{}", glyph.face_id)),
            charpos: glyph.charpos,
            bidi_level: glyph.bidi_level,
            wide: glyph.wide,
            padding: glyph.padding,
            pixel_width_bits: glyph.pixel_width.to_bits(),
            pixel_height_bits: glyph.pixel_height.to_bits(),
            pixel_ascent_bits: glyph.pixel_ascent.to_bits(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RowTrace {
    role: GlyphRowRole,
    enabled: bool,
    cursor_col: Option<u16>,
    cursor_type: Option<String>,
    truncated_left: bool,
    continued: bool,
    displays_text: bool,
    ends_at_zv: bool,
    mode_line: bool,
    pixel_y_bits: u32,
    height_px_bits: u32,
    ascent_px_bits: u32,
    start_charpos: usize,
    end_charpos: usize,
    glyph_areas: [Vec<GlyphTrace>; 3],
}

impl RowTrace {
    fn from_row(
        row: &GlyphRow,
        faces: &std::collections::HashMap<FaceId, neomacs_display_protocol::face::Face>,
    ) -> Self {
        Self {
            role: row.role,
            enabled: row.enabled,
            cursor_col: row.cursor_col,
            cursor_type: row.cursor_type.map(|cursor| format!("{cursor:?}")),
            truncated_left: row.truncated_left,
            continued: row.continued,
            displays_text: row.displays_text,
            ends_at_zv: row.ends_at_zv,
            mode_line: row.mode_line,
            pixel_y_bits: row.pixel_y.to_bits(),
            height_px_bits: row.height_px.to_bits(),
            ascent_px_bits: row.ascent_px.to_bits(),
            start_charpos: row.start_charpos,
            end_charpos: row.end_charpos,
            glyph_areas: [
                row.glyphs[0]
                    .iter()
                    .map(|g| GlyphTrace::from_glyph(g, faces))
                    .collect(),
                row.glyphs[1]
                    .iter()
                    .map(|g| GlyphTrace::from_glyph(g, faces))
                    .collect(),
                row.glyphs[2]
                    .iter()
                    .map(|g| GlyphTrace::from_glyph(g, faces))
                    .collect(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendLayoutTrace {
    matrix_rows: Vec<RowTrace>,
    points: Vec<DisplayPointSnapshot>,
    output_rows: Vec<DisplayRowSnapshot>,
    phys_cursor: Option<WindowCursorSnapshot>,
    visible_span: Option<WindowVisibleBufferSpan>,
    window_start: LispCharPos1,
    window_point: LispCharPos1,
    /// GNU `w->window_end_pos` (offset of the last displayed char from Z) — the
    /// published `window-end`. Validated so a fast path that mis-derives it (e.g.
    /// a bounded walk that no longer reaches the last visible row) is caught.
    window_end_pos: usize,
    /// GNU `w->window_end_bytepos` (byte companion of `window_end_pos`).
    window_end_bytepos: usize,
}

fn selected_window_layout_trace(
    eval: &Context,
    engine: &LayoutEngine,
    frame_id: neovm_core::window::FrameId,
) -> BackendLayoutTrace {
    let selected = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    window_layout_trace(eval, engine, frame_id, selected)
}

/// Like [`selected_window_layout_trace`] but for an ARBITRARY window — used by
/// multi-window goldens to verify a NON-selected window's output byte-for-byte.
fn window_layout_trace(
    eval: &Context,
    engine: &LayoutEngine,
    frame_id: neovm_core::window::FrameId,
    selected_window: neovm_core::window::WindowId,
) -> BackendLayoutTrace {
    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let window_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let display_snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let (window_start, window_point, window_end_pos, window_end_bytepos) =
        match frame.find_window(selected_window).expect("selected window") {
            neovm_core::window::Window::Leaf {
                window_start,
                point,
                window_end_pos,
                window_end_bytepos,
                ..
            } => (*window_start, *point, *window_end_pos, *window_end_bytepos),
            other => panic!("expected leaf window, got {other:?}"),
        };
    BackendLayoutTrace {
        matrix_rows: window_entry
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled)
            .map(|row| RowTrace::from_row(row, &state.faces))
            .collect(),
        points: display_snapshot.points.clone(),
        output_rows: display_snapshot.rows.clone(),
        phys_cursor: display_snapshot.phys_cursor.clone(),
        visible_span: display_snapshot.visible_buffer_span(),
        window_start,
        window_point,
        window_end_pos,
        window_end_bytepos,
    }
}

fn backend_layout_trace_with_buffer_and_window_setup(
    kind: BufferTextBackendKind,
    frame_name: &str,
    text: &str,
    frame_width: u32,
    frame_height: u32,
    setup: impl FnOnce(&mut neovm_core::buffer::Buffer, BufferId, &str),
    setup_window: impl FnOnce(&mut neovm_core::window::Window),
) -> BackendLayoutTrace {
    let mut eval = Context::new();
    convert_current_buffer_text_backend(&mut eval, kind);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        insert_fragmented_current_buffer_text(&mut eval, text);
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        setup(buffer, buf_id, text);
        assert_eq!(buffer.text_backend_kind(), kind);
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame(frame_name, frame_width, frame_height, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
        setup_window(window);
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    selected_window_layout_trace(&eval, &engine, frame_id)
}

/// Layout-engine micro-benchmark — the REDISPLAY LAYOUT cost, the rank-1
/// interactive-latency cost center (the engine had ZERO timers in ~49k LOC). One
/// GUI engine over a realistic buffer/frame: COLD first layout then WARM
/// steady-state (min-of-N). The engine rebuilds the frame in full every cycle (no
/// incremental fast-path), so warm = the per-redisplay-cycle floor a keystroke
/// pays. Reports via panic! (like the jit_bench_* family) so the line surfaces
/// under nextest capture; the test "fails" by design. Build needs the jit feature
/// (a bare neovm-core build is broken on this branch, pre-existing). Run:
///   cargo nextest run -p neomacs-layout-engine --features neovm-core/jit \
///     --release --run-ignored ignored-only --no-capture layout_bench_warm
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn layout_bench_warm() {
    use std::time::{Duration, Instant};
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    // ~120 lines of representative code-like ASCII (a real editing buffer).
    let text = "(defun example-helper (alpha beta) (let ((sum (+ alpha beta))) (* sum sum)))\n"
        .repeat(120);
    insert_fragmented_current_buffer_text(&mut eval, &text);
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-bench", 1000, 700, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
    }
    let mut engine = LayoutEngine::new();
    let t0 = Instant::now();
    engine.layout_frame_rust(&mut eval, frame_id);
    let cold = t0.elapsed();
    let mut best = Duration::MAX;
    for _ in 0..100 {
        let t = Instant::now();
        engine.layout_frame_rust(&mut eval, frame_id);
        best = best.min(t.elapsed());
    }
    // Incremental-layout gate metric (Phase 0a): a warm repaint still relays
    // every body row and reuses none — the full-rebuild floor a keystroke pays.
    let stats = engine.last_layout_stats().clone();
    let shape_calls = engine
        .font_metrics
        .as_ref()
        .map(|m| m.shape_calls())
        .unwrap_or(0);
    panic!(
        "BENCH layout_frame_rust 1000x700 (~120-line buffer, GUI): warm {best:?} cold {cold:?} \
         | relaid_body={} relaid_chrome={} reused={} reused_shifted={} full_windows={} \
         shape_calls_total={}",
        stats.relaid_body_rows,
        stats.relaid_chrome_rows,
        stats.reused_rows,
        stats.reused_shifted_rows,
        stats.full_windows,
        shape_calls,
    );
}

/// Phase 0a baseline (incremental-layout gate). With no fast path wired yet,
/// every layout cycle is a full rebuild: even a no-op repaint relays every body
/// row and reuses none. This pins that baseline so the Phase 1+ fast paths have
/// a relaid-row-count to beat, and so a later phase that silently regresses to
/// full-rebuild is caught by the same metric (spec §7 overarching NO-GO).
#[test]
fn phase0a_layout_stats_reports_full_rebuild_baseline() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    insert_fragmented_current_buffer_text(&mut eval, &text);
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("p0a-baseline", 800, 600, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    // A single COLD layout (no retained matrix yet) is always a full rebuild —
    // every body row is laid from scratch, no reuse machinery applies. (A warm
    // no-op repaint now takes the no-change cursor-only path; see
    // `no_change_relayout_reuses_verbatim`.)
    engine.layout_frame_rust(&mut eval, frame_id);

    let body_rows: usize = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state")
        .window_matrices
        .iter()
        .flat_map(|entry| entry.matrix.rows.iter())
        .filter(|row| row.enabled && !row.mode_line)
        .count();
    assert!(
        body_rows > 0,
        "expected the buffer to lay out some body rows"
    );

    let stats = engine.last_layout_stats();
    assert_eq!(
        stats.relaid_body_rows, body_rows,
        "Phase 0a relays every body row every cycle (full rebuild)"
    );
    assert_eq!(stats.reused_rows, 0, "no reuse machinery exists yet");
    assert_eq!(
        stats.reused_shifted_rows, 0,
        "no reuse machinery exists yet"
    );
    assert!(
        stats.full_windows >= 1 && stats.total_windows() == stats.full_windows,
        "every window is classified Full in Phase 0a (got {stats:?})"
    );
}

/// One incremental-relayout measurement: the instrumentation of the SECOND
/// layout pass (the one an interactive keystroke pays), plus the body/chrome
/// row totals present in the final matrix. Produced by
/// [`measure_incremental_relayout`].
#[derive(Debug, Clone)]
struct IncrCaseMeasurement {
    stats: crate::incremental_layout::LayoutStats,
    total_body_rows: usize,
    total_chrome_rows: usize,
}

/// The incremental-layout bench harness. Warm the engine on `frame_id` (which
/// establishes the retained matrices), apply `perturb`, lay out again, and
/// capture the instrumentation of that second pass. The second pass is what a
/// keystroke pays, so its relaid-row-count is the number each fast path (Phases
/// 1-3) must drive down — and the metric a silent regression to full-rebuild
/// would expose (spec §5, §7).
fn measure_incremental_relayout(
    engine: &mut LayoutEngine,
    eval: &mut Context,
    frame_id: neovm_core::window::FrameId,
    perturb: impl FnOnce(&mut Context),
) -> IncrCaseMeasurement {
    engine.layout_frame_rust(eval, frame_id); // warm: builds retained matrices
    perturb(eval);
    engine.layout_frame_rust(eval, frame_id); // the measured (keystroke) pass
    let stats = engine.last_layout_stats().clone();
    let (mut total_body_rows, mut total_chrome_rows) = (0usize, 0usize);
    for entry in &engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state")
        .window_matrices
    {
        for row in &entry.matrix.rows {
            if !row.enabled {
                continue;
            }
            if row.mode_line {
                total_chrome_rows += 1;
            } else {
                total_body_rows += 1;
            }
        }
    }
    IncrCaseMeasurement {
        stats,
        total_body_rows,
        total_chrome_rows,
    }
}

/// Fresh editing context with `text` in the current buffer (gap backend, point
/// at beginning), plus a GUI frame whose selected window starts at BOB. Returns
/// the frame id, buffer id, and selected window id for perturbation.
fn incr_editing_frame(
    text: &str,
    width: u32,
    height: u32,
) -> (
    Context,
    neovm_core::window::FrameId,
    BufferId,
    neovm_core::window::WindowId,
) {
    let mut eval = Context::new();
    convert_current_buffer_text_backend(&mut eval, BufferTextBackendKind::GapBuffer);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    insert_fragmented_current_buffer_text(&mut eval, text);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("incr-bench", width, height, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
    }
    (eval, frame_id, buf_id, selected_window)
}

/// Phase 1 — CURSOR MOVE takes the cursor-only fast path. A bare point move (no
/// scroll, no text change, no tick movement) reuses the retained body rows
/// verbatim instead of re-laying them: the selected window is classified
/// `CursorOnly`, its body rows are `reused_rows` (not `relaid_body_rows`), and
/// chrome is always re-walked. Was the Phase 0a full-rebuild baseline.
#[test]
fn phase1_cursor_move_is_cursor_only() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, _win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    });
    assert!(m.total_body_rows > 0, "expected body rows laid out");
    // Exactly the selected content window took the cursor-only fast path; the
    // minibuffer is probe-excluded from retention so it stays Full.
    assert_eq!(
        m.stats.cursor_only_windows, 1,
        "selected window took the cursor-only fast path (got {:?})",
        m.stats
    );
    assert!(
        m.stats.reused_rows > 0,
        "cursor-only reuses retained body rows (got {:?})",
        m.stats
    );
    // Conservation: every enabled body row in the final matrix is either reused
    // verbatim or relaid from scratch.
    assert_eq!(
        m.stats.reused_rows + m.stats.relaid_body_rows,
        m.total_body_rows,
        "body rows conserved across reuse/relayout (got {:?})",
        m.stats
    );
    // The selected window relaid ZERO body rows; any residual relaid body rows
    // belong to the probe-excluded minibuffer only.
    assert!(
        m.stats.relaid_body_rows < m.total_body_rows,
        "cursor-only drives relaid body rows below the full-rebuild total (got {:?})",
        m.stats
    );
}

#[test]
fn cursor_only_replay_republishes_the_window_regions_with_the_new_presentation() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, selected) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    let (first_presentation, first_regions, first_cell_origin) = {
        let frame = eval.frame_manager().get(frame_id).expect("frame");
        let snapshot = frame.redisplay_snapshot(selected).expect("snapshot");
        (
            frame.active_presentation().expect("presentation"),
            snapshot.regions,
            snapshot.cell_origin,
        )
    };

    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);

    assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let second_presentation = frame.active_presentation().expect("presentation");
    let snapshot = frame.redisplay_snapshot(selected).expect("snapshot");
    assert_ne!(second_presentation, first_presentation);
    assert_eq!(snapshot.regions, first_regions);
    assert_eq!(snapshot.cell_origin, first_cell_origin);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    assert_eq!(state.presentation_id.get(), second_presentation.get());
    let info = state
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == selected.0 as i64)
        .expect("window info");
    let neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Complete {
        regions, ..
    } = info.geometry
    else {
        panic!("complete presented window regions");
    };
    assert_eq!(regions.outer.x, snapshot.regions.outer.x);
    assert_eq!(regions.text_body.y, snapshot.regions.text_body.y);
}

/// Phase 1 GOLDEN — the cursor-only fast path output must be BYTE-IDENTICAL to a
/// full rebuild of the same post-move state (honest layering, spec §4.6: all
/// glyphs still emitted; only the layout-CPU is saved). Compares the full window
/// trace — matrix glyphs, cursor decoration, display points, snapshot rows,
/// phys-cursor, visible span, and hit-test rows.
#[test]
fn phase1_cursor_move_matches_full_rebuild_golden() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);

    // Reference: a fresh engine lays out the MOVED state from scratch (no
    // retained matrix → full rebuild).
    let (mut eval_ref, frame_ref, buf_ref, _wr) = incr_editing_frame(&text, 800, 600);
    {
        let buffer = eval_ref
            .buffer_manager_mut()
            .get_mut(buf_ref)
            .expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    }
    let mut ref_engine = LayoutEngine::new();
    ref_engine.layout_frame_rust(&mut eval_ref, frame_ref);
    let reference = selected_window_layout_trace(&eval_ref, &ref_engine, frame_ref);

    // Incremental: warm at point 0, move to 10, second pass takes cursor-only.
    let (mut eval, frame_id, buf_id, _wi) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    }
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().cursor_only_windows,
        1,
        "expected the measured pass to take the cursor-only fast path"
    );
    let incremental = selected_window_layout_trace(&eval, &engine, frame_id);

    assert_eq!(
        incremental, reference,
        "cursor-only output must be byte-identical to a full rebuild"
    );
}

/// Phase 1 — an OVERLAY change (hl-line / show-paren / region) co-moving with
/// the cursor MUST bail to a full rebuild: the overlay tick moved, so the
/// retained rows are no longer trustworthy. This is the invalidation-completeness
/// guarantee (spec §3) — silently staying cursor-only here would ship stale rows.
#[test]
fn phase1_overlay_change_bails_to_full() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, _win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        // Use the Lisp `make-overlay` builtin (the path hl-line / show-paren take)
        // so the overlay tick is bumped, exactly as real code does.
        eval.eval_str("(overlay-put (make-overlay 1 24) 'face 'highlight)")
            .expect("make-overlay");
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    });
    assert_eq!(
        m.stats.cursor_only_windows, 0,
        "overlay tick moved → must NOT take the cursor-only fast path (got {:?})",
        m.stats
    );
    assert_eq!(
        m.stats.reused_rows, 0,
        "overlay change forces a full rebuild"
    );
}

/// Phase 1 REGRESSION (split-window cursor clobber) — with TWO windows on the
/// same buffer, a point move that drives the SELECTED window down the cursor-only
/// fast path while the NON-selected window is also re-decorated cursor-only must
/// leave the frame's single overwritable phys-cursor slot owned by the SELECTED
/// window (filled box). The bug: the cursor-only path installed the FRAME
/// phys_cursor UNCONDITIONALLY for every window, so a non-selected window taking
/// the fast path clobbered the selected window's cursor → the selected window's
/// caret vanished (C-x 3 then C-p C-p, live X11 repro). Production's full-rebuild
/// path gates the frame phys_cursor strictly on `selected`; the fast path must
/// match. Single-window goldens never exercised the clobber.
#[test]
fn phase1_split_window_frame_phys_cursor_stays_on_selected_window() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, selected_window) = incr_editing_frame(&text, 800, 600);
    // C-x 3: split-window-right onto the SAME buffer; the original (left) window
    // stays selected, the new (right) window is non-selected.
    let _right_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            selected_window,
            neovm_core::window::SplitDirection::Horizontal,
            buf_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window onto the same buffer");

    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        // C-p C-p: move point in the selected window only (no text edit, no tick
        // move) so both same-buffer windows re-decorate down the cursor-only path.
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    });

    // Both content windows re-decorate cursor-only (this is the case that clobbers).
    assert!(
        m.stats.cursor_only_windows >= 2,
        "both same-buffer windows should take the cursor-only fast path (got {:?})",
        m.stats
    );

    let phys = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state")
        .phys_cursor
        .clone()
        .expect("frame phys cursor present");
    assert_eq!(
        phys.window_id.get(),
        selected_window.0 as i64,
        "frame phys_cursor must belong to the SELECTED window, not a non-selected \
         window that clobbered it (got window {:?}, selected {:?}); style={:?}",
        phys.window_id.get(),
        selected_window.0,
        phys.style,
    );
    assert_eq!(
        phys.style,
        CursorStyle::FilledBox,
        "the selected window's frame cursor is a filled box, not a non-selected \
         window's hollow box (clobber signature)",
    );
}

/// Phase 1 REGRESSION (reverse window order) — same clobber hazard as
/// [`phase1_split_window_frame_phys_cursor_stays_on_selected_window`] but with the
/// RIGHT window selected, so the non-selected window renders in the opposite
/// order. The frame phys_cursor must still end up owned by the selected (right)
/// window regardless of which window the render loop visits last.
#[test]
fn phase1_split_window_frame_phys_cursor_stays_on_selected_window_reverse_order() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, selected_window) = incr_editing_frame(&text, 800, 600);
    let right_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            selected_window,
            neovm_core::window::SplitDirection::Horizontal,
            buf_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window onto the same buffer");
    // Select the RIGHT window: now the LEFT (original) window is non-selected and
    // renders in the opposite order relative to the selected one.
    assert!(
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .select_window(right_window),
        "select the right window",
    );

    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    });
    assert!(
        m.stats.cursor_only_windows >= 2,
        "both same-buffer windows should take the cursor-only fast path (got {:?})",
        m.stats
    );

    let phys = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state")
        .phys_cursor
        .clone()
        .expect("frame phys cursor present");
    assert_eq!(
        phys.window_id.get(),
        right_window.0 as i64,
        "frame phys_cursor must belong to the SELECTED (right) window (got {:?}, \
         selected {:?}); style={:?}",
        phys.window_id.get(),
        right_window.0,
        phys.style,
    );
    assert_eq!(phys.style, CursorStyle::FilledBox);
}

/// Phase 1 — a `put-text-property` (face/display/invisible) co-moving with the
/// cursor MUST bail: the props tick moved (the soundness hazard of spec §3, where
/// a non-fontify text-property write would otherwise be invisible to redisplay).
#[test]
fn phase1_put_text_property_bails_to_full() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, _win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        eval.eval_str("(put-text-property 1 6 'face 'bold)")
            .expect("put-text-property");
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    });
    assert_eq!(
        m.stats.cursor_only_windows, 0,
        "props tick moved → must NOT take the cursor-only fast path (got {:?})",
        m.stats
    );
    assert_eq!(
        m.stats.reused_rows, 0,
        "text-property change forces a full rebuild"
    );
}

/// Phase 1 — a face-attribute change (theme load / `set-face-attribute`) co-moving
/// with the cursor MUST bail: `face_change_count` moved, mutating pixels with no
/// buffer tick (spec §3: the per-glyph hash cannot backstop face-content drift).
#[test]
fn phase1_face_attribute_change_bails_to_full() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, _win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        eval.eval_str(
            "(internal-set-lisp-face-attribute 'default :foreground \"red\" (selected-frame))",
        )
        .expect("set-face-attribute");
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    });
    assert_eq!(
        m.stats.cursor_only_windows, 0,
        "face_change_count moved → must NOT take the cursor-only fast path (got {:?})",
        m.stats
    );
}

/// Set the selected window's `window_start` (1-based) and point (0-based byte).
fn scroll_window_to(
    eval: &mut Context,
    frame_id: neovm_core::window::FrameId,
    selected_window: neovm_core::window::WindowId,
    buf_id: BufferId,
    window_start_1based: i64,
    point_byte: usize,
) {
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point_byte));
    }
    let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
    let window = frame
        .find_window_mut(selected_window)
        .expect("selected window");
    if let neovm_core::window::Window::Leaf { window_start, .. } = window {
        *window_start = LispCharPos1::new(window_start_1based);
    }
}

/// Phase 2 — a whole-row SCROLL takes the pure-scroll fast path: the overlapping
/// rows are reused shifted (`reused_shifted_rows`), only the newly-exposed rows
/// are walked, and chrome is re-walked. Was the Phase 0a full-rebuild baseline.
#[test]
fn phase2_scroll_is_pure_scroll() {
    let line = "(defun f (a b) (+ a b))\n"; // 24 bytes incl newline
    let text = line.repeat(80);
    let (mut eval, frame_id, buf_id, selected_window) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        // Scroll down 5 whole lines, point following into the visible region.
        scroll_window_to(
            eval,
            frame_id,
            selected_window,
            buf_id,
            5 * line.len() as i64 + 1,
            7 * line.len(),
        );
    });
    assert!(m.total_body_rows > 0, "expected body rows laid out");
    assert_eq!(
        m.stats.scroll_windows, 1,
        "selected window took the pure-scroll fast path (got {:?})",
        m.stats
    );
    assert!(
        m.stats.reused_shifted_rows > 0,
        "pure-scroll reuses overlapping rows shifted (got {:?})",
        m.stats
    );
    assert_eq!(
        m.stats.reused_shifted_rows + m.stats.relaid_body_rows,
        m.total_body_rows,
        "every body row is reused-shifted or newly relaid (got {:?})",
        m.stats
    );
    assert!(
        m.stats.relaid_body_rows < m.total_body_rows,
        "most rows reused; only the newly-exposed ones relaid (got {:?})",
        m.stats
    );
}

/// REGRESSION (recenter-to-top first-row corruption): scroll DOWN so line 1 is
/// no longer visible, then scroll back to the TOP (window_start charpos 0). The
/// scroll-up must bail to a full rebuild — NOT spuriously match the trailing
/// past-last-line placeholder row (start_charpos == 0) and emit a phantom blank
/// leading row + one-column-clipped first line. The final output must be
/// byte-identical (role/start/end/pixel_y) to a full rebuild at window_start 1.
#[test]
fn scroll_to_top_does_not_match_trailing_placeholder_row() {
    let line = "recenter line 01\n"; // 17 bytes, like the failing TUI test
    let text = line.repeat(80);
    let down_start = 30 * line.len() as i64 + 1; // line 31 at top; line 1 not visible

    // Incremental: warm at top, scroll DOWN, then scroll back to the TOP.
    let (mut eval, frame_id, buf_id, win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    scroll_window_to(
        &mut eval,
        frame_id,
        win,
        buf_id,
        down_start,
        30 * line.len(),
    );
    engine.layout_frame_rust(&mut eval, frame_id);
    scroll_window_to(&mut eval, frame_id, win, buf_id, 1, 0);
    engine.layout_frame_rust(&mut eval, frame_id);
    let incremental = selected_window_layout_trace(&eval, &engine, frame_id);

    // Reference: fresh full rebuild at window_start 1.
    let (mut eval_ref, frame_ref, _br, _wr) = incr_editing_frame(&text, 800, 600);
    let mut ref_engine = LayoutEngine::new();
    ref_engine.layout_frame_rust(&mut eval_ref, frame_ref);
    let reference = selected_window_layout_trace(&eval_ref, &ref_engine, frame_ref);

    let shape = |t: &BackendLayoutTrace| -> Vec<(GlyphRowRole, usize, usize, u32)> {
        t.matrix_rows
            .iter()
            .map(|r| (r.role, r.start_charpos, r.end_charpos, r.pixel_y_bits))
            .collect()
    };
    assert_eq!(
        shape(&incremental),
        shape(&reference),
        "\nscroll-to-top must match full rebuild, not phantom-shift the first row\n INC={:#?}\n REF={:#?}",
        shape(&incremental),
        shape(&reference)
    );
}

/// Phase 2 GOLDEN — the pure-scroll output must be BYTE-IDENTICAL to a full
/// rebuild of the same scrolled state (reused-shifted rows + newly-exposed rows
/// + re-walked chrome + cursor).
#[test]
fn phase2_scroll_matches_full_rebuild_golden() {
    let line = "(defun f (a b) (+ a b))\n";
    let text = line.repeat(80);
    let new_window_start = 5 * line.len() as i64 + 1;
    let point_byte = 7 * line.len();

    // Reference: fresh engine lays out the scrolled state from scratch.
    let (mut eval_ref, frame_ref, buf_ref, win_ref) = incr_editing_frame(&text, 800, 600);
    scroll_window_to(
        &mut eval_ref,
        frame_ref,
        win_ref,
        buf_ref,
        new_window_start,
        point_byte,
    );
    let mut ref_engine = LayoutEngine::new();
    ref_engine.layout_frame_rust(&mut eval_ref, frame_ref);
    let reference = selected_window_layout_trace(&eval_ref, &ref_engine, frame_ref);

    // Incremental: warm, scroll, second pass takes the pure-scroll path.
    let (mut eval, frame_id, buf_id, win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    scroll_window_to(
        &mut eval,
        frame_id,
        win,
        buf_id,
        new_window_start,
        point_byte,
    );
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().scroll_windows,
        1,
        "expected the measured pass to take the pure-scroll fast path"
    );
    let incremental = selected_window_layout_trace(&eval, &engine, frame_id);

    assert_eq!(
        incremental, reference,
        "pure-scroll output must be byte-identical to a full rebuild"
    );
}

/// Phase 2 — a PARTIAL-ROW scroll (window_start not on a retained row boundary)
/// must bail to a full rebuild: the uniform row shift only applies to whole-row
/// scrolls.
#[test]
fn phase2_partial_row_scroll_bails_to_full() {
    let line = "(defun f (a b) (+ a b))\n";
    let text = line.repeat(80);
    let (mut eval, frame_id, buf_id, selected_window) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        // window_start mid-line (not a row boundary).
        scroll_window_to(
            eval,
            frame_id,
            selected_window,
            buf_id,
            5 * line.len() as i64 + 4,
            7 * line.len(),
        );
    });
    assert_eq!(
        m.stats.scroll_windows, 0,
        "partial-row scroll must NOT take the pure-scroll fast path (got {:?})",
        m.stats
    );
    assert_eq!(m.stats.reused_shifted_rows, 0);
}

/// Phase 0a baseline — SINGLE-CHAR INSERT in a FONT-LOCKED buffer. The
/// `fontification-functions` hook re-applies `font-lock-face` over the edited
/// region during layout (a `put-text-property` that bumps NO tick today — the
/// soundness hazard of spec §3). Phase 0a relays everything; Phase 3 (gated on
/// per-span fontify reporting, §0b) is what narrows this.
#[test]
fn phase0a_baseline_fontlocked_edit_is_full_rebuild() {
    let text = "alpha beta gamma delta epsilon zeta\n".repeat(30);
    let (mut eval, frame_id, buf_id, _win) = incr_editing_frame(&text, 480, 400);
    eval.eval_str(
        r#"
        (setq neomacs-test-fontify-face 'font-lock-keyword-face)
        (setq fontification-functions
              (list (lambda (start)
                      (let ((end (min (point-max) (+ start 80))))
                        (put-text-property start end 'fontified t)
                        (put-text-property start end 'font-lock-face
                                           neomacs-test-fontify-face)))))
        "#,
    )
    .expect("install fontification hook");
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(6));
        buffer.insert("x");
    });
    assert!(m.total_body_rows > 0, "expected body rows laid out");
    assert_eq!(
        m.stats.relaid_body_rows, m.total_body_rows,
        "Phase 0a: font-locked edit is a full rebuild (got {:?})",
        m.stats
    );
    assert_eq!(m.stats.edit_windows, 0, "no localized-edit fast path yet");
    // Ordinary characters are measured independently, matching GNU's
    // IT_CHARACTER path, so a full redisplay need not invoke the contextual
    // run shaper when the concrete character advances are already cached.
}

/// Phase 0a baseline — MULTI-WINDOW SAME BUFFER. Two windows on one buffer; an
/// edit relays BOTH fully today. This is the case the multi-window race fix
/// (spec §4.2) must keep sound once the fast paths land: each window diffs from
/// its own retained tick. Phase 0a just pins that both are `Full`.
#[test]
fn phase0a_baseline_multi_window_same_buffer_is_full_rebuild() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, selected_window) = incr_editing_frame(&text, 800, 600);
    eval.frame_manager_mut()
        .split_window(
            frame_id,
            selected_window,
            neovm_core::window::SplitDirection::Horizontal,
            buf_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window onto the same buffer");
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
        buffer.insert("z");
    });
    assert!(
        m.stats.full_windows >= 2,
        "both same-buffer windows full-rebuild in Phase 0a (got {:?})",
        m.stats
    );
    assert_eq!(m.stats.reused_rows, 0, "no reuse machinery yet");
    assert_eq!(
        m.stats.total_windows(),
        m.stats.full_windows,
        "every window is classified Full in Phase 0a (got {:?})",
        m.stats
    );
    let _ = m.total_chrome_rows; // tracked for later phases (chrome always re-walked)
}

fn backend_layout_trace_with_buffer_setup(
    kind: BufferTextBackendKind,
    frame_name: &str,
    text: &str,
    frame_width: u32,
    frame_height: u32,
    setup: impl FnOnce(&mut neovm_core::buffer::Buffer, BufferId, &str),
) -> BackendLayoutTrace {
    backend_layout_trace_with_buffer_and_window_setup(
        kind,
        frame_name,
        text,
        frame_width,
        frame_height,
        setup,
        |_| {},
    )
}

fn layout_trace_for_plain_text(text: &str) -> BackendLayoutTrace {
    layout_trace_with_buffer_setup(text, 360, 180, |_, _, _| {})
}

#[test]
fn mouse_position_query_resolves_blank_buffer_row() {
    let trace = layout_trace_for_plain_text("a\n\nb");
    let blank_row = trace
        .output_rows
        .iter()
        .find(|row| row.row == 1)
        .expect("layout must publish the blank second row");
    let blank_row_y = blank_row.y + blank_row.height / 2;
    assert_eq!(
        blank_row.start_buffer_pos,
        Some(LispCharPos1::new(3)),
        "the blank row must retain its semantic buffer anchor"
    );
    let snapshot = WindowDisplaySnapshot {
        points: trace.points,
        rows: trace.output_rows,
        ..WindowDisplaySnapshot::default()
    };

    let hit = snapshot
        .point_at_coords(0, blank_row_y)
        .expect("a blank displayed row must still resolve to its buffer position");

    assert_eq!(hit.buffer_pos, LispCharPos1::new(3));
}

fn layout_trace_with_buffer_setup(
    text: &str,
    frame_width: u32,
    frame_height: u32,
    setup: impl FnOnce(&mut neovm_core::buffer::Buffer, BufferId, &str),
) -> BackendLayoutTrace {
    layout_trace_with_buffer_and_window_setup(text, frame_width, frame_height, setup, |_| {})
}

fn layout_trace_with_buffer_and_window_setup(
    text: &str,
    frame_width: u32,
    frame_height: u32,
    setup: impl FnOnce(&mut neovm_core::buffer::Buffer, BufferId, &str),
    setup_window: impl FnOnce(&mut neovm_core::window::Window),
) -> BackendLayoutTrace {
    let mut eval = Context::new();
    convert_current_buffer_text_backend(&mut eval, BufferTextBackendKind::GapBuffer);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        insert_fragmented_current_buffer_text(&mut eval, text);
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        setup(buffer, buf_id, text);
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "typed-source-parity",
        frame_width,
        frame_height,
        buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
        setup_window(window);
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    selected_window_layout_trace(&eval, &engine, frame_id)
}

#[test]
fn current_body_row_metrics_reads_retained_matrix_rows() {
    // Smooth scroll P1-T2: after a layout, the engine exposes the selected window's
    // body rows as (start_charpos, height_px) metrics for the pixel-scroll mapper.
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    insert_fragmented_current_buffer_text(&mut eval, &text);
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("p1-metrics", 800, 600, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let metrics = engine
        .current_body_row_metrics(neomacs_display_protocol::types::DisplayWindowId::new(
            selected_window.0 as i64,
        ))
        .expect("body row metrics after a layout populates the retained matrix");
    assert!(!metrics.is_empty(), "expected some body rows");
    assert!(
        metrics.iter().all(|m| m.height_px > 0),
        "every body row has a positive pixel height: {metrics:?}"
    );
    for pair in metrics.windows(2) {
        assert!(
            pair[1].start_charpos >= pair[0].start_charpos,
            "body rows ascend by start_charpos: {metrics:?}"
        );
    }
}

#[test]
fn pixel_scroll_window_applies_sub_line_vscroll() {
    // Smooth scroll P1-T3b: a small (sub-row) pixel scroll keeps window-start and
    // sets vscroll to the negated residual; end-to-end through the mapper + setter.
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    insert_fragmented_current_buffer_text(&mut eval, &text);
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("p1-scroll", 800, 600, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let start_before = match eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .find_window(selected_window)
        .expect("window")
    {
        neovm_core::window::Window::Leaf { window_start, .. } => window_start.as_i64(),
        _ => panic!("expected leaf"),
    };

    // Scroll down 3 pixels — smaller than a row, so window-start is unchanged and
    // vscroll becomes -3.
    assert_eq!(
        engine.pixel_scroll_window(&mut eval, selected_window, 3),
        Some(()),
        "sub-line pixel scroll applies"
    );

    match eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .find_window(selected_window)
        .expect("window")
    {
        neovm_core::window::Window::Leaf {
            window_start,
            vscroll,
            ..
        } => {
            assert_eq!(
                window_start.as_i64(),
                start_before,
                "sub-line scroll keeps window-start"
            );
            assert_eq!(*vscroll, -3, "vscroll = -residual after a 3px scroll");
        }
        _ => panic!("expected leaf"),
    }
}

#[test]
fn layout_frame_rust_lays_out_plain_text() {
    let text = "Hello, world!\nThis is a test.\n";
    let trace = layout_trace_for_plain_text(text);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_lays_out_mixed_chars() {
    let text = "a\tb\n\u{0001}c\n日\nd\u{200b}\n";
    let trace = layout_trace_for_plain_text(text);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_lays_out_face_property() {
    let text = "abc\ndef\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, text: &str| {
        let start = text.find('b').expect("b start");
        let end = start + "bc".len();
        assert!(buffer.put_text_property(start, end, Value::symbol("face"), Value::symbol("bold")));
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_lays_out_simple_display_property() {
    let text = "abcXYZdef\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, text: &str| {
        let start = text.find("XYZ").expect("XYZ start");
        let end = start + "XYZ".len();
        assert!(buffer.put_text_property(start, end, Value::symbol("display"), Value::string("R")));
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_lays_out_truncated_long_line() {
    let text = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, _text: &str| {
        buffer.set_buffer_local("truncate-lines", Value::T);
    };
    let trace = layout_trace_with_buffer_setup(text, 120, 120, setup);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_lays_out_wrapped_long_line() {
    let text = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, _text: &str| {
        buffer.set_buffer_local("truncate-lines", Value::NIL);
    };
    let trace = layout_trace_with_buffer_setup(text, 120, 120, setup);

    assert!(!trace.matrix_rows.is_empty());
}

/// Stage 5: a long line with `truncate-lines=t` (wider than the window) sets
/// the `right-arrow` truncation bitmap in the RIGHT fringe of the truncated row.
#[test]
fn layout_frame_rust_truncated_row_sets_right_arrow_fringe() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let right_arrow_index: u16 = eval
        .eval_str("(get 'right-arrow 'fringe)")
        .expect("right-arrow fringe prop")
        .as_fixnum()
        .expect("fringe index") as u16;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ\n");
        buf.set_buffer_local("truncate-lines", Value::T);
        buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("trunc-fringe", 48, 200, buf_id);
    // Window-system frame so the fringes have width (GNU only draws fringe
    // bitmaps on GUI frames; TTY frames have 0-width fringes).
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");

    let right_arrow_rows = entry
        .matrix
        .rows
        .iter()
        .filter(|row| {
            row.right_fringe_bitmap
                .is_some_and(|info| info.bitmap_index == right_arrow_index)
        })
        .count();
    assert!(
        right_arrow_rows >= 1,
        "a truncated long line should set right-arrow in the right fringe \
         (right_arrow_index={right_arrow_index}); right fringe bitmaps = {:?}",
        entry
            .matrix
            .rows
            .iter()
            .map(|r| r.right_fringe_bitmap.map(|i| i.bitmap_index))
            .collect::<Vec<_>>()
    );
}

/// Stage 5: a long line with `truncate-lines=nil` (wraps) sets the
/// `right-curly-arrow` continuation bitmap in the RIGHT fringe of the continued
/// row, and the `left-curly-arrow` on the continuation row's LEFT fringe.
#[test]
fn layout_frame_rust_continued_row_sets_curly_arrow_fringe() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let right_curly_index: u16 = eval
        .eval_str("(get 'right-curly-arrow 'fringe)")
        .expect("right-curly-arrow fringe prop")
        .as_fixnum()
        .expect("fringe index") as u16;
    let left_curly_index: u16 = eval
        .eval_str("(get 'left-curly-arrow 'fringe)")
        .expect("left-curly-arrow fringe prop")
        .as_fixnum()
        .expect("fringe index") as u16;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ\n");
        buf.set_buffer_local("truncate-lines", Value::NIL);
        buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("cont-fringe", 48, 200, buf_id);
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");

    let right_curly_rows = entry
        .matrix
        .rows
        .iter()
        .filter(|row| {
            row.right_fringe_bitmap
                .is_some_and(|info| info.bitmap_index == right_curly_index)
        })
        .count();
    let left_curly_rows = entry
        .matrix
        .rows
        .iter()
        .filter(|row| {
            row.left_fringe_bitmap
                .is_some_and(|info| info.bitmap_index == left_curly_index)
        })
        .count();
    assert!(
        right_curly_rows >= 1,
        "a wrapped line should set right-curly-arrow on the continued row's right \
         fringe (idx={right_curly_index}); right = {:?}",
        entry
            .matrix
            .rows
            .iter()
            .map(|r| r.right_fringe_bitmap.map(|i| i.bitmap_index))
            .collect::<Vec<_>>()
    );
    assert!(
        left_curly_rows >= 1,
        "the continuation row should set left-curly-arrow on its left fringe \
         (idx={left_curly_index}); left = {:?}",
        entry
            .matrix
            .rows
            .iter()
            .map(|r| r.left_fringe_bitmap.map(|i| i.bitmap_index))
            .collect::<Vec<_>>()
    );
}

/// diff-hl / git-gutter / flycheck attach their fringe marker via an overlay
/// before-string (or a text property) whose `display` value is LIST-WRAPPED:
///   `((left-fringe BITMAP FACE))`
/// — a list whose single element is the bare `(left-fringe …)` spec. GNU
/// iterates such a list (`handle_display_spec`) and draws BITMAP in the LEFT
/// fringe while showing NO inline glyph for the covered char (no text shift).
/// Before the list-of-specs unwrap, neomacs failed to classify the inner spec
/// and rendered the covered space INLINE, shifting the line's text right.
#[test]
fn layout_frame_rust_list_wrapped_left_fringe_spec_draws_in_fringe_not_inline() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();

    // Register a user fringe bitmap (diff-hl does this at load time) so the
    // bitmap symbol resolves to a registry index.
    eval.eval_str("(define-fringe-bitmap 'fringespec-test-bmp [255 255] nil nil 'center)")
        .expect("define-fringe-bitmap");
    let bmp_index: u16 = eval
        .eval_str("(get 'fringespec-test-bmp 'fringe)")
        .expect("fringe bitmap index property")
        .as_fixnum()
        .expect("fringe index") as u16;

    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        // Line 1 plain; line 2 begins with a space carrying the fringe marker.
        buf.insert("first\n MARK\n");
        buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }
    // The leading space of line 2 is at char position 7 (1-based: "first" =
    // chars 1-5, "\n" = char 6, the space = char 7). put-text-property covers
    // the half-open range [START, END), so 7..8 marks just the space. Apply the
    // LIST-WRAPPED display spec — exactly diff-hl's shape:
    //   ((left-fringe BITMAP FACE)).
    eval.eval_str(
        "(put-text-property 7 8 'display \
           '((left-fringe fringespec-test-bmp fringe)) \
           (current-buffer))",
    )
    .expect("put-text-property");

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("fringespec", 48, 200, buf_id);
    // Window-system frame so the fringes have width.
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");

    // Gap 1: the list-wrapped `(left-fringe …)` is classified + recorded, so
    // exactly one row carries the bitmap in its LEFT fringe.
    let fringe_rows = entry
        .matrix
        .rows
        .iter()
        .filter(|row| {
            row.left_fringe_bitmap
                .is_some_and(|info| info.bitmap_index == bmp_index)
        })
        .count();
    assert_eq!(
        fringe_rows,
        1,
        "the list-wrapped (left-fringe …) display spec should set the bitmap in \
         the left fringe of exactly one row (idx={bmp_index}); left fringe bitmaps = {:?}",
        entry
            .matrix
            .rows
            .iter()
            .map(|r| r.left_fringe_bitmap.map(|i| i.bitmap_index))
            .collect::<Vec<_>>()
    );

    // The marked row is the one with the fringe bitmap. Its text area must NOT
    // begin with an inline space glyph for the covered char (no text shift): the
    // first text glyph is 'M' (the char after the fringe-marked space).
    let marked_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| {
            row.left_fringe_bitmap
                .is_some_and(|info| info.bitmap_index == bmp_index)
        })
        .expect("marked row");
    let text_glyphs = &marked_row.glyphs[GlyphArea::Text.index()];
    let first_char = text_glyphs.iter().find_map(|g| match &g.glyph_type {
        GlyphType::Char { ch } => Some(*ch),
        _ => None,
    });
    assert_eq!(
        first_char,
        Some('M'),
        "the fringe-covered space must produce NO inline glyph (text not shifted); \
         got text glyphs = {:?}",
        text_glyphs
            .iter()
            .map(|g| g.glyph_type.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn layout_frame_rust_lays_out_line_numbers() {
    let text = "abc\ndef\nghi\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, _text: &str| {
        buffer.set_buffer_local("display-line-numbers", Value::T);
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_lays_out_word_wrap() {
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, _text: &str| {
        buffer.set_buffer_local("truncate-lines", Value::NIL);
        buffer.set_buffer_local("word-wrap", Value::T);
    };
    let trace = layout_trace_with_buffer_setup(text, 120, 120, setup);

    assert!(!trace.matrix_rows.is_empty());
}

/// Full-pipeline regression for the word-wrap word-splitting bug: with
/// `word-wrap=t` / `truncate-lines=nil`, GNU keeps whole words across a wrapped
/// break (`...word02 `|`word03...`), never splitting a word (`...word02 wor`|
/// `d03...`) or dropping the word-start char (`...word02 `|`d03...`).
///
/// The bug had TWO coupled parts and this test catches BOTH:
///   A. the partial word that fit on the first row was left drawn (leftover
///      glyphs), and
///   B. the word-start (candidate) char was already consumed during the
///      overflow attempt and never re-produced — so the continuation row
///      started AFTER it, dropping the word prefix.
/// A first-row-only check catches only (A). This drives a real buffer through
/// the whole layout pipeline and asserts the CONTINUATION row re-renders
/// starting at the SAME word-boundary char the first row stopped before, which
/// only holds when (B) is fixed too (the consumption cursor is rewound).
#[test]
fn word_wrap_keeps_words_whole_across_wrapped_rows() {
    // Equal-length space-separated words: word00 starts at charpos 0, and word
    // N starts at charpos 7*N (each "wordNN " is 7 chars). Buffer chars are pure
    // ASCII so charpos == byte index.
    let text = "word00 word01 word02 word03 word04 word05 word06 word07 word08 word09 word10 word11 word12";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, _text: &str| {
        buffer.set_buffer_local("truncate-lines", Value::NIL);
        buffer.set_buffer_local("word-wrap", Value::T);
    };
    let trace = layout_trace_with_buffer_setup(text, 200, 240, setup);

    // The first text glyph (char + its charpos) of each non-mode-line Text row.
    let row_first_text_glyphs: Vec<(usize, char)> = trace
        .matrix_rows
        .iter()
        .filter(|row| !row.mode_line && row.role == GlyphRowRole::Text && row.displays_text)
        .filter_map(|row| {
            row.glyph_areas[1]
                .iter()
                .find_map(|glyph| match glyph.kind {
                    GlyphKindTrace::Char(ch) => Some((glyph.charpos, ch)),
                    _ => None,
                })
        })
        .collect();

    // The buffer wraps onto multiple rows (200px / 8px-per-char ≈ 24 cols, the
    // 89-char line needs >=4 rows). Need at least one wrapped continuation row to
    // exercise the break.
    assert!(
        row_first_text_glyphs.len() >= 2,
        "expected the long line to wrap onto multiple rows, got {row_first_text_glyphs:?}"
    );

    // Every Text row (the first AND every continuation row) must begin at a WORD
    // START. Word starts are at charpos 7*N ('w'); a split/dropped word would
    // begin mid-word (e.g. charpos 22 'r' of word03 after dropping "wor", or
    // charpos 25 '0' if the whole "word" prefix was dropped). This is the
    // load-bearing assertion that part B fixes: without the consumption-cursor
    // rewind the continuation row starts AFTER the candidate char.
    for (charpos, ch) in &row_first_text_glyphs {
        assert_eq!(
            charpos % 7,
            0,
            "continuation row starts mid-word at charpos {charpos} (char {ch:?}); \
             word-wrap split or dropped a word. first-text glyphs per row: {row_first_text_glyphs:?}"
        );
        assert_eq!(
            *ch, 'w',
            "the word-start char at charpos {charpos} should be 'w' (the 'w' of a 'wordNN'); \
             got {ch:?} — the candidate char was dropped. rows: {row_first_text_glyphs:?}"
        );
    }

    // Pin the EXACT continuation seam: the first wrapped continuation row must
    // re-render starting at the candidate char 'w' of word03 (charpos 21) — the
    // same word boundary the first row stopped before. With the bug, this row
    // instead started at charpos 25 ('0' of "...03"), dropping "word".
    assert_eq!(
        row_first_text_glyphs[1],
        (21, 'w'),
        "first continuation row must re-render the dropped word-start char (word03 @ charpos 21); \
         rows: {row_first_text_glyphs:?}"
    );
}

// Walk-state coverage guards: these scenarios exercise the typed-source walk
// through item-step arms (control chars, NBSP/SHY, selective-display '\r') or
// bypass item consumption entirely (invisible/hscroll short-circuit before
// source-item consumption). They remain as single-path regression guards so
// the NBSP / selective-display / invisible / hscroll / complex-text scenarios
// keep being laid out.

#[test]
fn layout_frame_rust_lays_out_invisible_text() {
    let text = "abcXYZdef\nghi\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, text: &str| {
        let start = text.find("XYZ").expect("XYZ start");
        let end = start + "XYZ".len();
        assert!(buffer.put_text_property(start, end, Value::symbol("invisible"), Value::T));
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_display_string_newline_terminates_row() {
    // Regression for the Info *dir* breadcrumbs bug: a `display` string that
    // ENDS IN A NEWLINE, covering a buffer line plus its terminating newline,
    // must render its text and then break the row (GNU xdisp.c: a display line
    // "ends in a newline from a display string"). The bare buffer newline that
    // follows (an empty line) then produces its own blank row. Without the fix
    // the display string's '\n' was ignored, the following buffer newline
    // terminated the display-string row instead, and the blank row was dropped
    // -- shifting every later row up by one.
    //
    // Buffer "AAA\n\nBBB\n": line0 "AAA" (chars 0..3), '\n' at 3, empty line1
    // ('\n' at 4), "BBB" at 5. The `display` = "X\n" covers "AAA\n" (0..4).
    let text = "AAA\n\nBBB\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, _text: &str| {
        assert!(buffer.put_text_property(0, 4, Value::symbol("display"), Value::string("X\n"),));
    };
    let trace = layout_trace_with_buffer_setup(text, 400, 240, setup);

    // Text of each non-mode-line Text row, in display order.
    let row_texts: Vec<String> = trace
        .matrix_rows
        .iter()
        .filter(|row| !row.mode_line && row.role == GlyphRowRole::Text && row.enabled)
        .map(|row| {
            row.glyph_areas[1]
                .iter()
                .filter_map(|glyph| match &glyph.kind {
                    GlyphKindTrace::Char(ch) | GlyphKindTrace::Glyphless(ch) => {
                        Some(ch.to_string())
                    }
                    GlyphKindTrace::Composite(text) => Some(text.clone()),
                    _ => None,
                })
                .collect::<String>()
        })
        .collect();

    let x_row = row_texts
        .iter()
        .position(|t| t == "X")
        .unwrap_or_else(|| panic!("display string row `X` must render, rows={row_texts:?}"));
    let bbb_row = row_texts
        .iter()
        .position(|t| t == "BBB")
        .unwrap_or_else(|| panic!("buffer text `BBB` must render, rows={row_texts:?}"));

    // The empty buffer line must occupy its own blank row BETWEEN `X` and `BBB`.
    assert_eq!(
        bbb_row,
        x_row + 2,
        "display-string newline must leave a blank row for the empty buffer line, rows={row_texts:?}"
    );
    assert_eq!(
        row_texts[x_row + 1],
        "",
        "the row between `X` and `BBB` must be the blank empty-line row, rows={row_texts:?}"
    );
}

#[test]
fn layout_frame_rust_emits_one_ellipsis_for_invisible_region_split_by_face() {
    // Regression for the org-fold "long dot-fill" bug: ONE contiguous invisible
    // region that has a DIFFERENT text property (`face`) changing in its middle
    // must collapse to exactly ONE ellipsis.  The buggy code computed the
    // invisible region's end via the next change of ANY text property, so the
    // mid-region `face` boundary fragmented the region into several `...` runs
    // (a long dot-fill).  The fix scans only the `invisible` property's next
    // change (GNU `next_single_char_property_change(pos, Qinvisible, ...)`), so
    // the whole region is skipped once -> one ellipsis.
    //
    // Buffer text avoids literal `.` so every `.` glyph comes from an ellipsis.
    let text = "AAAfooBBBbarCCC\nDDD\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, text: &str| {
        buffer.set_buffer_local(
            "buffer-invisibility-spec",
            Value::list(vec![Value::cons(Value::symbol("outline"), Value::T)]),
        );
        // One contiguous invisible region covering "fooBBBbar".
        let invis_start = text.find("foo").expect("foo start");
        let invis_end = text.find("CCC").expect("CCC start");
        assert!(buffer.put_text_property(
            invis_start,
            invis_end,
            Value::symbol("invisible"),
            Value::symbol("outline"),
        ));
        // A face change strictly INSIDE the invisible region: this is the
        // unrelated property whose boundary used to fragment the region.
        let face_start = text.find("BBB").expect("BBB start");
        let face_end = face_start + "BBB".len();
        assert!(buffer.put_text_property(
            face_start,
            face_end,
            Value::symbol("face"),
            Value::symbol("bold"),
        ));
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    let rendered = backend_trace_text_area_text(&trace);
    let dot_count = rendered.matches('.').count();
    let ellipsis_runs = rendered.matches("...").count();

    // Exactly one ellipsis (the default `...` = 3 dots) for the whole region.
    assert_eq!(
        dot_count, 3,
        "expected exactly one 3-dot ellipsis for the folded region, got {dot_count} dots; rendered={rendered:?}"
    );
    assert_eq!(
        ellipsis_runs, 1,
        "expected exactly one `...` ellipsis run, got {ellipsis_runs}; rendered={rendered:?}"
    );
    // Visible text on both sides of the fold survives.
    assert!(
        rendered.contains("AAA") && rendered.contains("CCC"),
        "visible text around the fold must render, rendered={rendered:?}"
    );
}

#[test]
fn layout_frame_rust_collapses_consecutive_invisible_runs_to_one_ellipsis() {
    // Regression for org folding over a link: a CONTIGUOUS hidden region whose
    // `invisible` VALUE changes mid-region must collapse to ONE ellipsis. A
    // folded org subtree (`outline`, shows ellipsis) containing a link whose URL
    // is separately invisible (`org-link`, no ellipsis) is three runs of
    // differing `invisible` value but all hidden. GNU `handle_invisible_prop`
    // advances over the consecutive invisible runs showing a single ellipsis;
    // stopping at each value change emitted one per ellipsis-bearing run.
    let text = "AAAfooBBBbarCCC\nDDD\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, text: &str| {
        buffer.set_buffer_local(
            "buffer-invisibility-spec",
            Value::list(vec![
                Value::cons(Value::symbol("outline"), Value::T),
                Value::list(vec![Value::symbol("org-link")]),
            ]),
        );
        let foo = text.find("foo").expect("foo start");
        let bbb = text.find("BBB").expect("BBB start");
        let bbb_end = bbb + "BBB".len();
        let ccc = text.find("CCC").expect("CCC start");
        // `outline` (ellipsis) around an `org-link` (no ellipsis) middle: the
        // `invisible` value changes twice inside one contiguous hidden region.
        assert!(buffer.put_text_property(
            foo,
            bbb,
            Value::symbol("invisible"),
            Value::symbol("outline"),
        ));
        assert!(buffer.put_text_property(
            bbb,
            bbb_end,
            Value::symbol("invisible"),
            Value::symbol("org-link"),
        ));
        assert!(buffer.put_text_property(
            bbb_end,
            ccc,
            Value::symbol("invisible"),
            Value::symbol("outline"),
        ));
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    let rendered = backend_trace_text_area_text(&trace);
    let dot_count = rendered.matches('.').count();
    let ellipsis_runs = rendered.matches("...").count();

    // One ellipsis for the whole collapsed region (the opening `outline` run's
    // ellipsis). Without collapsing this is two (`outline` foo + `outline` bar).
    assert_eq!(
        dot_count, 3,
        "expected ONE 3-dot ellipsis for the collapsed region, got {dot_count} dots; rendered={rendered:?}"
    );
    assert_eq!(
        ellipsis_runs, 1,
        "expected ONE `...` run, got {ellipsis_runs}; rendered={rendered:?}"
    );
    assert!(
        rendered.contains("AAA") && rendered.contains("CCC"),
        "visible text around the fold must render, rendered={rendered:?}"
    );
}

#[test]
fn layout_frame_rust_display_table_maps_char_to_glyph_vector() {
    // `buffer-display-table` maps a buffer char (`x`) to a glyph VECTOR
    // `[?< ?>]`.  A char source position that resolves to MULTIPLE glyphs must
    // render those glyphs into the text area (`a<>b`) AND keep the row
    // non-blank.  The rejected single-char-run + pending_render_items design
    // produced ZERO text glyphs (the whole buffer blanked) while source-item
    // unit tests still saw an item; this drives the full
    // `engine.layout_frame_rust` path and inspects the rendered `GlyphRow`.
    let text = "axb\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _id: BufferId, _t: &str| {
        // A display table is a char-table with 6 extra slots.  Map the
        // per-char slot for `x` to the glyph vector [?< ?>].
        let table = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
        let glyphs = Value::vector(vec![Value::fixnum('<' as i64), Value::fixnum('>' as i64)]);
        neovm_core::emacs_core::chartable::ct_set_single(&table, 'x' as i64, glyphs);
        buffer.set_buffer_local("buffer-display-table", table);
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    // (1) The RENDERED text area shows the mapped glyphs in place of `x`.
    let rendered = backend_trace_text_area_text(&trace);
    assert!(
        rendered.contains("a<>b"),
        "display-table char must render its glyph vector inline, got {rendered:?}"
    );
    assert!(
        !rendered.contains('x'),
        "the source char must not also render literally, got {rendered:?}"
    );

    // (2) THE regression assertion the unit tests missed: the row is NOT blank.
    //     A real text row that displays_text and has Char glyphs in glyphs[Text].
    let text_row = trace
        .matrix_rows
        .iter()
        .find(|r| r.role == GlyphRowRole::Text && r.displays_text)
        .expect("a non-blank text row must exist for the display-table line");
    let text_glyphs = &text_row.glyph_areas[GlyphArea::Text.index()];
    assert!(
        text_glyphs
            .iter()
            .any(|g| matches!(g.kind, GlyphKindTrace::Char('<'))),
        "the mapped '<' glyph must be present in the rendered text area, glyphs={text_glyphs:?}"
    );
    assert!(
        text_glyphs
            .iter()
            .any(|g| matches!(g.kind, GlyphKindTrace::Char('>'))),
        "the mapped '>' glyph must be present in the rendered text area, glyphs={text_glyphs:?}"
    );
    // Both mapped glyphs carry the SAME (single) source charpos: `x` is at
    // index 1 in "axb".  This is the GNU `it->position`-frozen invariant.
    let mapped: Vec<usize> = text_glyphs
        .iter()
        .filter(|g| {
            matches!(
                g.kind,
                GlyphKindTrace::Char('<') | GlyphKindTrace::Char('>')
            )
        })
        .map(|g| g.charpos)
        .collect();
    assert_eq!(
        mapped,
        vec![1, 1],
        "both mapped glyphs must share the single source charpos of `x`"
    );
}

#[test]
fn layout_frame_rust_display_table_maps_tab_to_glyph_then_tab() {
    // whitespace-mode pattern: `buffer-display-table` maps TAB to `[?> ?\t]` so
    // a leading indentation tab shows a `>` marker followed by tab spacing.  The
    // tab element inside the vector must re-expand to the tab stop (it flows
    // through the ordinary tab path), and the leading-tab-on-every-line layout
    // (the exact case that blanked the buffer in the rejected attempt) must
    // still render the following text.
    let text = "\tabc\n\tdef\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _id: BufferId, _t: &str| {
        let table = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
        let glyphs = Value::vector(vec![Value::fixnum('>' as i64), Value::fixnum('\t' as i64)]);
        neovm_core::emacs_core::chartable::ct_set_single(&table, '\t' as i64, glyphs);
        buffer.set_buffer_local("buffer-display-table", table);
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    let rendered = backend_trace_text_area_text(&trace);
    // The leading tab on EVERY line renders its `>` marker, and the text that
    // follows survives (the blank-buffer regression dropped all of it).
    assert!(
        rendered.contains(">abc"),
        "tab-mapped `>` marker must precede the line text, got {rendered:?}"
    );
    assert!(
        rendered.contains(">def"),
        "second leading-tab line must also render, got {rendered:?}"
    );

    // Both lines produced non-blank text rows (rejected attempt: tildes only).
    let text_rows: Vec<_> = trace
        .matrix_rows
        .iter()
        .filter(|r| r.role == GlyphRowRole::Text && r.displays_text)
        .collect();
    assert!(
        text_rows.len() >= 2,
        "both display-table lines must produce non-blank text rows, got {}",
        text_rows.len()
    );
    // The tab element re-expanded: the first row has a Stretch glyph (tab stop)
    // after the `>` marker.
    let first = text_rows[0];
    let glyphs = &first.glyph_areas[GlyphArea::Text.index()];
    assert!(
        glyphs
            .iter()
            .any(|g| matches!(g.kind, GlyphKindTrace::Stretch(_))),
        "the mapped tab element must re-expand to a tab-stop stretch, glyphs={glyphs:?}"
    );
}

#[test]
fn layout_frame_rust_without_display_table_renders_text_unchanged() {
    // Sibling guard: a NORMAL buffer with NO display table renders its text
    // verbatim.  This is the hot path that must be untouched by the
    // display-table hook (a single cheap check when no table is present).
    let text = "axb\n";
    let trace = layout_trace_for_plain_text(text);

    let rendered = backend_trace_text_area_text(&trace);
    assert!(
        rendered.contains("axb"),
        "a buffer without a display table must render its text verbatim, got {rendered:?}"
    );
    assert!(
        !rendered.contains('<') && !rendered.contains('>'),
        "no display-table glyphs must appear without a display table, got {rendered:?}"
    );
}

#[test]
fn layout_frame_rust_display_table_empty_vector_displays_nothing() {
    // GNU `get_next_display_element`: an EMPTY display vector means the char is
    // displayed as nothing.  The char is consumed (no literal glyph) but the
    // surrounding text and row survive.
    let text = "axb\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _id: BufferId, _t: &str| {
        let table = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
        neovm_core::emacs_core::chartable::ct_set_single(&table, 'x' as i64, Value::vector(vec![]));
        buffer.set_buffer_local("buffer-display-table", table);
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    let rendered = backend_trace_text_area_text(&trace);
    assert!(
        rendered.contains("ab") && !rendered.contains('x'),
        "an empty display vector must drop the char, keeping neighbors, got {rendered:?}"
    );
    assert!(
        trace
            .matrix_rows
            .iter()
            .any(|r| r.role == GlyphRowRole::Text && r.displays_text),
        "the line must still produce a non-blank text row"
    );
}

#[test]
fn layout_frame_rust_display_table_decodes_cons_glyph_code() {
    // A glyph code can be a `(char . face-id)` cons (GNU `make-glyph-code` when
    // the face id needs more than 6 bits) as well as a packed fixnum.  Both
    // decode to their character via GLYPH_CODE_CHAR.
    let text = "axb\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _id: BufferId, _t: &str| {
        let table = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
        // [ (?< . 285)  (?> | (12 << 22)) ] : a cons-form and a face-packed
        // fixnum-form glyph code; both must decode to '<' and '>'.
        let packed = ('>' as i64) | (12i64 << 22);
        let glyphs = Value::vector(vec![
            Value::cons(Value::fixnum('<' as i64), Value::fixnum(285)),
            Value::fixnum(packed),
        ]);
        neovm_core::emacs_core::chartable::ct_set_single(&table, 'x' as i64, glyphs);
        buffer.set_buffer_local("buffer-display-table", table);
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    let rendered = backend_trace_text_area_text(&trace);
    assert!(
        rendered.contains("a<>b"),
        "both cons-form and face-packed glyph codes must decode to their char, got {rendered:?}"
    );
}

#[test]
fn layout_frame_rust_lays_out_nobreak_chars() {
    // U+00A0 NBSP and U+00AD SHY are delivered as plain Text by the typed cursor;
    // the nobreak display policy is applied downstream by the walk.
    let text = "a\u{00A0}b\u{00AD}c\nd\u{00A0}\u{00A0}e\n";
    let trace = layout_trace_for_plain_text(text);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_lays_out_selective_display() {
    // selective-display>0 + embedded '\r': the typed cursor emits '\r' as a
    // ControlChar (a translated shim arm); the selective-display tail-skip is
    // walk-state run on the consumed char.
    let text = "visible\rhidden\nnext\rgone\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, _text: &str| {
        buffer.set_buffer_local("selective-display", Value::fixnum(1));
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_lays_out_hscroll() {
    // Window hscroll>0 on a truncated long line: the walk skips leading columns
    // BEFORE consuming the source (render_hscroll_skip), so sourcing is bypassed
    // for the skipped span.
    let text = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ\n";
    let buf_setup = |buffer: &mut neovm_core::buffer::Buffer, _buf_id: BufferId, _text: &str| {
        buffer.set_buffer_local("truncate-lines", Value::T);
    };
    let win_setup = |window: &mut neovm_core::window::Window| {
        if let neovm_core::window::Window::Leaf { hscroll, .. } = window {
            *hscroll = 10;
        }
    };
    let trace = layout_trace_with_buffer_and_window_setup(text, 200, 120, buf_setup, win_setup);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_lays_out_complex_text() {
    // Arabic (contextual joining), Hebrew (RTL bidi), and an emoji ZWJ family
    // (composition): the typed cursor folds these into TextRuns that the append
    // layer re-shapes/clusters/reorders downstream — the source carries only the
    // chars + faces, not shaping decisions.
    let text = "العربية\nאבגד\n👨\u{200d}👩\u{200d}👧\nmixed العربية text\n";
    let trace = layout_trace_for_plain_text(text);

    assert!(!trace.matrix_rows.is_empty());
}

#[test]
fn layout_frame_rust_vscroll_shifts_body_rows_up_and_exposes_extra_row() {
    // GNU `w->vscroll` (task #64): an ordinary GUI window's contents are scrolled
    // UP by the vscroll pixels.  Byte-exact RowTrace pixel_y snapshot — baseline
    // (vscroll 0) vs a sub-line vscroll (the buffer over-fills the window so the
    // bottom exposes a NEW row):
    //   * every body row's pixel_y drops by EXACTLY `vscroll`,
    //   * the first row moves ABOVE the (unchanged) text-area top (top-clipped),
    //   * one extra partially-visible row is exposed at the bottom, continuing
    //     the row grid,
    //   * the cursor's y follows the (fully visible) row it sits on.
    // `w->vscroll` is stored negative; window_start is pinned at line 1.
    let text = "vscroll body line\n".repeat(200);
    const VSCROLL_PX: i64 = 1;

    let layout = |vscroll: i32| -> (Vec<f32>, Option<i64>) {
        // A GUI (window-system) frame: the vscroll content-up shift is graphical
        // (a TTY frame keeps the historical shrink -- see the geometry unit test).
        let mut eval = Context::new();
        convert_current_buffer_text_backend(&mut eval, BufferTextBackendKind::GapBuffer);
        let buf_id = eval
            .buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id();
        insert_fragmented_current_buffer_text(&mut eval, &text);
        {
            // Point on the second line: the cursor lands on a fully-visible row,
            // not the top-clipped first row.
            let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
            buffer.goto_emacs_byte_pos(EmacsBytePos::new(20));
        }
        let frame_id =
            eval.frame_manager_mut()
                .create_frame("layout-vscroll-body-shift", 300, 200, buf_id);
        if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
            frame.set_window_system(Some(Value::symbol("neo")));
        }
        let selected_window = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window;
        {
            let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
            let window = frame
                .find_window_mut(selected_window)
                .expect("selected window");
            if let neovm_core::window::Window::Leaf {
                window_start,
                vscroll: vs,
                ..
            } = window
            {
                *window_start = LispCharPos1::ONE;
                *vs = vscroll;
            }
        }
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame_id);
        let trace = selected_window_layout_trace(&eval, &engine, frame_id);
        let body_ys = trace
            .matrix_rows
            .iter()
            .filter(|row| row.role == GlyphRowRole::Text && row.displays_text)
            .map(|row| f32::from_bits(row.pixel_y_bits))
            .collect();
        let cursor_y = trace.phys_cursor.as_ref().map(|cursor| cursor.y);
        (body_ys, cursor_y)
    };

    let (baseline, base_cursor) = layout(0);
    let (scrolled, scrolled_cursor) = layout(-(VSCROLL_PX as i32));
    let vscroll_px = VSCROLL_PX as f32;

    assert!(
        baseline.len() >= 3,
        "need several visible body rows: {baseline:?}"
    );
    let char_height = baseline[1] - baseline[0];
    assert!(
        char_height > vscroll_px,
        "sub-line vscroll requires char_height ({char_height}) > vscroll ({vscroll_px})"
    );

    // Every overlapping row shifts up by EXACTLY vscroll (byte-exact f32).
    for (k, &base_y) in baseline.iter().enumerate() {
        assert_eq!(
            scrolled[k],
            base_y - vscroll_px,
            "row {k}: vscroll'd pixel_y must be baseline ({base_y}) - vscroll ({vscroll_px})"
        );
    }
    // The first row is lifted above the (unchanged) text-area top.
    assert!(
        scrolled[0] < baseline[0],
        "first row must move above the text-area top: {} !< {}",
        scrolled[0],
        baseline[0]
    );
    // Exactly one extra row is exposed at the bottom, continuing the grid.
    assert_eq!(
        scrolled.len(),
        baseline.len() + 1,
        "a {vscroll_px}px vscroll (< one {char_height}px row) exposes one extra bottom row"
    );
    assert_eq!(
        *scrolled.last().unwrap(),
        scrolled[scrolled.len() - 2] + char_height,
        "the exposed bottom row continues the uniform row grid"
    );

    // The cursor follows the row it sits on: same downward shift.
    let base_cursor = base_cursor.expect("baseline cursor should be visible");
    let scrolled_cursor = scrolled_cursor.expect("scrolled cursor should be visible");
    assert_eq!(
        scrolled_cursor,
        base_cursor - VSCROLL_PX,
        "cursor y must follow its row's downward shift"
    );
}

fn backend_layout_trace(kind: BufferTextBackendKind) -> BackendLayoutTrace {
    let text = "abé\tz\n日本x\nlast Ω line\n";
    backend_layout_trace_with_buffer_setup(
        kind,
        "layout-backend-parity",
        text,
        360,
        180,
        |buffer, _buf_id, text| {
            let omega_byte = text.find('Ω').expect("omega");
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(omega_byte));
            buffer.set_buffer_local("display-line-numbers", Value::T);
        },
    )
}

#[test]
fn layout_frame_rust_line_number_cursor_tracks_first_text_column_after_c_n() {
    let trace = backend_layout_trace_with_buffer_and_window_setup(
        BufferTextBackendKind::GapBuffer,
        "layout-line-number-cursor-first-text-column",
        "abc\ndef\n",
        360,
        140,
        |buffer, _buf_id, _text| {
            buffer.set_buffer_local("display-line-numbers", Value::T);
            buffer.goto_emacs_byte_pos(EmacsBytePos::new(4));
        },
        |window| {
            if let neovm_core::window::Window::Leaf { window_start, .. } = window {
                *window_start = LispCharPos1::ONE;
            }
        },
    );

    let cursor = trace.phys_cursor.as_ref().expect("phys cursor");
    let point = trace
        .points
        .iter()
        .find(|point| point.buffer_pos == LispCharPos1::from_one_based_usize(5))
        .expect("display point for first character on second line");

    assert_eq!(cursor.row, point.row);
    assert_eq!(cursor.col, point.col);
    assert_eq!(cursor.x, point.x);
}

/// An anonymous `(:background ... :extend t)` face value, the shape hl-line /
/// region use to highlight a whole line out to the window edge.
fn extend_face_value() -> Value {
    Value::list(vec![
        Value::keyword("background"),
        Value::string("#003366"),
        Value::keyword("extend"),
        Value::T,
    ])
}

/// Lay out a 360x180 frame over `text` with point at `point_byte`, an `:extend`
/// face on `extend_range`, and `display-line-numbers` optionally enabled.
/// Returns the frame's authoritative phys cursor (the geometry the GUI draws)
/// and the frame's pixel width.
fn empty_line_extend_cursor(
    text: &str,
    extend_range: (usize, usize),
    point_byte: usize,
    line_numbers: bool,
) -> (neomacs_display_protocol::frame_glyphs::PhysCursor, f32) {
    let mut eval = Context::new();
    convert_current_buffer_text_backend(&mut eval, BufferTextBackendKind::GapBuffer);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        insert_fragmented_current_buffer_text(&mut eval, text);
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        if line_numbers {
            buffer.set_buffer_local("display-line-numbers", Value::T);
        }
        assert!(buffer.put_text_property(
            extend_range.0,
            extend_range.1,
            Value::symbol("face"),
            extend_face_value()
        ));
        buffer.goto_emacs_byte_pos(EmacsBytePos::new(point_byte));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("empty-line-extend-cursor", 360, 180, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
    }
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let phys = state
        .phys_cursor
        .clone()
        .expect("frame phys cursor on the empty :extend line");
    (phys, state.frame_pixel_width)
}

/// Regression: with `display-line-numbers` + an `:extend` (hl-line-style) face,
/// the cursor on an EMPTY line must sit at column 0 of the text area (right
/// after the line-number gutter), NOT at the far-right window edge.
///
/// `extend_face_to_end_of_line` fills the highlighted background from EOL to the
/// window edge by appending a face-anchor space + a wide stretch glyph. Those
/// synthetic glyphs carry no buffer position; before the fix
/// `CursorVisualColumnResolutionRequest::resolve` counted them into the visual
/// column, shoving the blank-line cursor to the fill's right edge.
#[test]
fn empty_line_extend_cursor_sits_at_text_start_not_window_edge() {
    // Empty middle line of "abc\n\ndef\n", point on it (byte 4), line numbers on.
    let (cursor, frame_width) = empty_line_extend_cursor("abc\n\ndef\n", (4, 5), 4, true);
    // Reference: a real char at the start of a NON-empty line gets the first
    // text column (the gutter width). The empty-line cursor must match it, not
    // the :extend fill's far-right edge.
    let (non_empty_cursor, _) = empty_line_extend_cursor("abc\n\ndef\n", (0, 1), 0, true);
    assert_eq!(
        cursor.col, non_empty_cursor.col,
        "empty-line cursor column must equal the first text column (column 0 of \
         the text area), not the :extend fill's right edge; got {cursor:?}"
    );
    // The drawn cursor must be far from the window's right edge (the bug placed
    // it at/past `frame_width`).
    assert!(
        cursor.x <= non_empty_cursor.x + 1.0 && cursor.x < frame_width / 2.0,
        "empty-line cursor x must be at the text-area start (~{}), not near the \
         window right edge ({frame_width}); got x={}",
        non_empty_cursor.x,
        cursor.x
    );

    // Without line numbers the text area starts at x=0, so an empty line's
    // cursor must be at column 0 / x=0 exactly.
    let (no_ln_cursor, _) = empty_line_extend_cursor("abc\n\ndef\n", (4, 5), 4, false);
    assert_eq!(
        no_ln_cursor.col, 0,
        "empty-line cursor without line numbers must be at column 0; got {no_ln_cursor:?}"
    );
    assert_eq!(
        no_ln_cursor.x, 0.0,
        "empty-line cursor without line numbers must be at x=0; got {no_ln_cursor:?}"
    );

    // The fill's synthetic glyphs carry no buffer position, so they must not
    // displace the cursor at end-of-line on a NON-empty first line whose real
    // first char carries 0-based charpos 0: the cursor sits AFTER that char.
    let (single_char_cursor, _) = empty_line_extend_cursor("a\nbc\n", (0, 2), 1, false);
    assert_eq!(
        single_char_cursor.col, 1,
        "EOL cursor on a single-char first line must sit after the char (col 1), \
         not be pulled back over the trimmed fill; got {single_char_cursor:?}"
    );
}

#[test]
fn line_break_extend_fill_reaches_tty_reserved_right_column() {
    let mut eval = Context::new();
    convert_current_buffer_text_backend(&mut eval, BufferTextBackendKind::GapBuffer);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        insert_fragmented_current_buffer_text(&mut eval, "x\n");
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        assert!(buffer.put_text_property(0, 1, Value::symbol("face"), extend_face_value()));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("extend-fill-right-column", 360, 180, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("highlighted text row");
    let text_glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert!(
        matches!(
            text_glyphs.first().map(|glyph| &glyph.glyph_type),
            Some(GlyphType::Char { ch: 'x' })
        ),
        "first text glyph should be the source character, got {text_glyphs:?}"
    );
    let stretch_cols = text_glyphs
        .iter()
        .rev()
        .find_map(|glyph| match glyph.glyph_type {
            GlyphType::Stretch { width_cols } => Some(width_cols),
            _ => None,
        })
        .expect("line-break :extend fill stretch");
    assert_eq!(
        stretch_cols,
        u16::try_from(entry.matrix.ncols - 1).expect("matrix width fits u16"),
        "the :extend fill must cover through the full text area, including the \
         TTY-reserved right column"
    );
}

#[test]
fn layout_frame_rust_line_number_width_matches_gnu_visible_row_width() {
    let trace = backend_layout_trace_with_buffer_and_window_setup(
        BufferTextBackendKind::GapBuffer,
        "layout-line-number-width-visible-rows",
        "abc\ndef\n",
        360,
        430,
        |buffer, _buf_id, _text| {
            buffer.set_buffer_local("display-line-numbers", Value::T);
        },
        |window| {
            if let neovm_core::window::Window::Leaf { window_start, .. } = window {
                *window_start = LispCharPos1::ONE;
            }
        },
    );

    let first_text_row = trace
        .matrix_rows
        .iter()
        .find(|row| row.role == GlyphRowRole::Text && row.displays_text)
        .expect("first text row");
    let left_margin = &first_text_row.glyph_areas[GlyphArea::LeftMargin.index()];

    assert_eq!(
        left_margin
            .iter()
            .map(|glyph| glyph.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            GlyphKindTrace::Stretch(2),
            GlyphKindTrace::Char('1'),
            GlyphKindTrace::Stretch(1),
        ]
    );
}

fn display_replacement_backend_layout_trace(kind: BufferTextBackendKind) -> BackendLayoutTrace {
    let text = "abcXYZdef\n";
    backend_layout_trace_with_buffer_setup(
        kind,
        "layout-backend-display-replacement",
        text,
        360,
        140,
        |buffer, _buf_id, text| {
            let start = text.find("XYZ").expect("replacement start");
            let end = start + "XYZ".len();
            assert!(buffer.put_text_property(
                start,
                end,
                Value::symbol("display"),
                Value::string("R")
            ));
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(start + 1));
        },
    )
}

fn invisible_backend_layout_trace(kind: BufferTextBackendKind) -> BackendLayoutTrace {
    let text = "abc hidden xyz\n";
    backend_layout_trace_with_buffer_setup(
        kind,
        "layout-backend-invisible",
        text,
        360,
        140,
        |buffer, _buf_id, text| {
            let start = text.find("hidden").expect("hidden start");
            let end = start + "hidden".len();
            assert!(buffer.put_text_property(start, end, Value::symbol("invisible"), Value::T));
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(start + 2));
        },
    )
}

fn multiline_overlay_backend_layout_trace(kind: BufferTextBackendKind) -> BackendLayoutTrace {
    let text = "x";
    backend_layout_trace_with_buffer_setup(
        kind,
        "layout-backend-overlay",
        text,
        360,
        140,
        |buffer, buf_id, _text| {
            let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
                serial: 0,
                plist: Value::NIL,
                buffer: Some(buf_id),
                start: 0,
                end: 1,
                front_advance: false,
                rear_advance: false,
            });
            buffer.overlays_mut().insert_overlay(overlay);
            let _ = buffer.overlays_mut().overlay_put(
                overlay,
                Value::symbol("after-string"),
                Value::string("A\nB"),
            );
            buffer.goto_emacs_byte_pos(buffer.point_max_emacs_byte_pos());
        },
    )
}

#[test]
fn layout_frame_rust_renders_overlay_display_property() {
    // GNU get_char_property: an overlay `display` overrides the text property —
    // e.g. org-display-inline-images overlays the link with an `(image …)`.
    // Reading only the text property left those as raw text. Here an overlay
    // covering "HIDE" with display "SHOWN" must render "SHOWN", not "HIDE".
    let text = "AA HIDE BB\n";
    let setup = |buffer: &mut neovm_core::buffer::Buffer, buf_id: BufferId, text: &str| {
        let start = text.find("HIDE").expect("HIDE");
        let end = start + "HIDE".len();
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start,
            end,
            front_advance: false,
            rear_advance: false,
        });
        buffer.overlays_mut().insert_overlay(overlay);
        let _ = buffer.overlays_mut().overlay_put(
            overlay,
            Value::symbol("display"),
            Value::string("SHOWN"),
        );
    };
    let trace = layout_trace_with_buffer_setup(text, 360, 180, setup);
    let rendered = backend_trace_text_area_text(&trace);

    assert!(
        rendered.contains("SHOWN"),
        "overlay display string must render, rendered={rendered:?}"
    );
    assert!(
        !rendered.contains("HIDE"),
        "the overlay-covered text must be replaced, rendered={rendered:?}"
    );
    assert!(
        rendered.contains("AA") && rendered.contains("BB"),
        "text around the overlay must still render, rendered={rendered:?}"
    );
}

fn bidi_backend_layout_trace(kind: BufferTextBackendKind) -> BackendLayoutTrace {
    let text = "abc אבג def\n";
    backend_layout_trace_with_buffer_setup(
        kind,
        "layout-backend-bidi",
        text,
        360,
        140,
        |buffer, _buf_id, text| {
            let alef_byte = text.find('א').expect("alef");
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(alef_byte));
        },
    )
}

fn selective_display_backend_layout_trace(kind: BufferTextBackendKind) -> BackendLayoutTrace {
    let text = "head\rhidden tail\nshown\n  hidden by indent\nshown2\n";
    backend_layout_trace_with_buffer_setup(
        kind,
        "layout-backend-selective-display",
        text,
        360,
        180,
        |buffer, _buf_id, _text| {
            buffer.set_buffer_local("selective-display", Value::fixnum(1));
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(2));
        },
    )
}

fn glyphless_backend_layout_trace(kind: BufferTextBackendKind) -> BackendLayoutTrace {
    let text = "a\u{0080}b\u{FEFF}c\u{FFFC}d\n";
    backend_layout_trace_with_buffer_setup(
        kind,
        "layout-backend-glyphless",
        text,
        360,
        140,
        |buffer, _buf_id, text| {
            let c1_byte = text.find('\u{0080}').expect("C1 control");
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(c1_byte));
        },
    )
}

fn composition_backend_layout_trace(kind: BufferTextBackendKind) -> BackendLayoutTrace {
    let text = "e\u{0301} a\u{0300}\u{0301} 中\u{0300}\nplain\n";
    backend_layout_trace_with_buffer_setup(
        kind,
        "layout-backend-composition",
        text,
        360,
        140,
        |buffer, _buf_id, text| {
            let cjk_byte = text.find('中').expect("CJK base char");
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(cjk_byte));
        },
    )
}

fn wrapped_retry_backend_layout_trace(kind: BufferTextBackendKind) -> (BackendLayoutTrace, usize) {
    let logical_lines = (0..24)
        .map(|line| format!("line-{line:02} abcdefghijklmno\n"))
        .collect::<Vec<_>>();
    let text = logical_lines.join("");
    let target_pos = logical_lines
        .iter()
        .take(18)
        .map(|line| line.chars().count())
        .sum::<usize>()
        + 1;

    let trace = backend_layout_trace_with_buffer_and_window_setup(
        kind,
        "layout-backend-wrap-retry",
        &text,
        80,
        192,
        |buffer, _buf_id, _text| {
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(target_pos - 1));
            buffer.set_buffer_local("word-wrap", Value::T);
        },
        |window| {
            if let neovm_core::window::Window::Leaf { point, .. } = window {
                *point = LispCharPos1::from_one_based_usize(target_pos);
            }
        },
    );
    (trace, target_pos)
}

fn point_line_tail_backend_layout_trace(
    kind: BufferTextBackendKind,
) -> (BackendLayoutTrace, usize, usize) {
    let prefix = (0..2)
        .map(|line| format!("p{line:02}\n"))
        .collect::<Vec<_>>()
        .join("");
    let target_line = "abcdefghijklmno\n";
    let text = format!("{prefix}{target_line}");
    let point = prefix.chars().count() + 1;
    let later_pos = point + 10;

    let trace = backend_layout_trace_with_buffer_and_window_setup(
        kind,
        "layout-backend-point-line-tail",
        &text,
        80,
        256,
        |buffer, _buf_id, _text| {
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point - 1));
            buffer.set_buffer_local("word-wrap", Value::T);
        },
        |window| {
            if let neovm_core::window::Window::Leaf {
                point: window_point,
                ..
            } = window
            {
                *window_point = LispCharPos1::from_one_based_usize(point);
            }
        },
    );
    (trace, point, later_pos)
}

fn mode_line_geometry_backend_layout_trace(
    kind: BufferTextBackendKind,
) -> (BackendLayoutTrace, usize) {
    let text = (0..80)
        .map(|line| format!("Line {line:02}\n"))
        .collect::<String>();
    let point = text.chars().count() + 1;

    let trace = backend_layout_trace_with_buffer_and_window_setup(
        kind,
        "layout-backend-mode-line-geometry",
        &text,
        640,
        96,
        |buffer, _buf_id, _text| {
            buffer.set_buffer_local("mode-line-format", Value::string("%o|%p|%P"));
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point - 1));
        },
        |window| {
            if let neovm_core::window::Window::Leaf {
                point: window_point,
                ..
            } = window
            {
                *window_point = LispCharPos1::from_one_based_usize(point);
            }
        },
    );
    (trace, point)
}

fn hscroll_cursor_backend_layout_trace(kind: BufferTextBackendKind) -> BackendLayoutTrace {
    backend_layout_trace_with_buffer_and_window_setup(
        kind,
        "layout-backend-hscroll-cursor",
        "abcdef\n",
        160,
        120,
        |buffer, _buf_id, _text| {
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
            buffer.set_buffer_local("truncate-lines", Value::T);
        },
        |window| {
            if let neovm_core::window::Window::Leaf { point, hscroll, .. } = window {
                *point = LispCharPos1::from_one_based_usize(2);
                *hscroll = 3;
            }
        },
    )
}

fn edit_redisplay_backend_layout_trace(
    kind: BufferTextBackendKind,
) -> (BackendLayoutTrace, BackendLayoutTrace) {
    let mut eval = Context::new();
    convert_current_buffer_text_backend(&mut eval, kind);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        insert_fragmented_current_buffer_text(&mut eval, "alpha beta gamma\n");
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        assert_eq!(buffer.text_backend_kind(), kind);
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-backend-edit-redisplay", 360, 140, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let before = selected_window_layout_trace(&eval, &engine, frame_id);

    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        let start = buffer.buffer_string().find("beta").expect("beta");
        let end = start + "beta".len();
        buffer.delete_emacs_byte_range(emacs_byte_range(start, end));
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(start));
        buffer.insert("BETA");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        assert_eq!(buffer.buffer_string(), "alpha BETA gamma\n");
    }

    engine.layout_frame_rust(&mut eval, frame_id);
    let after = selected_window_layout_trace(&eval, &engine, frame_id);
    (before, after)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FontificationBackendTrace {
    before_layout: BackendLayoutTrace,
    before_props: String,
    after_layout: BackendLayoutTrace,
    after_props: String,
}

fn printed_eval_result(eval: &mut Context, form: &str) -> String {
    eval.eval_str(form)
        .unwrap_or_else(|err| panic!("eval {form}: {err}"))
        .as_runtime_string_owned()
        .unwrap_or_else(|| panic!("eval {form} did not return a string"))
}

fn fontification_edit_backend_trace(kind: BufferTextBackendKind) -> FontificationBackendTrace {
    let mut eval = Context::new();
    convert_current_buffer_text_backend(&mut eval, kind);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        insert_fragmented_current_buffer_text(&mut eval, "alpha beta gamma\n");
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        assert_eq!(buffer.text_backend_kind(), kind);
    }

    eval.eval_str(
        r#"
        (setq neomacs-test-fontify-face 'font-lock-keyword-face)
        (setq redisplay-fontify-calls nil)
        (setq fontification-functions
              (list (lambda (start)
                      (setq redisplay-fontify-calls
                            (cons start redisplay-fontify-calls))
                      (let ((end (min (point-max) (+ start 80))))
                        (put-text-property start end 'fontified t)
                        (put-text-property start end 'font-lock-face
                                           neomacs-test-fontify-face)))))
        "#,
    )
    .unwrap_or_else(|err| panic!("install redisplay fontification hook: {err}"));

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-backend-fontification-edit",
        360,
        140,
        buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let before_layout = selected_window_layout_trace(&eval, &engine, frame_id);
    let before_props = printed_eval_result(
        &mut eval,
        "(prin1-to-string (list redisplay-fontify-calls (get-text-property 1 'fontified) (get-text-property 1 'font-lock-face)))",
    );

    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        let start = buffer.buffer_string().find("beta").expect("beta");
        let end = start + "beta".len();
        buffer.delete_emacs_byte_range(emacs_byte_range(start, end));
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(start));
        buffer.insert("BETA");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        assert_eq!(buffer.buffer_string(), "alpha BETA gamma\n");
    }

    eval.eval_str(
        r#"
        (setq neomacs-test-fontify-face 'font-lock-warning-face)
        (setq redisplay-fontify-calls nil)
        (remove-text-properties (point-min) (point-max)
                                '(fontified nil font-lock-face nil))
        "#,
    )
    .unwrap_or_else(|err| panic!("clear fontification state after edit: {err}"));

    engine.layout_frame_rust(&mut eval, frame_id);
    let after_layout = selected_window_layout_trace(&eval, &engine, frame_id);
    let after_props = printed_eval_result(
        &mut eval,
        "(prin1-to-string (list redisplay-fontify-calls (get-text-property 1 'fontified) (get-text-property 1 'font-lock-face)))",
    );

    FontificationBackendTrace {
        before_layout,
        before_props,
        after_layout,
        after_props,
    }
}

fn glyph_trace_text(glyph: &GlyphTrace) -> String {
    match &glyph.kind {
        GlyphKindTrace::Char(ch) => ch.to_string(),
        GlyphKindTrace::Composite(text) => text.clone(),
        GlyphKindTrace::Stretch(width) => " ".repeat(usize::from(*width)),
        GlyphKindTrace::Image(_) | GlyphKindTrace::Surface(_) | GlyphKindTrace::Glyphless(_) => {
            String::new()
        }
    }
}

fn trace_rows_for_role(trace: &BackendLayoutTrace, role: GlyphRowRole) -> Vec<String> {
    trace
        .matrix_rows
        .iter()
        .filter(|row| row.role == role)
        .map(|row| {
            row.glyph_areas[1]
                .iter()
                .map(glyph_trace_text)
                .collect::<Vec<_>>()
                .join("")
        })
        .collect()
}

fn trace_text_rows(trace: &BackendLayoutTrace) -> Vec<String> {
    trace_rows_for_role(trace, GlyphRowRole::Text)
}

fn trace_mode_line_text(trace: &BackendLayoutTrace) -> String {
    trace_rows_for_role(trace, GlyphRowRole::ModeLine).join("")
}

fn trace_text_faces(trace: &BackendLayoutTrace) -> Vec<String> {
    trace
        .matrix_rows
        .iter()
        .filter(|row| row.role == GlyphRowRole::Text)
        .flat_map(|row| row.glyph_areas[1].iter().map(|glyph| glyph.face.clone()))
        .collect()
}

fn trace_composite_texts(trace: &BackendLayoutTrace) -> Vec<String> {
    trace
        .matrix_rows
        .iter()
        .filter(|row| row.role == GlyphRowRole::Text)
        .flat_map(|row| row.glyph_areas[1].iter())
        .filter_map(|glyph| match &glyph.kind {
            GlyphKindTrace::Composite(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn trace_has_nonzero_bidi_level(trace: &BackendLayoutTrace) -> bool {
    trace.matrix_rows.iter().any(|row| {
        row.glyph_areas
            .iter()
            .flat_map(|area| area.iter())
            .any(|glyph| glyph.bidi_level > 0)
    })
}

fn assert_echo_message_renders_in_minibuffer_window(use_gui_metrics: bool) {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-minibuffer-echo", 640, 160, buf_id);
    let echo = "Echo lives in minibuffer";
    eval.set_current_message(Some(LispString::from_utf8(echo)));

    let mut engine = LayoutEngine::new();
    if use_gui_metrics {
        engine.enable_cosmic_metrics();
    }
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let minibuffer_window_id = state
        .window_infos
        .iter()
        .find(|info| info.is_minibuffer)
        .expect("minibuffer window info")
        .window_id
        .get() as u64;
    let root_window_id = state
        .window_infos
        .iter()
        .find(|info| !info.is_minibuffer)
        .expect("root window info")
        .window_id
        .get() as u64;

    let minibuffer_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == minibuffer_window_id as i64)
        .expect("minibuffer matrix");
    let root_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == root_window_id as i64)
        .expect("root matrix");

    let minibuffer_text = window_matrix_text(minibuffer_entry);
    let root_text = window_matrix_text(root_entry);

    assert!(
        minibuffer_text.contains(echo),
        "expected echo text in minibuffer matrix, got {minibuffer_text:?}"
    );
    assert!(
        !root_text.contains(echo),
        "echo text leaked into root window matrix: {root_text:?}"
    );
    // Post slice-8 the echo area is rendered through the ordinary buffer-text
    // walk over ` *Echo Area 0*` (GNU `display_echo_area_1`), so its rows are
    // plain buffer-text rows — the same role the *active* minibuffer walk
    // already produces — not a special Minibuffer-tagged row.
    assert!(
        minibuffer_entry
            .matrix
            .rows
            .iter()
            .any(|row| row.enabled && row.role == GlyphRowRole::Text && !row.mode_line),
        "expected a non-chrome buffer-text row for echo text"
    );
    assert!(
        !root_text.contains(echo),
        "echo text must not leak into the root window matrix"
    );
}

#[test]
fn layout_frame_rust_preserves_propertized_echo_message_faces() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-propertized-echo", 320, 120, buf_id);
    let echo = Value::string_with_text_properties(
        "A中👨‍👩",
        vec![StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]),
            ]),
        }],
    );
    eval.set_current_message(echo.as_lisp_string().cloned());

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    // Post slice-8 the echo area renders through the ordinary buffer-text walk
    // over ` *Echo Area 0*`, so locate the mini-window by identity and take its
    // non-chrome buffer-text row.
    let minibuffer_window_id = state
        .window_infos
        .iter()
        .find(|info| info.is_minibuffer)
        .expect("minibuffer window info")
        .window_id
        .get() as u64;
    let minibuffer_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == minibuffer_window_id as i64)
        .expect("minibuffer echo matrix");
    let echo_glyphs = minibuffer_entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text && !row.mode_line)
        .expect("echo row")
        .glyphs[1]
        .clone();

    assert_eq!(glyphs_logical_text(&echo_glyphs), "A中👨‍👩");
    assert_ne!(
        echo_glyphs[0].face_id, echo_glyphs[1].face_id,
        "propertized echo character should receive its property face"
    );
    assert!(
        echo_glyphs[1].wide,
        "echo CJK glyph should use the shared wide-glyph builder: {echo_glyphs:?}"
    );
    assert!(
        echo_glyphs.iter().any(|glyph| glyph.padding),
        "echo CJK glyph should retain its padding cell: {echo_glyphs:?}"
    );
    assert!(
        echo_glyphs.iter().any(
            |glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.as_ref() == "👨‍👩")
        ),
        "echo ZWJ emoji should be clustered by the shared builder: {echo_glyphs:?}"
    );
    assert!(
        echo_glyphs
            .iter()
            .filter(|glyph| !glyph.padding)
            .all(|glyph| glyph.pixel_width > 0.0),
        "echo glyphs should carry real pixel widths: {echo_glyphs:?}"
    );
}

#[test]
fn inactive_echo_area_grows_to_contain_tall_display_image() {
    let mut eval = Context::new();
    eval.obarray_mut()
        .set_symbol_value("resize-mini-windows", Value::symbol("grow-only"));
    eval.obarray_mut()
        .set_symbol_value("max-mini-window-height", Value::fixnum(10));
    let requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::clone(&requests),
        video_requests: Arc::new(Mutex::new(Vec::new())),
        webkit_requests: Arc::new(Mutex::new(Vec::new())),
        surface_requests: Arc::new(Mutex::new(Vec::new())),
    }));
    let root = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("tall-echo-image", 320, 120, root);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.char_width = 8.0;
        frame.char_height = 18.0;
        frame.shrink_mini_window();
    }
    let image_spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword("type"),
        Value::symbol("png"),
        Value::keyword("file"),
        Value::string("./tmp/tall-echo-image.png"),
        Value::keyword("max-width"),
        Value::fixnum(32),
        Value::keyword("max-height"),
        Value::fixnum(24),
        Value::keyword("ascent"),
        Value::fixnum(60),
    ]);
    let echo = Value::string_with_text_properties(
        "I",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![Value::symbol("display"), image_spec]),
        }],
    );
    eval.set_current_message(echo.as_lisp_string().cloned());

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let materialized = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state")
        .materialize();
    let image = materialized
        .glyphs
        .iter()
        .find(|glyph| {
            matches!(
                glyph,
                FrameGlyph::Image {
                    row_role: GlyphRowRole::Text,
                    image_id,
                    ..
                } if image_id.get() == 77
            )
        })
        .expect("echo image glyph");
    let geometry = image.geometry().expect("image geometry");
    let clip = image.clip_rect().expect("echo-area clip");

    assert!(
        geometry.y >= clip.y && geometry.y + geometry.height <= clip.y + clip.height,
        "GNU sizes the mini-window from displayed pixel ascent/descent; the echo image must fit its clip: image={geometry:?} clip={clip:?}",
    );
}

fn assert_multiline_echo_message_resizes_minibuffer_rows(use_gui_metrics: bool) {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-minibuffer-echo-lines", 640, 160, buf_id);
    eval.set_current_message(Some(LispString::from_utf8("ALPHA\nBETA")));

    let mut engine = LayoutEngine::new();
    if use_gui_metrics {
        engine.enable_cosmic_metrics();
    }
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let minibuffer_window_id = state
        .window_infos
        .iter()
        .find(|info| info.is_minibuffer)
        .expect("minibuffer window info")
        .window_id
        .get() as u64;
    let minibuffer_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == minibuffer_window_id as i64)
        .expect("minibuffer matrix");
    let row_texts = enabled_window_row_texts(minibuffer_entry);

    assert!(
        row_texts.iter().any(|row| row == "ALPHA"),
        "expected ALPHA in its own minibuffer row, got {row_texts:?}"
    );
    assert!(
        row_texts.iter().any(|row| row == "BETA"),
        "expected BETA in its own minibuffer row, got {row_texts:?}"
    );
    assert!(
        !row_texts.iter().any(|row| row.contains("ALPHABETA")),
        "multiline echo text was flattened into one row: {row_texts:?}"
    );
}

#[test]
fn layout_frame_rust_publishes_increasing_display_positions() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abcd\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-test", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let a = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("a");
    let b = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(2))
        .expect("b");
    let c = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("c");
    assert!(
        a.x < b.x,
        "expected increasing x positions, got {a:?} then {b:?}"
    );
    assert!(
        b.x < c.x,
        "expected increasing x positions, got {b:?} then {c:?}"
    );
}

#[test]
fn layout_frame_rust_tracks_multibyte_sample_positions() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a好好b\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-test", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let all_points = snapshot.points.clone();
    let a = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("a");
    let hao1 = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(2))
        .expect("hao1");
    let hao2 = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("hao2");
    let b = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(4))
        .expect("b");
    assert!(
        a.x < hao1.x,
        "expected a before first 好, got {a:?} then {hao1:?}; points={all_points:?}"
    );
    assert!(
        hao1.x < hao2.x,
        "expected first 好 before second 好, got {hao1:?} then {hao2:?}; points={all_points:?}"
    );
    assert!(
        hao2.x < b.x,
        "expected second 好 before b, got {hao2:?} then {b:?}; points={all_points:?}"
    );
    assert!(
        a.width > 0,
        "expected positive width for a, got {a:?}; points={all_points:?}"
    );
    assert!(
        hao1.width > 0,
        "expected positive width for first 好, got {hao1:?}; points={all_points:?}"
    );
    assert!(
        hao2.width > 0,
        "expected positive width for second 好, got {hao2:?}; points={all_points:?}"
    );
    assert!(
        b.width > 0,
        "expected positive width for b, got {b:?}; points={all_points:?}"
    );
}

#[test]
fn implemented_text_backends_match_layout_frame_rows_points_and_cursor() {
    let baseline = backend_layout_trace(BufferTextBackendKind::GapBuffer);
    assert!(
        baseline
            .matrix_rows
            .iter()
            .any(|row| row.role == GlyphRowRole::Text
                && row.glyph_areas[1]
                    .iter()
                    .any(|glyph| glyph.kind == GlyphKindTrace::Char('Ω'))),
        "baseline should render omega row, got {baseline:?}"
    );
    assert!(
        baseline
            .matrix_rows
            .iter()
            .any(|row| !row.glyph_areas[0].is_empty()),
        "baseline should exercise left-margin line-number glyphs, got {baseline:?}"
    );
    assert!(
        baseline.phys_cursor.is_some(),
        "baseline should publish physical cursor geometry"
    );

    for kind in implemented_text_backends() {
        let trace = backend_layout_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_layout_frame_display_replacement_output() {
    let baseline = display_replacement_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let rows = trace_text_rows(&baseline);
    assert!(
        rows.iter().any(|row| row.contains("abcRdef")),
        "baseline should render display replacement text, rows={rows:?}"
    );
    assert!(
        rows.iter().all(|row| !row.contains("XYZ")),
        "baseline should not render covered source text, rows={rows:?}"
    );
    assert!(
        baseline.phys_cursor.is_some(),
        "baseline should publish cursor geometry for replacement slot"
    );

    for kind in implemented_text_backends() {
        let trace = display_replacement_backend_layout_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_layout_frame_invisible_text_output() {
    let baseline = invisible_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let rows = trace_text_rows(&baseline);
    assert!(
        rows.iter().any(|row| row.contains("abc  xyz")),
        "baseline should omit invisible source text while preserving surrounding text, rows={rows:?}"
    );
    assert!(
        rows.iter().all(|row| !row.contains("hidden")),
        "baseline should not render invisible text, rows={rows:?}"
    );
    assert!(
        baseline.phys_cursor.is_some(),
        "baseline should keep a physical cursor when point is inside invisible text"
    );

    for kind in implemented_text_backends() {
        let trace = invisible_backend_layout_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn layout_frame_rust_renders_invisible_ellipsis_through_row_builder() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abc hidden xyz");
        buf.set_buffer_local(
            "buffer-invisibility-spec",
            Value::list(vec![Value::cons(Value::symbol("folded"), Value::T)]),
        );
        let start = "abc ".len();
        let end = start + "hidden".len();
        assert!(buf.put_text_property(
            start,
            end,
            Value::symbol("invisible"),
            Value::symbol("folded"),
        ));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-invisible-ellipsis", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");
    let logical_text = glyphs_logical_text(&text_row.glyphs[1]);

    assert_eq!(logical_text, "abc ... xyz");
    assert!(
        text_row.glyphs[1]
            .iter()
            .filter(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '.' }))
            .all(|glyph| (glyph.pixel_width - 8.0).abs() <= 0.01),
        "ellipsis dots should carry measured pixel widths, row={:?}",
        text_row.glyphs[1]
    );
}

#[test]
fn implemented_text_backends_match_layout_frame_multiline_overlay_output() {
    let baseline = multiline_overlay_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let rows = trace_text_rows(&baseline);
    assert!(
        rows.iter().any(|row| row.contains("xA")),
        "baseline should render overlay after-string suffix on the source row, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains('B')),
        "baseline should render multiline overlay continuation row, rows={rows:?}"
    );
    assert!(
        baseline.output_rows.iter().any(|row| row.row == 1),
        "baseline should publish a second output row for multiline overlay, rows={:?}",
        baseline.output_rows
    );

    for kind in implemented_text_backends() {
        let trace = multiline_overlay_backend_layout_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_layout_frame_bidi_row_output() {
    let baseline = bidi_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let rows = trace_text_rows(&baseline);
    assert!(
        rows.iter()
            .any(|row| row.contains('א') && row.contains('ג')),
        "baseline should render Hebrew text in bidi row, rows={rows:?}"
    );
    assert!(
        trace_has_nonzero_bidi_level(&baseline),
        "baseline should mark reordered bidi glyphs, trace={baseline:?}"
    );

    for kind in implemented_text_backends() {
        let trace = bidi_backend_layout_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn arabic_run_composes_into_one_glyph_in_layout() {
    // ا ل م (U+0627 U+0644 U+0645) — an Arabic run. The layout walk must grow
    // it into ONE composed glyph so the renderer joins it, rather than three
    // isolated Char cells. (Structural: holds regardless of font availability,
    // since grouping is driven by complex_script, not by shaping success.)
    let trace = backend_layout_trace_with_buffer_setup(
        BufferTextBackendKind::GapBuffer,
        "layout-backend-arabic",
        "\u{0627}\u{0644}\u{0645}\n",
        360,
        140,
        |_buffer, _buf_id, _text| {},
    );
    let composites = trace_composite_texts(&trace);
    assert!(
        composites
            .iter()
            .any(|t| t.contains('\u{0627}') && t.contains('\u{0645}')),
        "Arabic run should compose into one Composite glyph spanning the run, \
         composites={composites:?}"
    );
}

#[test]
fn implemented_text_backends_match_selective_display_output() {
    let baseline = selective_display_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let rows = trace_text_rows(&baseline);
    assert!(
        rows.iter().any(|row| row.contains("head")),
        "baseline should render text before carriage-return selective display marker, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("head...")),
        "baseline should render the selective-display ellipsis, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("shown")),
        "baseline should render visible line after selective display marker, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("shown2")),
        "baseline should resume rendering after an indented hidden block, rows={rows:?}"
    );
    assert!(
        rows.iter()
            .all(|row| !row.contains("hidden tail") && !row.contains("hidden by indent")),
        "baseline should not render selective-display hidden text, rows={rows:?}"
    );
    assert!(
        baseline.output_rows.len() >= 2,
        "baseline should publish rows across selective-display output"
    );

    for kind in implemented_text_backends() {
        let trace = selective_display_backend_layout_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_glyphless_display_geometry() {
    let baseline = glyphless_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let rows = trace_text_rows(&baseline);
    assert!(
        rows.iter().any(|row| row.contains("abcd")),
        "baseline should keep surrounding text around glyphless source chars, rows={rows:?}"
    );
    let text_row = baseline
        .output_rows
        .iter()
        .find(|row| row.row == 0)
        .expect("baseline text output row");
    assert!(
        text_row.end_col > 4,
        "baseline should account for glyphless replacement columns, row={text_row:?}"
    );
    assert!(
        baseline
            .points
            .iter()
            .any(|point| point.buffer_pos == LispCharPos1::new(2)),
        "baseline should publish a display point for the C1 glyphless source char, trace={baseline:?}"
    );
    assert!(
        baseline
            .points
            .iter()
            .any(|point| point.buffer_pos == LispCharPos1::new(6)),
        "baseline should publish a display point for the object-replacement source char, trace={baseline:?}"
    );

    for kind in implemented_text_backends() {
        let trace = glyphless_backend_layout_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn layout_frame_rust_renders_buffer_glyphless_chars_as_glyphless() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a\u{fff0}b");
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-buffer-glyphless-text", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    assert!(
        text_row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Glyphless { ch: '\u{fff0}' })),
        "buffer glyphless source char should emit a glyphless glyph, row={:?}",
        text_row.glyphs[1]
    );
}

#[test]
fn layout_frame_rust_renders_buffer_control_chars_with_caret_notation() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a\u{0001}b");
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-buffer-control-text", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    assert_eq!(glyphs_logical_text(&text_row.glyphs[1]), "a^Ab");
}

/// GNU renders control-character substitute glyphs (`^A`, `\003`, ...) in a
/// face that merges the `escape-glyph` face over the surrounding base face
/// (`merge_escape_glyph_face` -> `merge_faces(w, Qescape_glyph, 0,
/// it->face_id)`, xdisp.c:8372-8389). The single merged face id is stamped
/// onto BOTH the `^` glyph and the caret letter (`dpvec_face_id`, xdisp.c:8663).
///
/// This test pins the RENDERED-glyph foreground (the guard the prior latent
/// groundwork lacked): both `^` and `A` must resolve to the escape-glyph
/// foreground, not the default text foreground, while the surrounding plain
/// `a`/`b` keep the base face.
#[test]
fn layout_frame_rust_control_char_caret_uses_escape_glyph_foreground() {
    use neomacs_display_protocol::types::Color;

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a\u{0001}b\n");
    }

    // GNU's cyan-on-dark escape-glyph color (#46D9FF), used by the Doom GUI
    // repro. Defined through the Lisp face machinery so it survives the
    // per-frame face sync (`sync_runtime_faces_for_frame`) that layout runs.
    let escape_fg = Color::from_pixel((0x46u32 << 16) | (0xD9u32 << 8) | 0xFFu32);

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-control-char-escape-glyph", 640, 160, buf_id);
    // Window-system frame: GNU only paints escape-glyph faces in graphical
    // redisplay; a TTY frame collapses many face distinctions.
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    assert!(eval.frame_manager_mut().select_frame(frame_id));
    let results = eval.eval_str_each(
        "(internal-set-lisp-face-attribute 'escape-glyph :foreground \"#46D9FF\" (selected-frame))",
    );
    assert!(
        results.iter().all(Result::is_ok),
        "escape-glyph face must accept a foreground, got {results:?}"
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text && row.displays_text)
        .expect("text row");
    let text_glyphs = &text_row.glyphs[GlyphArea::Text.index()];

    // The control char produced the caret "^A" (proves the ControlChar path ran).
    assert!(
        glyphs_logical_text(text_glyphs).contains("^A"),
        "control char should render as ^A, got {:?}",
        glyphs_logical_text(text_glyphs)
    );

    let caret = text_glyphs
        .iter()
        .find(|g| matches!(g.glyph_type, GlyphType::Char { ch: '^' }))
        .expect("caret '^' glyph");
    let caret_letter = text_glyphs
        .iter()
        .find(|g| matches!(g.glyph_type, GlyphType::Char { ch: 'A' }))
        .expect("caret letter 'A' glyph");
    let plain_a = text_glyphs
        .iter()
        .find(|g| matches!(g.glyph_type, GlyphType::Char { ch: 'a' }))
        .expect("plain 'a' glyph");

    // GNU stamps ONE merged face id on both substitute glyphs.
    assert_eq!(
        caret.face_id, caret_letter.face_id,
        "the '^' and caret letter must share one escape-glyph-merged face id"
    );
    // ...and it is distinct from the surrounding base text face.
    assert_ne!(
        caret.face_id, plain_a.face_id,
        "escape glyph must realize a separate face, not reuse the base text face"
    );

    // THE guard the prior groundwork lacked: the caret's RESOLVED foreground is
    // the escape-glyph color, not the default text foreground.
    let caret_face = state
        .faces
        .get(&caret.face_id)
        .expect("escape-glyph face must be registered in the frame face table");
    assert_eq!(
        caret_face.foreground, escape_fg,
        "caret '^' fg must be the escape-glyph foreground, got {:?}",
        caret_face.foreground
    );
    let caret_letter_face = state
        .faces
        .get(&caret_letter.face_id)
        .expect("escape-glyph face for caret letter");
    assert_eq!(
        caret_letter_face.foreground, escape_fg,
        "caret letter 'A' fg must be the escape-glyph foreground"
    );

    // And it differs from the plain text face's foreground (override applied).
    let base_face = state
        .faces
        .get(&plain_a.face_id)
        .expect("base text face for 'a'");
    assert_ne!(
        caret_face.foreground, base_face.foreground,
        "escape glyph fg must differ from the default text fg"
    );
}

/// Sibling guard: ordinary (non-control) text glyphs keep the surrounding base
/// face -- the escape-glyph merge must NOT leak onto normal characters.
#[test]
fn layout_frame_rust_normal_text_keeps_base_face_not_escape_glyph() {
    use neomacs_display_protocol::types::Color;

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a\u{0001}b\n");
    }
    {
        let table = eval.face_table_mut();
        let mut escape = neovm_core::face::Face::new("escape-glyph");
        escape.foreground = Some(neovm_core::face::Color::rgb(0x46, 0xD9, 0xFF));
        table.define("escape-glyph", escape);
    }
    let escape_fg = Color::from_pixel((0x46u32 << 16) | (0xD9u32 << 8) | 0xFFu32);

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-normal-text-base-face", 640, 160, buf_id);
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text && row.displays_text)
        .expect("text row");
    let text_glyphs = &text_row.glyphs[GlyphArea::Text.index()];

    let default_face_id = FaceId::from(neomacs_display_protocol::face::BasicFaceId::Default);
    let default_fg = state
        .faces
        .get(&default_face_id)
        .expect("default face")
        .foreground;

    for ch in ['a', 'b'] {
        let glyph = text_glyphs
            .iter()
            .find(|g| matches!(g.glyph_type, GlyphType::Char { ch: c } if c == ch))
            .unwrap_or_else(|| panic!("plain '{ch}' glyph"));
        let face = state
            .faces
            .get(&glyph.face_id)
            .unwrap_or_else(|| panic!("resolved face for '{ch}'"));
        assert_eq!(
            face.foreground, default_fg,
            "normal text '{ch}' must keep the default base foreground, not escape-glyph"
        );
        assert_ne!(
            face.foreground, escape_fg,
            "normal text '{ch}' must NOT take the escape-glyph foreground"
        );
    }
}

/// GNU `get_next_display_element` (xdisp.c:8594-8603) merges the `nobreak-space`
/// face over the surrounding base face for a non-ASCII space (e.g. nbsp U+00A0)
/// when `nobreak-char-display` is `t` (the default), painting the substitute
/// glyph in the merged face -- `merge_faces (it->w, Qnobreak_space, 0,
/// it->face_id)`. `nobreak-space` inherits `escape-glyph`, so when escape-glyph
/// carries a themed foreground the nbsp glyph resolves to it.
///
/// This pins the RENDERED-glyph foreground: the nbsp substitute must resolve to
/// the (escape-glyph-inherited) nobreak-space foreground, distinct from the
/// surrounding base face -- while an adjacent NORMAL space keeps the base face.
#[test]
fn layout_frame_rust_nbsp_uses_nobreak_space_foreground() {
    use neomacs_display_protocol::types::Color;

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        // `a`, normal space, nbsp, `c`. The normal space is the base-face control
        // case; the nbsp must get the nobreak-space face.
        buf.insert("a \u{00A0}c\n");
    }

    // GNU's cyan-on-dark escape-glyph color (#46D9FF). `nobreak-space` inherits
    // escape-glyph, so the nbsp glyph must resolve to this color WITHOUT
    // nobreak-space setting its own foreground -- proving the `:inherit` chain
    // flows through `merge_named_face_over`. Defined through the Lisp face
    // machinery so it survives the per-frame face sync.
    let nbsp_fg = Color::from_pixel((0x46u32 << 16) | (0xD9u32 << 8) | 0xFFu32);

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-nbsp-nobreak-space", 640, 160, buf_id);
    // Window-system frame: GNU only paints nobreak faces in graphical redisplay.
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    assert!(eval.frame_manager_mut().select_frame(frame_id));
    // Set escape-glyph's foreground and wire `nobreak-space :inherit
    // escape-glyph` (faces.el's defface, which the bare test context does not
    // load). The nbsp glyph must then resolve to the escape-glyph color WITHOUT
    // nobreak-space setting its own foreground -- proving the `:inherit` chain
    // flows through `merge_named_face_over`.
    let results = eval.eval_str_each(
        "(internal-set-lisp-face-attribute 'escape-glyph :foreground \"#46D9FF\" (selected-frame))\
         (internal-set-lisp-face-attribute 'nobreak-space :inherit 'escape-glyph (selected-frame))",
    );
    assert!(
        results.iter().all(Result::is_ok),
        "escape-glyph/nobreak-space faces must accept attributes, got {results:?}"
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text && row.displays_text)
        .expect("text row");
    let text_glyphs = &text_row.glyphs[GlyphArea::Text.index()];

    let default_face_id = FaceId::from(neomacs_display_protocol::face::BasicFaceId::Default);
    let default_fg = state
        .faces
        .get(&default_face_id)
        .expect("default face")
        .foreground;

    // In highlight mode the nbsp substitute renders as a space glyph, so there
    // are two `' '` glyphs: the literal space (base face) and the nbsp substitute
    // (nobreak-space face). Distinguish them by resolved foreground, not by char.
    let space_glyphs: Vec<&Glyph> = text_glyphs
        .iter()
        .filter(|g| matches!(g.glyph_type, GlyphType::Char { ch: ' ' }))
        .collect();
    assert_eq!(
        space_glyphs.len(),
        2,
        "expected one literal space and one nbsp substitute space, got {:?}",
        glyphs_logical_text(text_glyphs)
    );

    let mut nbsp_glyph = None;
    let mut normal_space_glyph = None;
    for glyph in &space_glyphs {
        let face = state
            .faces
            .get(&glyph.face_id)
            .expect("resolved face for space glyph");
        if face.foreground == nbsp_fg {
            nbsp_glyph = Some(*glyph);
        } else if face.foreground == default_fg {
            normal_space_glyph = Some(*glyph);
        }
    }

    let nbsp_glyph = nbsp_glyph.expect(
        "exactly one space glyph (the nbsp substitute) must resolve to the nobreak-space \
         (escape-glyph-inherited) foreground",
    );
    let normal_space_glyph =
        normal_space_glyph.expect("the literal space must keep the default base foreground");

    // The nbsp realized a SEPARATE face id, not the base/default face reused.
    assert_ne!(
        nbsp_glyph.face_id, normal_space_glyph.face_id,
        "nbsp must realize a separate nobreak-space face, not reuse the base text face"
    );
    assert_ne!(
        nbsp_glyph.face_id, default_face_id,
        "nbsp face must not be the default face id"
    );

    // The surrounding plain text `a`/`c` keep the base foreground.
    for ch in ['a', 'c'] {
        let glyph = text_glyphs
            .iter()
            .find(|g| matches!(g.glyph_type, GlyphType::Char { ch: c } if c == ch))
            .unwrap_or_else(|| panic!("plain '{ch}' glyph"));
        let face = state
            .faces
            .get(&glyph.face_id)
            .unwrap_or_else(|| panic!("resolved face for '{ch}'"));
        assert_eq!(
            face.foreground, default_fg,
            "normal text '{ch}' must keep the default base foreground, not nobreak-space"
        );
        assert_ne!(
            face.foreground, nbsp_fg,
            "normal text '{ch}' must NOT take the nobreak-space foreground"
        );
    }
}

/// Sibling guard for the nobreak hyphens. GNU treats SOFT_HYPHEN (U+00AD),
/// HYPHEN (U+2010) and NON_BREAKING_HYPHEN (U+2011) via
/// `merge_faces (it->w, Qnobreak_hyphen, 0, it->face_id)` (xdisp.c:8608-8617).
/// The substitute renders as `-`; it must resolve to the nobreak-hyphen
/// foreground, distinct from a normal `-`/base text.
#[test]
fn layout_frame_rust_nobreak_hyphen_uses_nobreak_hyphen_foreground() {
    use neomacs_display_protocol::types::Color;

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        // `x`, normal hyphen, non-breaking hyphen U+2011, `y`.
        buf.insert("x-\u{2011}y\n");
    }

    let hyphen_fg = Color::from_pixel((0x12u32 << 16) | (0x34u32 << 8) | 0x56u32);

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-nobreak-hyphen", 640, 160, buf_id);
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id) {
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    assert!(eval.frame_manager_mut().select_frame(frame_id));
    let results = eval.eval_str_each(
        "(internal-set-lisp-face-attribute 'nobreak-hyphen :foreground \"#123456\" (selected-frame))",
    );
    assert!(
        results.iter().all(Result::is_ok),
        "nobreak-hyphen face must accept a foreground, got {results:?}"
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text && row.displays_text)
        .expect("text row");
    let text_glyphs = &text_row.glyphs[GlyphArea::Text.index()];

    let default_face_id = FaceId::from(neomacs_display_protocol::face::BasicFaceId::Default);
    let default_fg = state
        .faces
        .get(&default_face_id)
        .expect("default face")
        .foreground;

    // Two `-` glyphs: literal hyphen (base) and the U+2011 substitute (nobreak).
    let hyphen_glyphs: Vec<&Glyph> = text_glyphs
        .iter()
        .filter(|g| matches!(g.glyph_type, GlyphType::Char { ch: '-' }))
        .collect();
    assert_eq!(
        hyphen_glyphs.len(),
        2,
        "expected one literal hyphen and one U+2011 substitute, got {:?}",
        glyphs_logical_text(text_glyphs)
    );

    let mut nobreak_glyph = None;
    let mut normal_hyphen_glyph = None;
    for glyph in &hyphen_glyphs {
        let face = state
            .faces
            .get(&glyph.face_id)
            .expect("resolved face for hyphen glyph");
        if face.foreground == hyphen_fg {
            nobreak_glyph = Some(*glyph);
        } else if face.foreground == default_fg {
            normal_hyphen_glyph = Some(*glyph);
        }
    }

    let nobreak_glyph = nobreak_glyph
        .expect("the U+2011 substitute hyphen must resolve to the nobreak-hyphen foreground");
    let normal_hyphen_glyph =
        normal_hyphen_glyph.expect("the literal hyphen must keep the default base foreground");

    assert_ne!(
        nobreak_glyph.face_id, normal_hyphen_glyph.face_id,
        "U+2011 must realize a separate nobreak-hyphen face, not reuse the base text face"
    );
}

#[test]
fn layout_frame_rust_renders_line_prefix_through_row_builder() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abc");
        buf.set_buffer_local("line-prefix", Value::string("中\t"));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-buffer-line-prefix", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");
    let logical_text = glyphs_logical_text(&text_row.glyphs[1]);

    assert!(
        logical_text.starts_with("中      abc"),
        "line-prefix should render through the shared row builder with wide/tab semantics, text={logical_text:?}, row={:?}",
        text_row.glyphs[1]
    );
    assert!(
        text_row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }) && glyph.wide),
        "line-prefix wide char should carry wide glyph metadata, row={:?}",
        text_row.glyphs[1]
    );
    assert!(
        text_row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { width_cols: 6 })),
        "line-prefix tab should expand to the next tab stop, row={:?}",
        text_row.glyphs[1]
    );
}

#[test]
fn layout_frame_rust_renders_nobreak_chars_as_mapped_text() {
    let mut eval = Context::new();
    eval.obarray_mut()
        .set_symbol_value("nobreak-char-display", Value::T);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a\u{00a0}b\u{00ad}c");
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-buffer-nobreak-text", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    assert_eq!(glyphs_logical_text(&text_row.glyphs[1]), "a b-c");
}

#[test]
fn layout_frame_rust_renders_nobreak_chars_in_escape_mode_as_mapped_text() {
    let mut eval = Context::new();
    eval.obarray_mut()
        .set_symbol_value("nobreak-char-display", Value::fixnum(2));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a\u{00a0}b\u{00ad}c");
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-buffer-nobreak-escape-text",
        640,
        160,
        buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    assert_eq!(glyphs_logical_text(&text_row.glyphs[1]), "a\\ b\\-c");
}

#[test]
fn implemented_text_backends_match_composite_glyph_output() {
    let baseline = composition_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let composites = trace_composite_texts(&baseline);
    assert!(
        composites.contains(&"e\u{0301}".to_string()),
        "baseline should merge Latin base plus acute mark into a composite glyph, composites={composites:?}"
    );
    assert!(
        composites.contains(&"a\u{0300}\u{0301}".to_string()),
        "baseline should keep multiple combining marks on one composite glyph, composites={composites:?}"
    );
    assert!(
        composites.contains(&"中\u{0300}".to_string()),
        "baseline should compose combining marks on multibyte base chars, composites={composites:?}"
    );
    assert!(
        baseline
            .points
            .iter()
            .any(|point| point.buffer_pos == LispCharPos1::new(1)),
        "baseline should publish display geometry for the first composite base char, trace={baseline:?}"
    );

    for kind in implemented_text_backends() {
        let trace = composition_backend_layout_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_wrapped_redisplay_retry_output() {
    let (baseline, target_pos) =
        wrapped_retry_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    assert!(
        baseline
            .points
            .iter()
            .any(|point| point.buffer_pos == LispCharPos1::from_one_based_usize(target_pos)),
        "baseline should converge wrapped redisplay on target point {target_pos}, trace={baseline:?}"
    );
    assert!(
        baseline.window_start > LispCharPos1::ONE,
        "baseline should advance window-start after wrapped redisplay retry, trace={baseline:?}"
    );
    assert!(
        baseline.output_rows.iter().any(|row| row.row > 0),
        "baseline should publish wrapped visual rows, rows={:?}",
        baseline.output_rows
    );

    for kind in implemented_text_backends() {
        let (trace, backend_target_pos) = wrapped_retry_backend_layout_trace(kind);
        assert_eq!(backend_target_pos, target_pos, "{kind:?}");
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_point_line_tail_retry_output() {
    let (baseline, point, later_pos) =
        point_line_tail_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    assert!(
        baseline
            .points
            .iter()
            .any(|item| item.buffer_pos == LispCharPos1::from_one_based_usize(point)),
        "baseline should publish geometry for point {point}, trace={baseline:?}"
    );
    assert!(
        baseline
            .points
            .iter()
            .any(|item| item.buffer_pos == LispCharPos1::from_one_based_usize(later_pos)),
        "baseline should publish later positions from the point line after retry, later_pos={later_pos}, trace={baseline:?}"
    );

    for kind in implemented_text_backends() {
        let (trace, backend_point, backend_later_pos) = point_line_tail_backend_layout_trace(kind);
        assert_eq!(backend_point, point, "{kind:?}");
        assert_eq!(backend_later_pos, later_pos, "{kind:?}");
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_mode_line_geometry_after_redisplay_retry() {
    let (baseline, point) =
        mode_line_geometry_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let mode_line = trace_mode_line_text(&baseline);
    assert!(
        baseline.window_start > LispCharPos1::ONE,
        "baseline should advance window-start for EOB redisplay retry, trace={baseline:?}"
    );
    assert_eq!(
        baseline.window_point,
        LispCharPos1::from_one_based_usize(point),
        "baseline should preserve the selected-window EOB point after retry"
    );
    assert!(
        mode_line.contains('|') && !mode_line.contains("%o"),
        "baseline should render expanded mode-line geometry, mode_line={mode_line:?}"
    );

    for kind in implemented_text_backends() {
        let (trace, backend_point) = mode_line_geometry_backend_layout_trace(kind);
        assert_eq!(backend_point, point, "{kind:?}");
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_hscroll_cursor_and_position_output() {
    let baseline = hscroll_cursor_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let cursor = baseline.phys_cursor.as_ref().expect("baseline cursor");
    assert_eq!(cursor.x, 0);
    assert_eq!(cursor.row, 0);
    assert_eq!(cursor.col, 0);
    let text_rows = trace_text_rows(&baseline);
    assert!(
        text_rows.iter().any(|row| row.starts_with('$')),
        "baseline should render the left truncation marker, rows={text_rows:?}"
    );
    assert!(
        text_rows.iter().any(|row| row.contains("def")),
        "baseline should render the hscrolled visible suffix, rows={text_rows:?}"
    );
    assert!(
        text_rows.iter().all(|row| !row.contains("abc")),
        "baseline should not render hscrolled-away prefix text, rows={text_rows:?}"
    );
    assert_eq!(
        baseline.visible_span,
        Some(WindowVisibleBufferSpan::new(
            LispCharPos1::new(4),
            LispCharPos1::new(7)
        )),
        "baseline should publish the visible hscrolled buffer span"
    );

    for kind in implemented_text_backends() {
        let trace = hscroll_cursor_backend_layout_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_edit_redisplay_cache_invalidation() {
    let (baseline_before, baseline_after) =
        edit_redisplay_backend_layout_trace(BufferTextBackendKind::GapBuffer);
    let before_rows = trace_text_rows(&baseline_before);
    let after_rows = trace_text_rows(&baseline_after);
    assert!(
        before_rows
            .iter()
            .any(|row| row.contains("alpha beta gamma")),
        "baseline before edit should render original text, rows={before_rows:?}"
    );
    assert!(
        after_rows
            .iter()
            .any(|row| row.contains("alpha BETA gamma")),
        "baseline after edit should render replacement text, rows={after_rows:?}"
    );
    assert!(
        after_rows
            .iter()
            .all(|row| !row.contains("alpha beta gamma")),
        "baseline after edit should not reuse stale glyph text, rows={after_rows:?}"
    );
    assert_ne!(
        baseline_before, baseline_after,
        "same-engine redisplay after edit should update the trace"
    );

    for kind in implemented_text_backends() {
        let (before, after) = edit_redisplay_backend_layout_trace(kind);
        assert_eq!(before, baseline_before, "{kind:?} before");
        assert_eq!(after, baseline_after, "{kind:?} after");
    }
}

#[test]
fn implemented_text_backends_match_redisplay_fontification_after_edit() {
    let baseline = fontification_edit_backend_trace(BufferTextBackendKind::GapBuffer);
    let before_rows = trace_text_rows(&baseline.before_layout);
    let after_rows = trace_text_rows(&baseline.after_layout);
    assert!(
        before_rows
            .iter()
            .any(|row| row.contains("alpha beta gamma")),
        "baseline before fontification edit should render original text, rows={before_rows:?}"
    );
    assert!(
        after_rows
            .iter()
            .any(|row| row.contains("alpha BETA gamma")),
        "baseline after fontification edit should render edited text, rows={after_rows:?}"
    );
    assert!(
        baseline.before_props.contains("font-lock-keyword-face"),
        "baseline should apply the initial font-lock face from redisplay fontification, props={}",
        baseline.before_props
    );
    assert!(
        baseline.after_props.contains("font-lock-warning-face"),
        "baseline should re-enter redisplay fontification after edit, props={}",
        baseline.after_props
    );
    assert!(
        !trace_text_faces(&baseline.before_layout).is_empty(),
        "baseline should emit text glyphs with face ids"
    );

    for kind in implemented_text_backends() {
        let trace = fontification_edit_backend_trace(kind);
        assert_eq!(trace, baseline, "{kind:?}");
    }
}

#[test]
fn layout_frame_rust_publishes_face_scaled_advances_for_inline_plist_faces() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a好好b ");
        let plist = Value::list(vec![
            Value::keyword("family"),
            Value::string("JetBrains Mono"),
            Value::keyword("height"),
            Value::make_float(1.6),
            Value::keyword("weight"),
            Value::symbol("extra-bold"),
        ]);
        buf.put_text_property(
            0,
            buf.total_emacs_byte_len().get(),
            Value::symbol("face"),
            plist,
        );
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-face-advance", 800, 160, buf_id);
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }

    {
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        let face_resolver = crate::neovm_bridge::FaceResolver::new(
            eval.face_table(),
            0x00FFFFFF,
            0x00000000,
            eval.frame_manager()
                .get(frame_id)
                .expect("frame")
                .font_pixel_size,
            Some("neo".to_string()),
        );
        let mut next_check = buffer.point_max_char_pos().get();
        let resolved = face_resolver.base_face_for_origin(
            Some(buffer),
            &DisplayOrigin::BufferText {
                charpos: neovm_core::buffer::CharPos0::new(0),
            },
            BaseFacePolicy::BufferFaceIncludingOverlays,
            &mut next_check,
        );
        assert_eq!(resolved.font_family, "JetBrains Mono");
        assert_eq!(resolved.font_weight, 800);
        assert!(
            resolved.font_size > face_resolver.default_face().font_size * 1.5,
            "expected face resolver to scale the inline plist face before layout, got {:?}",
            resolved
        );
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let all_points = snapshot.points.clone();
    let a = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("a");
    let hao1 = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(2))
        .expect("hao1");
    let hao2 = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("hao2");
    let b = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(4))
        .expect("b");
    let space = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(5))
        .expect("space");

    let default_font_size = frame.font_pixel_size;
    let face_font_size = default_font_size * 1.6;
    let mut metrics = FontMetricsService::new();
    let expected_a = expected_gui_glyph_advance(
        &mut metrics,
        'a',
        "JetBrains Mono",
        800,
        false,
        face_font_size,
    );
    let expected_hao = expected_gui_glyph_advance(
        &mut metrics,
        '好',
        "JetBrains Mono",
        800,
        false,
        face_font_size,
    );
    let expected_b = expected_gui_glyph_advance(
        &mut metrics,
        'b',
        "JetBrains Mono",
        800,
        false,
        face_font_size,
    );
    assert_point_width_matches_advance(a, expected_a, "inline face a", &all_points);
    assert_point_width_matches_advance(hao1, expected_hao, "inline face first 好", &all_points);
    assert_point_width_matches_advance(hao2, expected_hao, "inline face second 好", &all_points);
    assert_point_width_matches_advance(b, expected_b, "inline face b", &all_points);
    assert_point_delta_matches_advance(a, hao1, expected_a, "inline face first 好", &all_points);
    assert_point_delta_matches_advance(
        hao1,
        hao2,
        expected_hao,
        "inline face second 好",
        &all_points,
    );
    assert_point_delta_matches_advance(hao2, b, expected_hao, "inline face b", &all_points);
    assert_point_delta_matches_advance(b, space, expected_b, "inline face space", &all_points);
}

#[test]
fn layout_frame_rust_cursor_width_uses_current_glyph_advance_not_next_glyph() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("iW ");
        let plist = Value::list(vec![
            Value::keyword("family"),
            Value::string("Noto Sans"),
            Value::keyword("weight"),
            Value::symbol("regular"),
        ]);
        buf.put_text_property(
            0,
            buf.total_emacs_byte_len().get(),
            Value::symbol("face"),
            plist,
        );
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-cursor-current-glyph-advance",
        800,
        400,
        buf_id,
    );
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let face_font_size = frame.font_pixel_size;
    let mut metrics = FontMetricsService::new();
    let expected_i = metrics
        .char_width('i', "Noto Sans", 400, false, face_font_size)
        .round() as i64;
    let expected_w = metrics
        .char_width('W', "Noto Sans", 400, false, face_font_size)
        .round() as i64;
    assert_ne!(
        expected_i, expected_w,
        "test requires proportional metrics for i and W"
    );
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let i_point = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("i point");
    let cursor = snapshot.phys_cursor.as_ref().expect("cursor");

    assert_eq!(
        i_point.width, expected_i,
        "point geometry should publish the current glyph advance"
    );
    assert_eq!(
        cursor.width, i_point.width,
        "box cursor width must come from the glyph under point, not the following glyph"
    );
    assert_ne!(
        cursor.width, expected_w,
        "cursor must not use the following W glyph advance"
    );
}

#[test]
fn layout_frame_rust_places_cursor_at_newline_terminated_row_end() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "first line\nsecond line\nthird line\n";
    let newline_byte = text.find('\n').expect("newline");
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(newline_byte));
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-cursor-eol", 640, 240, buf_id);
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(newline_byte + 1);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let last_char = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(newline_byte))
        .expect("last visible char before newline");
    let cursor = snapshot.phys_cursor.as_ref().expect("phys cursor");

    assert_eq!(cursor.row, last_char.row);
    assert_eq!(cursor.col, last_char.col + 1);
    assert_eq!(cursor.x, last_char.x + last_char.width);
    assert!(cursor.width > 0);
}

#[test]
fn layout_frame_rust_emits_neomacs_visual_cursors_without_moving_phys_cursor() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("alpha\nbeta\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        let visual_cursor = Value::list(vec![
            Value::keyword(":position"),
            Value::fixnum(3),
            Value::keyword(":cursor-type"),
            Value::cons(Value::symbol("bar"), Value::fixnum(6)),
            Value::keyword(":color"),
            Value::string("#ff0000"),
        ]);
        buf.set_buffer_local("neomacs-visual-cursors", Value::list(vec![visual_cursor]));
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-visual-cursor", 320, 120, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let visual = state
        .cursors
        .iter()
        .find(|cursor| cursor.window_id.get() < 0)
        .expect("visual cursor");
    assert_eq!(visual.window_id.get(), -1_000_000);
    assert_eq!(visual.width, 6.0);
    assert_eq!(visual.color, Color::from_pixel(0xff0000));

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let selected_window = frame.selected_window;
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let phys = snapshot.phys_cursor.as_ref().expect("phys cursor");
    assert_eq!(phys.x, 0, "visual cursor must not move GNU point");
}

#[test]
fn layout_frame_rust_visual_cursor_uses_display_point_geometry() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("iW ");
        let plist = Value::list(vec![
            Value::keyword("family"),
            Value::string("Noto Sans"),
            Value::keyword("weight"),
            Value::symbol("regular"),
        ]);
        buf.put_text_property(
            0,
            buf.total_emacs_byte_len().get(),
            Value::symbol("face"),
            plist,
        );
        let visual_cursor = Value::list(vec![
            Value::keyword(":position"),
            Value::fixnum(1),
            Value::keyword(":cursor-type"),
            Value::symbol("box"),
            Value::keyword(":color"),
            Value::string("#00ff00"),
        ]);
        buf.set_buffer_local("neomacs-visual-cursors", Value::list(vec![visual_cursor]));
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-visual-cursor-display-point-geometry",
        320,
        120,
        buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }

    let mut metrics = FontMetricsService::new();
    let face_font_size = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .font_pixel_size;
    let expected_i = metrics
        .char_width('i', "Noto Sans", 400, false, face_font_size)
        .round() as i64;
    let expected_w = metrics
        .char_width('W', "Noto Sans", 400, false, face_font_size)
        .round() as i64;
    assert_ne!(
        expected_i, expected_w,
        "test requires proportional metrics for i and W"
    );

    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let i_point = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("i point");
    let visual = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state")
        .cursors
        .iter()
        .find(|cursor| cursor.window_id.get() < 0)
        .expect("visual cursor");

    assert_eq!(
        visual.width.round() as i64,
        i_point.width,
        "visual box cursor width must use the rendered glyph under :position"
    );
    assert_eq!(
        visual.height.round() as i64,
        i_point.height,
        "visual box cursor height must use the rendered glyph under :position"
    );
    assert_ne!(
        visual.width.round() as i64,
        expected_w,
        "visual cursor must not use the following glyph's width"
    );
}

#[test]
fn layout_frame_rust_visual_hbar_uses_full_display_point_box() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abc");
        let visual_cursor = Value::list(vec![
            Value::keyword(":position"),
            Value::fixnum(2),
            Value::keyword(":cursor-type"),
            Value::cons(Value::symbol("hbar"), Value::fixnum(3)),
            Value::keyword(":color"),
            Value::string("#00ff00"),
        ]);
        buf.set_buffer_local("neomacs-visual-cursors", Value::list(vec![visual_cursor]));
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-visual-hbar-display-point-box",
        320,
        120,
        buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let b_point = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(2))
        .expect("b point");
    let visual = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state")
        .cursors
        .iter()
        .find(|cursor| cursor.window_id.get() < 0)
        .expect("visual cursor");

    assert_eq!(visual.width.round() as i64, b_point.width);
    assert_eq!(
        visual.height.round() as i64,
        b_point.height,
        "hbar visual cursor stores the full glyph box; renderer draws the bar from style"
    );
}

#[test]
fn layout_frame_rust_records_row_metrics_for_plain_text_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("plain text row\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-plain-row-metrics", 800, 160, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let text_row = engine
        .last_frame_display_state
        .as_ref()
        .and_then(|state| {
            state
                .window_matrices
                .iter()
                .flat_map(|wm| wm.matrix.rows.iter())
                .find(|row| row.role == GlyphRowRole::Text && row.enabled)
        })
        .expect("text row");

    assert!(
        text_row.height_px > 0.0,
        "expected ordinary text rows to record authoritative height, got {text_row:?}"
    );
    assert!(
        text_row.ascent_px > 0.0,
        "expected ordinary text rows to record authoritative ascent, got {text_row:?}"
    );
}

#[test]
fn layout_frame_rust_uses_buffer_default_face_height_for_body_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("scaled default\n");
        buf.set_buffer_local(
            "face-remapping-alist",
            Value::list(vec![Value::list(vec![
                Value::symbol("default"),
                Value::list(vec![Value::keyword("height"), Value::make_float(0.75)]),
                Value::symbol("default"),
            ])]),
        );
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }
    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-scaled-default-row-metrics",
        800,
        160,
        buf_id,
    );
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text && row.displays_text)
        .expect("text row");

    assert!(
        text_row.height_px < state.char_height.max(1.0),
        "buffer-local default face remapping should shrink body row height below the frame default; frame_char_height={} row={text_row:?}",
        state.char_height
    );
}

#[test]
fn layout_frame_rust_applies_extra_line_spacing_once_to_newline_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("alpha\nbeta\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        buf.set_buffer_local("line-spacing", Value::fixnum(5));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-extra-line-spacing-once", 800, 160, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let selected_window = frame.selected_window;
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let first_row = snapshot.row_metrics(0).expect("first text row");
    let second_row = snapshot.row_metrics(1).expect("second text row");

    assert_eq!(
        second_row.y - first_row.y,
        first_row.height + 5,
        "newline row advance should include extra line-spacing exactly once"
    );
}

#[test]
fn layout_frame_rust_applies_display_height_to_buffer_text_faces() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("AB\n");
        buf.put_text_property(
            1,
            2,
            Value::symbol("display"),
            Value::list(vec![Value::symbol("height"), Value::make_float(2.0)]),
        );
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-display-height-text-face", 640, 160, buf_id);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    assert_eq!(glyphs_logical_text(&text_row.glyphs[1]), "AB");
    let text_faces = text_row.glyphs[1]
        .iter()
        .filter(|glyph| !glyph.padding)
        .map(|glyph| glyph.face_id)
        .collect::<Vec<_>>();
    assert_eq!(
        text_faces.len(),
        2,
        "expected two visible glyphs in {text_row:?}"
    );
    assert_ne!(
        text_faces[0], text_faces[1],
        "display height should realize a separate face for the covered glyph"
    );
    let base_face = state
        .faces
        .get(&text_faces[0])
        .expect("base text face should be registered");
    let adjusted_face = state
        .faces
        .get(&text_faces[1])
        .expect("height-adjusted text face should be registered");
    assert!(
        adjusted_face.font_size > base_face.font_size,
        "display height should scale the realized render face, base={base_face:?} adjusted={adjusted_face:?}"
    );
    assert!(
        text_row.height_px > state.char_height.max(1.0),
        "height display property should grow text row metrics, frame_char_height={} row={text_row:?}",
        state.char_height
    );
}

#[test]
fn layout_frame_rust_applies_display_height_to_overlay_strings() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("x\n");
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let after_string = Value::string_with_text_properties(
            "Y",
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![
                    Value::symbol("display"),
                    Value::list(vec![Value::symbol("height"), Value::make_float(2.0)]),
                ]),
            }],
        );
        let _ =
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("after-string"), after_string);
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-overlay-display-height", 640, 160, buf_id);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    assert_eq!(glyphs_logical_text(&text_row.glyphs[1]), "xY");
    assert!(
        text_row.height_px > state.char_height.max(1.0),
        "display height in overlay string should grow text row metrics, frame_char_height={} row={text_row:?}",
        state.char_height
    );
}

#[test]
fn layout_frame_rust_advances_overlay_newline_by_measured_row_height() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("x");
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let after_string = Value::string_with_text_properties(
            "A\nB",
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![
                    Value::symbol("display"),
                    Value::list(vec![Value::symbol("height"), Value::make_float(2.0)]),
                ]),
            }],
        );
        let _ =
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("after-string"), after_string);
    }
    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-overlay-newline-measured-height",
        640,
        180,
        buf_id,
    );
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_rows = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect::<Vec<_>>();
    let first_row = text_rows
        .iter()
        .find(|row| glyphs_logical_text(&row.glyphs[1]).contains("xA"))
        .expect("first overlay row");
    let second_row = text_rows
        .iter()
        .find(|row| glyphs_logical_text(&row.glyphs[1]).contains("B"))
        .expect("second overlay row");

    assert!(
        first_row.height_px > state.char_height.max(1.0),
        "test setup should make first overlay row taller than default, frame_char_height={} row={first_row:?}",
        state.char_height
    );
    assert_eq!(
        second_row.pixel_y - first_row.pixel_y,
        first_row.height_px,
        "overlay newline should advance by the measured first row height"
    );
}

#[test]
fn layout_frame_rust_captures_cursor_inside_invisible_text_without_rescan() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "abc hidden xyz";
    let hidden_byte_start = text.find("hidden").expect("hidden start");
    let hidden_byte_end = hidden_byte_start + "hidden".len();
    let hidden_char_start = text[..hidden_byte_start].chars().count() + 1;
    let point_pos = hidden_char_start + 2;
    let next_visible_pos = hidden_char_start + "hidden".chars().count();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point_pos - 1));
        buf.put_text_property(
            hidden_byte_start,
            hidden_byte_end,
            Value::symbol("invisible"),
            Value::T,
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-invisible-cursor", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(point_pos);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let cursor = snapshot.phys_cursor.as_ref().expect("cursor");
    let next_visible = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(next_visible_pos))
        .expect("next visible point");
    assert_eq!(cursor.x, next_visible.x);
    assert_eq!(cursor.row, next_visible.row);
    assert_eq!(cursor.col, next_visible.col);
}

#[test]
fn layout_frame_rust_preserves_logical_cursor_when_window_cursor_is_nil() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abcdef");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(2));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-logical-cursor-only", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(3);
        }
    }
    eval.frame_manager_mut()
        .set_window_cursor_type(selected_window, Value::NIL);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let logical_cursor = snapshot.logical_cursor.expect("logical cursor");
    let point = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("point snapshot");

    assert_eq!(snapshot.phys_cursor, None);
    assert_eq!(logical_cursor.x, point.x);
    assert_eq!(logical_cursor.row, point.row);
    assert_eq!(logical_cursor.col, point.col);
}

/// Lay out "abcXYZdef" with a `display` property replacing "XYZ" and point
/// inside the replacement, at the given default-face `:height`, then return
/// `(cursor.x, c.x + c.width, cursor.row, c.row)` where `c` is the glyph point
/// for buffer position 3 (the "c" immediately preceding the replacement slot).
///
/// Used by the font-size sweep below to prove the display-replacement cursor's
/// x stays byte-identical to the preceding glyph's already-rounded right edge
/// for EVERY font size (no ±1px double-rounding drift).
fn display_replacement_cursor_probe_at_height(height: i64) -> (i64, i64, i64, i64) {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "abcXYZdef";
    let repl_byte_start = text.find("XYZ").expect("replacement start");
    let repl_byte_end = repl_byte_start + "XYZ".len();
    let point_pos = repl_byte_start + 2;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point_pos - 1));
        buf.put_text_property(
            repl_byte_start,
            repl_byte_end,
            Value::symbol("display"),
            Value::string("R"),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-display-cursor-sweep", 800, 400, buf_id);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.install_gnu_gui_default_parameters();
    }
    assert!(eval.frame_manager_mut().select_frame(frame_id));
    let results = eval.eval_str_each(&format!(
        "(internal-set-lisp-face-attribute 'default :height {height} (selected-frame))"
    ));
    assert!(
        results.iter().all(Result::is_ok),
        "height {height}: default face height should realize, got {results:?}"
    );

    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(point_pos);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let cursor = snapshot
        .phys_cursor
        .as_ref()
        .unwrap_or_else(|| panic!("height {height}: cursor"));
    let c = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .unwrap_or_else(|| panic!("height {height}: c"));
    (cursor.x, c.x + c.width, cursor.row, c.row)
}

/// The display-replacement cursor's x is placed by deriving from the preceding
/// glyph's already-rounded x-position (like GNU `set_cursor_from_row` reading
/// the glyph matrix), NOT by independently rounding a sub-pixel accumulation.
/// Sweeping the font size exercises every sub-pixel fraction: a single-round
/// cursor `round(x+w)` diverges ±1px from the glyph edge `round(x)+round(w)`
/// for ~27% of sizes. This asserts zero drift across the whole sweep.
#[test]
fn layout_frame_rust_display_replacement_cursor_aligns_glyph_edge_across_font_sizes() {
    // Representative sweep: a dense low-range walk (40..=120, step 1) crosses
    // every sub-pixel rounding boundary in the band where the old single-round
    // cursor placement drifted — including the proven-broken sizes 44/51/59/74/96
    // — and a coarse tail samples larger sizes for breadth. The full 40..=300
    // step-1 sweep passes identically but is kept out of the permanent test for
    // speed.
    let mut heights: Vec<i64> = (40..=120).collect();
    heights.extend([130, 150, 175, 200, 240, 300]);
    for known in [44, 51, 59, 74, 96, 100] {
        assert!(heights.contains(&known));
    }
    let mut mismatches: Vec<(i64, i64, i64)> = Vec::new();
    let mut row_mismatches: Vec<(i64, i64, i64)> = Vec::new();
    for &height in &heights {
        let (cursor_x, glyph_edge, cursor_row, c_row) =
            display_replacement_cursor_probe_at_height(height);
        if cursor_x != glyph_edge {
            mismatches.push((height, cursor_x, glyph_edge));
        }
        if cursor_row != c_row {
            row_mismatches.push((height, cursor_row, c_row));
        }
    }
    assert!(
        mismatches.is_empty(),
        "cursor.x must equal the preceding glyph edge (c.x + c.width) for every \
         font size; {} of {} sizes drifted (height, cursor.x, glyph_edge): {:?}",
        mismatches.len(),
        heights.len(),
        &mismatches[..mismatches.len().min(12)],
    );
    assert!(
        row_mismatches.is_empty(),
        "cursor.row must equal c.row for every font size; drift: {:?}",
        &row_mismatches[..row_mismatches.len().min(12)],
    );
}

#[test]
fn layout_frame_rust_captures_cursor_at_display_replacement_slot_without_rescan() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "abcXYZdef";
    let repl_byte_start = text.find("XYZ").expect("replacement start");
    let repl_byte_end = repl_byte_start + "XYZ".len();
    let point_pos = repl_byte_start + 2;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point_pos - 1));
        buf.put_text_property(
            repl_byte_start,
            repl_byte_end,
            Value::symbol("display"),
            Value::string("R"),
        );
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-display-cursor", 800, 400, buf_id);
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(point_pos);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let cursor = snapshot.phys_cursor.as_ref().expect("cursor");
    let c = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("c");
    let d = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(7))
        .expect("d");
    assert_eq!(cursor.x, c.x + c.width);
    assert!(cursor.x < d.x, "cursor should target replacement slot");
    assert_eq!(cursor.row, c.row);
}

#[test]
fn layout_frame_rust_records_display_point_for_display_replacement_slot() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "abcXYZdef";
    let repl_byte_start = text.find("XYZ").expect("replacement start");
    let repl_byte_end = repl_byte_start + "XYZ".len();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.put_text_property(
            repl_byte_start,
            repl_byte_end,
            Value::symbol("display"),
            Value::string("R"),
        );
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-display-point", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let c = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("c");
    let replacement = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(4))
        .expect("replacement point");
    let d = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(7))
        .expect("d");

    assert_eq!(replacement.x, c.x + c.width);
    assert!(
        replacement.x < d.x,
        "replacement point should stay before following text"
    );
    assert!(replacement.width > 0);
    assert_eq!(replacement.row, c.row);
}

#[test]
fn layout_frame_rust_emits_display_string_replacement_glyphs() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("dir:");
        buf.put_text_property(
            3,
            4,
            Value::symbol("display"),
            Value::string(": (287 GiB available)"),
        );
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-display-string", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let window_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = window_entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");
    let rendered: String = text_row.glyphs[1]
        .iter()
        .filter_map(|glyph| match &glyph.glyph_type {
            GlyphType::Char { ch } => Some(*ch),
            GlyphType::Composite { text } => text.chars().next(),
            _ => None,
        })
        .collect();

    assert_eq!(rendered, "dir: (287 GiB available)");
}

#[test]
fn layout_frame_rust_renders_display_replacement_tabs_as_stretches() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("px");
        buf.put_text_property(1, 2, Value::symbol("display"), Value::string("a\tb"));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-display-tab-replacement", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    let logical_text = glyphs_logical_text(&text_row.glyphs[1]);
    assert!(
        !logical_text.contains('\t'),
        "display replacement tab should not render as a literal tab, row={:?}",
        text_row.glyphs[1]
    );
    assert!(
        logical_text.contains("pa      b"),
        "display replacement tab should expand to the next row tab stop, text={logical_text:?}"
    );
    assert!(
        text_row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { width_cols: 6 })),
        "display replacement tab should be a stretch glyph, row={:?}",
        text_row.glyphs[1]
    );
}

#[test]
fn layout_frame_rust_ignores_replacing_display_properties_inside_display_replacement_string() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("px");
        let replacement = Value::string_with_text_properties(
            "a b",
            vec![StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![
                    Value::symbol("display"),
                    Value::list(vec![
                        Value::symbol("space"),
                        Value::keyword(":width"),
                        Value::fixnum(3),
                    ]),
                ]),
            }],
        );
        buf.put_text_property(1, 2, Value::symbol("display"), replacement);
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-display-propertized-replacement",
        640,
        160,
        buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    let logical_text = glyphs_logical_text(&text_row.glyphs[1]);
    assert_eq!(
        logical_text, "pa b",
        "display replacement strings should ignore nested replacing display specs"
    );
    assert!(
        text_row.glyphs[1]
            .iter()
            .all(|glyph| !matches!(glyph.glyph_type, GlyphType::Stretch { width_cols: 3 })),
        "display replacement string display property should not produce a nested stretch, row={:?}",
        text_row.glyphs[1]
    );
}

#[test]
fn layout_frame_rust_honors_display_replacement_string_face_properties() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("px");
        let replacement = Value::string_with_text_properties(
            "ab",
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![
                    Value::symbol("face"),
                    Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]),
                ]),
            }],
        );
        buf.put_text_property(1, 2, Value::symbol("display"), replacement);
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-display-replacement-face", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    let a_face = text_row.glyphs[1]
        .iter()
        .find_map(|glyph| match glyph.glyph_type {
            GlyphType::Char { ch: 'a' } => Some(glyph.face_id),
            _ => None,
        })
        .expect("propertized replacement glyph face");
    let b_face = text_row.glyphs[1]
        .iter()
        .find_map(|glyph| match glyph.glyph_type {
            GlyphType::Char { ch: 'b' } => Some(glyph.face_id),
            _ => None,
        })
        .expect("plain replacement glyph face");

    assert_ne!(
        a_face, b_face,
        "replacement string face property should affect only its covered glyph, row={:?}",
        text_row.glyphs[1]
    );
}

#[test]
fn layout_frame_rust_emits_inline_image_glyphs_for_display_image_specs() {
    let mut eval = Context::new();
    let requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::clone(&requests),
        video_requests: Arc::new(Mutex::new(Vec::new())),
        webkit_requests: Arc::new(Mutex::new(Vec::new())),
        surface_requests: Arc::new(Mutex::new(Vec::new())),
    }));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "aXb";
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.put_text_property(
            1,
            2,
            Value::symbol("display"),
            Value::list(vec![
                Value::symbol("image"),
                Value::keyword("type"),
                Value::symbol("png"),
                Value::keyword("file"),
                Value::string("/tmp/neomacs-inline-image.png"),
                Value::keyword("max-width"),
                Value::fixnum(32),
                Value::keyword("max-height"),
                Value::fixnum(24),
                Value::keyword("foreground"),
                Value::string("#112233"),
                Value::keyword("background"),
                Value::string("red"),
            ]),
        );
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-inline-image", 320, 120, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state");
    let presentation = state.materialize();
    let (image_id, width, height, slot_id) = presentation
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            FrameGlyph::Image {
                image_id,
                width,
                height,
                slot_id,
                ..
            } => Some((*image_id, *width, *height, *slot_id)),
            _ => None,
        })
        .expect("inline image glyph");
    assert_eq!(image_id.get(), 77);
    assert_eq!(width, 32.0);
    assert_eq!(height, 24.0);
    let replacement = assert_replacement_slot_between_neighbors(&eval, frame_id, 2, 32);
    let slot_id = slot_id.expect("image slot id");
    assert_eq!(i64::from(slot_id.col), replacement.col);

    let requests = requests.lock().expect("requests lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].max_width, 32);
    assert_eq!(requests[0].max_height, 24);
    assert_eq!(requests[0].fg_color, 0x112233);
    assert_eq!(requests[0].bg_color, 0xff0000);
}

#[test]
fn layout_frame_rust_renders_display_image_fallback_placeholder_through_row_builder() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("aXb");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.put_text_property(
            1,
            2,
            Value::symbol("display"),
            Value::list(vec![
                Value::symbol("image"),
                Value::keyword("type"),
                Value::symbol("png"),
                Value::keyword("file"),
                Value::string("/tmp/neomacs-inline-image.png"),
            ]),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-inline-image-fallback", 320, 120, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| {
            entry.window_id.get()
                == eval
                    .frame_manager()
                    .get(frame_id)
                    .expect("frame")
                    .selected_window
                    .0 as i64
        })
        .expect("selected window matrix");
    assert!(
        enabled_window_row_texts(entry)
            .iter()
            .any(|row| row.contains("a[img]b")),
        "fallback placeholder should be rendered as row-builder text, rows={:?}",
        enabled_window_row_texts(entry)
    );

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let expected_width = (5.0 * frame.char_width).round() as i64;
    assert_replacement_slot_between_neighbors(&eval, frame_id, 2, expected_width);
}

#[test]
fn layout_frame_rust_emits_inline_video_glyphs_for_display_video_specs() {
    let mut eval = Context::new();
    let video_requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::new(Mutex::new(Vec::new())),
        video_requests: Arc::clone(&video_requests),
        webkit_requests: Arc::new(Mutex::new(Vec::new())),
        surface_requests: Arc::new(Mutex::new(Vec::new())),
    }));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("aVb");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.put_text_property(
            1,
            2,
            Value::symbol("display"),
            Value::list(vec![
                Value::symbol("video"),
                Value::keyword("file"),
                Value::string("/tmp/neomacs-inline-video.mp4"),
                Value::keyword("width"),
                Value::fixnum(80),
                Value::keyword("height"),
                Value::fixnum(45),
                Value::keyword("autoplay"),
                Value::T,
                Value::keyword("loop"),
                Value::T,
            ]),
        );
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-inline-video", 320, 120, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state");
    let presentation = state.materialize();
    let (video_id, width, height, loop_count, autoplay, slot_id) = presentation
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            FrameGlyph::Video {
                video_id,
                width,
                height,
                loop_count,
                autoplay,
                slot_id,
                ..
            } => Some((*video_id, *width, *height, *loop_count, *autoplay, *slot_id)),
            _ => None,
        })
        .expect("inline video glyph");
    assert_eq!(video_id.get(), 88);
    assert_eq!(width, 80.0);
    assert_eq!(height, 45.0);
    assert_eq!(loop_count, -1);
    assert!(autoplay);
    let replacement = assert_replacement_slot_between_neighbors(&eval, frame_id, 2, 80);
    let slot_id = slot_id.expect("video slot id");
    assert_eq!(i64::from(slot_id.col), replacement.col);

    let requests = video_requests.lock().expect("video requests lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].loop_count, -1);
    assert!(requests[0].autoplay);
}

#[test]
fn layout_frame_rust_emits_inline_webkit_glyphs_for_display_webkit_specs() {
    let mut eval = Context::new();
    let webkit_requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::new(Mutex::new(Vec::new())),
        video_requests: Arc::new(Mutex::new(Vec::new())),
        webkit_requests: Arc::clone(&webkit_requests),
        surface_requests: Arc::new(Mutex::new(Vec::new())),
    }));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("aWb");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.put_text_property(
            1,
            2,
            Value::symbol("display"),
            Value::list(vec![
                Value::symbol("webkit"),
                Value::keyword("uri"),
                Value::string("https://example.com"),
                Value::keyword("width"),
                Value::fixnum(80),
                Value::keyword("height"),
                Value::fixnum(45),
            ]),
        );
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-inline-webkit", 320, 120, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state");
    let presentation = state.materialize();
    let (xwidget_id, width, height, slot_id) = presentation
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            FrameGlyph::Xwidget {
                xwidget_id,
                width,
                height,
                slot_id,
                ..
            } => Some((*xwidget_id, *width, *height, *slot_id)),
            _ => None,
        })
        .expect("inline xwidget glyph");
    assert_eq!(xwidget_id.get(), 99);
    assert_eq!(width, 80.0);
    assert_eq!(height, 45.0);
    let replacement = assert_replacement_slot_between_neighbors(&eval, frame_id, 2, 80);
    let slot_id = slot_id.expect("webkit slot id");
    assert_eq!(i64::from(slot_id.col), replacement.col);

    let requests = webkit_requests.lock().expect("webkit requests lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].width, 80);
    assert_eq!(requests[0].height, 45);
}

#[test]
fn layout_frame_rust_emits_inline_xwidget_glyphs_for_gnu_display_xwidget_specs() {
    let mut eval = Context::new();
    let webkit_requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::new(Mutex::new(Vec::new())),
        video_requests: Arc::new(Mutex::new(Vec::new())),
        webkit_requests: Arc::clone(&webkit_requests),
        surface_requests: Arc::new(Mutex::new(Vec::new())),
    }));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let xwidget = Value::make_xwidget(
        Value::symbol("webkit"),
        Value::string("Title"),
        Value::make_buffer(buf_id),
        96,
        54,
        1234,
    );
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("aXb");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.put_text_property(
            1,
            2,
            Value::symbol("display"),
            Value::list(vec![
                Value::symbol("xwidget"),
                Value::keyword("xwidget"),
                xwidget,
            ]),
        );
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-inline-xwidget", 320, 120, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state");
    let presentation = state.materialize();
    let (xwidget_id, width, height, slot_id) = presentation
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            FrameGlyph::Xwidget {
                xwidget_id,
                width,
                height,
                slot_id,
                ..
            } => Some((*xwidget_id, *width, *height, *slot_id)),
            _ => None,
        })
        .expect("inline xwidget glyph");
    assert_eq!(xwidget_id.get(), 1234);
    assert_eq!(width, 96.0);
    assert_eq!(height, 54.0);
    let replacement = assert_replacement_slot_between_neighbors(&eval, frame_id, 2, 96);
    let slot_id = slot_id.expect("xwidget slot id");
    assert_eq!(i64::from(slot_id.col), replacement.col);

    let requests = webkit_requests.lock().expect("webkit requests lock");
    assert!(requests.is_empty());
}

#[test]
fn layout_frame_rust_emits_inline_surface_glyphs_for_display_surface_specs() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("aSb");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.put_text_property(
            1,
            2,
            Value::symbol("display"),
            Value::list(vec![
                Value::symbol("surface"),
                Value::keyword("id"),
                Value::fixnum(4242),
                Value::keyword("width"),
                Value::fixnum(80),
                Value::keyword("height"),
                Value::fixnum(45),
            ]),
        );
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-inline-surface", 320, 120, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state");
    let presentation = state.materialize();
    let (surface_id, width, height, slot_id) = presentation
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            FrameGlyph::Surface {
                surface_id,
                width,
                height,
                slot_id,
                ..
            } => Some((*surface_id, *width, *height, *slot_id)),
            _ => None,
        })
        .expect("inline surface glyph");
    assert_eq!(surface_id.get(), 4242);
    assert_eq!(width, 80.0);
    assert_eq!(height, 45.0);
    let replacement = assert_replacement_slot_between_neighbors(&eval, frame_id, 2, 80);
    let slot_id = slot_id.expect("surface slot id");
    assert_eq!(i64::from(slot_id.col), replacement.col);
}

/// Declarative form: `(surface :shader …)` with no Lisp-side id resolves
/// through the display host (memoized by spec content, the video pattern).
#[test]
fn layout_frame_rust_emits_inline_surface_glyphs_for_declarative_shader_specs() {
    let mut eval = Context::new();
    let surface_requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::new(Mutex::new(Vec::new())),
        video_requests: Arc::new(Mutex::new(Vec::new())),
        webkit_requests: Arc::new(Mutex::new(Vec::new())),
        surface_requests: Arc::clone(&surface_requests),
    }));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("aDb");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.put_text_property(
            1,
            2,
            Value::symbol("display"),
            Value::list(vec![
                Value::symbol("surface"),
                Value::keyword("shader"),
                Value::string(
                    "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { \
                     return vec4<f32>(1.0); }",
                ),
                Value::keyword("uniforms"),
                Value::list(vec![Value::cons(
                    Value::symbol("speed"),
                    Value::make_float(2.0),
                )]),
                Value::keyword("width"),
                Value::fixnum(80),
                Value::keyword("height"),
                Value::fixnum(45),
            ]),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-declarative-surface", 320, 120, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame display state");
    let presentation = state.materialize();
    let (surface_id, width, height, slot_id) = presentation
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            FrameGlyph::Surface {
                surface_id,
                width,
                height,
                slot_id,
                ..
            } => Some((*surface_id, *width, *height, *slot_id)),
            _ => None,
        })
        .expect("inline surface glyph");
    assert_eq!(surface_id.get(), 4343);
    assert_eq!(width, 80.0);
    assert_eq!(height, 45.0);
    let replacement = assert_replacement_slot_between_neighbors(&eval, frame_id, 2, 80);
    let slot_id = slot_id.expect("surface slot id");
    assert_eq!(i64::from(slot_id.col), replacement.col);

    let requests = surface_requests.lock().expect("surface requests lock");
    assert!(!requests.is_empty());
    let request = &requests[0];
    assert!(request.source.contains("mainImage"));
    assert_eq!(request.width, 80);
    assert_eq!(request.height, 45);
    assert!(request.animate);
    assert_eq!(
        request.uniforms,
        vec![("speed".to_owned(), [2.0f32.to_bits(), 0, 0, 0], 1u8)]
    );
}

/// `:channel0` in a declarative spec resolves image and video sources through
/// the display host into `(kind, cache id)` on the memoized request.
#[test]
fn declarative_surface_channel0_resolves_image_and_video_sources() {
    let mut eval = Context::new();
    let surface_requests = Arc::new(Mutex::new(Vec::new()));
    let video_requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::new(Mutex::new(Vec::new())),
        video_requests: Arc::clone(&video_requests),
        webkit_requests: Arc::new(Mutex::new(Vec::new())),
        surface_requests: Arc::clone(&surface_requests),
    }));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let shader = "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { \
                  return textureSample(iChannel0, iChannel0Sampler, \
                  fragCoord / u.iResolution.xy); }";
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("aVbIc");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        // V: video channel; I: image channel.
        buf.put_text_property(
            1,
            2,
            Value::symbol("display"),
            Value::list(vec![
                Value::symbol("surface"),
                Value::keyword("shader"),
                Value::string(shader),
                Value::keyword("channel0"),
                Value::list(vec![
                    Value::symbol("video"),
                    Value::keyword("file"),
                    Value::string("/tmp/neomacs-channel.mp4"),
                ]),
                Value::keyword("width"),
                Value::fixnum(60),
                Value::keyword("height"),
                Value::fixnum(40),
            ]),
        );
        buf.put_text_property(
            3,
            4,
            Value::symbol("display"),
            Value::list(vec![
                Value::symbol("surface"),
                Value::keyword("shader"),
                Value::string(shader),
                Value::keyword("channel0"),
                Value::list(vec![
                    Value::symbol("image"),
                    Value::keyword("type"),
                    Value::symbol("png"),
                    Value::keyword("file"),
                    Value::string("/tmp/neomacs-channel.png"),
                ]),
                Value::keyword("width"),
                Value::fixnum(60),
                Value::keyword("height"),
                Value::fixnum(40),
            ]),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-surface-channels", 320, 120, buf_id);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let requests = surface_requests.lock().expect("surface requests lock");
    let channels: Vec<_> = requests.iter().filter_map(|r| r.channel0).collect();
    assert!(
        channels.contains(&(SurfaceChannelKind::Video, 88)),
        "video channel resolved through the host (got {channels:?})"
    );
    assert!(
        channels.contains(&(SurfaceChannelKind::Image, 77)),
        "image channel resolved through the catalog (got {channels:?})"
    );
    // The video channel spec defaulted :autoplay to t.
    let video_requests = video_requests.lock().expect("video requests lock");
    assert!(video_requests.iter().any(|r| r.autoplay));
}

#[test]
fn layout_frame_rust_captures_cursor_inside_hscroll_skipped_text_without_rescan() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abcdef\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.set_buffer_local("truncate-lines", Value::T);
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-hscroll-cursor", 160, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            hscroll,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(2);
            *hscroll = 3;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let cursor = snapshot.phys_cursor.as_ref().expect("cursor");
    assert_eq!(cursor.x, 0);
    assert_eq!(cursor.row, 0);
    assert_eq!(cursor.col, 0);
}

fn assert_layout_frame_rust_tab_cursor_width(x_stretch_cursor: bool, cursor_type: Value) {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a\tb");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.set_buffer_local("cursor-type", cursor_type);
    }
    eval.set_variable(
        "x-stretch-cursor",
        if x_stretch_cursor {
            Value::T
        } else {
            Value::NIL
        },
    );

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-tab-cursor", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(2);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let cursor = snapshot.phys_cursor.as_ref().expect("cursor");
    let a = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("a");
    let b = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("b");
    let full_tab_slot_width = b.x - (a.x + a.width);
    let single_column_width = frame.char_width.round() as i64;

    assert_eq!(cursor.x, a.x + a.width);
    assert_eq!(cursor.row, a.row);
    assert_eq!(b.x - cursor.x, full_tab_slot_width);
    assert!(full_tab_slot_width > single_column_width);
    if x_stretch_cursor {
        assert_eq!(cursor.width, full_tab_slot_width);
    } else {
        assert_eq!(cursor.width, single_column_width);
    }
}

#[test]
fn layout_frame_rust_clamps_tab_cursor_width_when_x_stretch_cursor_is_nil() {
    assert_layout_frame_rust_tab_cursor_width(false, Value::T);
}

#[test]
fn layout_frame_rust_expands_tab_cursor_width_when_x_stretch_cursor_is_t() {
    assert_layout_frame_rust_tab_cursor_width(true, Value::T);
}

#[test]
fn layout_frame_rust_clamps_tab_hbar_cursor_width_when_x_stretch_cursor_is_nil() {
    assert_layout_frame_rust_tab_cursor_width(false, Value::symbol("hbar"));
}

#[test]
fn layout_frame_rust_expands_tab_hbar_cursor_width_when_x_stretch_cursor_is_t() {
    assert_layout_frame_rust_tab_cursor_width(true, Value::symbol("hbar"));
}

#[test]
fn layout_frame_rust_emits_buffer_tab_as_stretch_glyph() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a\tb");
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-tab-stretch", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let window_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = window_entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");
    let glyphs = &text_row.glyphs[1];

    assert!(matches!(
        glyphs.first().map(|glyph| &glyph.glyph_type),
        Some(GlyphType::Char { ch: 'a' })
    ));
    assert!(matches!(
        glyphs.get(1).map(|glyph| &glyph.glyph_type),
        Some(GlyphType::Stretch { width_cols: 7 })
    ));
    assert!(matches!(
        glyphs.get(2).map(|glyph| &glyph.glyph_type),
        Some(GlyphType::Char { ch: 'b' })
    ));
    assert_eq!(text_row.role, GlyphRowRole::Text);
    assert!(
        glyphs.iter().all(|glyph| glyph.pixel_width > 0.0),
        "main buffer text glyphs should keep pixel widths: {glyphs:?}"
    );
}

#[test]
fn layout_frame_rust_tab_stops_are_window_relative_in_split_windows() {
    let mut eval = Context::new();
    let left_buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let right_buf_id = eval.buffer_manager_mut().create_buffer("*right*");
    {
        let buf = eval
            .buffer_manager_mut()
            .get_mut(right_buf_id)
            .expect("right buffer");
        buf.insert("C-f\t;; forward-char");
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-tab-split", 800, 160, left_buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let right_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            selected_window,
            neovm_core::window::SplitDirection::Horizontal,
            right_buf_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let window_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == right_window.0 as i64)
        .expect("right window matrix");
    let text_row = window_entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");
    let text = text_row.glyphs[1]
        .iter()
        .flat_map(|glyph| match &glyph.glyph_type {
            GlyphType::Char { ch } => std::iter::repeat_n(*ch, 1).collect::<Vec<_>>(),
            GlyphType::Stretch { width_cols } => {
                std::iter::repeat_n(' ', usize::from(*width_cols)).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect::<String>();

    assert!(
        text.contains("C-f     ;; forward-char"),
        "right-window tab should expand relative to the right window text area, got {text:?}"
    );
}

#[test]
fn layout_frame_rust_display_space_align_keeps_suffix_text_in_split_windows() {
    let mut eval = Context::new();
    let left_buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let right_buf_id = eval
        .buffer_manager_mut()
        .create_buffer("*right-display-space*");
    let text = concat!(
        "   m \tShow help for current major and minor modes and their commands\n",
        "   b \tShow all key bindings\n",
        "   k \tShow help for key\n",
        "   c \tShow help for key briefly\n",
        "   w \tShow which key runs a specific command\n"
    );
    {
        let buf = eval
            .buffer_manager_mut()
            .get_mut(right_buf_id)
            .expect("right buffer");
        buf.insert(text);
        for (byte_idx, ch) in text.char_indices() {
            if ch == '\t' {
                buf.put_text_property(
                    byte_idx,
                    byte_idx + 1,
                    Value::symbol("display"),
                    Value::list(vec![
                        Value::symbol("space"),
                        Value::keyword(":align-to"),
                        Value::fixnum(8),
                    ]),
                );
            }
        }
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-display-space-align-split",
        800,
        160,
        left_buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let right_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            selected_window,
            neovm_core::window::SplitDirection::Horizontal,
            right_buf_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let window_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == right_window.0 as i64)
        .expect("right window matrix");
    let rows = enabled_window_row_texts_expanding_stretches(window_entry);

    assert!(
        rows.iter()
            .any(|row| row.contains("   c    Show help for key briefly")),
        "display-space align-to should preserve suffix text after the stretch, rows={rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("   w    Show which key runs a specific command")),
        "display-space align-to should not swallow following help rows, rows={rows:?}"
    );
}

#[test]
fn layout_frame_rust_tty_display_space_align_stays_one_cell_high() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = concat!(
        "   m \tShow help for current major and minor modes and their commands\n",
        "   b \tShow all key bindings\n",
        "   k \tShow help for key\n",
        "   c \tShow help for key briefly\n",
        "   w \tShow which key runs a specific command\n"
    );
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        for (byte_idx, ch) in text.char_indices() {
            if ch == '\t' {
                buf.put_text_property(
                    byte_idx,
                    byte_idx + 1,
                    Value::symbol("display"),
                    Value::list(vec![
                        Value::symbol("space"),
                        Value::keyword(":align-to"),
                        Value::fixnum(8),
                    ]),
                );
            }
        }
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-tty-display-space-align", 80, 25, buf_id);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(None);
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 16.0;
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window
        .0;
    let window_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window_id as i64)
        .expect("selected window matrix");
    let rows = enabled_window_row_texts_expanding_stretches(window_entry);

    assert!(
        rows.iter()
            .any(|row| row.contains("   w    Show which key runs a specific command")),
        "TTY display-space align-to should not inflate rows and hide later Help entries, rows={rows:?}"
    );

    for row in window_entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text && row.total_glyphs() > 0)
    {
        assert_eq!(
            row.height_px, 1.0,
            "TTY display-space rows must stay one cell high: row={row:?}"
        );
        assert!(
            row.ascent_px <= row.height_px,
            "TTY row ascent must not exceed row height: row={row:?}"
        );
    }
}

#[test]
fn layout_frame_rust_emits_pixel_window_divider_geometry() {
    let mut eval = Context::new();
    let left_buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let right_buf_id = eval.buffer_manager_mut().create_buffer("*right*");
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-divider-split", 800, 160, left_buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_parameter(Value::symbol("right-divider-width"), Value::fixnum(6));
    }
    eval.frame_manager_mut()
        .split_window(
            frame_id,
            selected_window,
            neovm_core::window::SplitDirection::Horizontal,
            right_buf_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");
    let left_bounds = {
        let frame = eval.frame_manager().get(frame_id).expect("frame");
        *frame
            .find_window(selected_window)
            .expect("left window")
            .bounds()
    };

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let divider_borders: Vec<_> = state
        .borders
        .iter()
        .filter(|border| {
            border.window_id.get() == selected_window.0 as i64
                && (border.x - (left_bounds.x + left_bounds.width - 6.0)).abs() <= 6.0
        })
        .collect();

    assert_eq!(
        divider_borders.len(),
        3,
        "a six-pixel right divider should be split into first/inner/last rectangles"
    );
    assert!(
        divider_borders.iter().any(|border| border.width == 1.0),
        "divider should include one-pixel edge rectangles"
    );
    assert!(
        divider_borders.iter().any(|border| border.width == 4.0),
        "divider should include a four-pixel inner rectangle"
    );

    let left_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("left window matrix");
    assert!(
        left_entry.matrix.rows.iter().all(|row| {
            row.glyphs[1]
                .last()
                .is_none_or(|glyph| !matches!(glyph.glyph_type, GlyphType::Char { ch: '|' }))
        }),
        "real pixel window dividers must not be represented as vertical-border text glyphs"
    );
}

#[test]
fn layout_frame_rust_gui_zero_width_divider_uses_pixel_vertical_border() {
    let mut eval = Context::new();
    let left_buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let right_buf_id = eval.buffer_manager_mut().create_buffer("*right*");
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-gui-border-split", 800, 160, left_buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    eval.frame_manager_mut()
        .split_window(
            frame_id,
            selected_window,
            neovm_core::window::SplitDirection::Horizontal,
            right_buf_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");
    let left_bounds = {
        let frame = eval.frame_manager().get(frame_id).expect("frame");
        *frame
            .find_window(selected_window)
            .expect("left window")
            .bounds()
    };

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    assert!(
        state.borders.iter().any(|border| {
            border.window_id.get() == selected_window.0 as i64
                && (border.x - (left_bounds.x + left_bounds.width - 1.0)).abs() < 0.01
                && border.width == 1.0
        }),
        "GNU GUI draws a one-pixel vertical border when window-divider-mode is off"
    );

    let left_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("left window matrix");
    assert!(
        left_entry.matrix.rows.iter().all(|row| {
            row.glyphs[1]
                .last()
                .is_none_or(|glyph| !matches!(glyph.glyph_type, GlyphType::Char { ch: '|' }))
        }),
        "GUI vertical borders must not be represented as terminal `|' glyphs"
    );
}

#[test]
fn layout_frame_rust_bottom_divider_does_not_separate_root_from_minibuffer() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-minibuffer-divider", 800, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_parameter(Value::symbol("bottom-divider-width"), Value::fixnum(6));
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    assert!(
        state.borders.iter().all(
            |border| border.window_id.get() != selected_window.0 as i64 || border.height != 6.0
        ),
        "GNU does not draw a bottom window divider between a bottommost root window and the minibuffer"
    );
}

#[test]
fn layout_frame_rust_emits_display_space_as_stretch_glyph() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "a b";
    let space_byte_start = text.find(' ').expect("space start");
    let space_byte_end = space_byte_start + 1;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.put_text_property(
            space_byte_start,
            space_byte_end,
            Value::symbol("display"),
            display_space_width_spec(4),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-display-space-stretch", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let window_entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = window_entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");
    let glyphs = &text_row.glyphs[1];

    assert!(matches!(
        glyphs.first().map(|glyph| &glyph.glyph_type),
        Some(GlyphType::Char { ch: 'a' })
    ));
    assert!(matches!(
        glyphs.get(1).map(|glyph| &glyph.glyph_type),
        Some(GlyphType::Stretch { width_cols: 4 })
    ));
    assert!(matches!(
        glyphs.get(2).map(|glyph| &glyph.glyph_type),
        Some(GlyphType::Char { ch: 'b' })
    ));
}

fn display_space_width_spec(columns: i64) -> Value {
    Value::list(vec![
        Value::symbol("space"),
        Value::keyword("width"),
        Value::fixnum(columns),
    ])
}

fn display_space_relative_width_spec(factor: i64) -> Value {
    Value::list(vec![
        Value::symbol("space"),
        Value::keyword("relative-width"),
        Value::fixnum(factor),
    ])
}

fn display_space_relative_height_spec(factor: i64, ascent_percent: i64) -> Value {
    Value::list(vec![
        Value::symbol("space"),
        Value::keyword("width"),
        Value::fixnum(2),
        Value::keyword("relative-height"),
        Value::fixnum(factor),
        Value::keyword("ascent"),
        Value::fixnum(ascent_percent),
    ])
}

#[test]
fn display_space_relative_width_uses_displayed_character_width() {
    let _eval = Context::new();
    let params = test_window_params();
    let geometry = DisplaySpaceGeometry::from_display_space_spec(
        &display_space_relative_width_spec(2),
        0.0,
        0.0,
        8.0,
        16.0,
        10.0,
        7.0,
        &params,
    );

    assert_eq!(geometry.width, 32.0);
}

#[test]
fn display_space_geometry_uses_relative_height_and_percent_ascent() {
    let _eval = Context::new();
    let params = test_window_params();
    let geometry = DisplaySpaceGeometry::from_display_space_spec(
        &display_space_relative_height_spec(2, 25),
        0.0,
        0.0,
        8.0,
        8.0,
        10.0,
        7.0,
        &params,
    );

    assert_eq!(
        geometry,
        DisplaySpaceGeometry {
            width: 16.0,
            height: 20.0,
            ascent: 5.0,
        }
    );
}

#[test]
fn display_space_geometry_accepts_pixel_ascent_expression() {
    let _eval = Context::new();
    let params = test_window_params();
    let spec = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("height"),
        Value::list(vec![Value::fixnum(20)]),
        Value::keyword("ascent"),
        Value::list(vec![Value::fixnum(3)]),
    ]);
    let geometry = DisplaySpaceGeometry::from_display_space_spec(
        &spec, 0.0, 0.0, 8.0, 8.0, 10.0, 7.0, &params,
    );

    assert_eq!(geometry.height, 20.0);
    assert_eq!(geometry.ascent, 3.0);
}

fn scaled_face_plist() -> Value {
    Value::list(vec![
        Value::keyword("family"),
        Value::string("JetBrains Mono"),
        Value::keyword("height"),
        Value::make_float(1.6),
        Value::keyword("weight"),
        Value::symbol("extra-bold"),
    ])
}

fn assert_layout_frame_rust_display_space_cursor_width(x_stretch_cursor: bool, cursor_type: Value) {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "a b";
    let space_byte_start = text.find(' ').expect("space start");
    let space_byte_end = space_byte_start + 1;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.put_text_property(
            space_byte_start,
            space_byte_end,
            Value::symbol("display"),
            display_space_width_spec(4),
        );
        buf.put_text_property(
            space_byte_start,
            space_byte_end,
            Value::symbol("face"),
            scaled_face_plist(),
        );
        buf.set_buffer_local("cursor-type", cursor_type);
    }
    eval.set_variable(
        "x-stretch-cursor",
        if x_stretch_cursor {
            Value::T
        } else {
            Value::NIL
        },
    );

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-display-space-cursor", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(2);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let cursor = snapshot.phys_cursor.as_ref().expect("cursor");
    let a = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("a");
    let b = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("b");
    let full_slot_width = b.x - (a.x + a.width);
    let single_column_width = frame.char_width.round() as i64;
    let expected_space_width = (4.0 * frame.char_width).round() as i64;

    assert_eq!(cursor.x, a.x + a.width);
    assert_eq!(b.x - cursor.x, full_slot_width);
    assert!((full_slot_width - expected_space_width).abs() <= 1);
    if x_stretch_cursor {
        assert_eq!(cursor.width, full_slot_width);
    } else {
        assert_eq!(cursor.width, single_column_width);
    }
}

#[test]
fn layout_frame_rust_display_space_width_uses_canonical_column_width() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "a b";
    let space_byte_start = text.find(' ').expect("space start");
    let space_byte_end = space_byte_start + 1;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(1));
        buf.put_text_property(
            space_byte_start,
            space_byte_end,
            Value::symbol("display"),
            display_space_width_spec(4),
        );
        buf.put_text_property(
            space_byte_start,
            space_byte_end,
            Value::symbol("face"),
            scaled_face_plist(),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-display-space-width", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf { window_start, .. } = window {
            *window_start = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let a = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("a");
    let b = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("b");
    let slot_width = b.x - (a.x + a.width);
    let expected_width = (4.0 * frame.char_width).round() as i64;

    assert!(
        (slot_width - expected_width).abs() <= 1,
        "display space width should follow canonical frame column width; got slot {slot_width}, expected {expected_width}, frame char width {}, points={:?}",
        frame.char_width,
        snapshot.points
    );
}

#[test]
fn layout_frame_rust_records_display_point_for_display_space_slot() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "a b";
    let space_byte_start = text.find(' ').expect("space start");
    let space_byte_end = space_byte_start + 1;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.put_text_property(
            space_byte_start,
            space_byte_end,
            Value::symbol("display"),
            display_space_width_spec(4),
        );
        buf.put_text_property(
            space_byte_start,
            space_byte_end,
            Value::symbol("face"),
            scaled_face_plist(),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-display-space-point", 320, 120, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let a = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("a");
    let space = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(2))
        .expect("space");
    let b = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(3))
        .expect("b");
    let expected_width = (4.0 * frame.char_width).round() as i64;

    assert_eq!(space.x, a.x + a.width);
    assert!(space.x < b.x);
    assert!((space.width - expected_width).abs() <= 1);
    assert_eq!(space.row, a.row);
}

#[test]
fn layout_frame_rust_clamps_display_space_cursor_width_when_x_stretch_cursor_is_nil() {
    assert_layout_frame_rust_display_space_cursor_width(false, Value::T);
}

#[test]
fn layout_frame_rust_expands_display_space_cursor_width_when_x_stretch_cursor_is_t() {
    assert_layout_frame_rust_display_space_cursor_width(true, Value::T);
}

#[test]
fn layout_frame_rust_clamps_display_space_hbar_cursor_width_when_x_stretch_cursor_is_nil() {
    assert_layout_frame_rust_display_space_cursor_width(false, Value::symbol("hbar"));
}

#[test]
fn layout_frame_rust_expands_display_space_hbar_cursor_width_when_x_stretch_cursor_is_t() {
    assert_layout_frame_rust_display_space_cursor_width(true, Value::symbol("hbar"));
}

#[test]
fn layout_frame_rust_keeps_mixed_width_advances_correct_after_mid_line_face_change() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();

    let prefix = "  h=0.9 w=normal:                     ";
    let sample = "a好好b  ABCXYZ 0123456789  -> <= >=";
    let sample_pos = prefix.chars().count() + 1;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(prefix);
        let sample_byte_start = buf.total_emacs_byte_len().get();
        buf.insert(sample);
        let sample_byte_end = buf.total_emacs_byte_len().get();
        let plist = Value::list(vec![
            Value::keyword("family"),
            Value::string("Noto Sans Mono"),
            Value::keyword("height"),
            Value::make_float(0.9),
            Value::keyword("weight"),
            Value::symbol("normal"),
        ]);
        buf.put_text_property(
            sample_byte_start,
            sample_byte_end,
            Value::symbol("face"),
            plist,
        );
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-face-mid-line", 1400, 160, buf_id);
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let all_points = snapshot.points.clone();
    let a = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(sample_pos))
        .expect("a");
    let hao1 = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(sample_pos + 1))
        .expect("first 好");
    let hao2 = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(sample_pos + 2))
        .expect("second 好");
    let b = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(sample_pos + 3))
        .expect("b");

    let face_font_size = frame.font_pixel_size * 0.9;
    let mut metrics = FontMetricsService::new();
    let expected_a = expected_gui_glyph_advance(
        &mut metrics,
        'a',
        "Noto Sans Mono",
        400,
        false,
        face_font_size,
    );
    let expected_hao = expected_gui_glyph_advance(
        &mut metrics,
        '好',
        "Noto Sans Mono",
        400,
        false,
        face_font_size,
    );
    let expected_b = expected_gui_glyph_advance(
        &mut metrics,
        'b',
        "Noto Sans Mono",
        400,
        false,
        face_font_size,
    );

    assert_point_width_matches_advance(a, expected_a, "a", &all_points);
    assert_point_width_matches_advance(hao1, expected_hao, "first 好", &all_points);
    assert_point_width_matches_advance(hao2, expected_hao, "second 好", &all_points);
    assert_point_width_matches_advance(b, expected_b, "b", &all_points);
    assert_point_delta_matches_advance(a, hao1, expected_a, "first 好", &all_points);
    assert_point_delta_matches_advance(hao1, hao2, expected_hao, "second 好", &all_points);
    assert_point_delta_matches_advance(hao2, b, expected_hao, "b", &all_points);
    let space = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(sample_pos + 4))
        .expect("space");
    assert!(
        ((space.x - b.x) as f32 - expected_b).abs() <= 1.0,
        "expected next point after 'b' to land near one logical advance later; b={b:?} space={space:?} points={all_points:?}"
    );
}

#[test]
fn layout_frame_rust_keeps_face_positions_after_truncated_multibyte_line() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();

    let truncated_prefix = format!("{}\n", "好".repeat(20));
    let sample = "a好好b";
    let sample_pos = truncated_prefix.chars().count() + 1;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(&truncated_prefix);
        let sample_byte_start = buf.total_emacs_byte_len().get();
        buf.insert(sample);
        let sample_byte_end = buf.total_emacs_byte_len().get();
        buf.insert("\n");
        let plist = Value::list(vec![
            Value::keyword("family"),
            Value::string("Noto Sans Mono"),
            Value::keyword("height"),
            Value::make_float(0.9),
            Value::keyword("weight"),
            Value::symbol("normal"),
        ]);
        buf.put_text_property(
            sample_byte_start,
            sample_byte_end,
            Value::symbol("face"),
            plist,
        );
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        buf.set_buffer_local("truncate-lines", Value::T);
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-truncated-multibyte-face", 128, 160, buf_id);
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(sample_pos);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let all_points = snapshot.points.clone();
    let a = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(sample_pos))
        .expect("a");
    let hao1 = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(sample_pos + 1))
        .expect("first 好");
    let hao2 = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(sample_pos + 2))
        .expect("second 好");
    let b = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(sample_pos + 3))
        .expect("b");

    let face_font_size = frame.font_pixel_size * 0.9;
    let mut metrics = FontMetricsService::new();
    let expected_a = expected_gui_glyph_advance(
        &mut metrics,
        'a',
        "Noto Sans Mono",
        400,
        false,
        face_font_size,
    );
    let expected_hao = expected_gui_glyph_advance(
        &mut metrics,
        '好',
        "Noto Sans Mono",
        400,
        false,
        face_font_size,
    );
    let expected_b = expected_gui_glyph_advance(
        &mut metrics,
        'b',
        "Noto Sans Mono",
        400,
        false,
        face_font_size,
    );

    assert_point_width_matches_advance(a, expected_a, "a", &all_points);
    assert_point_width_matches_advance(hao1, expected_hao, "first 好", &all_points);
    assert_point_width_matches_advance(hao2, expected_hao, "second 好", &all_points);
    assert_point_width_matches_advance(b, expected_b, "b", &all_points);
    assert_point_delta_matches_advance(a, hao1, expected_a, "first 好", &all_points);
    assert_point_delta_matches_advance(hao1, hao2, expected_hao, "second 好", &all_points);
    assert_point_delta_matches_advance(hao2, b, expected_hao, "b", &all_points);
}

#[test]
fn layout_frame_rust_keeps_mixed_width_positions_correct_after_sequential_window_point_moves() {
    #[derive(Clone, Copy, Debug)]
    struct TargetRow {
        line_beg: usize,
        sample_pos: usize,
        height: f32,
        weight: u16,
    }

    fn char_at_lisp_pos(buffer: &neovm_core::buffer::Buffer, pos: usize) -> Option<char> {
        if pos == 0 {
            return None;
        }
        let byte_pos = buffer
            .char_pos_to_emacs_byte_pos_clamped(neovm_core::buffer::CharPos0::new(pos - 1))
            .get();
        buffer.char_after_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(byte_pos))
    }

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let sample = "a好好b  ABCXYZ 0123456789  -> <= >=";
    let mut targets = Vec::new();
    let weights = [
        ("normal", 400_u16),
        ("semi-bold", 600_u16),
        ("bold", 700_u16),
        ("extra-bold", 800_u16),
    ];

    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        for height in [0.9_f32, 1.0_f32, 1.2_f32, 1.6_f32] {
            for (weight_name, weight_value) in weights {
                let line_beg = if buf.is_text_empty() {
                    1usize
                } else {
                    buf.point_max_char_pos().get() as usize + 1
                };
                let prefix = format!("  {:<35} ", format!("h={height} w={weight_name}:"));
                let sample_pos = line_beg + prefix.chars().count();
                buf.insert(&prefix);
                let sample_byte_start = buf.total_emacs_byte_len().get();
                buf.insert(sample);
                let sample_byte_end = buf.total_emacs_byte_len().get();
                buf.insert("\n");
                let plist = Value::list(vec![
                    Value::keyword("family"),
                    Value::string("JetBrains Mono"),
                    Value::keyword("height"),
                    Value::make_float(height as f64),
                    Value::keyword("weight"),
                    Value::symbol(weight_name),
                ]);
                buf.put_text_property(
                    sample_byte_start,
                    sample_byte_end,
                    Value::symbol("face"),
                    plist,
                );
                targets.push(TargetRow {
                    line_beg,
                    sample_pos,
                    height,
                    weight: weight_value,
                });
            }
        }
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-sequential-window-point", 1400, 256, buf_id);
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    let mut metrics = FontMetricsService::new();

    for target in &targets {
        let byte_pos = {
            let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
            buffer
                .lisp_pos_to_emacs_byte_pos(LispCharPos1::from_one_based_usize(target.line_beg))
                .get()
        };
        let _ = eval
            .buffer_manager_mut()
            .goto_buffer_emacs_byte_pos(buf_id, neovm_core::buffer::EmacsBytePos::new(byte_pos));
        {
            let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
            let window = frame
                .find_window_mut(selected_window)
                .expect("selected window");
            if let neovm_core::window::Window::Leaf { point, .. } = window {
                *point = LispCharPos1::from_one_based_usize(target.line_beg);
            }
        }

        engine.layout_frame_rust(&mut eval, frame_id);

        let frame = eval.frame_manager().get(frame_id).expect("frame");
        let snapshot = frame
            .redisplay_snapshot(selected_window)
            .expect("display snapshot");
        let all_points = snapshot.points.clone();
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        let sample_chars = [
            (target.line_beg, char_at_lisp_pos(buffer, target.line_beg)),
            (
                target.sample_pos,
                char_at_lisp_pos(buffer, target.sample_pos),
            ),
            (
                target.sample_pos + 1,
                char_at_lisp_pos(buffer, target.sample_pos + 1),
            ),
            (
                target.sample_pos + 2,
                char_at_lisp_pos(buffer, target.sample_pos + 2),
            ),
            (
                target.sample_pos + 3,
                char_at_lisp_pos(buffer, target.sample_pos + 3),
            ),
        ];
        let a = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos))
            .expect("sample a");
        let hao1 = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos + 1))
            .expect("sample first 好");
        let hao2 = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos + 2))
            .expect("sample second 好");
        let b = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos + 3))
            .expect("sample b");
        let after_b = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos + 4))
            .expect("sample trailing space");

        let face_font_size = frame.font_pixel_size * target.height;
        let expected_a = expected_gui_glyph_advance(
            &mut metrics,
            'a',
            "JetBrains Mono",
            target.weight,
            false,
            face_font_size,
        );
        let expected_hao = expected_gui_glyph_advance(
            &mut metrics,
            '好',
            "JetBrains Mono",
            target.weight,
            false,
            face_font_size,
        );
        let expected_b = expected_gui_glyph_advance(
            &mut metrics,
            'b',
            "JetBrains Mono",
            target.weight,
            false,
            face_font_size,
        );

        assert_point_width_matches_advance(a, expected_a, "sequential a", &all_points);
        assert_point_width_matches_advance(hao1, expected_hao, "sequential first 好", &all_points);
        assert_point_width_matches_advance(hao2, expected_hao, "sequential second 好", &all_points);
        assert_point_width_matches_advance(b, expected_b, "sequential b", &all_points);
        assert_point_delta_matches_advance(a, hao1, expected_a, "sequential first 好", &all_points);
        assert_point_delta_matches_advance(
            hao1,
            hao2,
            expected_hao,
            "sequential second 好",
            &all_points,
        );
        assert_point_delta_matches_advance(hao2, b, expected_hao, "sequential b", &all_points);
        assert_point_delta_matches_advance(
            b,
            after_b,
            expected_b,
            "sequential after b",
            &all_points,
        );

        let _ = sample_chars;
    }
}

#[test]
fn layout_frame_rust_keeps_mixed_width_positions_correct_across_family_switches() {
    #[derive(Clone, Copy, Debug)]
    struct TargetRow<'a> {
        family: &'a str,
        line_beg: usize,
        sample_pos: usize,
        height: f32,
        weight_name: &'a str,
        weight: u16,
    }

    fn char_at_lisp_pos(buffer: &neovm_core::buffer::Buffer, pos: usize) -> Option<char> {
        if pos == 0 {
            return None;
        }
        let byte_pos = buffer
            .char_pos_to_emacs_byte_pos_clamped(neovm_core::buffer::CharPos0::new(pos - 1))
            .get();
        buffer.char_after_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(byte_pos))
    }

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let sample = "a好好b  ABCXYZ 0123456789  -> <= >=";
    let mut targets = Vec::new();
    let weights = [
        ("normal", 400_u16),
        ("semi-bold", 600_u16),
        ("bold", 700_u16),
        ("extra-bold", 800_u16),
    ];
    let families = [
        "JetBrains Mono",
        "Hack",
        "DejaVu Sans Mono",
        "Noto Sans Mono",
    ];

    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        for family in families {
            let heading = format!("  -- family: {family} --\n");
            buf.insert(&heading);
            for height in [0.9_f32, 1.0_f32, 1.2_f32, 1.6_f32] {
                for (weight_name, weight_value) in weights {
                    let line_beg = if buf.is_text_empty() {
                        1usize
                    } else {
                        buf.point_max_char_pos().get() as usize + 1
                    };
                    let prefix = format!("  {:<35} ", format!("h={height} w={weight_name}:"));
                    let sample_pos = line_beg + prefix.chars().count();
                    buf.insert(&prefix);
                    let sample_byte_start = buf.total_emacs_byte_len().get();
                    buf.insert(sample);
                    let sample_byte_end = buf.total_emacs_byte_len().get();
                    buf.insert("\n");
                    let plist = Value::list(vec![
                        Value::keyword("family"),
                        Value::string(family),
                        Value::keyword("height"),
                        Value::make_float(height as f64),
                        Value::keyword("weight"),
                        Value::symbol(weight_name),
                    ]);
                    buf.put_text_property(
                        sample_byte_start,
                        sample_byte_end,
                        Value::symbol("face"),
                        plist,
                    );
                    targets.push(TargetRow {
                        family,
                        line_beg,
                        sample_pos,
                        height,
                        weight_name,
                        weight: weight_value,
                    });
                }
            }
            buf.insert("\n");
        }
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-family-switches", 1400, 1600, buf_id);
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    let mut metrics = FontMetricsService::new();

    for target in &targets {
        let byte_pos = {
            let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
            buffer
                .lisp_pos_to_emacs_byte_pos(LispCharPos1::from_one_based_usize(target.line_beg))
                .get()
        };
        let _ = eval
            .buffer_manager_mut()
            .goto_buffer_emacs_byte_pos(buf_id, neovm_core::buffer::EmacsBytePos::new(byte_pos));
        {
            let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
            let window = frame
                .find_window_mut(selected_window)
                .expect("selected window");
            if let neovm_core::window::Window::Leaf { point, .. } = window {
                *point = LispCharPos1::from_one_based_usize(target.line_beg);
            }
        }

        engine.layout_frame_rust(&mut eval, frame_id);

        let frame = eval.frame_manager().get(frame_id).expect("frame");
        let snapshot = frame
            .redisplay_snapshot(selected_window)
            .expect("display snapshot");
        let all_points = snapshot.points.clone();
        let visible_span = snapshot.visible_buffer_span();
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        let sample_chars = [
            (
                target.sample_pos,
                char_at_lisp_pos(buffer, target.sample_pos),
            ),
            (
                target.sample_pos + 1,
                char_at_lisp_pos(buffer, target.sample_pos + 1),
            ),
            (
                target.sample_pos + 2,
                char_at_lisp_pos(buffer, target.sample_pos + 2),
            ),
            (
                target.sample_pos + 3,
                char_at_lisp_pos(buffer, target.sample_pos + 3),
            ),
        ];
        let a = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos))
            .unwrap_or_else(|| {
                panic!(
                    "sample a missing; target={target:?}; visible_span={visible_span:?}; chars={sample_chars:?}; points={all_points:?}"
                )
            });
        let hao1 = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos + 1))
            .unwrap_or_else(|| {
                panic!(
                    "sample first 好 missing; target={target:?}; visible_span={visible_span:?}; chars={sample_chars:?}; points={all_points:?}"
                )
            });
        let hao2 = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos + 2))
            .unwrap_or_else(|| {
                panic!(
                    "sample second 好 missing; target={target:?}; visible_span={visible_span:?}; chars={sample_chars:?}; points={all_points:?}"
                )
            });
        let b = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos + 3))
            .unwrap_or_else(|| {
                panic!(
                    "sample b missing; target={target:?}; visible_span={visible_span:?}; chars={sample_chars:?}; points={all_points:?}"
                )
            });
        let after_b = snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target.sample_pos + 4))
            .unwrap_or_else(|| {
                panic!(
                    "sample trailing space missing; target={target:?}; visible_span={visible_span:?}; chars={sample_chars:?}; points={all_points:?}"
                )
            });

        let face_font_size = frame.font_pixel_size * target.height;
        let expected_a = expected_gui_glyph_advance(
            &mut metrics,
            'a',
            target.family,
            target.weight,
            false,
            face_font_size,
        );
        let expected_hao = expected_gui_glyph_advance(
            &mut metrics,
            '好',
            target.family,
            target.weight,
            false,
            face_font_size,
        );
        let expected_b = expected_gui_glyph_advance(
            &mut metrics,
            'b',
            target.family,
            target.weight,
            false,
            face_font_size,
        );

        assert_point_width_matches_advance(a, expected_a, "family-switch a", &all_points);
        assert_point_width_matches_advance(
            hao1,
            expected_hao,
            "family-switch first 好",
            &all_points,
        );
        assert_point_width_matches_advance(
            hao2,
            expected_hao,
            "family-switch second 好",
            &all_points,
        );
        assert_point_width_matches_advance(b, expected_b, "family-switch b", &all_points);
        assert_point_delta_matches_advance(
            a,
            hao1,
            expected_a,
            "family-switch first 好",
            &all_points,
        );
        assert_point_delta_matches_advance(
            hao1,
            hao2,
            expected_hao,
            "family-switch second 好",
            &all_points,
        );
        assert_point_delta_matches_advance(hao2, b, expected_hao, "family-switch b", &all_points);
        assert_point_delta_matches_advance(
            b,
            after_b,
            expected_b,
            "family-switch after b",
            &all_points,
        );

        let _ = sample_chars;
        let _ = target.weight_name;
    }
}

#[test]
fn layout_frame_rust_word_wrap_snapshot_stays_sorted_after_rewind() {
    fn char_at_lisp_pos(buffer: &neovm_core::buffer::Buffer, pos: usize) -> Option<char> {
        if pos == 0 {
            return None;
        }
        let byte_pos = buffer
            .char_pos_to_emacs_byte_pos_clamped(neovm_core::buffer::CharPos0::new(pos - 1))
            .get();
        buffer.char_after_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(byte_pos))
    }

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("aaaa bbbb cccc dddd\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        buf.set_buffer_local("word-wrap", Value::T);
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-wrap", 96, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    assert!(
        snapshot.points.iter().any(|point| point.row > 0),
        "expected word-wrap to create multiple rows, got points={:?}",
        snapshot.points
    );
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let point_chars = snapshot
        .points
        .iter()
        .map(|point| {
            (
                point.buffer_pos,
                char_at_lisp_pos(buffer, point.buffer_pos.to_one_based_usize()),
            )
        })
        .collect::<Vec<_>>();
    for window in snapshot.points.windows(2) {
        assert!(
            window[0].buffer_pos < window[1].buffer_pos,
            "expected snapshot points to stay sorted after wrap rewind, got {:?}; chars={:?}",
            snapshot.points,
            point_chars
        );
    }
}

#[test]
fn layout_frame_rust_reads_far_enough_for_last_visible_truncated_line() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let mut text = String::new();
    for line in 0..32 {
        text.push_str(&format!("line-{line:02} abcdefghijklmnop\n"));
    }
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(&text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        buf.set_buffer_local("truncate-lines", Value::T);
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-read-span", 96, 640, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let target_pos = {
        let mut pos = 1usize;
        for line in 0..26 {
            pos += format!("line-{line:02} abcdefghijklmnop\n").chars().count();
        }
        pos
    };
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        // Selected-window point lives in the buffer; keep pt_char in
        // sync with the target point so redisplay retries read the same
        // location the leaf window advertises.
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(target_pos - 1));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(target_pos);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let target = snapshot.point_for_buffer_pos(LispCharPos1::from_one_based_usize(target_pos));
    assert!(
        target.is_some(),
        "expected last visible truncated line to remain readable by layout, target_pos={target_pos}, points={:?}",
        snapshot.points
    );
}

#[test]
fn layout_frame_rust_retries_window_when_point_starts_below_visible_span() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let lines = (0..40)
        .map(|line| format!("line-{line:02}\n"))
        .collect::<Vec<_>>();
    let text = lines.join("");
    let target_pos = lines
        .iter()
        .take(20)
        .map(|line| line.chars().count())
        .sum::<usize>()
        + 1;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(&text);
        // Selected-window point lives in the buffer; see
        // window.c:window_point. Set buffer pt_char to
        // target_pos so window_params_from_neovm reads it as
        // params.point.
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(target_pos - 1));
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-retry", 160, 192, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(target_pos);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let window = frame.find_window(selected_window).expect("selected window");

    assert!(
        snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target_pos))
            .is_some(),
        "expected retried layout to publish geometry for point {target_pos}, points={:?}",
        snapshot.points
    );
    match window {
        neovm_core::window::Window::Leaf { window_start, .. } => {
            assert!(
                *window_start > LispCharPos1::ONE,
                "expected window-start to advance after retry, got {window_start:?}"
            );
        }
        other => panic!("expected leaf window, got {other:?}"),
    }
}

#[test]
fn next_window_start_from_visible_rows_uses_visual_row_boundaries() {
    let rows = vec![
        DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(1)),
            end_buffer_pos: Some(LispCharPos1::new(8)),
        },
        DisplayRowSnapshot {
            row: 1,
            y: 16,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(9)),
            end_buffer_pos: Some(LispCharPos1::new(16)),
        },
        DisplayRowSnapshot {
            row: 2,
            y: 32,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(17)),
            end_buffer_pos: Some(LispCharPos1::new(24)),
        },
        DisplayRowSnapshot {
            row: 3,
            y: 48,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(25)),
            end_buffer_pos: Some(LispCharPos1::new(32)),
        },
    ];

    assert_eq!(
        next_window_start_from_visible_rows(&rows, 1),
        Some(32),
        "expected retry to advance to the next internal 0-based char position after the last visible row"
    );
    assert_eq!(
        next_window_start_from_visible_rows(&rows, 25),
        Some(32),
        "expected retry to keep the furthest internal 0-based visible progress that still advances"
    );
    assert_eq!(
        next_window_start_from_visible_rows(&rows, 33),
        None,
        "expected no retry candidate once the rendered span no longer advances"
    );
}

#[test]
fn next_window_start_for_partially_visible_point_row_scrolls_enough_to_fit_row() {
    let rows = vec![
        DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 20,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(1)),
            end_buffer_pos: Some(LispCharPos1::new(10)),
        },
        DisplayRowSnapshot {
            row: 1,
            y: 20,
            height: 20,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(11)),
            end_buffer_pos: Some(LispCharPos1::new(20)),
        },
        DisplayRowSnapshot {
            row: 2,
            y: 40,
            height: 30,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(21)),
            end_buffer_pos: Some(LispCharPos1::new(30)),
        },
    ];

    assert_eq!(
        next_window_start_for_partially_visible_point_row(&rows, 25, 0, 60, 1),
        Some(10),
        "expected retry to scroll away enough top rows to fit the point row using the next internal 0-based char position"
    );
    assert_eq!(
        next_window_start_for_partially_visible_point_row(&rows, 15, 0, 60, 1),
        None,
        "expected no retry when the point row is already fully visible"
    );
}

#[test]
fn next_window_start_for_point_line_continuation_advances_last_visible_row() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer_size = {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abcdefghijklmnopqrstuvwxyz\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        buf.point_max_char_pos().get() as i64
    };
    let access = {
        let buf = eval.buffer_manager().get(buf_id).expect("buffer");
        RustBufferAccess::new(buf)
    };
    let rows = vec![
        DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(1)),
            end_buffer_pos: Some(LispCharPos1::new(10)),
        },
        DisplayRowSnapshot {
            row: 1,
            y: 16,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(11)),
            end_buffer_pos: Some(LispCharPos1::new(20)),
        },
        DisplayRowSnapshot {
            row: 2,
            y: 32,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(21)),
            end_buffer_pos: Some(LispCharPos1::new(25)),
        },
    ];

    assert_eq!(
        next_window_start_for_point_line_continuation(&rows, 21, 1, &access, buffer_size),
        Some(20),
        "expected retry to move point toward the top when the visible point row continues below the window"
    );

    let terminated_rows = vec![
        DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(1)),
            end_buffer_pos: Some(LispCharPos1::new(10)),
        },
        DisplayRowSnapshot {
            row: 1,
            y: 16,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(11)),
            end_buffer_pos: Some(LispCharPos1::new(27)),
        },
    ];
    assert_eq!(
        next_window_start_for_point_line_continuation(
            &terminated_rows,
            11,
            1,
            &access,
            buffer_size
        ),
        None,
        "expected no retry once the final visible row already reaches the newline"
    );
}

#[test]
fn next_window_start_for_point_line_continuation_ignores_newline_terminated_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer_size = {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("needle target\nfiller line 06\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        buf.point_max_char_pos().get() as i64
    };
    let access = {
        let buf = eval.buffer_manager().get(buf_id).expect("buffer");
        RustBufferAccess::new(buf)
    };
    let rows = vec![DisplayRowSnapshot {
        row: 0,
        y: 0,
        height: 16,
        start_x: 0,
        start_col: 0,
        end_x: 0,
        end_col: 0,
        start_buffer_pos: Some(LispCharPos1::new(1)),
        end_buffer_pos: Some(LispCharPos1::new(14)),
    }];

    assert_eq!(
        next_window_start_for_point_line_continuation(&rows, 0, 0, &access, buffer_size),
        None,
        "expected no retry when the last visible row ended on a real newline"
    );
}

#[test]
fn next_window_start_for_point_line_continuation_ignores_tail_clipping_when_point_row_is_not_last_visible_row()
 {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer_size = {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\n");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        buf.point_max_char_pos().get() as i64
    };
    let access = {
        let buf = eval.buffer_manager().get(buf_id).expect("buffer");
        RustBufferAccess::new(buf)
    };
    let rows = vec![
        DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(1)),
            end_buffer_pos: Some(LispCharPos1::new(10)),
        },
        DisplayRowSnapshot {
            row: 1,
            y: 16,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(11)),
            end_buffer_pos: Some(LispCharPos1::new(20)),
        },
        DisplayRowSnapshot {
            row: 2,
            y: 32,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(21)),
            end_buffer_pos: Some(LispCharPos1::new(30)),
        },
        DisplayRowSnapshot {
            row: 3,
            y: 48,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(31)),
            end_buffer_pos: Some(LispCharPos1::new(40)),
        },
        DisplayRowSnapshot {
            row: 4,
            y: 64,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(LispCharPos1::new(41)),
            end_buffer_pos: Some(LispCharPos1::new(50)),
        },
    ];

    assert_eq!(
        next_window_start_for_point_line_continuation(&rows, 21, 1, &access, buffer_size),
        None,
        "expected no retry here because the point row is not the final visible row; partially visible rows are handled by the separate point-row retry path"
    );
}

#[test]
fn display_row_measurement_face_distinguishes_semantic_font_identity() {
    let mut font_metrics_svc = Some(FontMetricsService::new());
    let mut regular = crate::neovm_bridge::ResolvedFace::default();
    regular.font_family = "monospace".to_string();
    regular.font_size = 14.0;
    regular.font_weight = 400;
    regular.set_measured_char_width_px(8.0);
    let mut bold = regular.clone();
    bold.font_weight = 700;
    let measurement_policy = DisplayRowMeasurementPolicy::for_frame(true);
    let regular_face = measurement_policy.measurement_face(FaceId::new(42), &regular, None, 8.0);
    let bold_face = measurement_policy.measurement_face(FaceId::new(43), &bold, None, 8.0);

    let regular_width = regular_face.advance_for_char(&mut font_metrics_svc, 'A', 8.0);
    let bold_width = bold_face.advance_for_char(&mut font_metrics_svc, 'A', 8.0);
    let repeated_regular_width = regular_face.advance_for_char(&mut font_metrics_svc, 'A', 8.0);

    assert!(
        regular_width > 0.0,
        "expected measurable width for regular ASCII glyph"
    );
    assert!(
        bold_width > 0.0,
        "expected measurable width for bold ASCII glyph"
    );
    assert_eq!(
        repeated_regular_width, regular_width,
        "expected repeated measurement for the same semantic font spec to be stable"
    );
}

#[test]
fn display_row_measurement_face_preserves_fractional_gui_cell_width_without_font_metrics() {
    let mut resolved = crate::neovm_bridge::ResolvedFace::default();
    resolved.font_family = "JetBrainsMono Nerd Font".to_string();
    resolved.font_size = 12.0;
    resolved.set_measured_char_width_px(7.2);
    let current_face = DisplayRowMeasurementPolicy::for_frame(true).measurement_face(
        FaceId::new(42),
        &resolved,
        None,
        7.2,
    );
    let mut font_metrics_svc = None;

    let width = current_face.advance_for_char(&mut font_metrics_svc, 'x', 7.2);

    assert_eq!(width, 7.2);
}

#[test]
fn display_row_glyph_measurer_is_reusable_for_engine_measurements() {
    let mut resolved = crate::neovm_bridge::ResolvedFace::default();
    resolved.font_family = "monospace".to_string();
    resolved.font_size = 14.0;
    resolved.set_measured_char_width_px(8.0);
    let faces = [DisplayRowFace::from_resolved(FaceId::new(42), &resolved)];
    let mut font_metrics_svc = None;
    let mut measurer = DisplayRowGlyphMeasurer::new(&faces, font_metrics_svc.as_mut(), 7.2);

    let width = measurer
        .glyph_advance_px('x', FaceId::new(42), 1, 7.2)
        .expect("measure known face");

    assert_eq!(width, 8.0);
}

#[test]
fn display_row_glyph_measurement_face_carries_engine_measurement_policy() {
    let mut resolved = crate::neovm_bridge::ResolvedFace::default();
    resolved.font_family = "monospace".to_string();
    resolved.font_size = 14.0;
    resolved.set_measured_char_width_px(7.2);
    let current_face = DisplayRowMeasurementPolicy::for_frame(false).measurement_face(
        FaceId::new(42),
        &resolved,
        None,
        7.2,
    );
    let mut font_metrics_svc = None;

    let width = current_face.glyph_advance_px(&mut font_metrics_svc, 'x', 1, 7.2);

    assert_eq!(width, 7.0);
}

#[test]
fn layout_frame_rust_converges_visibility_for_wrapped_rows_in_one_redisplay() {
    fn char_at_lisp_pos(buffer: &neovm_core::buffer::Buffer, pos: usize) -> Option<char> {
        if pos == 0 {
            return None;
        }
        let byte_pos = buffer
            .char_pos_to_emacs_byte_pos_clamped(neovm_core::buffer::CharPos0::new(pos - 1))
            .get();
        buffer.char_after_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(byte_pos))
    }

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let logical_lines = (0..24)
        .map(|line| format!("line-{line:02} abcdefghijklmno\n"))
        .collect::<Vec<_>>();
    let text = logical_lines.join("");
    let target_pos = logical_lines
        .iter()
        .take(18)
        .map(|line| line.chars().count())
        .sum::<usize>()
        + 1;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(&text);
        // Move the buffer point to target_pos so the selected
        // window reads it as params.point (GNU
        // window.c:window_point says selected windows use
        // BUF_PT, not pointm). Without this, the Window::point
        // assignment below would be shadowed by buffer.pt_char
        // during window_params_from_neovm and layout would
        // never see the target.
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(target_pos - 1));
        buf.set_buffer_local("word-wrap", Value::T);
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-wrap-retry", 80, 192, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::from_one_based_usize(target_pos);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let window = frame.find_window(selected_window).expect("selected window");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let point_chars = snapshot
        .points
        .iter()
        .map(|point| {
            (
                point.buffer_pos,
                char_at_lisp_pos(buffer, point.buffer_pos.to_one_based_usize()),
            )
        })
        .collect::<Vec<_>>();

    assert!(
        snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(target_pos))
            .is_some(),
        "expected wrapped-line redisplay to converge on point {target_pos}, points={:?}, rows={:?}, chars={:?}",
        snapshot.points,
        snapshot.rows,
        point_chars
    );
    match window {
        neovm_core::window::Window::Leaf { window_start, .. } => {
            assert!(
                *window_start > LispCharPos1::ONE,
                "expected window-start to advance for wrapped redisplay, got {window_start:?}"
            );
        }
        other => panic!("expected leaf window, got {other:?}"),
    }
}

#[test]
fn layout_frame_rust_converges_visibility_for_point_line_tail_clipping() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let prefix = (0..2)
        .map(|line| format!("p{line:02}\n"))
        .collect::<Vec<_>>()
        .join("");
    let target_line = "abcdefghijklmno\n";
    let text = format!("{prefix}{target_line}");
    let point = prefix.chars().count() + 1;
    let later_pos = point + 10;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(&text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        buf.set_buffer_local("word-wrap", Value::T);
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-point-line-tail", 80, 256, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point: window_point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *window_point = LispCharPos1::from_one_based_usize(point);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    assert!(
        snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(later_pos))
            .is_some(),
        "expected redisplay to publish later positions from the point line after retry, points={:?}, rows={:?}",
        snapshot.points,
        snapshot.rows
    );
}

#[test]
fn layout_frame_rust_keeps_visible_eob_cursor_on_short_trailing_newline_buffer() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "LEFT WINDOW\nLine 2\nLine 3\n";
    let point = {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
        buf.point_max_char_pos().get() + 1
    };
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-eob-visible", 320, 640, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point: window_point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *window_point = LispCharPos1::from_one_based_usize(point);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let window = frame.find_window(selected_window).expect("selected window");

    assert!(
        snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
            .is_some(),
        "expected first line to remain visible when EOB cursor is already onscreen, points={:?}, rows={:?}",
        snapshot.points,
        snapshot.rows
    );
    match window {
        neovm_core::window::Window::Leaf { window_start, .. } => {
            assert_eq!(
                *window_start,
                LispCharPos1::ONE,
                "expected visible EOB cursor not to force a retry scroll"
            );
        }
        other => panic!("expected leaf window, got {other:?}"),
    }
}

#[test]
fn layout_frame_rust_keeps_default_scratch_message_at_top_when_eob_is_visible() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = ";; This buffer is for text that is not saved, and for Lisp evaluation.\n\
;; To create a file, visit it with \u{2018}C-x C-f\u{2019} and enter text in its buffer.\n\n";
    let point = {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(text);
        let point = buf.point_max_char_pos().get() + 1;
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point - 1));
        point
    };
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-scratch-eob-visible", 600, 1188, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point: window_point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *window_point = LispCharPos1::from_one_based_usize(point);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let window = frame.find_window(selected_window).expect("selected window");

    assert!(
        snapshot
            .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
            .is_some(),
        "expected the first scratch row to remain visible when EOB fits onscreen, points={:?}, rows={:?}",
        snapshot.points,
        snapshot.rows
    );
    match window {
        neovm_core::window::Window::Leaf { window_start, .. } => {
            assert_eq!(
                *window_start,
                LispCharPos1::ONE,
                "expected short scratch buffer to stay at top, got window-start {window_start:?}"
            );
        }
        other => panic!("expected leaf window, got {other:?}"),
    }
}

#[test]
fn layout_frame_rust_formats_mode_line_from_current_redisplay_geometry() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = (0..80)
        .map(|line| format!("Line {line:02}\n"))
        .collect::<String>();
    let point = {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert(&text);
        buf.set_buffer_local("mode-line-format", Value::string("%o|%p|%P"));
        let point = buf.point_max_char_pos().get() + 1;
        // Selected-window point lives in the buffer; see
        // window.c:window_point.
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point - 1));
        point
    };
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-mode-line-geometry", 640, 96, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        if let neovm_core::window::Window::Leaf {
            window_start,
            point: window_point,
            ..
        } = window
        {
            *window_start = LispCharPos1::ONE;
            *window_point = LispCharPos1::from_one_based_usize(point);
        }
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let mode_line_text = engine
        .last_frame_display_state
        .as_ref()
        .map(|state| {
            state
                .window_matrices
                .iter()
                .flat_map(|wm| wm.matrix.rows.iter())
                .filter(|row| row.role == GlyphRowRole::ModeLine && row.enabled)
                .flat_map(|row| row.glyphs[1].iter())
                .filter_map(|g| match &g.glyph_type {
                    neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch } => Some(*ch),
                    _ => None,
                })
                .collect::<String>()
        })
        .unwrap_or_default();
    let published_window_start = {
        let frame = eval.frame_manager().get(frame_id).expect("frame");
        let window = frame.find_window(selected_window).expect("selected window");
        match window {
            neovm_core::window::Window::Leaf { window_start, .. } => *window_start,
            other => panic!("expected leaf window, got {other:?}"),
        }
    };
    let expected_mode_line = eval_status_line_format(
        &mut eval,
        "mode-line-format",
        selected_window.0 as i64,
        buf_id.0,
        80,
    )
    .expect("mode-line text");

    assert!(
        published_window_start > LispCharPos1::ONE,
        "expected point at EOB to advance window-start, got {published_window_start:?}"
    );
    assert!(
        mode_line_text == expected_mode_line,
        "expected rendered mode-line to match freshly evaluated mode-line after redisplay publish, got rendered={mode_line_text:?} expected={expected_mode_line:?}"
    );
}

#[test]
fn layout_frame_rust_honors_window_mode_line_format_none() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
        buf.set_buffer_local("mode-line-format", Value::string("BUFFER MODE"));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-window-mode-line-none", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    eval.frame_manager_mut().set_window_parameter(
        selected_window,
        Value::symbol("mode-line-format"),
        Value::symbol("none"),
    );

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let mode_line_text = engine
        .last_frame_display_state
        .as_ref()
        .map(|state| {
            state
                .window_matrices
                .iter()
                .flat_map(|wm| wm.matrix.rows.iter())
                .filter(|row| row.role == GlyphRowRole::ModeLine && row.enabled)
                .flat_map(|row| row.glyphs[1].iter())
                .filter_map(|g| match &g.glyph_type {
                    GlyphType::Char { ch } => Some(*ch),
                    _ => None,
                })
                .collect::<String>()
        })
        .unwrap_or_default();
    let snapshot = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.redisplay_snapshot(selected_window))
        .expect("display snapshot");

    assert_eq!(
        snapshot.mode_line_height, 0,
        "window parameter mode-line-format=none should suppress mode-line height like GNU"
    );
    assert!(
        mode_line_text.is_empty(),
        "window parameter mode-line-format=none should suppress rendered mode-line, got {mode_line_text:?}"
    );
}

#[test]
fn layout_frame_rust_uses_window_mode_line_format_override() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
        buf.set_buffer_local("mode-line-format", Value::NIL);
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-window-mode-line-format", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    eval.frame_manager_mut().set_window_parameter(
        selected_window,
        Value::symbol("mode-line-format"),
        Value::string("WINDOW MODE"),
    );

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let mode_line_text = engine
        .last_frame_display_state
        .as_ref()
        .map(|state| {
            state
                .window_matrices
                .iter()
                .flat_map(|wm| wm.matrix.rows.iter())
                .filter(|row| row.role == GlyphRowRole::ModeLine && row.enabled)
                .flat_map(|row| row.glyphs[1].iter())
                .filter_map(|g| match &g.glyph_type {
                    GlyphType::Char { ch } => Some(*ch),
                    _ => None,
                })
                .collect::<String>()
        })
        .unwrap_or_default();
    let snapshot = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.redisplay_snapshot(selected_window))
        .expect("display snapshot");

    assert!(
        snapshot.mode_line_height > 0,
        "non-nil window mode-line-format should request a mode-line like GNU"
    );
    assert!(
        mode_line_text.contains("WINDOW MODE"),
        "expected window parameter mode-line-format to override nil buffer format, got {mode_line_text:?}"
    );
}

/// A mode line whose format carries a tall `display` element (here a glyph
/// with `(display (height 2.0))`, the same shape doom-modeline's bar uses)
/// must reserve a mode-line height taller than the bare font/char height —
/// mirroring GNU, where `display_mode_line` returns the laid-out row's max
/// ascent+descent and that becomes `w->mode_line_height`. Before the fix the
/// reserved height was a fixed face/char height and clamped the bar short.
#[test]
fn layout_frame_rust_grows_mode_line_height_for_tall_display_element() {
    fn mode_line_height_for(build_format: impl FnOnce() -> Value) -> i64 {
        let mut eval = Context::new();
        let buf_id = eval
            .buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id();
        {
            // Build the format Value after the Context (and its thread heap)
            // exists so heap-allocated display properties are valid.
            let format = build_format();
            let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
            buf.insert("body line\n");
            buf.set_buffer_local("mode-line-format", format);
        }
        let frame_id =
            eval.frame_manager_mut()
                .create_frame("layout-window-mode-line-tall", 640, 160, buf_id);
        let selected_window = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window;

        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame_id);

        eval.frame_manager()
            .get(frame_id)
            .and_then(|frame| frame.redisplay_snapshot(selected_window))
            .expect("display snapshot")
            .mode_line_height
    }

    let plain_height = mode_line_height_for(|| Value::string("MODE"));
    let tall_height = mode_line_height_for(|| {
        Value::string_with_text_properties(
            "MB",
            vec![StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![
                    Value::symbol("display"),
                    Value::list(vec![Value::symbol("height"), Value::make_float(2.0)]),
                ]),
            }],
        )
    });

    assert!(plain_height > 0, "plain mode line should reserve height");
    assert!(
        tall_height > plain_height,
        "tall display element in the mode line must grow the reserved mode-line height \
         (tall={tall_height} should exceed plain={plain_height})"
    );
}

#[test]
fn tall_chrome_publishes_the_same_body_used_by_text_and_scrollbar_rendering() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let format = Value::string_with_text_properties(
            "MB",
            vec![StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![
                    Value::symbol("display"),
                    Value::list(vec![Value::symbol("height"), Value::make_float(2.0)]),
                ]),
            }],
        );
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("body line\n");
        buffer.set_buffer_local("mode-line-format", format);
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("tall-chrome-regions", 640, 160, buffer_id);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));
    let selected = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let snapshot = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.redisplay_snapshot(selected))
        .expect("snapshot");
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let info = state
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == selected.0 as i64)
        .expect("window info");
    assert_eq!(info.mode_line_height, snapshot.mode_line_height as f32);

    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected.0 as i64)
        .expect("window matrix");
    let body = entry.text_area_clip_rect();
    assert_eq!(body.y, snapshot.regions.text_body.y);
    assert_eq!(body.height, snapshot.regions.text_body.height);

    let scroll_bar = state
        .scroll_bars
        .iter()
        .find(|scroll_bar| {
            scroll_bar.window_id.get() == selected.0 as i64 && !scroll_bar.horizontal
        })
        .expect("vertical scroll bar");
    let published_scroll_bar = snapshot
        .regions
        .left_scroll_bar
        .or(snapshot.regions.right_scroll_bar)
        .expect("published vertical scroll bar");
    assert_eq!(scroll_bar.y, published_scroll_bar.y);
    assert_eq!(scroll_bar.height, published_scroll_bar.height);
}

/// A chrome row whose intrinsic height exceeds its retained/face estimate must
/// invalidate the speculative frame.  The published body is from the retry
/// that uses the measured tab-line height, never from the stale first attempt.
///
/// GNU `redisplay_window` compares CURRENT_TAB_LINE_HEIGHT with the desired
/// matrix row and immediately retries before `update_frame` on a mismatch.
#[test]
fn layout_frame_rust_retries_tall_tab_line_before_publishing_body_geometry() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let format = Value::string_with_text_properties(
            "TB",
            vec![StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![
                    Value::symbol("display"),
                    Value::list(vec![Value::symbol("height"), Value::make_float(2.0)]),
                ]),
            }],
        );
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("first body line\nsecond body line\n");
        buffer.set_buffer_local("tab-line-format", format);
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("tall-tab-line-layout-retry", 640, 240, buffer_id);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));
    let selected = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("stable display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected.0 as i64)
        .unwrap_or_else(|| {
            panic!(
                "selected window matrix; selected={}, matrices={:?}",
                selected.0,
                state
                    .window_matrices
                    .iter()
                    .map(|entry| entry.window_id.get())
                    .collect::<Vec<_>>()
            )
        });
    let tab_line = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::TabLine)
        .expect("tab-line row");
    let first_body = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text && row.displays_text)
        .expect("first body row");

    assert_eq!(
        first_body.pixel_y,
        tab_line.pixel_y + tab_line.height_px,
        "published body must begin at the measured tab-line bottom; stale estimated geometry \
         would place it inside the tab-line"
    );
    assert_eq!(
        entry.pixel_bounds.y + first_body.pixel_y,
        entry.text_area_clip_rect().y,
        "published body row and canonical text clip must have the same origin"
    );
}

/// GNU `estimate_mode_line_height` uses the tab-line face's realized
/// `normal_char_height`, even when it is smaller than the frame's default
/// line height.  A relative `:height` must therefore shrink both the
/// published tab-line row and the body partition below it.
#[test]
fn layout_frame_rust_tab_line_can_be_shorter_than_default_face() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("first body line\n");
        buffer.set_buffer_local("tab-line-format", Value::string("tab"));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("small-tab-line-face", 640, 240, buffer_id);
    realize_test_gui_frame(&mut eval, frame_id);
    let results = eval
        .eval_str_each("(internal-set-lisp-face-attribute 'tab-line :height 0.5 (selected-frame))");
    assert!(
        results.iter().all(Result::is_ok),
        "configure a deliberately small tab-line face, got {results:?}"
    );
    let selected = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected.0 as i64)
        .expect("selected window matrix");
    let tab_line = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::TabLine)
        .expect("tab-line row");

    assert!(
        tab_line.height_px < state.char_height,
        "relative tab-line face should shrink below default row: tab-line={} default={}",
        tab_line.height_px,
        state.char_height
    );
    assert_eq!(entry.text_area_clip_rect().y, tab_line.height_px);
}

/// The single-eval invariant: a full `layout_frame_rust` must evaluate
/// `mode-line-format` exactly ONCE per window with a mode line. The chrome
/// rows are laid out once up front (their measured height feeds the geometry)
/// and that same laid-out row is reused at render time — no second
/// `format-mode-line` run. The expensive elisp (~4.3ms in a Doom config) must
/// not be doubled on every keystroke.
///
/// This test FAILS on the rejected two-pass approach (a separate measure pass
/// + the render pass each evaluate the format, count == 2) and PASSES on the
/// single-layout approach (count == 1). The counter is reset at the start of
/// `layout_frame_rust`, so it reflects exactly this redisplay.
#[test]
fn layout_frame_rust_evaluates_mode_line_exactly_once_per_redisplay() {
    use crate::display_status_line::mode_line_eval_count;

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
        buf.set_buffer_local("mode-line-format", Value::string("MODE LINE"));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-mode-line-single-eval", 640, 160, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    assert_eq!(
        mode_line_eval_count(),
        1,
        "mode-line-format must be evaluated exactly once per redisplay (a two-pass \
         measure+render approach would make this 2)"
    );
}

#[test]
fn layout_frame_rust_advances_live_output_through_mode_line_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
        let point = buf.point_max_char_pos().get() + 1;
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point - 1));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-output-progress-mode-line", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(selected_window))
        .and_then(|window| window.display())
        .expect("window display state");
    let logical_cursor = display.cursor.expect("logical cursor");
    let output_cursor = display.output_cursor.expect("output cursor");

    assert!(
        output_cursor.row > logical_cursor.row,
        "expected live output progression to continue past text rows into mode-line rows, cursor={logical_cursor:?} output={output_cursor:?}"
    );
}

#[test]
fn layout_frame_rust_renders_header_line_text_for_non_nil_header_line_format() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
        buf.set_buffer_local("header-line-format", Value::string("LEFT HEADER"));
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-header-line", 640, 160, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let header_text = engine
        .last_frame_display_state
        .as_ref()
        .map(|state| {
            state
                .window_matrices
                .iter()
                .flat_map(|wm| wm.matrix.rows.iter())
                .filter(|row| row.role == GlyphRowRole::HeaderLine && row.enabled)
                .flat_map(|row| row.glyphs[1].iter())
                .filter_map(|g| match &g.glyph_type {
                    neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch } => Some(*ch),
                    _ => None,
                })
                .collect::<String>()
        })
        .unwrap_or_default();

    assert!(
        header_text.contains("LEFT HEADER"),
        "expected header-line row to render buffer-local header-line-format text, got {header_text:?}"
    );
}

#[test]
fn layout_frame_rust_places_window_chrome_images_inside_each_window_clip() {
    let mut eval = Context::new();
    eval.set_display_host(Box::new(RecordingImageDisplayHost::default()));
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let image_format = Value::string_with_text_properties(
        "I",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("image"),
                    Value::keyword("type"),
                    Value::symbol("png"),
                    Value::keyword("file"),
                    Value::string("./tmp/neomacs-window-chrome-image.png"),
                    Value::keyword("max-width"),
                    Value::fixnum(32),
                    Value::keyword("max-height"),
                    Value::fixnum(24),
                ]),
            ]),
        }],
    );
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("body line\n");
        buffer.set_buffer_local("tab-line-format", image_format);
        buffer.set_buffer_local("header-line-format", image_format);
        buffer.set_buffer_local("mode-line-format", image_format);
    }
    let frame_id = eval.frame_manager_mut().create_frame(
        "window-chrome-image-coordinates",
        640,
        220,
        buffer_id,
    );
    let left_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let right_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            left_window,
            neovm_core::window::SplitDirection::Horizontal,
            buffer_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("right window");
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let presentation = state.materialize();
    let chrome_images = presentation
        .glyphs
        .iter()
        .filter_map(|glyph| match glyph {
            FrameGlyph::Image {
                window_id,
                row_role,
                clip_rect,
                x,
                width,
                ..
            } if row_role.is_chrome() => Some((*window_id, *row_role, *clip_rect, *x, *width)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        chrome_images.len(),
        6,
        "tab/header/mode line should each emit an image for both windows"
    );
    assert!(chrome_images.iter().any(|image| {
        image.0.get() == right_window.0 as i64 && image.1 == GlyphRowRole::HeaderLine
    }));
    for (window_id, row_role, clip_rect, x, width) in chrome_images {
        let clip = clip_rect.expect("window chrome image clip");
        assert!(
            x >= clip.x && x + width <= clip.x + clip.width,
            "window chrome media must be in frame coordinates and contained by its window clip: \
             role={:?} window={} image=({:.1}..{:.1}) clip=({:.1}..{:.1})",
            row_role,
            window_id.get(),
            x,
            x + width,
            clip.x,
            clip.x + clip.width,
        );
    }
}

#[test]
fn layout_frame_rust_uses_full_window_row_space_for_header_text_and_mode_line() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
        buf.set_buffer_local("header-line-format", Value::string("LEFT HEADER"));
        let point = buf.point_max_char_pos().get() + 1;
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point - 1));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-header-row-space", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("window display snapshot");
    let display = frame
        .find_window(selected_window)
        .and_then(|window| window.display())
        .expect("window display state");
    let logical_cursor = display.cursor.expect("logical cursor");
    let output_cursor = display.output_cursor.expect("output cursor");

    let header_row = snapshot
        .rows
        .iter()
        .find(|row| row.row == 0)
        .expect("header row snapshot");

    assert!(
        header_row.start_buffer_pos.is_none() && header_row.end_buffer_pos.is_none(),
        "expected row 0 to be reserved for header-line chrome, got {header_row:?}"
    );
    assert!(
        logical_cursor.row >= 1,
        "expected logical cursor row to be offset below header-line chrome, got {logical_cursor:?}"
    );
    assert!(
        output_cursor.row > logical_cursor.row,
        "expected mode-line output to advance past logical text rows, cursor={logical_cursor:?} output={output_cursor:?}"
    );
}

#[test]
fn layout_frame_rust_advances_live_output_through_tab_line_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
        buf.set_buffer_local("tab-line-format", Value::string("TAB ROW"));
        let point = buf.point_max_char_pos().get() + 1;
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point - 1));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-tab-line-row-space", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("window display snapshot");
    let display = frame
        .find_window(selected_window)
        .and_then(|window| window.display())
        .expect("window display state");
    let logical_cursor = display.cursor.expect("logical cursor");
    let output_cursor = display.output_cursor.expect("output cursor");

    let tab_row = snapshot
        .rows
        .iter()
        .find(|row| row.row == 0)
        .expect("tab-line row snapshot");

    assert!(
        tab_row.start_buffer_pos.is_none() && tab_row.end_buffer_pos.is_none(),
        "expected row 0 to be reserved for tab-line chrome, got {tab_row:?}"
    );
    assert!(
        logical_cursor.row >= 1,
        "expected logical cursor row to be offset below tab-line chrome, got {logical_cursor:?}"
    );
    assert!(
        output_cursor.row > logical_cursor.row,
        "expected mode-line output to advance past logical text rows, cursor={logical_cursor:?} output={output_cursor:?}"
    );
}

#[test]
fn layout_frame_rust_tab_line_unicode_uses_shared_display_row_builder() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
        buf.set_buffer_local("tab-line-format", Value::string("A中👨‍👩"));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-tab-line-unicode-baseline", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let tab_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::TabLine)
        .expect("tab-line row");
    let glyphs = &tab_row.glyphs[1];
    let cjk = glyphs
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }))
        .expect("tab-line CJK glyph");

    assert_eq!(glyphs_logical_text(glyphs), "A中👨‍👩");
    assert!(
        cjk.wide,
        "tab-line chrome row should record CJK as a wide glyph through the shared builder: {glyphs:?}"
    );
    assert!(
        glyphs.iter().any(|glyph| glyph.padding),
        "tab-line chrome row should retain padding cells through the shared builder: {glyphs:?}"
    );
    assert!(
        glyphs
            .iter()
            .any(|glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.contains('\u{200d}'))),
        "tab-line chrome row should compose ZWJ emoji through the shared builder: {glyphs:?}"
    );
}

#[test]
fn layout_frame_rust_baseline_buffer_text_uses_main_buffer_wide_and_cluster_glyphs() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("A中👨‍👩B\n");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-buffer-unicode-baseline", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_glyphs = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .flat_map(|row| row.glyphs[1].iter())
        .collect::<Vec<_>>();

    assert!(
        text_glyphs.iter().any(|glyph| {
            matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }) && glyph.wide
        }),
        "main buffer path should record CJK as a wide glyph: {text_glyphs:?}"
    );
    assert!(
        text_glyphs.iter().any(|glyph| glyph.padding),
        "main buffer wide/cluster glyphs should retain padding cells: {text_glyphs:?}"
    );
    assert!(
        text_glyphs.iter().any(|glyph| {
            matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.contains('\u{200d}'))
        }),
        "main buffer path should compose the ZWJ emoji sequence: {text_glyphs:?}"
    );
}

#[test]
fn buffer_text_source_shadow_matches_main_buffer_simple_unicode_row() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("A中👨‍👩B\n");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-buffer-source-shadow", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let main_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("main buffer text row");

    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let line_end = CharPos0::new("A中👨‍👩B".chars().count());
    let shadow_row =
        render_buffer_text_source_shadow_row(buf_id, &snapshot, line_end, 640.0, 16.0, 12.0, 8.0);

    assert_eq!(
        glyphs_logical_text(&shadow_row.glyphs[1]),
        glyphs_logical_text(&main_row.glyphs[1])
    );
    assert_eq!(
        shadow_row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }) && glyph.wide),
        main_row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }) && glyph.wide)
    );
    assert_eq!(
        shadow_row.glyphs[1].iter().any(|glyph| glyph.padding),
        main_row.glyphs[1].iter().any(|glyph| glyph.padding)
    );
    assert_eq!(
        shadow_row
            .glyphs[1]
            .iter()
            .any(|glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.contains('\u{200d}'))),
        main_row
            .glyphs[1]
            .iter()
            .any(|glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.contains('\u{200d}')))
    );
}

#[test]
fn buffer_text_source_shadow_matches_main_buffer_tab_row() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("a\tb\n");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-buffer-source-tab-shadow", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let main_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("main buffer text row");

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let line_end = CharPos0::new("a\tb".chars().count());
    let shadow_row = render_buffer_text_source_shadow_row(
        buf_id,
        &snapshot,
        line_end,
        640.0,
        frame.char_height,
        frame.char_height,
        frame.char_width,
    );

    let main_glyphs = &main_row.glyphs[1];
    let shadow_glyphs = &shadow_row.glyphs[1];
    let main_tab = main_glyphs
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { .. }))
        .expect("main tab stretch");
    let shadow_tab = shadow_glyphs
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { .. }))
        .expect("shadow tab stretch");

    assert_eq!(
        glyphs_logical_text(main_glyphs),
        glyphs_logical_text(shadow_glyphs)
    );
    assert_eq!(main_tab.glyph_type, shadow_tab.glyph_type);
    assert_eq!(main_tab.pixel_width, shadow_tab.pixel_width);
}

#[test]
fn layout_frame_rust_preserves_multiline_overlay_output_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("x");
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("after-string"),
            Value::string("A\nB"),
        );
        let point = buf.point_max_char_pos().get() + 1;
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point - 1));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-overlay-output-rows", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("window display snapshot");
    let display = frame
        .find_window(selected_window)
        .and_then(|window| window.display())
        .expect("window display state");
    assert!(
        snapshot
            .rows
            .iter()
            .any(|row| row.row == 0 && row.start_buffer_pos.is_some()),
        "expected first text row snapshot to survive multiline overlay output, rows={:?}",
        snapshot.rows
    );
    assert!(
        snapshot.rows.iter().any(|row| row.row == 1),
        "expected multiline overlay output to publish a second text row, rows={:?}",
        snapshot.rows
    );
    assert!(
        display.output_cursor.is_some_and(|cursor| cursor.row >= 1),
        "expected live output cursor to advance onto multiline overlay rows, output={:?}",
        display.output_cursor
    );
}

#[test]
fn layout_frame_rust_renders_overlay_string_tabs_as_stretches() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("x");
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("after-string"),
            Value::string("a\tb"),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-overlay-tab-string", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    let logical_text = glyphs_logical_text(&text_row.glyphs[1]);
    assert!(
        !logical_text.contains('\t'),
        "overlay tab should not render as a literal tab, row={:?}",
        text_row.glyphs[1]
    );
    assert!(
        logical_text.contains("a      b"),
        "overlay tab should expand to the next tab stop, text={logical_text:?}"
    );
    assert!(
        text_row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { width_cols: 6 })),
        "overlay tab should be a stretch glyph, row={:?}",
        text_row.glyphs[1]
    );
}

#[test]
fn layout_frame_rust_nerd_font_alias_icon_uses_resolved_monospace_cell_width() {
    let mut eval = Context::new();
    eval.eval_str(r#"(set-fontset-font t '(#xe6ad . #xe6ad) "JetBrainsMono Nerd Font")"#)
        .expect("install the concrete Nerd Font fallback used by the compatibility alias");
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("x");
        buf.set_buffer_local("tab-width", Value::fixnum(1));
        buf.set_buffer_local(
            "face-remapping-alist",
            Value::list(vec![Value::list(vec![
                Value::symbol("default"),
                Value::list(vec![
                    Value::keyword("family"),
                    Value::string("JetBrainsMono Nerd Font"),
                    Value::keyword("height"),
                    Value::make_float(0.75),
                ]),
                Value::symbol("default"),
            ])]),
        );
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let after_string = Value::string_with_text_properties(
            "\u{e6ad}\tn",
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![
                    Value::symbol("face"),
                    Value::list(vec![
                        Value::keyword("family"),
                        Value::string("Symbols Nerd Font Mono"),
                    ]),
                ]),
            }],
        );
        let _ =
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("before-string"), after_string);
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-nerd-font-alias-tab", 640, 160, buf_id);
    realize_test_gui_frame(&mut eval, frame_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");
    let glyphs = &text_row.glyphs[1];
    let icon_index = glyphs
        .iter()
        .position(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '\u{e6ad}' }))
        .expect("nerd icon glyph");
    let icon = &glyphs[icon_index];
    let tab = glyphs
        .get(icon_index + 1)
        .filter(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { .. }))
        .expect("tab stretch after nerd icon");
    let icon_face = state.faces.get(&icon.face_id).expect("icon face");
    let tab_face = state.faces.get(&tab.face_id).expect("tab face");
    assert_eq!(
        icon_face.font_family, "Symbols Nerd Font Mono",
        "test must exercise the Nerd Font compatibility alias"
    );
    let mut metrics = FontMetricsService::new();
    let resolved_icon_advance = metrics.char_width(
        '\u{e6ad}',
        &icon_face.font_family,
        icon_face.font_weight,
        icon_face.is_italic(),
        icon_face.font_size,
    );
    let resolved_tab_space = metrics.char_width(
        ' ',
        &tab_face.font_family,
        tab_face.font_weight,
        tab_face.is_italic(),
        tab_face.font_size,
    );

    assert!(
        (icon.pixel_width - resolved_icon_advance).abs() < 0.01,
        "the frame must publish the concrete fontset glyph advance, not the unresolved compatibility-face cell width; icon={} resolved={} row={glyphs:?}",
        icon.pixel_width,
        resolved_icon_advance,
    );
    assert!(
        (tab.pixel_width - resolved_tab_space).abs() < 0.01,
        "at row start with tab-width 1, GNU places the tab after a one-cell icon at the next realized-font space; tab={} space={} row={glyphs:?}",
        tab.pixel_width,
        resolved_tab_space,
    );
}

#[test]
fn layout_frame_rust_overlay_after_string_display_replacement_renders_once() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("x");
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let icon_with_nested_display = Value::string("N\t");
        neovm_core::emacs_core::value::set_string_text_properties_for_value(
            icon_with_nested_display,
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![Value::symbol("display"), icon_with_nested_display]),
            }],
        );
        let after_string = Value::string_with_text_properties(
            "N\t",
            vec![
                StringTextPropertyRun {
                    start: 0,
                    end: 1,
                    plist: Value::list(vec![
                        Value::symbol("display"),
                        icon_with_nested_display,
                        Value::symbol("face"),
                        Value::symbol("bold"),
                    ]),
                },
                StringTextPropertyRun {
                    start: 1,
                    end: 2,
                    plist: Value::list(vec![Value::symbol("display"), icon_with_nested_display]),
                },
            ],
        );
        let _ =
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("after-string"), after_string);
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-overlay-display-replacement-once",
        640,
        160,
        buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    let logical_text = glyphs_logical_text(&text_row.glyphs[1]);
    assert_eq!(
        logical_text.matches('N').count(),
        1,
        "overlay after-string display replacement should render one icon, not duplicate it: {logical_text:?}"
    );
    assert!(
        !logical_text.contains('\t'),
        "display replacement tab should be expanded before reaching the glyph row: {logical_text:?}"
    );
}

#[test]
fn layout_frame_rust_renders_overlay_string_glyphless_chars_as_glyphless() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("x");
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("after-string"),
            Value::string("\u{fff0}"),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-overlay-glyphless-string", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let text_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .expect("text row");

    assert!(
        text_row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Glyphless { ch: '\u{fff0}' })),
        "overlay glyphless source char should emit a glyphless glyph, row={:?}",
        text_row.glyphs[1]
    );
}

#[test]
fn layout_frame_rust_places_cursor_inside_overlay_string_text_run() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("x");
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: 1,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let overlay_text = Value::string_with_text_properties(
            "AB",
            vec![StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![Value::symbol("cursor"), Value::T]),
            }],
        );
        let _ =
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("after-string"), overlay_text);
        buf.goto_emacs_byte_pos(EmacsBytePos::new(1));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-overlay-cursor-run", 640, 160, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let snapshot = frame
        .redisplay_snapshot(selected_window)
        .expect("display snapshot");
    let cursor = snapshot.phys_cursor.as_ref().expect("cursor");
    let x_point = snapshot
        .point_for_buffer_pos(LispCharPos1::from_one_based_usize(1))
        .expect("x point");
    let expected_overlay_slot_width = frame.char_width.round() as i64;

    assert_eq!(cursor.row, x_point.row);
    assert_eq!(
        cursor.x,
        x_point.x + x_point.width + expected_overlay_slot_width
    );
    assert_eq!(cursor.col, x_point.col + 2);
    assert_eq!(cursor.width, expected_overlay_slot_width);
}

#[test]
fn layout_frame_rust_renders_zero_length_eob_before_string_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("Find file: ~/.config/doom/");
        let eob = buf.point_max_emacs_byte_pos().get();
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("before-string"),
            Value::string("\ninit.el\nconfig.el"),
        );
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(eob));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-eob-before-overlay", 640, 180, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let rows = enabled_window_row_texts(entry);

    assert!(
        rows.iter().any(|row| row.contains("init.el")),
        "expected zero-length EOB before-string to render init.el, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("config.el")),
        "expected zero-length EOB before-string to render config.el, rows={rows:?}"
    );
}

#[test]
fn layout_frame_rust_renders_row_start_before_string_at_point_min() {
    // GNU `handle_stop`-at-init loads the before-strings of overlays anchored at
    // the iterator's starting charpos (window-start) before producing the first
    // buffer char (`get_overlay_strings_1`, `src/xdisp.c`). vertico's "n/m"
    // candidate count is exactly such a before-string at point-min; it must
    // render at the very start of the first row, ahead of the buffer text.
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("M-x switch");
        // Zero-length overlay anchored at point-min carrying the count in its
        // before-string, mirroring vertico's overlay (vertico.el:444-448/614).
        let bob = 0;
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: bob,
            end: bob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("before-string"),
            Value::string_with_text_properties(
                "1/1 ",
                vec![StringTextPropertyRun {
                    start: 0,
                    end: 4,
                    plist: Value::list(vec![
                        Value::symbol("mouse-face"),
                        Value::symbol("highlight"),
                    ]),
                }],
            ),
        );
        buf.goto_emacs_byte_pos(EmacsBytePos::new(bob));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-row-start-before-overlay", 640, 180, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let rows = enabled_window_row_texts(entry);
    let first_row = rows.first().cloned().unwrap_or_default();

    let _ = state.materialize();

    assert!(
        first_row.contains("1/1"),
        "expected point-min before-string to render on the first row, rows={rows:?}"
    );
    // The before-string must precede the buffer text on the row, exactly like
    // GNU shows "1/1   M-x …" rather than "M-x …".
    let count_idx = first_row
        .find("1/1")
        .expect("before-string present on first row");
    let buffer_idx = first_row
        .find("M-x")
        .expect("buffer text present on first row");
    assert!(
        count_idx < buffer_idx,
        "before-string must render ahead of the first buffer char, first_row={first_row:?}"
    );
    assert_eq!(
        first_row.matches("1/1").count(),
        1,
        "point-min before-string must be emitted exactly once, first_row={first_row:?}"
    );
}

#[test]
fn layout_frame_rust_suppresses_left_fringe_display_spec_before_string() {
    // magit attaches an overlay before-string `#("fringe" 0 6 (display
    // (left-fringe magit-fringe-bitmapv fringe)))` to every collapsible section
    // heading. GNU's `(left-fringe BITMAP FACE)` display spec REPLACES the
    // covered text in the text area (it renders a bitmap in the fringe instead),
    // so the literal "fringe" string must NOT appear inline. We don't draw the
    // fringe bitmap yet, but the text area must match GNU: nothing for the spec.
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    // Register the magit fold-arrow bitmap so the fringe spec resolves to a real
    // registry index (first user bitmap => 25).
    eval.eval_str(
        "(define-fringe-bitmap 'magit-fringe-bitmapv \
             [#b00000000 #b10000010 #b11000110 #b01101100 #b00111000 #b00010000 \
              #b00000000 #b00000000])",
    )
    .expect("define magit fringe bitmap");
    let fringe_index = eval
        .eval_str("(get 'magit-fringe-bitmapv 'fringe)")
        .expect("magit fringe bitmap index property")
        .as_fixnum()
        .expect("fringe index") as u16;
    // Build the propertized before-string out of band so the `display` property
    // is a real `(left-fringe …)` list, exactly as magit constructs it.
    let before_string = eval
        .eval_str("(propertize \"fringe\" 'display '(left-fringe magit-fringe-bitmapv fringe))")
        .expect("propertize fringe before-string");
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("Head:");
        let bob = 0;
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: bob,
            end: bob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ =
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("before-string"), before_string);
        buf.goto_emacs_byte_pos(EmacsBytePos::new(bob));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-left-fringe-before-string", 640, 180, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let rows = enabled_window_row_texts(entry);

    // The buffer's own heading text still renders.
    assert!(
        rows.iter().any(|row| row.contains("Head:")),
        "expected heading text to render, rows={rows:?}"
    );
    // The `(left-fringe …)` before-string produces NO inline glyph: the literal
    // "fringe" must not appear anywhere in the text area.
    assert!(
        rows.iter().all(|row| !row.contains("fringe")),
        "expected (left-fringe …) before-string to render nothing inline, rows={rows:?}"
    );

    // Stage 2/3: the covered row records a left-fringe bitmap descriptor with the
    // resolved registry index, so the renderer can draw the arrow in the fringe.
    let fringe_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.left_fringe_bitmap.is_some())
        .expect("a row carries the left-fringe bitmap");
    let info = fringe_row.left_fringe_bitmap.expect("left fringe info");
    assert_eq!(info.bitmap_index, fringe_index);

    // Stage 3: the bitmap bits are embedded once per frame for the renderer.
    assert!(
        state.fringe_bitmaps.contains_key(&fringe_index),
        "frame display state embeds the resolved fringe bitmap data"
    );
}

#[test]
fn layout_frame_rust_resolves_standard_fringe_bitmap_spec() {
    // Foundation Stage 1: an explicit `(left-fringe right-arrow fringe)` display
    // spec — referencing a GNU STANDARD built-in bitmap (no
    // `define-fringe-bitmap` call) — now resolves to a real bitmap descriptor.
    // Before seeding the standard bitmaps, `record_fringe_bitmap_layout` returned
    // None for `right-arrow` (its index 1..24 slot was empty), so nothing drew.
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    // `right-arrow` is GNU standard_bitmaps[] index 4, pre-seeded at startup.
    let right_arrow_index: u16 = eval
        .eval_str("(get 'right-arrow 'fringe)")
        .expect("right-arrow fringe prop")
        .as_fixnum()
        .expect("fringe index") as u16;
    assert_eq!(right_arrow_index, 4, "right-arrow is fringe.c index 4");

    let before_string = eval
        .eval_str("(propertize \"fringe\" 'display '(left-fringe right-arrow fringe))")
        .expect("propertize fringe before-string");
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("Standard:");
        let bob = 0;
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: bob,
            end: bob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ =
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("before-string"), before_string);
        buf.goto_emacs_byte_pos(EmacsBytePos::new(bob));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-standard-left-fringe", 640, 180, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let rows = enabled_window_row_texts(entry);

    // The literal "fringe" string is suppressed (replacement spec), heading shows.
    assert!(
        rows.iter().any(|row| row.contains("Standard:")),
        "expected heading text to render, rows={rows:?}"
    );
    assert!(
        rows.iter().all(|row| !row.contains("fringe")),
        "expected (left-fringe …) before-string to render nothing inline, rows={rows:?}"
    );

    // The covered row carries the resolved STANDARD bitmap index (4), proving the
    // explicit-spec path now works for standard symbols, not just user bitmaps.
    let fringe_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| row.left_fringe_bitmap.is_some())
        .expect("a row carries the left-fringe bitmap");
    let info = fringe_row.left_fringe_bitmap.expect("left fringe info");
    assert_eq!(info.bitmap_index, right_arrow_index);

    // The standard bitmap's bits are embedded once per frame for the renderer.
    assert!(
        state.fringe_bitmaps.contains_key(&right_arrow_index),
        "frame display state embeds the standard fringe bitmap data"
    );
}

#[test]
fn layout_frame_rust_fills_empty_line_fringe_below_buffer_end() {
    // Stage 3/4: a buffer that ends well before the window bottom, with
    // `indicate-empty-lines` on (Doom's vi-tilde-fringe `~`), produces blank
    // filler rows below the last buffer line — each carrying the periodic
    // `empty-line` bitmap in the LEFT fringe. GNU's redisplay tail fills these
    // (xdisp.c sets `row->indicate_empty_line_p`); before this change neomacs
    // left bare frame background below the last line.
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    // `empty-line` is GNU standard_bitmaps[] index 24, pre-seeded at startup.
    let empty_line_index: u16 = eval
        .eval_str("(get 'empty-line 'fringe)")
        .expect("empty-line fringe prop")
        .as_fixnum()
        .expect("fringe index") as u16;
    assert_eq!(empty_line_index, 24, "empty-line is fringe.c index 24");

    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        // A single short line, so the bulk of the window is below buffer-end.
        buf.insert("hello\n");
        buf.set_buffer_local("indicate-empty-lines", Value::T);
        buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }

    // A tall enough frame that many rows sit below the one buffer line.
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-empty-line-fringe", 640, 400, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");

    // The real buffer line still renders — the filler rows must NOT overwrite or
    // double-count the last buffer row (off-by-one guard).
    let texts = enabled_window_row_texts(entry);
    assert!(
        texts.iter().any(|row| row.contains("hello")),
        "the buffer's own line must still render below the fillers, rows={texts:?}"
    );

    // Every filler row below the buffer carries the empty-line bitmap in the
    // LEFT fringe, resolved to the standard registry index.
    let empty_line_rows: Vec<_> = entry
        .matrix
        .rows
        .iter()
        .filter_map(|row| row.left_fringe_bitmap)
        .filter(|info| info.bitmap_index == empty_line_index)
        .collect();
    assert!(
        empty_line_rows.len() >= 5,
        "expected several empty-line filler rows below the single buffer line, \
         got {} (rows total = {})",
        empty_line_rows.len(),
        entry.matrix.rows.len()
    );

    // The periodic empty-line bitmap is embedded once per frame for the renderer,
    // and it carries its period (3) so the renderer tiles the dotted pattern.
    let bitmap = state
        .fringe_bitmaps
        .get(&empty_line_index)
        .expect("frame embeds the empty-line bitmap data");
    assert_eq!(bitmap.period, 3, "empty-line is periodic with period 3");

    // The filler rows are blank text rows that end at ZV (not chrome / mode-line).
    for row in entry.matrix.rows.iter() {
        if row
            .left_fringe_bitmap
            .is_some_and(|info| info.bitmap_index == empty_line_index)
        {
            assert!(
                !row.mode_line,
                "empty-line filler rows must not be mode-line rows"
            );
            assert!(
                row.glyphs.iter().all(|area| area.is_empty()),
                "empty-line filler rows must be blank (no glyphs)"
            );
        }
    }
}

#[test]
fn layout_frame_rust_omits_empty_line_fringe_when_indicator_off() {
    // Control: with `indicate-empty-lines` OFF, no filler rows carry the
    // empty-line bitmap (proves the filler path is gated on the buffer-local).
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let empty_line_index: u16 = eval
        .eval_str("(get 'empty-line 'fringe)")
        .expect("empty-line fringe prop")
        .as_fixnum()
        .expect("fringe index") as u16;
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("hello\n");
        buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-empty-line-fringe-off", 640, 400, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    assert!(
        entry
            .matrix
            .rows
            .iter()
            .filter_map(|row| row.left_fringe_bitmap)
            .all(|info| info.bitmap_index != empty_line_index),
        "no empty-line bitmaps when indicate-empty-lines is off"
    );
}

#[test]
fn layout_frame_rust_renders_eob_overlay_strings_in_gnu_interleaved_order() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("x");
        let eob = buf.point_max_emacs_byte_pos().get();

        let after_overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(after_overlay);
        let _ = buf.overlays_mut().overlay_put(
            after_overlay,
            Value::symbol("after-string"),
            Value::string("A"),
        );

        let before_overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(before_overlay);
        let _ = buf.overlays_mut().overlay_put(
            before_overlay,
            Value::symbol("before-string"),
            Value::string("B"),
        );
        buf.goto_emacs_byte_pos(EmacsBytePos::new(eob));
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-eob-overlay-interleaved-order",
        640,
        180,
        buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let rendered = enabled_window_row_texts(entry).join("\n");

    assert!(
        rendered.contains("xAB"),
        "GNU compare_overlay_entries renders after-strings from other overlays before before-strings, rows={rendered:?}"
    );
}

#[test]
fn layout_frame_rust_overlay_before_string_uses_overlay_string_base_face() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("M-x s");
        let prompt_face = Value::list(vec![
            Value::keyword("background"),
            Value::string("#ffff00"),
            Value::keyword("foreground"),
            Value::string("#000000"),
        ]);
        buf.put_text_property(
            0,
            buf.total_emacs_byte_len().get(),
            Value::symbol("face"),
            prompt_face,
        );

        let eob = buf.point_max_emacs_byte_pos().get();
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("before-string"),
            Value::string("\ncandidate"),
        );
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(eob));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-overlay-string-base-face", 640, 180, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let default_bg = state
        .faces
        .get(&FaceId::from(
            neomacs_display_protocol::face::BasicFaceId::Default,
        ))
        .expect("default face")
        .background;

    let prompt_face_id = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .flat_map(|row| row.glyphs[1].iter())
        .find_map(|glyph| match glyph.glyph_type {
            GlyphType::Char { ch: 'M' } => Some(glyph.face_id),
            _ => None,
        })
        .expect("prompt glyph face");
    let prompt_bg = state
        .faces
        .get(&prompt_face_id)
        .expect("prompt face")
        .background;
    assert_ne!(
        prompt_bg, default_bg,
        "test setup should make prompt face distinguishable from default"
    );

    let candidate_face_id = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .flat_map(|row| row.glyphs[1].iter())
        .find_map(|glyph| match glyph.glyph_type {
            GlyphType::Char { ch: 'c' } => Some(glyph.face_id),
            _ => None,
        })
        .expect("candidate glyph face");
    let candidate_bg = state
        .faces
        .get(&candidate_face_id)
        .expect("candidate face")
        .background;

    assert_eq!(
        candidate_bg, default_bg,
        "GNU overlay strings use a default/text-property base face, not the current prompt face"
    );
}

#[test]
fn layout_frame_rust_merges_overlay_face_with_text_property_face() {
    // GNU face_at_buffer_position merges the `face' text property FIRST, then
    // overlay faces LAST (overlays win on conflict but both contribute). A char
    // carrying both a text-property face and an overlay face must render the
    // merged face, not either contribution alone.
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("ab");
        let len = buf.total_emacs_byte_len().get();
        // Text-property face: a NAMED face (`bold`) — this resolves to a concrete
        // face id (unlike a plist face, which defers to face_at_pos). The named
        // face sets weight but not background.
        buf.put_text_property(0, len, Value::symbol("face"), Value::symbol("bold"));
        // Overlay face spanning the same text: distinctive BACKGROUND only.
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 0,
            end: len,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("face"),
            Value::list(vec![Value::keyword("background"), Value::string("#00ff00")]),
        );
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-overlay-face-merge", 640, 180, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let default_face = state
        .faces
        .get(&FaceId::from(
            neomacs_display_protocol::face::BasicFaceId::Default,
        ))
        .expect("default face");
    let default_bg = default_face.background;

    let a_face_id = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .flat_map(|row| row.glyphs[1].iter())
        .find_map(|glyph| match glyph.glyph_type {
            GlyphType::Char { ch: 'a' } => Some(glyph.face_id),
            _ => None,
        })
        .expect("glyph 'a' face");
    let face = state.faces.get(&a_face_id).expect("resolved face for 'a'");

    assert!(
        face.is_bold(),
        "text-property bold face must survive the merge"
    );
    assert_ne!(
        face.background, default_bg,
        "overlay background must merge in (GNU merges overlays after the text-prop face); \
         dropping it means the named text-prop face overrode the overlay-merged checkpoint face"
    );
}

#[test]
fn layout_frame_rust_overlay_face_nil_cancels_text_underline() {
    use neomacs_display_protocol::face::UnderlineStyle;

    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-overlay-face-cancels-underline",
        640,
        180,
        buf_id,
    );
    realize_test_gui_frame(&mut eval, frame_id);

    let results = eval.eval_str_each(
        "(internal-set-lisp-face-attribute '__underlined_text :underline t (selected-frame))\
         (internal-set-lisp-face-attribute '__highlight_without_underline :underline nil (selected-frame))\
         (internal-set-lisp-face-attribute '__highlight_without_underline :background \"#4d4d4d\" (selected-frame))\
         (erase-buffer)\
         (insert \"neomacs\")\
         (put-text-property 1 8 'face '__underlined_text)\
         (overlay-put (make-overlay 1 8) 'face '__highlight_without_underline)",
    );
    assert!(
        results.iter().all(Result::is_ok),
        "face and overlay setup must succeed, got {results:?}"
    );

    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("selected window matrix");
    let glyph = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .flat_map(|row| row.glyphs[GlyphArea::Text.index()].iter())
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: 'n' }))
        .expect("rendered n glyph");
    let face = state.faces.get(&glyph.face_id).expect("merged glyph face");

    assert_eq!(
        face.underline_style,
        UnderlineStyle::None,
        "GNU overlay precedence: explicit :underline nil must cancel the text property's underline"
    );
}

#[test]
fn layout_frame_rust_applies_face_only_overlay_starting_mid_run() {
    // Regression (isearch current-match highlight): a face-only overlay (no
    // display string) that begins/ends INSIDE a text-property run must bound the
    // run so each piece carries its own face. GNU folds `next_overlay_change`
    // into `compute_stop_pos` (src/xdisp.c). Before the fix, the run was bounded
    // only by text-property changes, so an overlay starting mid-run never split
    // it and the overlay face never painted (C-s "counter" left the match
    // unhighlighted because the overlay began inside the "my-counter" run).
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("abcdef");
        let len = buf.total_emacs_byte_len().get();
        // One uniform text-property face over the WHOLE run, so the only face
        // boundaries come from the overlay (start at 2, end at 4).
        buf.put_text_property(0, len, Value::symbol("face"), Value::symbol("bold"));
        // Face-only overlay (distinctive background) over "cd" — text before
        // ('ab') and after ('ef'), so it begins AND ends mid-run.
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: 2,
            end: 4,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("face"),
            Value::list(vec![Value::keyword("background"), Value::string("#00ff00")]),
        );
    }

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-midrun-overlay", 640, 180, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let bg_of = |ch: char| {
        let face_id = entry
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .flat_map(|row| row.glyphs[1].iter())
            .find_map(|glyph| match glyph.glyph_type {
                GlyphType::Char { ch: c } if c == ch => Some(glyph.face_id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("glyph {ch:?} not found"));
        state
            .faces
            .get(&face_id)
            .unwrap_or_else(|| panic!("face for {ch:?}"))
            .background
    };

    let overlay_bg = bg_of('c');
    assert_ne!(
        overlay_bg,
        bg_of('a'),
        "face-only overlay over 'cd' must paint its background; 'a' (before it) \
         must keep the plain face — the run must split at the overlay START"
    );
    assert_eq!(
        overlay_bg,
        bg_of('d'),
        "'d' is inside the same overlay as 'c'"
    );
    assert_ne!(
        overlay_bg,
        bg_of('e'),
        "'e' (after the overlay) must keep the plain face — the run must split \
         at the overlay END too"
    );
}

#[test]
fn layout_frame_rust_continues_eob_before_string_after_overlong_line() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        let eob = buf.point_max_emacs_byte_pos().get();
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("before-string"),
            Value::string("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nsecond.el\nthird.el"),
        );
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(eob));
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-eob-overlong-before-overlay",
        96,
        180,
        buf_id,
    );
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.char_width = 8.0;
        frame.char_height = 16.0;
        frame.font_pixel_size = 16.0;
    }
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let rows = enabled_window_row_texts(entry);

    assert!(
        rows.iter().any(|row| row.contains("second.el")),
        "expected overlong overlay row not to suppress the next candidate row, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("third.el")),
        "expected rendering to continue after later overlay newlines, rows={rows:?}"
    );
}

#[test]
fn layout_frame_rust_honors_display_space_align_in_overlay_strings() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        let eob = buf.point_max_emacs_byte_pos().get();
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let display_space = Value::string_with_text_properties(
            "config.el -rw",
            vec![StringTextPropertyRun {
                start: "config.el".chars().count(),
                end: "config.el ".chars().count(),
                plist: Value::list(vec![
                    Value::symbol("display"),
                    Value::list(vec![
                        Value::symbol("space"),
                        Value::keyword(":align-to"),
                        Value::list(vec![
                            Value::symbol("+"),
                            Value::symbol("left"),
                            Value::fixnum(20),
                        ]),
                    ]),
                ]),
            }],
        );
        let _ =
            buf.overlays_mut()
                .overlay_put(overlay, Value::symbol("before-string"), display_space);
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(eob));
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-overlay-display-space-align",
        640,
        180,
        buf_id,
    );
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == selected_window.0 as i64)
        .expect("window matrix entry");
    let rendered_rows: Vec<String> = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled)
        .map(|row| {
            row.glyphs[1]
                .iter()
                .map(|glyph| match &glyph.glyph_type {
                    GlyphType::Char { ch } => ch.to_string(),
                    GlyphType::Composite { text } => text.to_string(),
                    GlyphType::Stretch { width_cols } => " ".repeat(*width_cols as usize),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .collect();

    assert!(
        rendered_rows
            .iter()
            .any(|row| row.contains("config.el           -rw")),
        "GNU TTY expands overlay-string display spaces before suffix text, rows={rendered_rows:?}"
    );
}

#[test]
fn layout_frame_rust_grows_minibuffer_for_eob_before_string_like_gnu() {
    // GNU `load_overlay_strings` (src/xdisp.c:~7164) DOES measure a non-empty
    // EOB `before-string`, so `resize_mini_window` grows the parent minibuffer
    // to display it. With the unclamped walk measurement (no estimator), the
    // minibuffer grows and renders the overlay's `before-string` lines.
    let mut eval = Context::new();
    eval.obarray_mut()
        .set_symbol_value("resize-mini-windows", Value::symbol("grow-only"));
    eval.obarray_mut()
        .set_symbol_value("max-mini-window-height", Value::fixnum(10));

    let root_buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let minibuf_id = eval.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    {
        let buf = eval
            .buffer_manager_mut()
            .get_mut(minibuf_id)
            .expect("buffer");
        buf.insert("Find file: ~/.config/doom/");
        let eob = buf.point_max_emacs_byte_pos().get();
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayData {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(minibuf_id),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buf.overlays_mut().insert_overlay(overlay);
        let _ = buf.overlays_mut().overlay_put(
            overlay,
            Value::symbol("before-string"),
            Value::string("\ninit.el\nconfig.el"),
        );
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(eob));
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-mini-eob-before-overlay",
        120,
        40,
        root_buf_id,
    );
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.shrink_mini_window();
    }
    let minibuffer_window_id = eval
        .activate_minibuffer_window_for_buffer(
            minibuf_id,
            LispString::from_utf8("Find file: "),
            Some(LispString::from_utf8("~/.config/doom/")),
        )
        .expect("activate minibuffer")
        .expect("minibuffer window");

    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == minibuffer_window_id.0 as i64)
        .expect("minibuffer matrix entry");
    let rows = enabled_window_row_texts(entry);

    assert!(
        rows.iter()
            .any(|row| row.contains("Find file: ~/.config/doom/")),
        "expected minibuffer prompt row to render, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("init.el")),
        "GNU grows the minibuffer for a non-empty EOB before-string \
         (load_overlay_strings), so init.el must render, rows={rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("config.el")),
        "GNU grows the minibuffer for a non-empty EOB before-string \
         (load_overlay_strings), so config.el must render, rows={rows:?}"
    );
}

/// Lay out a frame whose ACTIVE minibuffer displays `content` (the
/// minibuffer buffer's own text), with `max-mini-window-height` set to
/// `max_mini_lines` (a fixnum). Returns the enabled minibuffer row texts.
///
/// Models the active fido/vertico path: an active mini-window renders its
/// own buffer text (GNU `resize_mini_window` measures `move_it_to(ZV)` over
/// that buffer), as opposed to the inactive echo-area path that swaps in
/// ` *Echo Area 0*`.
fn layout_active_minibuffer_rows(
    content: &str,
    max_mini_lines: i64,
    use_gui_metrics: bool,
) -> Vec<String> {
    let mut eval = Context::new();
    eval.obarray_mut()
        .set_symbol_value("resize-mini-windows", Value::symbol("grow-only"));
    eval.obarray_mut()
        .set_symbol_value("max-mini-window-height", Value::fixnum(max_mini_lines));

    let root_buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let minibuf_id = eval.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    {
        let buf = eval
            .buffer_manager_mut()
            .get_mut(minibuf_id)
            .expect("buffer");
        buf.insert(content);
        let eob = buf.point_max_emacs_byte_pos().get();
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(eob));
    }

    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-active-minibuffer", 640, 200, root_buf_id);

    let mut engine = if use_gui_metrics {
        let mut e = LayoutEngine::new();
        e.enable_cosmic_metrics();
        e
    } else {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.shrink_mini_window();
        LayoutEngine::new_without_font_metrics()
    };

    let minibuffer_window_id = eval
        .activate_minibuffer_window_for_buffer(minibuf_id, LispString::from_utf8(""), None)
        .expect("activate minibuffer")
        .expect("minibuffer window");

    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == minibuffer_window_id.0 as i64)
        .expect("minibuffer matrix entry");
    enabled_window_row_texts(entry)
}

/// An active minibuffer holding a prompt line plus several candidate lines
/// must grow to one display row per logical line, render every line, and never
/// flatten content into one row.  Exercises the active-fido/vertico measure
/// path the unclamped GNU `resize_mini_window` walk now drives.
fn assert_active_minibuffer_grows_for_multiline_content(use_gui_metrics: bool) {
    let rows = layout_active_minibuffer_rows(
        "Find file: cand\nalpha.el\nbeta.el\ngamma.el",
        10,
        use_gui_metrics,
    );

    for needle in ["Find file: cand", "alpha.el", "beta.el", "gamma.el"] {
        assert!(
            rows.iter().any(|row| row.contains(needle)),
            "expected active minibuffer to grow and render {needle:?}, rows={rows:?}"
        );
    }
    assert!(
        !rows.iter().any(|row| row.contains("candalpha")),
        "multiline minibuffer content was flattened into one row: {rows:?}"
    );
    let content_rows = rows.iter().filter(|row| !row.trim().is_empty()).count();
    assert!(
        content_rows >= 4,
        "expected four content rows (one per logical line), got {content_rows}: {rows:?}"
    );
}

#[test]
fn active_minibuffer_grows_for_multiline_content_tty() {
    assert_active_minibuffer_grows_for_multiline_content(false);
}

#[test]
fn active_minibuffer_grows_for_multiline_content_gui() {
    assert_active_minibuffer_grows_for_multiline_content(true);
}

#[test]
fn minibuffer_relayout_discards_fast_path_classification_from_rejected_attempt() {
    let mut eval = Context::new();
    eval.obarray_mut()
        .set_symbol_value("resize-mini-windows", Value::symbol("grow-only"));
    eval.obarray_mut()
        .set_symbol_value("max-mini-window-height", Value::fixnum(10));

    let root_buffer = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let minibuffer = eval.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("minibuffer-retry-fast-path", 120, 40, root_buffer);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.shrink_mini_window();
    }
    eval.activate_minibuffer_window_for_buffer(minibuffer, LispString::from_utf8(""), None)
        .expect("activate minibuffer")
        .expect("minibuffer window");

    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(minibuffer)
            .expect("minibuffer buffer");
        buffer.insert("Find file: cand\nalpha.el\nbeta.el\ngamma.el");
        let eob = buffer.point_max_emacs_byte_pos();
        buffer.goto_emacs_byte_pos(eob);
    }

    engine.layout_frame_rust(&mut eval, frame_id);

    assert_eq!(
        engine.last_layout_stats().cursor_only_windows,
        0,
        "the root window was cursor-only only in the rejected pre-resize attempt"
    );
    assert_eq!(engine.last_layout_stats().full_windows, 2);
}

#[test]
fn resetting_speculative_frame_output_discards_fast_path_classification() {
    let mut engine = LayoutEngine::new_without_font_metrics();
    let window = neomacs_display_protocol::DisplayWindowId::new(7);
    engine.cursor_only_window_ids.insert(window);
    engine.scroll_window_ids.insert(window, (3, 16.0));
    engine.edit_window_ids.insert(window, 2);

    engine.reset_frame_attempt_state();

    assert!(engine.cursor_only_window_ids.is_empty());
    assert!(engine.scroll_window_ids.is_empty());
    assert!(engine.edit_window_ids.is_empty());
}

#[test]
fn active_minibuffer_resize_uses_buffer_local_max_mini_window_height() {
    let mut eval = Context::new();
    eval.obarray_mut()
        .set_symbol_value("resize-mini-windows", Value::symbol("grow-only"));
    eval.obarray_mut()
        .set_symbol_value("max-mini-window-height", Value::fixnum(10));

    let root_buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let minibuf_id = eval.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    {
        let buf = eval
            .buffer_manager_mut()
            .get_mut(minibuf_id)
            .expect("buffer");
        buf.set_buffer_local("max-mini-window-height", Value::fixnum(1));
        buf.insert("Find file: \nalpha.el\nbeta.el\ngamma.el");
        let eob = buf.point_max_emacs_byte_pos().get();
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(eob));
    }

    let frame_id = eval.frame_manager_mut().create_frame(
        "layout-active-minibuffer-local-max",
        120,
        40,
        root_buf_id,
    );
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.shrink_mini_window();
    }

    let minibuffer_window_id = eval
        .activate_minibuffer_window_for_buffer(minibuf_id, LispString::from_utf8(""), None)
        .expect("activate minibuffer")
        .expect("minibuffer window");

    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == minibuffer_window_id.0 as i64)
        .expect("minibuffer matrix entry");
    let rows = enabled_window_row_texts(entry);
    let content_rows = rows.iter().filter(|row| !row.trim().is_empty()).count();

    assert_eq!(
        content_rows, 1,
        "GNU resize_mini_window reads max-mini-window-height in the minibuffer buffer; rows={rows:?}"
    );
}

/// Content taller than `max-mini-window-height` must clamp to the max row
/// count AND scroll so the END shows (GNU `resize_mini_window` sets `w->start`
/// to the end when the measured height exceeds `max_height`).  Point is at EOB,
/// as it is in an active fido/vertico minibuffer.
fn assert_active_minibuffer_overflow_clamps_and_shows_end(use_gui_metrics: bool) {
    // Eight candidate lines but max-mini-window-height = 3 lines.
    let rows = layout_active_minibuffer_rows(
        "PROMPT\ncand1\ncand2\ncand3\ncand4\ncand5\ncand6\nLASTCAND",
        3,
        use_gui_metrics,
    );
    let content_rows = rows.iter().filter(|row| !row.trim().is_empty()).count();
    assert!(
        content_rows <= 3,
        "expected minibuffer height clamped to <= 3 rows, got {content_rows}: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("LASTCAND")),
        "expected overflow minibuffer to show the END (LASTCAND), rows={rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("PROMPT")),
        "expected the first line to scroll off the top on overflow, rows={rows:?}"
    );
}

#[test]
fn active_minibuffer_overflow_clamps_and_shows_end_tty() {
    assert_active_minibuffer_overflow_clamps_and_shows_end(false);
}

#[test]
fn active_minibuffer_overflow_clamps_and_shows_end_gui() {
    assert_active_minibuffer_overflow_clamps_and_shows_end(true);
}

/// A single logical line (no newline) wider than the window wraps to more rows
/// than `max-mini-window-height`; it must clamp to the max and show the END of
/// the wrapped line (GNU's bottom-clamped path snapping `w->start` to a
/// screen-line boundary with `move_it_by_lines`).
#[test]
fn active_minibuffer_wrapped_overflow_clamps_and_shows_end_tty() {
    // 640px / 1px char => 640 cols.  Build a single line far wider than that
    // with unique START and END markers so we can detect which screen line is
    // shown.  max-mini-window-height = 2 lines.
    let mut line = String::from("WRAPSTART");
    line.push_str(&"x".repeat(640 * 6));
    line.push_str("WRAPEND");
    let rows = layout_active_minibuffer_rows(&line, 2, false);
    let content_rows = rows.iter().filter(|row| !row.trim().is_empty()).count();
    assert!(
        content_rows <= 2,
        "expected wrapped overflow clamped to <= 2 rows, got {content_rows}: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("WRAPEND")),
        "expected wrapped overflow to show the END of the line, rows={rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("WRAPSTART")),
        "expected the START of the wrapped line to scroll off, rows={rows:?}"
    );
}

#[test]
fn build_tab_bar_display_roots_transient_string_across_gc() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-tab-bar-gc", 1600, 160, buf_id);
    eval.obarray_mut()
        .set_symbol_value("layout-target-frame", Value::make_frame(frame_id.0));
    eval.eval_str(
        r#"
          (require 'tab-bar)
          ;; Keep this GC-rooting test independent from tab-bar's display
          ;; resizing and close-button formatting.
          (setq tab-bar-show 1
                tab-bar-auto-width nil
                tab-bar-close-button-show nil)
          (tab-bar-mode 1)
          (select-frame layout-target-frame)
          (switch-to-buffer (get-buffer-create "*tab-root*"))
          (tab-bar-new-tab)
          (switch-to-buffer (get-buffer-create "*tab-second*"))
          (tab-bar-select-tab 1)
        "#,
    )
    .expect("eval tab-bar forms");

    let gc_roots = ScratchGcRootScope::new();
    let tab_bar = build_tab_bar_display(&mut eval, frame_id.0, &gc_roots).expect("tab-bar display");
    let before_gc = tab_bar
        .text
        .as_runtime_string_owned()
        .expect("tab-bar text should be built before exact GC");
    assert!(
        before_gc.contains("*tab-root*") && before_gc.contains("*tab-second*"),
        "expected full tab-bar labels before exact GC, got {before_gc:?}"
    );
    eval.gc_collect_exact();

    let text = tab_bar
        .text
        .as_runtime_string_owned()
        .expect("tab-bar text should survive exact GC");
    assert_eq!(
        text, before_gc,
        "tab-bar text should remain unchanged after exact GC"
    );
    let props =
        neovm_core::emacs_core::value::get_string_text_properties_table_for_value(tab_bar.text)
            .expect("tab-bar string properties should survive exact GC");
    assert!(
        props
            .next_property_change_after_char_pos(CharPos0::ZERO)
            .is_some(),
        "tab-bar text properties should remain traversable after exact GC"
    );
}

#[test]
fn layout_frame_rust_renders_tab_bar_text_from_lisp_tab_bar_keymap() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    eval.set_display_host(Box::new(RecordingImageDisplayHost::default()));
    // Bootstrap may or may not install an initial selected
    // frame depending on cache state. Capture whatever exists
    // so we can restore the selection after switching to the
    // target frame for the tab-bar assertions.
    let prior_selected_frame = eval.frame_manager().selected_frame().map(|f| f.id);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-tab-bar", 1600, 160, buf_id);
    eval.obarray_mut()
        .set_symbol_value("layout-target-frame", Value::make_frame(frame_id.0));
    eval.eval_str(
        r#"
          (require 'tab-bar)
          (setq tab-bar-show 1)
          (tab-bar-mode 1)
          (switch-to-buffer (get-buffer-create "*frame-a*"))
          (tab-bar-new-tab)
          (switch-to-buffer (get-buffer-create "*frame-a-2*"))
          (tab-bar-select-tab 1)
          (select-frame layout-target-frame)
          (tab-bar-new-tab)
          (switch-to-buffer (get-buffer-create "*tb-2*"))
          (tab-bar-rename-tab "T中👨‍👩")
          (tab-bar-select-tab 1)
        "#,
    )
    .expect("eval tab-bar forms");
    eval.eval_form(Value::list(vec![
        Value::symbol("select-frame"),
        Value::make_frame(frame_id.0),
        Value::NIL,
    ]))
    .expect("select target frame for tab-bar debug");
    let keymap_debug =
        match eval.eval_form(Value::list(vec![Value::symbol("tab-bar-make-keymap-1")])) {
            Ok(value) => eval
                .eval_form(Value::list(vec![Value::symbol("prin1-to-string"), value]))
                .ok()
                .and_then(|rendered| rendered.as_runtime_string_owned())
                .unwrap_or_else(|| "<render-unavailable>".to_string()),
            Err(err) => format!("<error: {err}>"),
        };
    let tabs_debug = eval
        .eval_str("(prin1-to-string (frame-parameter nil 'tabs))")
        .ok()
        .and_then(|value| value.as_runtime_string_owned())
        .unwrap_or_else(|| "<unavailable>".to_string());
    let format_debug = eval
        .eval_str("(prin1-to-string tab-bar-format)")
        .ok()
        .and_then(|value| value.as_runtime_string_owned())
        .unwrap_or_else(|| "<unavailable>".to_string());
    if let Some(prev) = prior_selected_frame {
        eval.eval_form(Value::list(vec![
            Value::symbol("select-frame"),
            Value::make_frame(prev.0),
            Value::NIL,
        ]))
        .expect("restore selected frame");
    }

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    assert!(
        frame.tab_bar_height > 0,
        "expected tab-bar-mode to reserve frame tab-bar height"
    );
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame
            .parameters
            .insert(Value::symbol("menu-bar-lines"), Value::fixnum(1));
        frame
            .parameters
            .insert(Value::symbol("tool-bar-lines"), Value::fixnum(1));
        frame.sync_menu_bar_height_from_parameters();
        frame.sync_tool_bar_height_from_parameters();
        frame.sync_window_area_bounds();
    }

    let mut engine = LayoutEngine::new();
    #[cfg(debug_assertions)]
    neomacs_display_protocol::glyph_matrix::reset_materialize_call_count_for_current_thread();
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    #[cfg(debug_assertions)]
    assert_eq!(
        neomacs_display_protocol::glyph_matrix::materialize_call_count_for_current_thread(),
        0,
        "layout must publish source-addressed pointer metadata without pre-materializing"
    );

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("tab-bar display state");
    assert_ne!(state.presentation_id.get(), 0);
    assert_eq!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .active_presentation()
            .expect("evaluator display presentation")
            .get(),
        state.presentation_id.get(),
        "renderer output and evaluator geometry must belong to one presentation"
    );
    let presented_targets = state
        .frame_chrome
        .band(FrameChromeKind::TabBar)
        .expect("tab-bar band")
        .hit_regions()
        .iter()
        .map(|region| match region.action() {
            neomacs_display_protocol::frame_chrome::ChromeAction::Presented { interaction } => {
                (state.presentation_id.get(), interaction.get())
            }
            action => panic!("tab-bar hit leaked renderer policy: {action:?}"),
        })
        .collect::<Vec<_>>();
    assert!(!presented_targets.is_empty());
    assert!(presented_targets.iter().all(|(presentation, interaction)| {
        eval.resolve_presented_mouse_target(*presentation, *interaction)
            .is_some_and(|target| target.posn_string.is_cons())
    }));
    let semantics = presented_targets
        .iter()
        .filter_map(|(presentation, interaction)| {
            let target = eval.resolve_presented_mouse_target(*presentation, *interaction)?;
            let caption = target.posn_string.cons_car();
            let menu_item = eval
                .eval_form(Value::list(vec![
                    Value::symbol("get-text-property"),
                    Value::fixnum(0),
                    Value::list(vec![Value::symbol("quote"), Value::symbol("menu-item")]),
                    Value::list(vec![Value::symbol("quote"), caption]),
                ]))
                .ok()?;
            neovm_core::emacs_core::value::list_to_vec(&menu_item)
        })
        .collect::<Vec<_>>();
    assert!(semantics.iter().any(|item| {
        item.first().and_then(|value| value.as_symbol_name()) == Some("add-tab")
            && item.get(1).and_then(|value| value.as_symbol_name()) == Some("tab-bar-new-tab")
    }));
    assert!(semantics.iter().any(|item| item.get(2) == Some(&Value::T)));

    let materialized_pointer = state.materialize();
    #[cfg(debug_assertions)]
    assert_eq!(
        neomacs_display_protocol::glyph_matrix::materialize_call_count_for_current_thread(),
        1,
        "the consumer performs exactly one canonical materialization"
    );
    assert!(
        !materialized_pointer
            .presented_pointer()
            .appearances()
            .is_empty(),
        "tab-bar source mouse-face properties must publish renderer appearances"
    );
    let highlighted_face = materialized_pointer
        .presented_pointer()
        .appearances()
        .iter()
        .find_map(|appearance| match appearance.hover() {
            neomacs_display_protocol::PointerDrawMode::Face(face_id) => Some(face_id),
            neomacs_display_protocol::PointerDrawMode::ImageRelief(_) => None,
        })
        .expect("standard tab captions should publish a mouse-face override");
    assert_eq!(
        materialized_pointer
            .render_face(highlighted_face)
            .and_then(|face| face.lisp_name.as_deref()),
        Some("tab-bar-tab-highlight")
    );
    assert!(
        materialized_pointer
            .presented_pointer()
            .regions()
            .iter()
            .any(|left| materialized_pointer
                .presented_pointer()
                .regions()
                .iter()
                .any(|right| {
                    left.interaction() != right.interaction()
                        && left.appearance().is_some()
                        && left.appearance() == right.appearance()
                })),
        "tab body and close source slots should keep distinct clicks while sharing mouse-face"
    );

    let tab_bar_text = engine
        .last_frame_display_state
        .as_ref()
        .and_then(|state| {
            state.frame_chrome.bands().iter().find_map(|band| {
                let FrameChromeContent::DisplayRow(content) = band.content() else {
                    return None;
                };
                Some(glyphs_logical_text(&content.row().glyphs[1]))
            })
        })
        .unwrap_or_default();

    assert!(
        tab_bar_text.contains("T中👨‍👩"),
        "expected tab-bar row to render tab captions from tab-bar keymap, got {tab_bar_text:?}; tabs={tabs_debug}; format={format_debug}; keymap={keymap_debug}"
    );
    let tab_bar_glyphs = engine
        .last_frame_display_state
        .as_ref()
        .and_then(|state| {
            state.frame_chrome.bands().iter().find_map(|band| {
                let FrameChromeContent::DisplayRow(content) = band.content() else {
                    return None;
                };
                Some(content.row().glyphs[1].clone())
            })
        })
        .unwrap_or_default();
    assert!(
        tab_bar_glyphs
            .iter()
            .filter(|glyph| !glyph.padding)
            .all(|glyph| glyph.pixel_width > 0.0),
        "expected tab-bar glyphs to carry display-row pixel widths: {tab_bar_glyphs:?}"
    );
    let cjk = tab_bar_glyphs
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }))
        .expect("tab-bar CJK glyph");
    assert!(
        cjk.wide,
        "tab-bar CJK glyph should use the shared wide-glyph builder: {tab_bar_glyphs:?}"
    );
    assert!(
        tab_bar_glyphs.iter().any(|glyph| glyph.padding),
        "tab-bar CJK glyph should retain its padding cell: {tab_bar_glyphs:?}"
    );
    assert!(
        tab_bar_glyphs.iter().any(
            |glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.as_ref() == "👨‍👩")
        ),
        "tab-bar ZWJ emoji should be clustered by the shared builder: {tab_bar_glyphs:?}"
    );
    let window_tab_bar_rows = engine
        .last_frame_display_state
        .as_ref()
        .map(|state| {
            state
                .window_matrices
                .iter()
                .flat_map(|wm| wm.matrix.rows.iter())
                .filter(|row| row.role == GlyphRowRole::TabBar && row.enabled)
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        window_tab_bar_rows, 0,
        "expected frame tab bar to live in FrameChrome, not in leaf-window matrices"
    );
    // Note: a previous version of this test also asserted
    // `!tab_bar_text.contains("*frame-a-2*")` as a
    // "frame-isolation" check. The tab-bar.el keymap produced
    // by `tab-bar-make-keymap-1` walks all tabs reachable from
    // the current frame's `tabs` parameter and does not
    // filter by which frame created each tab, so the negative
    // assertion was testing a speculative behavior that isn't
    // part of the render contract. Dropping it keeps the
    // primary "renders any target-frame text at all" check
    // and leaves frame-scoped tab isolation as a separate
    // concern.
}

#[test]
fn layout_frame_rust_publishes_authoritative_frame_chrome() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("authoritative-frame-chrome", 640, 180, buf_id);
    eval.obarray_mut()
        .set_symbol_value("layout-target-frame", Value::make_frame(frame_id.0));
    eval.eval_str(
        r#"
          (require 'tab-bar)
          (setq tab-bar-format '(tab-bar-format-tabs))
          (select-frame layout-target-frame nil)
        "#,
    )
    .expect("configure frame chrome");
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.parent_frame = Value::NIL;
        frame.window_system = Some(Value::symbol("neomacs"));
        frame.displays_chrome = true;
        frame.char_height = 18.0;
        frame
            .parameters
            .insert(Value::symbol("menu-bar-lines"), Value::fixnum(1));
        frame
            .parameters
            .insert(Value::symbol("tool-bar-lines"), Value::fixnum(1));
        frame
            .parameters
            .insert(Value::symbol("compact-bar-lines"), Value::fixnum(0));
        frame.sync_menu_bar_height_from_parameters();
        frame.sync_tool_bar_height_from_parameters();
        frame.sync_compact_bar_height_from_parameters();
        frame.tab_bar_height = 18;
        frame.sync_window_area_bounds();
    }

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let bands = state.frame_chrome.bands();
    let frame = eval.frame_manager().get(frame_id).expect("frame");
    assert_eq!(
        bands.len(),
        3,
        "published kinds={:?}, frame heights=({}, {}, {}, {}), parent={:?}",
        bands.iter().map(|band| band.kind()).collect::<Vec<_>>(),
        frame.menu_bar_height,
        frame.tool_bar_height,
        frame.compact_bar_height,
        frame.tab_bar_height,
        frame.parent_frame,
    );
    assert_eq!(bands[0].kind(), FrameChromeKind::MenuBar);
    assert_eq!(bands[0].bounds().y(), 0.0);
    assert_eq!(bands[1].kind(), FrameChromeKind::ToolBar);
    assert_eq!(bands[1].bounds().y(), bands[0].bounds().height());
    assert_eq!(bands[2].kind(), FrameChromeKind::TabBar);
    assert_eq!(
        bands[2].bounds().y(),
        bands[0].bounds().height() + bands[1].bounds().height()
    );
    let FrameChromeContent::DisplayRow(tab_row) = bands[2].content() else {
        panic!("tab bar must contain a display row");
    };
    assert_eq!(tab_row.row().pixel_y, 0.0);
}

#[test]
fn layout_frame_rust_tab_bar_does_not_record_buffer_selection() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-tab-bar-no-record", 1600, 160, buf_id);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.tab_bar_height = 17;
    }
    eval.obarray_mut()
        .set_symbol_value("layout-target-frame", Value::make_frame(frame_id.0));
    eval.eval_str(
        r#"
          (require 'tab-bar)
          (setq tab-bar-show 1
                tab-bar-auto-width nil
                tab-bar-close-button-show nil)
          (tab-bar-mode 1)
          (select-frame layout-target-frame)
          (switch-to-buffer (get-buffer-create "*tab-no-record-root*"))
          (tab-bar-new-tab)
          (switch-to-buffer (get-buffer-create "*tab-no-record-second*"))
          (tab-bar-select-tab 1)
          (setq neomacs-layout-buffer-list-hook-count 0)
          (setq buffer-list-update-hook
                (list (lambda ()
                        (setq neomacs-layout-buffer-list-hook-count
                              (1+ neomacs-layout-buffer-list-hook-count)))))
        "#,
    )
    .expect("configure tab-bar hook counter");
    eval.eval_str("(select-window (selected-window))")
        .expect("direct select-window should be callable");
    let direct_hook_count = eval
        .eval_str("neomacs-layout-buffer-list-hook-count")
        .expect("direct hook count")
        .as_fixnum();
    assert_eq!(
        direct_hook_count,
        Some(1),
        "test setup should observe recorded select-window through buffer-list-update-hook"
    );
    eval.eval_str("(setq neomacs-layout-buffer-list-hook-count 0)")
        .expect("reset hook count");

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let tab_bar_rows = engine
        .last_frame_display_state
        .as_ref()
        .map(|state| {
            usize::from(state.frame_chrome.bands().iter().any(|band| {
                matches!(band.content(), FrameChromeContent::DisplayRow(content) if content.row().enabled)
            }))
        })
        .unwrap_or(0);
    assert!(
        tab_bar_rows > 0,
        "test setup must exercise frame tab-bar rendering"
    );

    let hook_count = eval
        .eval_str("neomacs-layout-buffer-list-hook-count")
        .expect("hook count")
        .as_fixnum();
    assert_eq!(
        hook_count,
        Some(0),
        "redisplay tab-bar rendering must not record buffer selection or run buffer-list-update-hook"
    );
}

#[test]
fn layout_frame_rust_installs_frame_tab_bar_image_media() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    let requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::clone(&requests),
        video_requests: Arc::new(Mutex::new(Vec::new())),
        webkit_requests: Arc::new(Mutex::new(Vec::new())),
        surface_requests: Arc::new(Mutex::new(Vec::new())),
    }));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
    }
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("layout-tab-bar-image", 640, 160, buf_id);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.tab_bar_height = 17;
    }
    eval.obarray_mut()
        .set_symbol_value("layout-target-frame", Value::make_frame(frame_id.0));
    eval.eval_str(
        r#"
          (require 'tab-bar)
          (setq tab-bar-format
                (list (lambda ()
                        (propertize
                         "I"
                         'display
                         '(image :type png
                                 :file "/tmp/neomacs-frame-tab-bar.png"
                                 :max-width 32
                                 :max-height 24)))))
          (select-frame layout-target-frame nil)
        "#,
    )
    .expect("configure tab-bar image format");

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let tab_bar_band = state
        .frame_chrome
        .band(FrameChromeKind::TabBar)
        .expect("frame tab-bar band");
    let FrameChromeContent::DisplayRow(tab_bar_row) = tab_bar_band.content() else {
        panic!("tab bar display row");
    };
    let materialized = state.materialize();
    let image = materialized
        .glyphs
        .iter()
        .find(|glyph| {
            matches!(
                glyph,
                FrameGlyph::Image {
                    row_role: GlyphRowRole::TabBar,
                    ..
                }
            )
        })
        .expect("frame tab-bar image side item");

    assert_eq!(image.geometry().expect("image geometry").width, 32.0);
    assert_eq!(image.geometry().expect("image geometry").height, 24.0);
    assert_eq!(tab_bar_row.row().height_px, 24.0);
    assert_eq!(tab_bar_band.bounds().height(), 24.0);
    assert_eq!(image.clip_rect(), Some(tab_bar_band.bounds().raw()));
    assert_eq!(image.slot_id().expect("tab-bar image slot id").row, 0);
    let requests = requests.lock().expect("requests lock");
    assert!(
        !requests.is_empty(),
        "expected at least one tab-bar image realization request"
    );
    assert!(
        requests
            .iter()
            .all(|request| request.max_width == 32 && request.max_height == 24),
        "unexpected image realization requests: {requests:?}"
    );
}

#[test]
fn layout_frame_rust_shrinks_frame_tab_bar_from_stale_reserved_height() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    let requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingImageDisplayHost {
        requests: Arc::clone(&requests),
        video_requests: Arc::new(Mutex::new(Vec::new())),
        webkit_requests: Arc::new(Mutex::new(Vec::new())),
        surface_requests: Arc::new(Mutex::new(Vec::new())),
    }));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("body line\n");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-tab-bar-stale-height", 640, 220, buf_id);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.tab_bar_height = 120;
        frame.sync_window_area_bounds();
    }
    eval.obarray_mut()
        .set_symbol_value("layout-target-frame", Value::make_frame(frame_id.0));
    eval.eval_str(
        r#"
          (require 'tab-bar)
          (setq tab-bar-format
                (list (lambda ()
                        (propertize
                         "I"
                         'display
                         '(image :type png
                                 :file "/tmp/neomacs-frame-tab-bar.png"
                                 :max-width 32
                                 :max-height 24)))))
          (select-frame layout-target-frame nil)
        "#,
    )
    .expect("configure tab-bar image format");

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let tab_bar_band = state
        .frame_chrome
        .band(FrameChromeKind::TabBar)
        .expect("frame tab-bar band");
    let FrameChromeContent::DisplayRow(tab_bar_row) = tab_bar_band.content() else {
        panic!("tab bar display row");
    };

    assert_eq!(tab_bar_row.row().height_px, 24.0);
    assert_eq!(tab_bar_band.bounds().height(), 24.0);
    assert_eq!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .tab_bar_height,
        24
    );
    let materialized = state.materialize();
    let image = materialized
        .glyphs
        .iter()
        .find(|glyph| {
            matches!(
                glyph,
                FrameGlyph::Image {
                    row_role: GlyphRowRole::TabBar,
                    ..
                }
            )
        })
        .expect("frame tab-bar image side item");
    assert_eq!(image.clip_rect(), Some(tab_bar_band.bounds().raw()));
    assert!(
        !requests.lock().expect("requests lock").is_empty(),
        "expected at least one tab-bar image realization request"
    );
}

#[test]
fn layout_frame_rust_keeps_echo_message_in_minibuffer_window_for_tty() {
    assert_echo_message_renders_in_minibuffer_window(false);
}

#[test]
fn layout_frame_rust_keeps_echo_message_in_minibuffer_window_for_gui() {
    assert_echo_message_renders_in_minibuffer_window(true);
}

#[test]
fn layout_frame_rust_resizes_multiline_echo_rows_for_tty() {
    assert_multiline_echo_message_resizes_minibuffer_rows(false);
}

#[test]
fn layout_frame_rust_resizes_multiline_echo_rows_for_gui() {
    assert_multiline_echo_message_resizes_minibuffer_rows(true);
}

#[test]
fn test_cursor_point_columns_wide_char() {
    let params = test_window_params();
    let text = "你".as_bytes();
    assert_eq!(cursor_point_columns(text, 0, 0, &params), 2);
}

#[test]
fn test_cursor_point_columns_tab_uses_tab_stop_list() {
    let mut params = test_window_params();
    params.tab_width = 8;
    params.tab_stop_list = vec![4, 10];
    let text = b"\t";

    assert_eq!(cursor_point_columns(text, 0, 3, &params), 1);
    assert_eq!(cursor_point_columns(text, 0, 4, &params), 6);
}

#[test]
fn test_cursor_width_for_style_bar_uses_bar_width() {
    let params = test_window_params();
    let text = "你".as_bytes();

    let width = cursor_width_for_style(CursorStyle::Bar(2.5), text, 0, 0, &params, 7.0);
    assert_eq!(width, 2.5);
}

#[test]
fn test_cursor_width_for_style_tab_clamps_when_x_stretch_cursor_is_nil() {
    let params = test_window_params();
    let text = b"\t";

    let width = cursor_width_for_style(CursorStyle::FilledBox, text, 0, 1, &params, 8.0);
    assert_eq!(width, 8.0);
}

#[test]
fn test_cursor_width_for_style_tab_expands_when_x_stretch_cursor_is_t() {
    let mut params = test_window_params();
    params.x_stretch_cursor = true;
    let text = b"\t";

    let width = cursor_width_for_style(CursorStyle::FilledBox, text, 0, 1, &params, 8.0);
    assert_eq!(width, 56.0);
}

#[test]
fn test_cursor_width_for_style_hbar_uses_glyph_columns() {
    let params = test_window_params();
    let text = "你".as_bytes();

    let width = cursor_width_for_style(CursorStyle::Hbar(2.0), text, 0, 0, &params, 7.0);
    assert_eq!(width, 14.0);
}

#[test]
fn cursor_slot_width_policy_names_style_and_buffer_width_sources() {
    let mut params = test_window_params();
    params.char_width = 6.0;
    let text = b"\t";

    assert_eq!(
        CursorSlotWidthRequest::from_window_params(CursorStyle::Bar(2.5), text, 0, 1, &params)
            .width_policy(),
        CursorSlotWidthPolicy::ExplicitPixels(2.5)
    );
    assert_eq!(
        CursorSlotWidthRequest::from_window_params(CursorStyle::FilledBox, text, 0, 1, &params)
            .width_policy(),
        CursorSlotWidthPolicy::TabClamp {
            frame_char_width: 6.0,
        }
    );

    params.x_stretch_cursor = true;
    assert_eq!(
        CursorSlotWidthRequest::from_window_params(CursorStyle::FilledBox, text, 0, 1, &params)
            .width_policy(),
        CursorSlotWidthPolicy::GlyphColumns(7)
    );
    assert_eq!(
        CursorSlotWidthRequest::from_window_params(
            CursorStyle::Hbar(2.0),
            "你".as_bytes(),
            0,
            0,
            &params,
        )
        .width_policy(),
        CursorSlotWidthPolicy::GlyphColumns(2)
    );
}

#[test]
fn cursor_slot_width_policy_tab_clamp_uses_frame_char_width() {
    let mut params = test_window_params();
    params.char_width = 6.0;
    let text = b"\t";

    let policy =
        CursorSlotWidthRequest::from_window_params(CursorStyle::FilledBox, text, 0, 1, &params)
            .width_policy();

    assert_eq!(policy.width_px(8.0), 6.0);
}

#[test]
fn test_cursor_style_for_nonselected_bar_uses_resolved_width() {
    let mut params = test_window_params();
    params.selected = false;
    params.cursor_kind = neomacs_display_protocol::frame_glyphs::CursorKind::Bar;
    params.cursor_bar_width = CursorBarWidth::new(4);

    assert_eq!(
        cursor_style_for_window(&params),
        Some(CursorStyle::Bar(4.0))
    );
}

#[test]
fn test_cursor_style_for_nonselected_no_cursor_is_none() {
    let mut params = test_window_params();
    params.selected = false;
    params.cursor_kind = neomacs_display_protocol::frame_glyphs::CursorKind::NoCursor;

    assert_eq!(cursor_style_for_window(&params), None);
}

#[test]
fn test_resolve_cursor_vertical_metrics_uses_row_metrics() {
    let (y, height, ascent) =
        resolve_cursor_vertical_metrics(20.0, 24.0, 18.0, 24.0, 14.0, 16.0, false);

    assert_eq!(y, 16.0);
    assert_eq!(height, 24.0);
    assert_eq!(ascent, 18.0);
}

#[test]
fn test_resolve_cursor_vertical_metrics_preserves_eob_origin() {
    let (y, height, ascent) =
        resolve_cursor_vertical_metrics(20.0, 24.0, 18.0, 24.0, 14.0, 16.0, true);

    assert_eq!(y, 20.0);
    assert_eq!(height, 20.0);
    assert_eq!(ascent, 14.0);
}

/// Child-frame independence (Slice 4 characterization; guards the Slice 5
/// mock-frame migration + the posframe face/width-independence property).
///
/// A detached child frame must carry its OWN identity (frame_id/parent_*/z_order),
/// resolve its OWN faces, and derive its OWN text width — never inherit the
/// parent's. Here an 800px parent and a 200px child must produce different
/// column counts, and the child state must report its own frame identity.
#[test]
fn child_frame_resolves_faces_and_width_independently_from_parent() {
    use crate::mock_frame::{
        MockChildFrameContent, MockFrameContent, MockStyledLine, MockWindowContent,
    };
    use neomacs_display_protocol::face::Face;
    use neomacs_display_protocol::types::{Color, Rect};

    let char_w = 8.0;
    let char_h = 16.0;

    let content = MockFrameContent {
        frame_id: 1,
        faces: vec![Face::new(FaceId::new(0))],
        windows: vec![MockWindowContent {
            window_id: 1,
            lines: vec![MockStyledLine::from_str(
                "parent buffer text",
                FaceId::new(0),
            )],
            mode_line: MockStyledLine::from_str("-- parent --", FaceId::new(0)),
            // Wide parent window: 800px / 8px = 100 cols.
            pixel_bounds: Rect::new(0.0, 0.0, 800.0, 15.0 * char_h),
            selected: true,
            truncated_lines: false,
        }],
        child_frames: vec![MockChildFrameContent {
            frame_id: 100,
            window: MockWindowContent {
                window_id: 2,
                lines: vec![MockStyledLine::from_str("child", FaceId::new(0))],
                mode_line: MockStyledLine::from_str("", FaceId::new(0)),
                // Narrow child: 200px / 8px = 25 cols — independent of the parent.
                pixel_bounds: Rect::new(0.0, 0.0, 200.0, 3.0 * char_h),
                selected: false,
                truncated_lines: false,
            },
            parent_x: 120.0,
            parent_y: 48.0,
            z_order: 1,
        }],
        frame_pixel_width: 800.0,
        frame_pixel_height: 16.0 * char_h,
        background: Color::from_pixel(0x00112233),
        menu_bar: None,
        minibuffer: Some(MockWindowContent {
            window_id: 999,
            lines: vec![MockStyledLine::from_str("", FaceId::new(0))],
            mode_line: MockStyledLine::from_str("", FaceId::new(0)),
            pixel_bounds: Rect::new(0.0, 15.0 * char_h, 800.0, char_h),
            selected: false,
            truncated_lines: false,
        }),
    };

    let mut engine = LayoutEngine::new();
    let states = engine.layout_mock_frame(&content, char_w, char_h);

    assert!(states.len() >= 2, "expected a parent + child frame state");
    let parent = &states[0];
    let child = &states[1];

    // The child carries its OWN identity, not the parent's.
    assert_eq!(child.frame_placement.frame().get(), 100);
    assert_eq!(
        child.frame_placement.parent().unwrap().get(),
        content.frame_id
    );
    assert_eq!(child.frame_placement.outer_in_parent().x(), 120.0);
    assert_eq!(child.frame_placement.outer_in_parent().y(), 48.0);
    assert_eq!(child.frame_placement.z_order(), 1);

    // The child resolves its own face map (not an empty/parent-shared identity).
    assert!(
        !child.faces.is_empty(),
        "child frame must resolve its own faces"
    );

    // The child's text width is independent of the parent's: 200px vs 800px
    // produce different column counts.
    let parent_cols = parent.window_matrices[0].matrix.ncols;
    let child_cols = child.window_matrices[0].matrix.ncols;
    assert_ne!(
        child_cols, parent_cols,
        "child width (200px) must derive its own ncols, not inherit the parent (800px): child={child_cols} parent={parent_cols}"
    );
}

/// Frame-snapshot contract: the display state must identify windows by
/// buffer NAME (not just file name) and realized faces by their Lisp face
/// name, so agents can assert on them from snapshot JSON/text.
#[test]
fn window_info_carries_buffer_name_and_faces_carry_lisp_names() {
    let mut eval = Context::new();
    convert_current_buffer_text_backend(&mut eval, BufferTextBackendKind::GapBuffer);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    insert_fragmented_current_buffer_text(&mut eval, "snapshot names\n");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("snapshot-names", 360, 180, buf_id);

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");

    let info = state
        .window_infos
        .iter()
        .find(|w| !w.is_minibuffer)
        .expect("non-minibuffer window info");
    assert!(
        !info.buffer_name.is_empty(),
        "buffer_name must be populated: {info:?}"
    );
    assert!(
        state
            .faces
            .values()
            .any(|f| f.lisp_name.as_deref() == Some("default")),
        "realized default face must carry its Lisp name: {:?}",
        state
            .faces
            .values()
            .map(|f| (f.id, f.lisp_name.clone()))
            .collect::<Vec<_>>()
    );
}

/// Phase 3 — a PLAIN (non-font-locked) edit takes the localized-edit fast path:
/// the rows ABOVE the edit are reused verbatim and only the edited line + the
/// rows below it are re-walked.
#[test]
fn phase3_plain_edit_is_localized() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, _win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        // Plain insert on line 10 (no fontification → props tick unchanged).
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10 * 24 + 5));
        buffer.insert("x");
    });
    assert!(m.total_body_rows > 0, "expected body rows laid out");
    assert_eq!(
        m.stats.edit_windows, 1,
        "selected window took the localized-edit fast path (got {:?})",
        m.stats
    );
    assert!(
        m.stats.reused_rows > 0,
        "rows above the edit reused verbatim (got {:?})",
        m.stats
    );
    assert!(
        m.stats.relaid_body_rows > 0,
        "the edited line + rows below relaid (got {:?})",
        m.stats
    );
    assert_eq!(
        m.stats.reused_rows + m.stats.relaid_body_rows,
        m.total_body_rows,
        "body rows conserved (got {:?})",
        m.stats
    );
}

/// Phase 3 GOLDEN — localized-edit output must be BYTE-IDENTICAL to a full
/// rebuild of the same edited state.
#[test]
fn phase3_plain_edit_matches_full_rebuild_golden() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let edit_at = 10 * 24 + 5;

    let (mut eval_ref, frame_ref, buf_ref, _wr) = incr_editing_frame(&text, 800, 600);
    {
        let buffer = eval_ref
            .buffer_manager_mut()
            .get_mut(buf_ref)
            .expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_at));
        buffer.insert("x");
    }
    let mut ref_engine = LayoutEngine::new();
    ref_engine.layout_frame_rust(&mut eval_ref, frame_ref);
    let reference = selected_window_layout_trace(&eval_ref, &ref_engine, frame_ref);

    let (mut eval, frame_id, buf_id, _wi) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_at));
        buffer.insert("x");
    }
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().edit_windows,
        1,
        "expected the measured pass to take the localized-edit fast path"
    );
    let incremental = selected_window_layout_trace(&eval, &engine, frame_id);

    assert_eq!(
        incremental, reference,
        "localized-edit output must be byte-identical to a full rebuild"
    );
}

#[test]
fn phase3_delete_at_eob_does_not_relay_reused_prefix_from_buffer_start() {
    let text = ";; first scratch line\n;; second scratch line\n\n";
    let with_typed_char = format!("{text}a");
    let (mut eval, frame_id, buf_id, win) = incr_editing_frame(&with_typed_char, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);

    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        let end = buffer.point_max_emacs_byte_pos().get();
        buffer.delete_emacs_byte_range(emacs_byte_range(end - 1, end));
    }
    engine.layout_frame_rust(&mut eval, frame_id);

    assert_eq!(
        engine.last_layout_stats().edit_windows,
        1,
        "expected the EOB deletion to exercise localized edit replay"
    );
    let state = engine.last_frame_display_state.as_ref().expect("state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == win.0 as i64)
        .expect("selected window");
    let rendered = enabled_window_row_texts(entry).join("");
    assert_eq!(
        rendered.matches(";; first scratch line").count(),
        1,
        "localized EOB deletion must not append a second copy of reused prefix rows: {rendered:?}"
    );
}

/// REGRESSION (yank-pop first-row corruption): a localized-edit INSERT with the
/// below-reuse fast path must not shift the charpos of the trailing
/// past-last-line placeholder row. Insert `delta` chars on a content line that
/// has real content + a trailing empty line below it; the edit-replay output
/// must be byte-identical (row role/start/end/pixel_y) to a full rebuild of the
/// same final state. Before the fix, the trailing row's charpos is shifted by
/// `delta` (e.g. (0,0) -> (delta,delta)) — a phantom row at pixel_y 0.
#[test]
fn edit_below_reuse_does_not_shift_trailing_placeholder_row() {
    let text = "alpha line\nbeta line\ncharlie line\ndelta line\n";
    let edit_byte: usize = 12; // inside "charlie" on line 3 (below has "delta line" + EOB)

    let run = |lay_incrementally: bool| -> Vec<(GlyphRowRole, usize, usize, u32, bool, bool)> {
        let (mut eval, frame_id, buf_id, _win) = incr_editing_frame(text, 800, 600);
        let mut engine = LayoutEngine::new();
        if lay_incrementally {
            engine.layout_frame_rust(&mut eval, frame_id);
        }
        {
            let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_byte));
            buffer.insert("X");
        }
        engine.layout_frame_rust(&mut eval, frame_id);
        selected_window_layout_trace(&eval, &engine, frame_id)
            .matrix_rows
            .iter()
            .map(|r| {
                (
                    r.role,
                    r.start_charpos,
                    r.end_charpos,
                    r.pixel_y_bits,
                    r.displays_text,
                    r.ends_at_zv,
                )
            })
            .collect()
    };
    let incremental = run(true);
    let reference = run(false);
    assert_eq!(
        incremental, reference,
        "\nedit below-reuse must not corrupt the trailing placeholder row\n INC={:#?}\n REF={:#?}",
        incremental, reference
    );
}

/// Phase 5 (#44) — the fast paths attach `RowDamage` to each authoritative
/// matrix row: cursor-only reuses every body row (`Reused`), scroll marks its
/// reused rows `ReusedShifted{dvpos}`, and a full rebuild is all `New`.
#[test]
fn phase5_fast_paths_emit_row_damage() {
    use neomacs_display_protocol::glyph_matrix::RowDamage;

    fn selected_damage(
        engine: &LayoutEngine,
        win: neovm_core::window::WindowId,
    ) -> (Vec<RowDamage>, Vec<GlyphRowRole>) {
        let state = engine
            .last_frame_display_state
            .as_ref()
            .expect("display state");
        let entry = state
            .window_matrices
            .iter()
            .find(|e| e.window_id.get() == win.0 as i64)
            .expect("selected window matrix");
        (
            entry.matrix.rows.iter().map(|row| row.damage).collect(),
            entry.matrix.rows.iter().map(|r| r.role).collect(),
        )
    }

    // --- cursor-only: all body rows Reused ---
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10));
    }
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
    let (damage, roles) = selected_damage(&engine, win);
    let mut reused_body = 0;
    for (dmg, role) in damage.iter().zip(&roles) {
        if *role == GlyphRowRole::Text {
            assert_eq!(*dmg, RowDamage::Reused, "cursor-only body row is Reused");
            reused_body += 1;
        }
    }
    assert!(reused_body > 0, "expected reused body rows");

    // --- scroll: reused rows ReusedShifted with a nonzero dvpos ---
    let line = "(defun f (a b) (+ a b))\n";
    let big = line.repeat(80);
    let (mut eval2, frame2, buf2, win2) = incr_editing_frame(&big, 800, 600);
    let mut engine2 = LayoutEngine::new();
    engine2.layout_frame_rust(&mut eval2, frame2);
    scroll_window_to(
        &mut eval2,
        frame2,
        win2,
        buf2,
        5 * line.len() as i64 + 1,
        7 * line.len(),
    );
    engine2.layout_frame_rust(&mut eval2, frame2);
    assert_eq!(engine2.last_layout_stats().scroll_windows, 1);
    let (damage2, _roles2) = selected_damage(&engine2, win2);
    assert!(
        damage2
            .iter()
            .any(|d| matches!(d, RowDamage::ReusedShifted { dvpos } if dvpos.get() != 0.0)),
        "scroll emits ReusedShifted rows with a nonzero dvpos"
    );
}

/// Phase 3 below-reuse (full GNU try_window_id) — a single-line edit that does
/// not change the row structure relays ONLY the edited line: the rows above are
/// reused verbatim AND the rows below are reused with a charpos shift (same
/// pixel_y). So relaid_body_rows is ~1, not "edited line + everything below".
///
/// (Enabled via the engine's `allow_below_reuse` opt-in; production default is
/// off until the render-side post-walk validation lands.)
#[test]
fn phase3_below_reuse_relays_only_edited_line() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, buf_id, _win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.allow_below_reuse = true;
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |eval| {
        // Plain single-char insert mid-line on line 10 (no newline, no wrap).
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(10 * 24 + 5));
        buffer.insert("x");
    });
    assert_eq!(
        m.stats.edit_windows, 1,
        "took the edit fast path (got {:?})",
        m.stats
    );
    assert!(
        m.stats.relaid_body_rows <= 2,
        "below-reuse relays only the edited line, not the rows below it (got {:?})",
        m.stats
    );
}

/// Phase 3 below-reuse GOLDEN — relaying only the edited line + reusing the rows
/// below (charpos-shifted) must be BYTE-IDENTICAL to a full rebuild of the same
/// edited state.
#[test]
fn phase3_below_reuse_matches_full_rebuild_golden() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let edit_at = 10 * 24 + 5;

    let (mut eval_ref, frame_ref, buf_ref, _wr) = incr_editing_frame(&text, 800, 600);
    {
        let buffer = eval_ref
            .buffer_manager_mut()
            .get_mut(buf_ref)
            .expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_at));
        buffer.insert("x");
    }
    let mut ref_engine = LayoutEngine::new();
    ref_engine.layout_frame_rust(&mut eval_ref, frame_ref);
    let reference = selected_window_layout_trace(&eval_ref, &ref_engine, frame_ref);

    let (mut eval, frame_id, buf_id, _wi) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.allow_below_reuse = true;
    engine.layout_frame_rust(&mut eval, frame_id);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_at));
        buffer.insert("x");
    }
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().edit_windows,
        1,
        "took the edit fast path"
    );
    let incremental = selected_window_layout_trace(&eval, &engine, frame_id);

    assert_eq!(
        incremental, reference,
        "below-reuse output must be byte-identical to a full rebuild"
    );
}

/// Drive a warm→edit→measured pass with below-reuse ENABLED (the default),
/// assert the output is byte-identical to a full rebuild of the edited state,
/// and return the measured stats. Used by the below-reuse safety-bail tests.
fn below_reuse_bail_golden_stats(insert_text: &str) -> LayoutStats {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let edit_at = 10 * 24 + 5;

    let (mut eval_ref, frame_ref, buf_ref, _wr) = incr_editing_frame(&text, 800, 600);
    {
        let buffer = eval_ref
            .buffer_manager_mut()
            .get_mut(buf_ref)
            .expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_at));
        buffer.insert(insert_text);
    }
    let mut ref_engine = LayoutEngine::new();
    ref_engine.layout_frame_rust(&mut eval_ref, frame_ref);
    let reference = selected_window_layout_trace(&eval_ref, &ref_engine, frame_ref);

    let (mut eval, frame_id, buf_id, _wi) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new(); // allow_below_reuse defaults to true
    engine.layout_frame_rust(&mut eval, frame_id);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_at));
        buffer.insert(insert_text);
    }
    engine.layout_frame_rust(&mut eval, frame_id);
    let incremental = selected_window_layout_trace(&eval, &engine, frame_id);
    assert_eq!(
        incremental, reference,
        "edit must be byte-identical to a full rebuild (below-reuse must bail safely)"
    );
    engine.last_layout_stats().clone()
}

/// A newline insert changes the line count, so below-reuse (which keeps the rows
/// below at the same pixel_y) MUST bail to above-only — caught by the ASCII gate.
#[test]
fn phase3_below_reuse_bails_on_newline_insert() {
    let stats = below_reuse_bail_golden_stats("\n");
    assert_eq!(
        stats.edit_windows, 1,
        "still the edit fast path (got {stats:?})"
    );
    assert!(
        stats.relaid_body_rows > 2,
        "above-only (not below-reuse, which would relay ~1) — got {stats:?}"
    );
}

/// A long ASCII insert that wraps the edited line changes the row structure, so
/// below-reuse MUST bail to above-only — caught by the width gate.
#[test]
fn phase3_below_reuse_bails_on_wrapping_insert() {
    let stats = below_reuse_bail_golden_stats(&"x".repeat(100));
    assert_eq!(
        stats.edit_windows, 1,
        "still the edit fast path (got {stats:?})"
    );
    assert!(
        stats.relaid_body_rows > 2,
        "above-only (not below-reuse) — got {stats:?}"
    );
}

/// A window whose layout inputs did not change AT ALL (point included) must
/// reuse its retained body verbatim — 0 relaid body rows — instead of
/// full-rebuilding. This is the multi-window win: editing one window leaves the
/// others untouched, so they should cost nothing. (No-change cursor-only.)
#[test]
fn no_change_relayout_reuses_verbatim() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, _buf, _win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    let m = measure_incremental_relayout(&mut engine, &mut eval, frame_id, |_eval| {
        // Nothing changes between the warm and measured passes.
    });
    assert_eq!(
        m.stats.cursor_only_windows, 1,
        "the unchanged main window reuses verbatim via the cursor-only path (got {:?})",
        m.stats
    );
    assert!(
        m.stats.reused_rows >= 30,
        "the main window's body is reused, not relaid (got {:?})",
        m.stats
    );
    // The only body rows relaid are the 1-row minibuffer (excluded from
    // retention as a probe-pass hazard); the 36-row main window relays nothing.
    assert!(
        m.stats.relaid_body_rows <= 1,
        "the unchanged main window's 36 body rows are no longer relaid (got {:?})",
        m.stats
    );
}

/// No-change cursor-only must be BYTE-IDENTICAL to a full rebuild of the same
/// (unchanged) state.
#[test]
fn no_change_relayout_matches_full_rebuild_golden() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval_ref, frame_ref, _br, _wr) = incr_editing_frame(&text, 800, 600);
    let mut ref_engine = LayoutEngine::new();
    ref_engine.layout_frame_rust(&mut eval_ref, frame_ref);
    let reference = selected_window_layout_trace(&eval_ref, &ref_engine, frame_ref);

    let (mut eval, frame_id, _bi, _wi) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id); // warm
    engine.layout_frame_rust(&mut eval, frame_id); // measured: nothing changed
    assert_eq!(
        engine.last_layout_stats().cursor_only_windows,
        1,
        "unchanged window took the no-change cursor-only path"
    );
    let incremental = selected_window_layout_trace(&eval, &engine, frame_id);
    assert_eq!(
        incremental, reference,
        "no-change must be byte-identical to a full rebuild"
    );
}

/// Multi-window cross-window correctness (adversarial-review dimension D6): in a
/// split frame on two DIFFERENT buffers, editing one buffer must leave BOTH
/// windows' output byte-identical to a full rebuild — the edited window via its
/// fast path, the other via full rebuild, with no cross-window corruption.
#[test]
fn multi_window_edit_matches_full_rebuild_for_both_windows() {
    fn setup() -> (
        Context,
        neovm_core::window::FrameId,
        BufferId,
        neovm_core::window::WindowId,
        neovm_core::window::WindowId,
    ) {
        let mut eval = Context::new();
        let left_buf = eval
            .buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id();
        {
            let buf = eval.buffer_manager_mut().get_mut(left_buf).expect("left");
            buf.insert(&"(left line xx)\n".repeat(40));
        }
        let right_buf = eval.buffer_manager_mut().create_buffer("*right-incr*");
        {
            let buf = eval.buffer_manager_mut().get_mut(right_buf).expect("right");
            buf.insert(&"(right line yy)\n".repeat(40));
        }
        let frame_id = eval
            .frame_manager_mut()
            .create_frame("multi-golden", 800, 600, left_buf);
        let left_window = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window;
        let right_window = eval
            .frame_manager_mut()
            .split_window(
                frame_id,
                left_window,
                neovm_core::window::SplitDirection::Horizontal,
                right_buf,
                None,
                neovm_core::window::SplitPlacement::AfterTarget,
            )
            .expect("split");
        (eval, frame_id, left_buf, left_window, right_window)
    }
    let edit_at = 5 * 15 + 4;

    // Reference: full rebuild of the edited state.
    let (mut eval_ref, frame_ref, left_ref, lw_ref, rw_ref) = setup();
    {
        let buf = eval_ref
            .buffer_manager_mut()
            .get_mut(left_ref)
            .expect("left");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_at));
        buf.insert("x");
    }
    let mut ref_engine = LayoutEngine::new();
    ref_engine.layout_frame_rust(&mut eval_ref, frame_ref);
    let ref_left = window_layout_trace(&eval_ref, &ref_engine, frame_ref, lw_ref);
    let ref_right = window_layout_trace(&eval_ref, &ref_engine, frame_ref, rw_ref);

    // Incremental: warm, edit the left buffer, measured.
    let (mut eval, frame_id, left_buf, lw, rw) = setup();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    {
        let buf = eval.buffer_manager_mut().get_mut(left_buf).expect("left");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_at));
        buf.insert("x");
    }
    engine.layout_frame_rust(&mut eval, frame_id);
    let inc_left = window_layout_trace(&eval, &engine, frame_id, lw);
    let inc_right = window_layout_trace(&eval, &engine, frame_id, rw);

    assert_eq!(
        inc_left, ref_left,
        "edited window must be byte-identical to a full rebuild"
    );
    assert_eq!(
        inc_right, ref_right,
        "the OTHER window must be byte-identical (no cross-window corruption)"
    );
}

/// DIAGNOSTIC: the goldens compare matrix face_ids but not the frame faces table.
/// Verify that the cursor-only fast path's reused body glyphs reference face_ids
/// that ARE registered in the (per-frame-cleared) frame faces table — otherwise
/// the renderer cannot resolve them (a latent dangling-face bug the goldens miss).
#[test]
fn cursor_only_reused_body_face_ids_are_registered_in_frame_faces() {
    let text = "(defun f (a b) (+ a b))\n".repeat(40);
    let (mut eval, frame_id, _buf, win) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id); // warm (registers faces)
    engine.layout_frame_rust(&mut eval, frame_id); // no-change cursor-only
    assert_eq!(
        engine.last_layout_stats().cursor_only_windows,
        1,
        "expected cursor-only"
    );
    let state = engine.last_frame_display_state.as_ref().expect("state");
    let faces = &state.faces;
    let entry = state
        .window_matrices
        .iter()
        .find(|e| e.window_id.get() == win.0 as i64)
        .expect("window");
    let mut missing: Vec<FaceId> = Vec::new();
    for row in entry.matrix.rows.iter().filter(|r| r.enabled) {
        for area in row.glyphs.iter() {
            for g in area.iter() {
                if !faces.contains_key(&g.face_id) {
                    missing.push(g.face_id);
                }
            }
        }
    }
    missing.sort_unstable();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "cursor-only reused body uses face_ids NOT in the frame faces table: {missing:?} \
         (faces table has ids {:?})",
        {
            let mut ks: Vec<FaceId> = faces.keys().copied().collect();
            ks.sort_unstable();
            ks
        }
    );
}

#[test]
fn cursor_only_reused_mouse_face_is_registered_in_frame_faces() {
    let text = "click me\n".repeat(40);
    let (mut eval, frame_id, _buf, _win) = incr_editing_frame(&text, 800, 600);
    eval.eval_str("(put-text-property 1 6 'mouse-face 'highlight)")
        .expect("put mouse-face property");
    let mut engine = LayoutEngine::new();

    engine.layout_frame_rust(&mut eval, frame_id);
    let _ = engine
        .last_frame_display_state
        .as_ref()
        .expect("warm display state")
        .materialize();

    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().cursor_only_windows,
        1,
        "expected the cursor-only fast path to retain the body"
    );
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("reused display state");
    assert!(
        !state.presented_pointer_source.appearances().is_empty(),
        "expected the retained mouse-face to publish a pointer appearance"
    );
    let _ = state.materialize();
}

#[test]
fn fido_vertical_minibuffer_has_no_trailing_blank_display_row() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("fido-vertical-row-count", 624, 648, buf_id);
    assert!(eval.frame_manager_mut().select_frame(frame_id));

    let minibuf_id = eval
        .buffer_manager_mut()
        .create_buffer(" *Minibuf-fido-row-count*");
    let minibuffer_window_id = eval
        .activate_minibuffer_window_for_buffer(
            minibuf_id,
            LispString::from_utf8("M-x "),
            Some(LispString::from_utf8("")),
        )
        .expect("activate minibuffer")
        .expect("minibuffer window");
    eval.eval_str(
        r#"(progn
             (fido-vertical-mode 1)
             (setq max-mini-window-height 25)
             (setq minibuffer-completion-table obarray
                   minibuffer-completion-predicate #'commandp
                   minibuffer--require-match t)
             (icomplete-minibuffer-setup)
             (icomplete--fido-mode-setup)
             (icomplete--vertical-minibuffer-setup)
             (setq icomplete-prospects-height 8)
             (icomplete-exhibit))"#,
    )
    .expect("exhibit fido candidates");

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == minibuffer_window_id.0 as i64)
        .expect("minibuffer matrix entry");
    let content_bottom = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.height_px > 0.0)
        .map(|row| row.pixel_y + row.height_px)
        .fold(0.0_f32, f32::max);
    let shortest_display_row = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.height_px > 0.0)
        .map(|row| row.height_px)
        .fold(f32::INFINITY, f32::min);
    let unused_height = entry.pixel_bounds.height - content_bottom;

    assert!(
        unused_height >= 0.0 && unused_height < shortest_display_row,
        "GNU sizes the fido minibuffer from its content pixel extent, leaving less than one display row below the final candidate; content_bottom={content_bottom}, shortest_display_row={shortest_display_row}, unused_height={unused_height}, allocated_height={}",
        entry.pixel_bounds.height,
    );
}

#[test]
fn fido_vertical_explicit_overlay_cursor_is_stable_across_unchanged_redisplay() {
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("fido-vertical-cursor", 624, 648, buf_id);
    assert!(eval.frame_manager_mut().select_frame(frame_id));

    let minibuf_id = eval
        .buffer_manager_mut()
        .create_buffer(" *Minibuf-fido-cursor*");
    let minibuffer_window_id = eval
        .activate_minibuffer_window_for_buffer(
            minibuf_id,
            LispString::from_utf8("M-x "),
            Some(LispString::from_utf8("")),
        )
        .expect("activate minibuffer")
        .expect("minibuffer window");
    eval.eval_str(
        r#"(progn
             (fido-vertical-mode 1)
             (setq max-mini-window-height 25
                   minibuffer-completion-table obarray
                   minibuffer-completion-predicate #'commandp
                   minibuffer--require-match t)
             (icomplete-minibuffer-setup)
             (icomplete--fido-mode-setup)
             (icomplete--vertical-minibuffer-setup)
             (setq icomplete-prospects-height 8)
             (icomplete-exhibit))"#,
    )
    .expect("exhibit fido candidates");

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let first_state = engine
        .last_frame_display_state
        .as_ref()
        .expect("first display state");
    let first_cursor = first_state
        .phys_cursor
        .as_ref()
        .expect("first cursor")
        .clone();
    engine.layout_frame_rust(&mut eval, frame_id);
    let second_cursor = engine
        .last_frame_display_state
        .as_ref()
        .expect("second display state")
        .phys_cursor
        .as_ref()
        .expect("second cursor");

    assert_eq!(first_cursor.window_id.get(), minibuffer_window_id.0 as i64);
    assert_eq!(first_cursor.row, 0);
    assert!(
        first_cursor.col > 0,
        "the full display walk must capture icomplete's explicit overlay cursor, not the empty minibuffer buffer point"
    );
    assert_eq!(
        second_cursor.slot_id, first_cursor.slot_id,
        "GNU honors icomplete's explicit `cursor` text property on every redisplay; the cursor must not jump back to the minibuffer buffer point"
    );
}

#[tracing_test::traced_test]
#[test]
fn mx_tab_completion_materializes_unique_pointer_source_identities() {
    // Reproduce the real GUI command path: M-x text followed by TAB runs
    // `minibuffer-complete`, displays *Completions* candidates carrying
    // `mouse-face`, and leaves the frame ready for the render-thread
    // materialization that reported DuplicateSourceIdentity.
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("mx-tab-pointer-identity", 960, 640, buf_id);
    assert!(eval.frame_manager_mut().select_frame(frame_id));

    let minibuf_id = eval
        .buffer_manager_mut()
        .create_buffer(" *Minibuf-pointer-completion*");
    eval.activate_minibuffer_window_for_buffer(
        minibuf_id,
        LispString::from_utf8("M-x "),
        Some(LispString::from_utf8("profiler-re")),
    )
    .expect("activate minibuffer")
    .expect("minibuffer window");
    eval.eval_str(
        r#"(progn
             (fido-vertical-mode 1)
             (setq minibuffer-completion-table obarray
                   minibuffer-completion-predicate #'commandp
                   minibuffer--require-match t)
             (icomplete-minibuffer-setup)
             (icomplete--fido-mode-setup)
             (icomplete--vertical-minibuffer-setup)
             (icomplete-exhibit))"#,
    )
    .expect("exhibit fido candidates before TAB");

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    assert!(
        !logs_contain("layout failed to converge"),
        "the warm M-x layout must converge before materialization"
    );
    let _ = engine
        .last_frame_display_state
        .as_ref()
        .expect("warm M-x display state")
        .materialize();

    eval.eval_str(
        r#"(with-output-to-temp-buffer "*Completions*"
             (display-completion-list
              (all-completions "" obarray #'commandp)))"#,
    )
    .expect("display M-x TAB completions");

    assert!(
        eval.eval_str("(get-buffer-window \"*Completions*\" 0)")
            .expect("query completions window")
            .is_truthy(),
        "TAB should leave *Completions* displayed"
    );

    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("display state");
    assert!(
        !state.presented_pointer_source.appearances().is_empty(),
        "test requires completion candidate mouse-face pointer metadata"
    );

    let _ = state.materialize();
}

/// REGRESSION (face-id collision audit, 2026-06-27): the incremental fast paths
/// install retained body rows VERBATIM carrying prior-frame face_ids, but the
/// frame faces table is rebuilt from scratch each frame. For a MULTI-FACE
/// (font-locked/propertized) body those face_ids are never re-registered, so at
/// render `resolved_face` misses and falls back to white-fg. The byte-identical
/// goldens miss it (they compare integer face_ids, not the resolved face). This
/// asserts every reused body glyph's face_id is present in the frame faces table.
#[test]
fn cursor_only_reused_multiface_body_face_ids_are_registered_in_frame_faces() {
    let text = "(defun f (a b) (+ a b))\n".repeat(20);
    let (mut eval, frame_id, buf_id, win) = incr_editing_frame(&text, 800, 600);
    // Distinct :face per several lines BEFORE the warm pass, so the props tick is
    // stable and a later bare point move takes the cursor-only fast path.
    for (i, color) in ["red", "green", "blue", "magenta", "cyan"]
        .iter()
        .enumerate()
    {
        let start = i * 24 + 3;
        let end = start + 8;
        eval.eval_str(&format!(
            "(put-text-property {start} {end} 'face '(:foreground \"{color}\"))"
        ))
        .expect("put-text-property");
    }
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id); // warm (allocates multi-face ids)
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buf");
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(60));
    }
    engine.layout_frame_rust(&mut eval, frame_id); // measured -> cursor-only
    assert_eq!(
        engine.last_layout_stats().cursor_only_windows,
        1,
        "expected the cursor-only fast path to fire"
    );
    let state = engine.last_frame_display_state.as_ref().expect("state");
    let faces = &state.faces;
    let entry = state
        .window_matrices
        .iter()
        .find(|e| e.window_id.get() == win.0 as i64)
        .expect("window");
    let mut missing: std::collections::BTreeSet<FaceId> = std::collections::BTreeSet::new();
    let mut total = 0usize;
    for row in entry
        .matrix
        .rows
        .iter()
        .filter(|r| r.enabled && r.role == GlyphRowRole::Text)
    {
        for g in row.glyphs[1].iter() {
            total += 1;
            if !faces.contains_key(&g.face_id) {
                missing.insert(g.face_id);
            }
        }
    }
    let mut keys: Vec<FaceId> = faces.keys().copied().collect();
    keys.sort_unstable();
    assert!(
        missing.is_empty(),
        "cursor-only reused MULTI-FACE body references face_ids absent from the frame \
         faces table: missing={missing:?}, faces_keys={keys:?}, total_body_glyphs={total}"
    );
}

/// The face-id collision fix re-registers a reused window's faces, which UNBLOCKS
/// reuse of a fully-unchanged NON-selected window: a redisplay where nothing
/// changed must reuse BOTH windows via cursor-only (no-change), not just the
/// selected one. Without non-selected reuse this would be 1 (the perf win is the
/// non-selected windows no longer full-rebuilding every frame).
#[test]
fn non_selected_unchanged_window_reuses_via_cursor_only() {
    let mut eval = Context::new();
    let left = eval.buffer_manager().current_buffer().expect("buf").id();
    eval.buffer_manager_mut()
        .get_mut(left)
        .expect("left")
        .insert(&"(left line)\n".repeat(30));
    let right = eval.buffer_manager_mut().create_buffer("*r-reuse*");
    eval.buffer_manager_mut()
        .get_mut(right)
        .expect("right")
        .insert(&"(right line)\n".repeat(30));
    // `insert` leaves point at buffer end (off-screen → cursor-only bails on the
    // missing cursor row); reset both to the top so each window's point sits in
    // its visible body and both windows are cursor-only eligible.
    for b in [left, right] {
        eval.buffer_manager_mut()
            .get_mut(b)
            .expect("buf")
            .goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(0));
    }
    let frame = eval
        .frame_manager_mut()
        .create_frame("mw-reuse", 800, 600, left);
    let lw = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .selected_window;
    eval.frame_manager_mut()
        .split_window(
            frame,
            lw,
            neovm_core::window::SplitDirection::Horizontal,
            right,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split");
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame); // warm
    engine.layout_frame_rust(&mut eval, frame); // nothing changed
    assert_eq!(
        engine.last_layout_stats().cursor_only_windows,
        2,
        "both the selected and the non-selected unchanged window must reuse via cursor-only"
    );
}
