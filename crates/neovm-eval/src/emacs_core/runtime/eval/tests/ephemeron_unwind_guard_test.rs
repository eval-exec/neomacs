//! A panic inside the explicit collection's armed window must not lose the
//! taken killed-buffer, retired-process, or deleted-frame records, and must
//! not leave the heap's ephemeron maps armed. The next explicit collection
//! reclaims those unreferenced records.

use std::panic::{AssertUnwindSafe, catch_unwind};

use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_value::display_vocab::FrameId;
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;

use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::data::process_state::{ProcessCodingSystems, ProcessKind};
use crate::emacs_core::eval::data::window_state::{DeletedFrameTombstone, DeletedWindowRecord};

fn gcs_done(ev: &Context) -> i64 {
    ev.builtin_var_value_by_name("gcs-done")
        .and_then(|value| value.as_fixnum())
        .unwrap_or(0)
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "<non-string panic>".to_string()
    }
}

struct ArmedRecords {
    buffer: neovm_edit::buffer::BufferId,
    process: neovm_value::heap_types::ProcessId,
    frame: FrameId,
    window: WindowId,
    marker: Value,
}

fn arm_unreferenced_records(ev: &mut Context) -> ArmedRecords {
    ev.setup_thread_locals();
    let buffer = ev
        .buffer_manager_mut()
        .create_buffer("ephemeron-unwind-buffer");
    assert!(ev.buffer_manager_mut().kill_buffer(buffer));
    let _ = Value::make_buffer(buffer);

    let process = ev.processes_mut().create_process_record(
        LispString::from_utf8("ephemeron-unwind-proc"),
        Value::NIL,
        LispString::from_utf8("true"),
        Vec::new(),
        ProcessKind::Real,
        None,
        ProcessCodingSystems {
            decode: Value::NIL,
            encode: Value::NIL,
        },
    );
    let record = ev
        .processes_mut()
        .processes
        .remove(&process)
        .expect("the new process is live");
    ev.processes_mut().deleted_processes.insert(process, record);
    let _ = Value::make_process(process);

    let frames = ev.frame_manager_mut();
    let frame = FrameId(frames.next_frame_id);
    frames.next_frame_id += 1;
    let window = WindowId(frames.next_window_id);
    frames.next_window_id += 1;
    let marker = Value::cons(Value::fixnum(4), Value::fixnum(1));
    frames
        .deleted_frames
        .insert(frame, DeletedFrameTombstone::text_for_test());
    frames.deleted_windows.insert(
        window,
        DeletedWindowRecord {
            owner: Some(frame),
            old_buffer: None,
            change_stamp: None,
        },
    );
    frames
        .deleted_window_parameters
        .insert(window, vec![(Value::symbol("unwind-marker"), marker)]);
    let _ = frames;
    let _ = Value::mint_frame(frame.0);

    assert!(
        ev.buffer_manager()
            .dead_buffer_last_name_value(buffer)
            .is_some(),
        "the killed buffer starts in the dead-buffer map"
    );
    assert!(
        ev.processes().deleted_processes.contains_key(&process),
        "the retired process starts in the deleted-process map"
    );
    assert!(
        ev.frame_manager().deleted_windows.contains_key(&window),
        "the deleted frame's window starts in the window map"
    );
    assert!(ev.tagged_heap.explicit_ephemeron_arm_is_clear());

    ArmedRecords {
        buffer,
        process,
        frame,
        window,
        marker,
    }
}

fn records_intact(ev: &Context, armed: &ArmedRecords) -> bool {
    let buffer_kept = ev
        .buffer_manager()
        .dead_buffer_last_name_value(armed.buffer)
        .is_some();
    let process_kept = ev
        .processes()
        .deleted_processes
        .contains_key(&armed.process);
    let window_kept = ev
        .frame_manager()
        .deleted_windows
        .contains_key(&armed.window);
    let marker_kept = ev
        .frame_manager()
        .deleted_window_parameters
        .get(&armed.window)
        .is_some_and(|params| params.len() == 1 && params[0].1 == armed.marker);
    let tombstone_kept = ev.frame_manager().deleted_frames.contains_key(&armed.frame);
    buffer_kept && process_kept && window_kept && marker_kept && tombstone_kept
}

#[test]
fn panic_inside_the_armed_window_restores_records_and_the_next_collection_reclaims() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let armed = arm_unreferenced_records(&mut ev);
    let done_before = gcs_done(&ev);

    super::command_loop::arm_ephemeron_window_panic_for_test();
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        ev.gc_collect_exact();
    }));
    let payload = panicked.expect_err("the armed window must panic");
    let message = panic_text(&*payload);
    assert!(
        message.contains("explicit collection armed-window test panic"),
        "unexpected panic: {message}"
    );

    assert!(
        records_intact(&ev, &armed),
        "unwind lost a taken buffer, process, or frame record"
    );
    assert!(
        ev.tagged_heap.explicit_ephemeron_arm_is_clear(),
        "unwind left an ephemeron map or reclaimed-id list armed"
    );
    assert_eq!(
        gcs_done(&ev),
        done_before,
        "the panicked collection published a completed cycle"
    );

    ev.gc_collect_exact();
    assert_eq!(gcs_done(&ev), done_before + 1);
    assert!(
        ev.buffer_manager()
            .dead_buffer_last_name_value(armed.buffer)
            .is_none(),
        "the next explicit collection did not reclaim the killed buffer"
    );
    assert!(
        !ev.processes()
            .deleted_processes
            .contains_key(&armed.process),
        "the next explicit collection did not reclaim the retired process"
    );
    assert!(
        !ev.frame_manager()
            .deleted_windows
            .contains_key(&armed.window),
        "the next explicit collection did not reclaim the deleted frame's window"
    );
    assert!(
        !ev.frame_manager().deleted_frames.contains_key(&armed.frame),
        "the next explicit collection did not drop the reclaimed frame tombstone"
    );
    assert!(
        ev.tagged_heap.buffer_value(armed.buffer).is_none(),
        "the killed buffer's registry entry survived"
    );
    assert!(ev.tagged_heap.explicit_ephemeron_arm_is_clear());
}

/// #776 R3: a forced major owes the record its DECIDING cycle. Arming clears
/// the debt because that major owns the decision; an abandoned armed cycle
/// (no collection completed since the arm) must put the debt back, so the
/// next automatic cycle forces the major again. The manager-level invariant
/// is what the driver's `has_pending_forced_retired_process` consumes.
#[test]
fn an_abandoned_forced_major_keeps_the_debt_for_the_next_cycle() {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.setup_thread_locals();
    let records = arm_unreferenced_records(&mut ev);
    ev.processes_mut().note_retired_process(records.process);
    assert!(
        ev.processes().has_pending_forced_retired_process(),
        "a fresh retirement owes its deciding major"
    );

    let (_ephemerons, taken) = ev.processes_mut().take_retired_processes_for_gc(true);
    assert!(taken.contains(&records.process));
    assert!(
        !ev.processes().has_pending_forced_retired_process(),
        "arming hands the decision to the in-flight major"
    );

    // Abandon before any heap collection completes.
    ev.processes_mut().abandon_armed_retired_processes(&taken);
    assert!(
        ev.processes().has_pending_forced_retired_process(),
        "an abandoned major must keep the debt for the next automatic cycle"
    );
    let (_again, taken_again) = ev.processes_mut().take_retired_processes_for_gc(true);
    assert!(
        taken_again.contains(&records.process),
        "the next forced major arms the record again"
    );
}
