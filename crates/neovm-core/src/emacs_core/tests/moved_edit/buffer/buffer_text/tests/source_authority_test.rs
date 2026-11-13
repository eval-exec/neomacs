use super::*;

use neovm_edit::buffer::Buffer;
use std::cell::{Ref, RefCell};

use neovm_edit::buffer::buffer_text::*;
use neovm_eval::emacs_core::eval::Context;

use neovm_edit::buffer::{
    BufferId, BufferManager, CharPos0, CharRange, EmacsBytePos, EmacsByteRange,
};

use neovm_value::heap_types::LispString;

use neovm_value::value::Value;

#[test]
fn production_insert_refuses_after_tick_restore_and_ack() {
    production_text_edit_refuses_after_tick_restore_and_ack(
        "(erase-buffer) (insert \"abc\\n\") (goto-char 2)",
        "(insert \"x\")",
    );
}

#[test]
fn production_delete_refuses_after_tick_restore_and_ack() {
    production_text_edit_refuses_after_tick_restore_and_ack(
        "(erase-buffer) (insert \"abc\\n\")",
        "(delete-region 1 2)",
    );
}

#[test]
fn production_same_length_ascii_substitution_refuses_after_tick_restore_and_ack() {
    production_text_edit_refuses_after_tick_restore_and_ack(
        "(erase-buffer) (insert \"abc\\n\")",
        "(subst-char-in-region 1 2 ?a ?A)",
    );
}

#[test]
fn production_same_length_multibyte_substitution_refuses_after_tick_restore_and_ack() {
    production_text_edit_refuses_after_tick_restore_and_ack(
        "(erase-buffer) (insert \"αβγ\\n\")",
        "(subst-char-in-region 1 2 ?α ?Α)",
    );
}

#[test]
fn production_transposition_with_properties_refuses_after_tick_restore_and_ack() {
    production_text_edit_refuses_after_tick_restore_and_ack(
        "(erase-buffer) (insert \"abcd\\n\") (put-text-property 1 2 'fontified t)",
        "(transpose-regions 1 3 3 5)",
    );
}

#[test]
fn production_same_extent_casing_refuses_after_tick_restore_and_ack() {
    production_text_edit_refuses_after_tick_restore_and_ack(
        "(erase-buffer) (insert \"abcd\\n\") (put-text-property 1 2 'fontified t)",
        "(upcase-region 1 5)",
    );
}

#[test]
fn production_expanding_casing_property_install_refuses_after_tick_restore_and_ack() {
    production_text_edit_refuses_after_tick_restore_and_ack(
        "(erase-buffer) (insert \"ßx\\n\") (put-text-property 1 2 'fontified t)",
        "(upcase-region 1 2)",
    );
}

#[test]
fn production_property_write_refuses_numeric_tick_collision_after_ack() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    evaluator
        .eval_str("(erase-buffer) (insert \"abc\\n\")")
        .unwrap();
    let id = evaluator.buffer_manager().current_buffer_id().unwrap();
    let before = evaluator.buffer_manager_mut().get_mut(id).unwrap();
    let authority = before.capture_motion_source_authority().unwrap();
    let text = before.full_text_string();
    let property_tick = before.props_modified_tick();
    let epoch = authority.revision_for_test().content_epoch_for_test();
    neovm_edit::test_support::authority::set_text_modified_tick_for_test(before, property_tick - 1);
    evaluator
        .eval_str("(put-text-property 1 2 'fontified t)")
        .unwrap();
    let after = evaluator.buffer_manager().get(id).unwrap();
    assert_eq!(after.full_text_string(), text, "property-only change");
    assert_eq!(
        after.props_modified_tick(),
        property_tick,
        "GNU-facing number really collides"
    );
    let revision = after.with_text_storage_observe_for_test(SourceRevision::of);
    assert_eq!(revision.content_epoch_for_test(), epoch);
    assert_ne!(
        revision.all_properties_for_test(),
        authority.revision_for_test().all_properties_for_test()
    );
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(after) }.is_none()
    );
    after.reset_unchanged_region();
    assert_eq!(after.changed_char_range(), None);
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(after) }.is_none()
    );
}

#[test]
fn production_multibyte_mode_toggles_refuse_after_tick_restore_and_ack() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    evaluator
        .eval_str("(erase-buffer) (insert \"abc\\n\") (set-buffer-multibyte t)")
        .unwrap();
    let id = evaluator.buffer_manager().current_buffer_id().unwrap();
    assert!(evaluator.buffer_manager().get(id).unwrap().get_multibyte());
    for (multibyte, edit) in [
        (false, "(set-buffer-multibyte nil)"),
        (true, "(set-buffer-multibyte t)"),
    ] {
        let before = evaluator.buffer_manager().get(id).unwrap();
        let authority = before.capture_motion_source_authority().unwrap();
        let text = before.full_text_string();
        let ticks = (
            before.modified_tick(),
            before.chars_modified_tick(),
            before.save_modified_tick(),
        );
        evaluator.eval_str(edit).unwrap();
        let after = evaluator.buffer_manager_mut().get_mut(id).unwrap();
        assert_eq!(after.get_multibyte(), multibyte);
        assert_eq!(after.full_text_string(), text, "ASCII bytes stay unchanged");
        assert!(authority.is_owner_of(after));
        assert_ne!(
            after
                .with_text_storage_observe_for_test(|storage| { storage.content_epoch_for_test() }),
            authority.revision_for_test().content_epoch_for_test(),
        );
        assert!(
            // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
            unsafe { authority.validate(after) }.is_none()
        );
        neovm_edit::test_support::authority::set_text_modification_state_for_test(
            after, ticks.0, ticks.1, ticks.2,
        );
        assert_eq!(
            (
                after.modified_tick(),
                after.chars_modified_tick(),
                after.save_modified_tick(),
            ),
            ticks,
        );
        after.reset_unchanged_region();
        assert_eq!(after.changed_char_range(), None);
        assert!(
            // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
            unsafe { authority.validate(after) }.is_none()
        );
    }
}

#[test]
fn production_same_byte_transposition_with_distinct_property_runs_refuses() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    evaluator
        .eval_str(
            "(erase-buffer) (insert \"abab\\n\") \
             (put-text-property 1 3 'source-authority-run 11) \
             (put-text-property 3 5 'source-authority-run 22)",
        )
        .unwrap();
    let id = evaluator.buffer_manager().current_buffer_id().unwrap();
    let before = evaluator.buffer_manager().get(id).unwrap();
    let authority = before.capture_motion_source_authority().unwrap();
    let text = before.full_text_string();
    let ticks = (
        before.modified_tick(),
        before.chars_modified_tick(),
        before.save_modified_tick(),
    );
    evaluator.eval_str("(transpose-regions 1 3 3 5)").unwrap();
    assert_eq!(
        evaluator
            .eval_str("(get-text-property 1 'source-authority-run)")
            .unwrap(),
        Value::fixnum(22),
    );
    assert_eq!(
        evaluator
            .eval_str("(get-text-property 3 'source-authority-run)")
            .unwrap(),
        Value::fixnum(11),
    );
    let after = evaluator.buffer_manager_mut().get_mut(id).unwrap();
    assert_eq!(
        after.full_text_string(),
        text,
        "the two byte slices are equal"
    );
    assert!(authority.is_owner_of(after));
    // The actual production transposition advances content_epoch in its
    // same-length byte replacement before installing the prepared table.
    // This checks the resulting epoch, not an instrumented intermediate step.
    assert_ne!(
        after.with_text_storage_observe_for_test(|storage| storage.content_epoch_for_test()),
        authority.revision_for_test().content_epoch_for_test(),
    );
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(after) }.is_none()
    );
    neovm_edit::test_support::authority::set_text_modification_state_for_test(
        after, ticks.0, ticks.1, ticks.2,
    );
    assert_eq!(
        (
            after.modified_tick(),
            after.chars_modified_tick(),
            after.save_modified_tick(),
        ),
        ticks,
    );
    after.reset_unchanged_region();
    assert_eq!(after.changed_char_range(), None);
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(after) }.is_none()
    );
}

fn production_text_edit_refuses_after_tick_restore_and_ack(setup: &str, edit: &str) {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    evaluator
        .eval_str(setup)
        .expect("set up actual production source");
    let id = evaluator.buffer_manager().current_buffer_id().unwrap();
    let before = evaluator.buffer_manager().get(id).unwrap();
    let authority = before.capture_motion_source_authority().unwrap();
    let text = before.full_text_string();
    let ticks = (
        before.modified_tick(),
        before.chars_modified_tick(),
        before.save_modified_tick(),
    );
    evaluator
        .eval_str(edit)
        .expect("perform actual production text edit");
    let after = evaluator.buffer_manager_mut().get_mut(id).unwrap();
    assert_ne!(
        after.full_text_string(),
        text,
        "the operation must really change text"
    );
    assert!(
        authority.is_owner_of(after),
        "the live outer owner remains the same"
    );
    assert_ne!(
        after.with_text_storage_observe_for_test(|storage| storage.content_epoch_for_test()),
        authority.revision_for_test().content_epoch_for_test()
    );
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(after) }.is_none()
    );
    neovm_edit::test_support::authority::set_text_modification_state_for_test(
        after, ticks.0, ticks.1, ticks.2,
    );
    assert_eq!(
        (
            after.modified_tick(),
            after.chars_modified_tick(),
            after.save_modified_tick()
        ),
        ticks
    );
    // This is the same acknowledgement primitive used at the accepted layout
    // break. This core unit test does not claim to run a full accepted frame.
    after.reset_unchanged_region();
    assert_eq!(after.changed_char_range(), None);
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(after) }.is_none(),
        "tick restoration and accepted-ack primitive cannot resurrect source authority"
    );
}
