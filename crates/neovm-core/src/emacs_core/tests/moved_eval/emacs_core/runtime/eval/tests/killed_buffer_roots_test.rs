//! A killed buffer stays addressable while Lisp holds its object (GNU keeps
//! `buffer-live-p' nil, `buffer-name' nil and `buffer-last-name' for it), and
//! is garbage once nothing does. `garbage-collect' must free the killed
//! buffer's record and its registry entry in that second case.

use crate::emacs_core::eval::Context;
use neovm_value::value::buffer_vocab::BufferId;

/// Create the buffer NAME, kill it, and return its id. Only the id survives
/// on the Rust side, so nothing here roots the buffer object.
fn create_and_kill(ev: &mut Context, name: &str) -> BufferId {
    let id = ev
        .eval_str(&format!("(get-buffer-create {name:?})"))
        .expect("get-buffer-create should evaluate")
        .as_buffer_id()
        .expect("get-buffer-create returns a buffer");
    ev.eval_str(&format!("(kill-buffer {name:?})"))
        .expect("kill-buffer should evaluate");
    id
}

/// Whether the manager still holds a killed-buffer record for ID, and
/// whether the heap's buffer registry still holds its object.
fn retained(ev: &Context, id: BufferId) -> (bool, bool) {
    (
        ev.buffer_manager().get_dead(id).is_some(),
        ev.tagged_heap().buffer_value(id).is_some(),
    )
}

#[test]
fn garbage_collect_reclaims_a_killed_buffer_nothing_refers_to() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let id = create_and_kill(&mut ev, "killed-roots-orphan");
    assert_eq!(retained(&ev, id), (true, true));

    ev.eval_str("(garbage-collect)")
        .expect("garbage-collect should evaluate");
    assert_eq!(retained(&ev, id), (false, false));
}

#[test]
fn a_killed_buffer_a_global_holds_keeps_identity_and_last_name_across_garbage_collect() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let held = ev
        .eval_str(
            r#"(progn
                 (setq killed-roots-held (get-buffer-create "killed-roots-held"))
                 (setq killed-roots-list (list killed-roots-held))
                 (kill-buffer killed-roots-held)
                 killed-roots-held)"#,
        )
        .expect("setup should evaluate");
    let id = held.as_buffer_id().expect("a buffer");
    let orphan = create_and_kill(&mut ev, "killed-roots-orphan");

    let result = ev
        .eval_str(
            r#"(progn
                 (garbage-collect)
                 (list (buffer-live-p killed-roots-held)
                       (buffer-name killed-roots-held)
                       (buffer-last-name killed-roots-held)
                       (eq killed-roots-held (car killed-roots-list))
                       (prin1-to-string killed-roots-held)))"#,
        )
        .expect("queries should evaluate");
    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        r##"(nil nil "killed-roots-held" t "#<killed buffer>")"##
    );
    assert_eq!(retained(&ev, id), (true, true));
    assert_eq!(
        ev.tagged_heap().buffer_value(id).map(|value| value.bits()),
        Some(held.bits()),
        "the registry must keep the same object for a buffer Lisp still holds"
    );
    assert_eq!(retained(&ev, orphan), (false, false));
}

#[test]
fn two_thousand_create_and_kill_cycles_leave_no_killed_buffer_records_after_garbage_collect() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let ids: Vec<BufferId> = (0..2000)
        .map(|i| create_and_kill(&mut ev, &format!("killed-roots-{i}")))
        .collect();
    let count = |ev: &Context| ids.iter().filter(|id| retained(ev, **id).0).count();
    assert_eq!(count(&ev), 2000);

    ev.eval_str("(garbage-collect)")
        .expect("garbage-collect should evaluate");
    assert_eq!(count(&ev), 0);
    assert!(
        ids.iter().all(|id| !retained(&ev, *id).1),
        "no registry entry may outlive its killed buffer"
    );
}
