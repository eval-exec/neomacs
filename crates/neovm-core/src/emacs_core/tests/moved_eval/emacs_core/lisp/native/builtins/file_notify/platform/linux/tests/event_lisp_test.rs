//! Re-homed from eval's file_notify/platform/linux/tests/linux_test.rs
//! (P1 F1): the two evaluator-encoding oracles.  The event construction is
//! re-pointed to the test-support authority forges (the event fields stay
//! private to the platform module); the Context is created before the
//! fixture allocations so they live on the evaluator's heap (post-split
//! there is no cfg(test) fallback heap in a dependency crate).  Test names
//! and assertions are unchanged.
use crate::test_utils::runtime_startup_context;
use neovm_eval::emacs_core::builtins::file_notify::{WatchId, WatchRegistration};
use neovm_eval::test_support::authority::{
    inotify_event_native_name_lisp_for_test, inotify_event_registered_name_lisp_for_test,
};
use neovm_value::value::Value;

#[test]
fn event_file_name_is_decoded_on_the_evaluator_without_loss() {
    let raw_name = vec![b'n', 0xff, b'm', b'e'];
    let mut eval = runtime_startup_context();
    eval.eval_str("(setq file-name-coding-system nil default-file-name-coding-system nil)")
        .expect("select identity file-name decoding");

    let registration = WatchRegistration::new(Value::NIL, Value::NIL);
    let fields = neovm_value::value::list_to_vec(&inotify_event_native_name_lisp_for_test(
        &eval,
        WatchId::new(17, 0),
        vec!["create"],
        raw_name.clone(),
        0,
        registration,
    ))
    .expect("inotify event is a proper list");
    assert_eq!(
        fields[2]
            .as_lisp_string()
            .expect("event file name is a string")
            .as_bytes(),
        raw_name
    );
}

#[test]
fn nameless_event_reuses_the_exact_registered_lisp_file_name() {
    // Post-split re-point (peer 15:35Z hold): create the Context first so the
    // fixture Value below is allocated on the evaluator's heap, and run the
    // coding-system setup BEFORE that allocation.  eval_str runs eval_sub and
    // can reach a GC safe point; a heap fixture held only in a Rust local is
    // not rooted across it.  No safe point sits between the allocation and
    // the into_lisp use.
    let mut eval = runtime_startup_context();
    eval.eval_str("(setq file-name-coding-system 'utf-8 default-file-name-coding-system 'utf-8)")
        .expect("select a coding system that would alter reconstructed bytes");
    let registered_name =
        Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
            b'w', 0xff, b't', b'c', b'h',
        ]));

    let registration = WatchRegistration::new(Value::NIL, registered_name);
    let fields = neovm_value::value::list_to_vec(&inotify_event_registered_name_lisp_for_test(
        &eval,
        WatchId::new(18, 0),
        vec!["modify"],
        0,
        registration,
    ))
    .expect("inotify event is a proper list");
    assert_eq!(fields[2], registered_name);
    // GNU returns the EXACT Lisp filename object retained by the watch.
    // Rust Value equality falls back to structural equality for distinct
    // heap strings, so pin object identity as well (peer 15:35Z hold: a
    // planted fresh-copy substitution must be able to go red here).
    assert_eq!(
        fields[2].bits(),
        registered_name.bits(),
        "the nameless event must reuse the exact registered filename object"
    );
}
