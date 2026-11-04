//! Live deletion undo state, with expectations captured from GNU Emacs 31.1.
//! GNU undo.c:163-190 records point, marker adjustments, then deleted text;
//! insdel.c:2023-2037 relocates markers even when undo is disabled.
use super::*;
use crate::buffer::{InsertionType, UndoRecording};
use crate::emacs_core::print::print_value;

fn anchor(buffer: &Buffer, position: usize) -> TextPositionAnchor {
    let chars = CharPos0::new(position);
    TextPositionAnchor::new(chars, buffer.text.char_pos_to_emacs_byte_pos(chars))
}

fn normalized_undo(mut undo: Value, markers: &[Value]) -> Value {
    if undo.is_t() {
        return undo;
    }
    let mut entries = Vec::new();
    while undo.is_cons() {
        let entry = undo.cons_car();
        entries.push(
            if entry.is_cons() && entry.cons_car().as_marker_data().is_some() {
                let index = markers
                    .iter()
                    .position(|m| m.bits() == entry.cons_car().bits())
                    .unwrap();
                Value::cons(Value::symbol(&format!("m{index}")), entry.cons_cdr())
            } else {
                entry
            },
        );
        undo = undo.cons_cdr();
    }
    Value::list(entries)
}

fn assert_live_delete(prepared: UndoRecording, committed: UndoRecording) {
    let mut buffers = BufferManager::new();
    let id = buffers.current_buffer_id().unwrap();
    buffers.insert_into_buffer(id, "aé中bcd").unwrap();
    let specifications = [
        (3, InsertionType::After),
        (2, InsertionType::Before),
        (4, InsertionType::After),
        (4, InsertionType::Before),
        (5, InsertionType::After),
    ];
    let mut ids = Vec::new();
    let mut markers = Vec::new();
    for (pos, insertion_type) in specifications {
        let marker_ptr = buffers.create_marker_at_anchor(
            id,
            anchor(buffers.get(id).unwrap(), pos),
            insertion_type,
        );
        let marker_ref = crate::buffer::MarkerRef::new(marker_ptr);
        ids.push(marker_ref);
        markers.push(buffers.marker_value(marker_ref).unwrap());
    }
    let live = buffers.get_mut(id).unwrap();
    live.set_modified(false);
    live.saved_point_before_command.save(id, CharPos0::new(6));
    live.set_undo_list(match prepared {
        UndoRecording::Enabled => Value::NIL,
        UndoRecording::Disabled => Value::T,
    });
    let range = live.edit_range_for_char_range(crate::buffer::CharRange::from_usize(1, 4));
    let plan = live.delete_text_plan_for_range(range);
    // Plan preparation grants no authority over marker state. Move, attach,
    // and detach markers before commit without changing measured text.
    let moved = anchor(live, 2);
    live.move_marker_ref_to_anchor(ids[0], moved);
    buffers.remove_marker(ids[2]);
    let anchor = anchor(buffers.get(id).unwrap(), 3);
    let late_ptr = buffers.create_marker_at_anchor(id, anchor, InsertionType::Before);
    markers.push(
        buffers
            .marker_value(crate::buffer::MarkerRef::new(late_ptr))
            .unwrap(),
    );
    let live = buffers.get_mut(id).unwrap();
    live.set_undo_list(match committed {
        UndoRecording::Enabled => Value::NIL,
        UndoRecording::Disabled => Value::T,
    });
    live.execute_delete_text_plan(plan);
    let positions = markers
        .iter()
        .map(|m| {
            let data = m.as_marker_data().unwrap();
            if data.buffer_id().is_some() {
                Value::fixnum(data.charpos.get() as i64 + 1)
            } else {
                Value::NIL
            }
        })
        .collect();
    let result = Value::list(vec![
        Value::string(&live.buffer_string()),
        Value::fixnum(live.point_char_pos().to_lisp().as_i64()),
        Value::list(positions),
        normalized_undo(live.get_undo_list(), &markers),
    ]);
    let expected = match committed {
        UndoRecording::Enabled => include_str!("delete-enabled.expect"),
        UndoRecording::Disabled => include_str!("delete-disabled.expect"),
    };
    assert_eq!(print_value(&result), expected.trim_end());
}

#[test]
fn deletion_enabled_at_commit_reads_live_marker_chain() {
    assert_live_delete(UndoRecording::Enabled, UndoRecording::Enabled);
}

#[test]
fn deletion_enabling_undo_after_prepare_records_live_markers() {
    assert_live_delete(UndoRecording::Disabled, UndoRecording::Enabled);
}

#[test]
fn deletion_disabling_undo_after_prepare_still_relocates_markers() {
    assert_live_delete(UndoRecording::Enabled, UndoRecording::Disabled);
}

#[test]
fn deletion_disabled_at_commit_still_relocates_live_markers() {
    assert_live_delete(UndoRecording::Disabled, UndoRecording::Disabled);
}
