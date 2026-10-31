//! Effective extents must cross raw boundaries with identical resolved values.
//! Run the regression in a time-bounded test process: the predecessor loops.
use super::*;
use neovm_core::buffer::{BufferId, BufferTextBackendKind};

fn range(start: usize, end: usize) -> EmacsByteRange {
    EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(end))
}

#[test]
fn effective_extent_progress_four_character_hidden_newline() {
    let mut eval = Context::new();
    let id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
    buffer.insert("a\nb\n");
    buffer.text_props_put_property_in_emacs_byte_range(
        range(0, 4),
        Value::symbol("invisible"),
        Value::T,
    );
    buffer.text_props_put_property_in_emacs_byte_range(
        range(1, 2),
        Value::symbol("category"),
        Value::symbol("extent-category"),
    );
    assert_eq!(
        RustBufferAccess::new(buffer).find_nth_preserved_newline_within(0, 4, 1),
        None,
    );
}

fn check_category_boundaries<B: LayoutBufferView>(buffer: &B) {
    let bounds = range(0, 16);
    // The first newline follows a category change. Exercise the preserved-
    // newline entry before checking its property extent directly.
    let access = RustBufferAccess::new(buffer);
    assert_eq!(access.find_nth_preserved_newline_within(0, 16, 1), Some(16));
    assert_eq!(access.find_nth_preserved_newline_within(0, 16, 2), None);
    let lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("invisible"));
    for pos in [0, 2, 5, 6, 7] {
        assert_eq!(
            lookup.effective_text_extent_at(buffer, EmacsBytePos::new(pos), bounds),
            Some(range(0, 8)),
            "same invisible value across category boundaries at byte {pos}"
        );
    }
    assert_eq!(
        lookup.effective_text_extent_at(buffer, EmacsBytePos::new(6), range(2, 8)),
        Some(range(2, 8)),
        "extent stays inside the narrowed scan bounds"
    );
}

#[test]
fn effective_extent_progress_across_irrelevant_categories_and_multibyte_text() {
    let _runtime = Context::new();
    for kind in BufferTextBackendKind::implemented_variants() {
        let mut buffer = Buffer::try_new_standalone_with_text_backend_kind(
            BufferId(700 + u64::from(u8::from(kind))),
            Value::string("*effective-extent-progress*"),
            kind,
        )
        .unwrap();
        buffer.insert("é日\nx\nvisible\n");
        buffer.widen();
        buffer.text_props_put_property_in_emacs_byte_range(
            range(0, 8),
            Value::symbol("invisible"),
            Value::T,
        );
        for (start, end, category) in [
            (0, 2, "extent-category-a"),
            (2, 6, "extent-category-b"),
            (6, 8, "extent-category-c"),
        ] {
            buffer.text_props_put_property_in_emacs_byte_range(
                range(start, end),
                Value::symbol("category"),
                Value::symbol(category),
            );
        }
        check_category_boundaries(&buffer);
        check_category_boundaries(&LayoutBufferSnapshot::from_buffer(&buffer));
    }
}

#[test]
fn effective_extent_progress_across_alias_and_canonical_handoff() {
    let mut eval = Context::new();
    let id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
    buffer.insert("a\nb\nc\nvisible\n");
    let canonical = Value::symbol("display");
    let alias = Value::symbol("extent-display-alias");
    buffer.set_buffer_local(
        "char-property-alias-alist",
        Value::list(vec![Value::list(vec![canonical, alias])]),
    );
    // The same object is supplied by an alias, then by the canonical property.
    let replacement = Value::string("replacement");
    buffer.text_props_put_property_in_emacs_byte_range(range(0, 6), alias, replacement);
    buffer.text_props_put_property_in_emacs_byte_range(range(2, 6), canonical, replacement);
    let check = |buffer: &Buffer| {
        let lookup = LayoutCharPropertyLookup::new(buffer, canonical);
        for pos in 0..6 {
            assert_eq!(
                lookup.effective_text_extent_at(buffer, EmacsBytePos::new(pos), range(0, 14)),
                Some(range(0, 6)),
            );
        }
        assert_eq!(
            RustBufferAccess::new(buffer).find_nth_preserved_newline_within(0, 14, 1),
            Some(14)
        );
    };
    check(buffer);
    // A real effective-value change must still stop the backward extent.
    let different = Value::string("different");
    buffer.text_props_put_property_in_emacs_byte_range(range(4, 6), canonical, different);
    let lookup = LayoutCharPropertyLookup::new(buffer, canonical);
    assert_eq!(
        lookup.effective_text_extent_at(buffer, EmacsBytePos::new(5), range(0, 14)),
        Some(range(4, 6)),
    );
}
