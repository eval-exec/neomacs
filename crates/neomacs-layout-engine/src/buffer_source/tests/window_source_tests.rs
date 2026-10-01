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
