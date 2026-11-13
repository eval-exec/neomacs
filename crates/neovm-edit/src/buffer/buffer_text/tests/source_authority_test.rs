use super::*;

use crate::buffer::{BufferId, BufferManager, CharPos0, CharRange, EmacsBytePos, EmacsByteRange};

use neovm_value::heap_types::LispString;

use neovm_value::value::Value;

#[test]
fn authority_and_short_lease_do_not_retain_inner_cow_owners() {
    for text in ["abc\n", "αβγ\n"] {
        let (mut buffers, id) = fixture(text);
        let buffer = buffers.get(id).unwrap();
        assert_eq!(inner_counts(buffer), (1, 0, 1, 0));
        let (bytes_before, props_before) = {
            let storage = buffer.text.storage.borrow();
            (
                Rc::as_ptr(&storage.backend),
                Rc::as_ptr(&storage.text_props),
            )
        };
        let outer_strong = Rc::strong_count(&buffer.text.storage);
        let outer_weak = Rc::weak_count(&buffer.text.storage);
        let authority = buffer.capture_motion_source_authority().unwrap();
        let second = authority.clone();
        assert_eq!(inner_counts(buffer), (1, 0, 1, 0));
        assert_eq!(Rc::strong_count(&buffer.text.storage), outer_strong);
        assert_eq!(Rc::weak_count(&buffer.text.storage), outer_weak + 2);
        {
            // SAFETY: The guard covers only native Rc-count reads and drops before any edit.
            let _lease = unsafe { authority.validate(buffer) }.unwrap();
            assert_eq!(Rc::strong_count(&buffer.text.storage), outer_strong + 1);
            assert_eq!(inner_counts(buffer), (1, 0, 1, 0));
        }
        assert_eq!(Rc::strong_count(&buffer.text.storage), outer_strong);
        drop(second);
        assert!(
            // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
            unsafe { authority.validate(buffer) }.is_some()
        );
        let buffer = buffers.get_mut(id).unwrap();
        buffer.insert("x");
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::from_usize(0, text.chars().next().unwrap().len_utf8()),
            Value::symbol("fontified"),
            Value::T,
        );
        assert_eq!(inner_counts(buffer), (1, 0, 1, 0));
        let storage = buffer.text.storage.borrow();
        assert_eq!(Rc::as_ptr(&storage.backend), bytes_before);
        assert_eq!(Rc::as_ptr(&storage.text_props), props_before);
        drop(storage);
        assert!(
            // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
            unsafe { authority.validate(buffer) }.is_none()
        );
    }
}

#[test]
fn stable_source_and_point_motion_keep_authority_valid() {
    let (mut buffers, id) = fixture("abc\n");
    let authority = buffers
        .get(id)
        .unwrap()
        .capture_motion_source_authority()
        .unwrap();
    buffers
        .get_mut(id)
        .unwrap()
        .goto_emacs_byte_pos(EmacsBytePos::ZERO);
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffers.get(id).unwrap()) }.is_some()
    );
}

#[test]
fn kill_releases_old_text_and_properties_while_authority_survives() {
    let (mut buffers, id) = fixture("abc\n");
    let buffer = buffers.get(id).unwrap();
    let authority = buffer.capture_motion_source_authority().unwrap();
    // Test-only observers: these are never retained by production authority,
    // and the source is killed rather than mutated while they are present.
    let (bytes, properties) = {
        let storage = buffer.text.storage.borrow();
        (
            Rc::downgrade(&storage.backend),
            Rc::downgrade(&storage.text_props),
        )
    };
    assert!(buffers.kill_buffer(id));
    assert!(buffers.get(id).is_none());
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffers.get_dead(id).unwrap()) }.is_none()
    );
    assert!(authority.owner.upgrade().is_none());
    assert!(bytes.upgrade().is_none());
    assert!(properties.upgrade().is_none());
    let replacement = buffers.create_buffer("source-authority");
    buffers.get_mut(replacement).unwrap().insert("abc\n");
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffers.get(replacement).unwrap()) }.is_none()
    );
}

#[test]
fn restoring_numeric_text_ticks_does_not_restore_content_epoch() {
    let (mut buffers, id) = fixture("abc\n");
    let buffer = buffers.get_mut(id).unwrap();
    let authority = buffer.capture_motion_source_authority().unwrap();
    let ticks = (
        buffer.modified_tick(),
        buffer.chars_modified_tick(),
        buffer.text.save_modified_tick(),
    );
    let before = buffer.full_text_string();
    let end = buffer.total_emacs_byte_len().get();
    buffer.goto_emacs_byte_pos(EmacsBytePos::new(end));
    buffer.insert("x");
    buffer.delete_emacs_byte_range(EmacsByteRange::from_usize(end, end + 1));
    buffer
        .text
        .set_modification_state(ticks.0, ticks.1, ticks.2);
    assert_eq!(buffer.full_text_string(), before);
    assert_eq!(buffer.modified_tick(), ticks.0);
    assert_eq!(buffer.chars_modified_tick(), ticks.1);
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffer) }.is_none()
    );
}

#[test]
fn numeric_property_tick_collision_does_not_restore_all_property_revision() {
    let (mut buffers, id) = fixture("abc\n");
    let buffer = buffers.get_mut(id).unwrap();
    let authority = buffer.capture_motion_source_authority().unwrap();
    let property_tick = buffer.props_modified_tick();
    buffer.text.set_modified_tick(property_tick - 1);
    buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::from_usize(0, 1),
        Value::symbol("display"),
        Value::fixnum(7),
    );
    buffer.record_text_property_modification();
    assert_eq!(buffer.props_modified_tick(), property_tick);
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffer) }.is_none()
    );
}

#[test]
fn indirect_sources_share_authority_but_mutations_refuse_both() {
    let (mut buffers, base) = fixture("abc\n");
    let indirect = buffers
        .create_indirect_buffer(base, "source-indirect", false)
        .unwrap();
    let base_authority = buffers
        .get(base)
        .unwrap()
        .capture_motion_source_authority()
        .unwrap();
    let indirect_authority = buffers
        .get(indirect)
        .unwrap()
        .capture_motion_source_authority()
        .unwrap();
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { base_authority.validate(buffers.get(indirect).unwrap()) }.is_some()
    );
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { indirect_authority.validate(buffers.get(base).unwrap()) }.is_some()
    );
    buffers
        .get_mut(indirect)
        .unwrap()
        .text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::from_usize(0, 1),
            Value::symbol("fontified"),
            Value::T,
        );
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { base_authority.validate(buffers.get(base).unwrap()) }.is_none()
    );
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { indirect_authority.validate(buffers.get(indirect).unwrap()) }.is_none()
    );
    let stable = buffers
        .get(base)
        .unwrap()
        .capture_motion_source_authority()
        .unwrap();
    assert!(buffers.kill_buffer(indirect));
    assert!(stable.owner.upgrade().is_some());
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { stable.validate(buffers.get(base).unwrap()) }.is_some()
    );
    assert!(buffers.kill_buffer(base));
    assert!(stable.owner.upgrade().is_none());
}

#[test]
fn real_full_replacement_refuses_colliding_property_table_lineages() {
    let (mut buffers, id) = fixture("abc\n");
    let buffer = buffers.get_mut(id).unwrap();
    buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::from_usize(0, 1),
        Value::symbol("fontified"),
        Value::T,
    );
    let authority = buffer.capture_motion_source_authority().unwrap();
    let mut replacement = crate::buffer::TextPropertyTable::new();
    replacement.put_property_in_char_range(
        CharRange::new(CharPos0::ZERO, CharPos0::new(1)),
        Value::symbol("fontified"),
        Value::NIL,
    );
    assert_eq!(
        replacement.mutation_tick(),
        authority.revision.all_properties
    );
    // Public full-source replacement advances existing content_epoch even
    // when the bytes, length and table-local mutation number are unchanged.
    buffer.replace_lisp_string_with_text_props(&LispString::from_utf8("abc\n"), replacement);
    assert_eq!(buffer.full_text_string(), "abc\n");
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffer) }.is_none()
    );
}

#[test]
fn whole_source_swap_refuses_equal_revision_numbers() {
    let (mut buffers, left) = fixture("abc\n");
    let right = buffers.create_buffer("source-other");
    buffers.get_mut(right).unwrap().insert("xyz\n");
    let authority = buffers
        .get(left)
        .unwrap()
        .capture_motion_source_authority()
        .unwrap();
    buffers.swap_buffer_text(left, right).unwrap();
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffers.get(left).unwrap()) }.is_none()
    );
}

#[test]
fn conflicting_storage_borrow_refuses_instead_of_panicking() {
    let (buffers, id) = fixture("abc\n");
    let buffer = buffers.get(id).unwrap();
    let authority = buffer.capture_motion_source_authority().unwrap();
    let _write = buffer.text.storage.borrow_mut();
    assert!(buffer.capture_motion_source_authority().is_none());
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffer) }.is_none()
    );
}

#[test]
fn read_lease_excludes_mutation_through_every_indirect_alias() {
    let (mut buffers, base) = fixture("abc\n");
    let indirect = buffers
        .create_indirect_buffer(base, "source-read-alias", false)
        .unwrap();
    let source = buffers.get(base).unwrap();
    let alias = buffers.get(indirect).unwrap();
    let authority = source.capture_motion_source_authority().unwrap();
    // SAFETY: The guard covers only inert read/borrow checks and drops before the alias edit.
    let lease = unsafe { authority.validate(source) }.unwrap();
    // Test the existing shared-storage edit borrow without calling a
    // mutation entry point that would panic on a conflicting RefCell borrow.
    assert!(source.text.storage.try_borrow_mut().is_err());
    assert!(alias.text.storage.try_borrow_mut().is_err());
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(alias) }.is_some(),
        "concurrent read leases are fine"
    );
    drop(lease);
    assert!(source.text.storage.try_borrow_mut().is_ok());
    assert!(alias.text.storage.try_borrow_mut().is_ok());
    buffers
        .get_mut(indirect)
        .unwrap()
        .text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::from_usize(0, 1),
            Value::symbol("fontified"),
            Value::T,
        );
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffers.get(base).unwrap()) }.is_none()
    );
}

#[test]
fn accepted_ack_primitive_preserves_unchanged_source_authority() {
    let (buffers, id) = fixture("abc\n");
    let buffer = buffers.get(id).unwrap();
    assert!(buffer.changed_char_range().is_some());
    let authority = buffer.capture_motion_source_authority().unwrap();
    buffer.reset_unchanged_region();
    assert_eq!(buffer.changed_char_range(), None);
    assert_eq!(
        SourceRevision::of(&buffer.text.storage.borrow()),
        authority.revision
    );
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffer) }.is_some()
    );
}

#[test]
fn base_finalization_intermediate_keeps_authority_only_for_live_indirect_owner() {
    let (mut buffers, base) = fixture("abc\n");
    let indirect = buffers
        .create_indirect_buffer(base, "source-finalization-alias", false)
        .unwrap();
    let authority = buffers
        .get(base)
        .unwrap()
        .capture_motion_source_authority()
        .unwrap();
    // Exercise the actual final object-state seam, not a completed manager
    // kill: BufferManager::kill_buffer(base) also retires all indirects.
    buffers
        .get_mut(base)
        .unwrap()
        .mark_killed_after_local_reset();
    let dead_base = buffers.get(base).unwrap();
    assert!(dead_base.name_value().is_nil());
    assert!(dead_base.capture_motion_source_authority().is_none());
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(dead_base) }.is_none()
    );
    let live_indirect = buffers.get(indirect).unwrap();
    assert!(!live_indirect.name_value().is_nil());
    assert_eq!(live_indirect.full_text_string(), "abc\n");
    assert_eq!(
        authority.owner.as_ptr(),
        Rc::as_ptr(&live_indirect.text.storage)
    );
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(live_indirect) }.is_some()
    );
    // Retiring the last actual owner releases its storage despite authority.
    assert!(buffers.kill_buffer(indirect));
    assert!(authority.owner.upgrade().is_none());
    assert!(
        // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
        unsafe { authority.validate(buffers.get_dead(indirect).unwrap()) }.is_none()
    );
}

#[test]
fn completed_manager_base_kill_refuses_both_base_and_indirect_authorities() {
    let (mut buffers, base) = fixture("abc\n");
    let indirect = buffers
        .create_indirect_buffer(base, "source-manager-kill-alias", false)
        .unwrap();
    let base_authority = buffers
        .get(base)
        .unwrap()
        .capture_motion_source_authority()
        .unwrap();
    let indirect_authority = buffers
        .get(indirect)
        .unwrap()
        .capture_motion_source_authority()
        .unwrap();
    assert!(buffers.kill_buffer(base));
    for id in [base, indirect] {
        assert!(buffers.get(id).is_none());
        let dead = buffers.get_dead(id).unwrap();
        assert!(dead.name_value().is_nil());
        assert!(dead.capture_motion_source_authority().is_none());
        assert!(
            // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
            unsafe { base_authority.validate(dead) }.is_none()
        );
        assert!(
            // SAFETY: This temporary guard drops in this inert assertion, before any edit, Lisp or GC.
            unsafe { indirect_authority.validate(dead) }.is_none()
        );
    }
    assert!(base_authority.owner.upgrade().is_none());
    assert!(indirect_authority.owner.upgrade().is_none());
}

fn fixture(text: &str) -> (BufferManager, BufferId) {
    crate::test_utils::init_test_tracing();
    let mut buffers = BufferManager::new();
    let id = buffers.create_buffer("source-authority");
    buffers.get_mut(id).unwrap().insert(text);
    (buffers, id)
}

fn inner_counts(buffer: &Buffer) -> (usize, usize, usize, usize) {
    let storage = buffer.text.storage.borrow();
    (
        Rc::strong_count(&storage.backend),
        Rc::weak_count(&storage.backend),
        Rc::strong_count(&storage.text_props),
        Rc::weak_count(&storage.text_props),
    )
}
