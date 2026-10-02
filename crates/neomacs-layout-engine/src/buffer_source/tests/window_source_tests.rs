use super::*;
use neovm_core::emacs_core::Context;

fn read_with_properties(properties: &str) -> Vec<u8> {
    let mut eval = Context::new();
    eval.eval_str("(progn (set-buffer (get-buffer-create \"*bounded-read*\")) (insert (apply 'concat (make-list 10000 \"café line\\n\"))))").unwrap();
    eval.eval_str(properties).unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let view = crate::neovm_bridge::BorrowedLayoutBuffer::for_window(
        buffer,
        eval.obarray(),
        CharPos0::new(0),
        128,
        crate::display_property::DisplayPropertyTarget::for_window_system(true),
    );
    let access = RustBufferAccess::new(&view);
    let request = BufferWindowSourceRequest::new(
        0,
        None,
        0,
        0,
        buffer.layout_point_max_char_pos().get() as i64,
        3,
        WindowKind::Main,
        ScrollPolicy::Recenter,
        0,
    );
    let mut bytes = Vec::new();
    request.read_exact_into(&access, &mut bytes);
    bytes
}

#[test]
fn source_read_with_rich_overlays_is_viewport_bounded() {
    let bytes = read_with_properties(
        "(progn
        (overlay-put (make-overlay 1 20) 'face 'bold)
        (overlay-put (make-overlay 2 2) 'before-string \"[before]\\n\")
        (overlay-put (make-overlay 3 5) 'display \"replacement\")
        (overlay-put (make-overlay 1000 2000) 'invisible t)
        (put-text-property 6 8 'display '(raise 0.2)))",
    );
    assert_eq!(bytes.len(), "café line\n".len() * 5);
    assert_eq!(bytes, "café line\n".repeat(5).as_bytes());
}

#[test]
fn source_read_counts_past_replaced_or_hidden_newlines() {
    for property in ["display", "invisible"] {
        let value = if property == "display" {
            "\"replacement\""
        } else {
            "t"
        };
        for setup in [
            format!("(overlay-put (make-overlay 1 31) '{property} {value})"),
            format!("(put-text-property 1 31 '{property} {value})"),
        ] {
            let bytes = read_with_properties(&setup);
            assert_eq!(bytes.len(), "café line\n".len() * 8, "{setup}");
            assert_eq!(bytes, "café line\n".repeat(8).as_bytes(), "{setup}");
        }
    }
}

#[test]
fn source_read_follows_indirect_hazard_properties() {
    for setup in [
        "(progn (put 'read-category 'invisible t) (overlay-put (make-overlay 1 31) 'category 'read-category))",
        "(progn (put 'read-category 'invisible t) (put-text-property 1 31 'category 'read-category))",
        "(progn (set (make-local-variable 'char-property-alias-alist) '((display alt-display))) (overlay-put (make-overlay 1 31) 'alt-display \"replacement\"))",
        "(progn (set (make-local-variable 'char-property-alias-alist) '((display alt-display))) (put-text-property 1 31 'alt-display \"replacement\"))",
    ] {
        assert_eq!(
            read_with_properties(setup).len(),
            "café line\n".len() * 8,
            "{setup}"
        );
    }
}

#[test]
fn source_read_empty_overlays_do_not_consume_newlines() {
    assert_eq!(
        read_with_properties("(overlay-put (make-overlay 10 10) 'invisible t)").len(),
        55
    );
}

#[test]
fn source_read_preserves_accessible_tail_when_no_safe_bound_exists() {
    for setup in [
        "(overlay-put (make-overlay 1 (point-max)) 'invisible t)",
        "(put-text-property 1 (point-max) 'display \"replacement\")",
        "(set (make-local-variable 'selective-display) 2)",
    ] {
        assert_eq!(read_with_properties(setup).len(), 110000, "{setup}");
    }
}

#[test]
fn sync_source_budget_preserves_semantic_end_point_and_window_start() {
    let mut eval = Context::new();
    eval.eval_str("(progn (set-buffer (get-buffer-create \"*sync-read*\")) (insert (apply 'concat (make-list 100 \"café line\\n\"))))").unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let view = crate::neovm_bridge::BorrowedLayoutBuffer::for_window(
        buffer,
        eval.obarray(),
        CharPos0::new(0),
        128,
        crate::display_property::DisplayPropertyTarget::for_window_system(true),
    );
    let access = RustBufferAccess::new(&view);
    let request = BufferWindowSourceRequest::new(
        0,
        None,
        7,
        0,
        1000,
        38,
        WindowKind::Main,
        ScrollPolicy::Recenter,
        0,
    )
    .with_sync_stop(crate::types::LayoutCharPos0::new(20));
    let mut bytes = Vec::new();
    let source = request.read_exact_into(&access, &mut bytes);
    assert_eq!(bytes, "café line\n".repeat(4).as_bytes());
    assert_eq!(
        source.read_boundary(),
        BufferWindowReadBoundary::SyncHorizon
    );
    assert_eq!(source.accessible_end(), 1000);
    assert_eq!(
        source.accessible_end_position().emacs_byte_pos().get(),
        1100
    );
    assert_eq!(source.point_charpos(), 7);
    assert_eq!(source.window_start(), 0);
    assert_eq!(request.max_rows, 38);
}

#[test]
fn sync_source_budget_keeps_real_eob_when_lookahead_runs_out() {
    let mut eval = Context::new();
    eval.eval_str(
        "(progn (set-buffer (get-buffer-create \"*sync-tail*\")) (insert \"one\\ntwo\\n\"))",
    )
    .unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let view = crate::neovm_bridge::BorrowedLayoutBuffer::for_window(
        buffer,
        eval.obarray(),
        CharPos0::new(0),
        128,
        crate::display_property::DisplayPropertyTarget::for_window_system(true),
    );
    let access = RustBufferAccess::new(&view);
    let request = BufferWindowSourceRequest::new(
        0,
        None,
        8,
        0,
        8,
        38,
        WindowKind::Main,
        ScrollPolicy::Recenter,
        0,
    )
    .with_sync_stop(crate::types::LayoutCharPos0::new(4));
    let mut bytes = Vec::new();
    let source = request.read_exact_into(&access, &mut bytes);
    assert_eq!(bytes, b"one\ntwo\n");
    assert_eq!(
        source.read_boundary(),
        BufferWindowReadBoundary::AccessibleEnd
    );
    assert_eq!(source.accessible_end(), 8);
}

#[test]
fn sync_horizon_exhaustion_is_independent_of_semantic_eob() {
    let horizon = BufferWindowReadBoundary::SyncHorizon;
    assert!(horizon.exhausts_sync_horizon(44, 44, 40, 1000));
    assert!(!horizon.exhausts_sync_horizon(43, 44, 40, 1000));
    assert!(!horizon.exhausts_sync_horizon(44, 44, 1000, 1000));
    assert!(!BufferWindowReadBoundary::AccessibleEnd.exhausts_sync_horizon(44, 44, 40, 1000));
    assert!(!BufferWindowReadBoundary::WindowRows.exhausts_sync_horizon(44, 44, 40, 1000));
}
fn sync_read_with_properties(properties: &str) -> (Vec<u8>, BufferWindowSource) {
    let mut eval = Context::new();
    eval.eval_str("(progn (set-buffer (get-buffer-create \"*sync-hazard*\")) (insert (apply 'concat (make-list 100 \"row\\n\"))))").unwrap();
    eval.eval_str(properties).unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let view = crate::neovm_bridge::BorrowedLayoutBuffer::for_window(
        buffer,
        eval.obarray(),
        CharPos0::new(0),
        128,
        crate::display_property::DisplayPropertyTarget::for_window_system(true),
    );
    let access = RustBufferAccess::new(&view);
    let request = BufferWindowSourceRequest::new(
        0,
        None,
        7,
        0,
        400,
        38,
        WindowKind::Main,
        ScrollPolicy::Recenter,
        0,
    )
    .with_sync_stop(crate::types::LayoutCharPos0::new(8));
    let mut bytes = Vec::new();
    let source = request.read_exact_into(&access, &mut bytes);
    (bytes, source)
}

#[test]
fn sync_source_budget_lookahead_skips_replaced_and_hidden_newlines() {
    for setup in [
        "(overlay-put (make-overlay 9 21) 'invisible t)",
        "(put-text-property 9 21 'display \"replacement\")",
    ] {
        let (bytes, source) = sync_read_with_properties(setup);
        assert_eq!(bytes, "row\n".repeat(7).as_bytes(), "{setup}");
        assert_eq!(
            source.read_boundary(),
            BufferWindowReadBoundary::SyncHorizon
        );
        assert_eq!(source.accessible_end(), 400);
        assert_eq!(source.point_charpos(), 7);
    }
}

#[test]
fn sync_source_budget_preserves_full_source_without_safe_lookahead() {
    for setup in [
        "(put-text-property 9 (point-max) 'invisible t)",
        "(set (make-local-variable 'selective-display) 2)",
    ] {
        let (bytes, source) = sync_read_with_properties(setup);
        assert_eq!(bytes, "row\n".repeat(100).as_bytes(), "{setup}");
        assert_eq!(
            source.read_boundary(),
            BufferWindowReadBoundary::AccessibleEnd
        );
        assert_eq!(source.accessible_end(), 400);
    }
}
