//! GNU `sys_unlink` clears the Windows read-only attribute before deleting.
//!
//! That preparation is the second FILE_ACTION_MODIFIED in the upstream
//! attribute-change scenario. The watch lives in neovm-eval and the unlink
//! lives in neovm-system, so the test sits in core, which sees both. Rust's
//! `std::fs::remove_file` bypasses the attribute and would not produce the
//! notification (`unlink_file_path`).

use std::path::Path;

use neovm_value::value::Value;

/// GNU's `sys_unlink' clears the read-only attribute before deleting a file.
/// That preparation is observable as FILE_ACTION_MODIFIED under an
/// FILE_NOTIFY_CHANGE_ATTRIBUTES watch.
#[test]
fn windows_attribute_watch_observes_readonly_clear_before_delete() {
    crate::test_utils::init_test_tracing();
    let directory = tempfile::tempdir().expect("create delete fixture directory");
    let file = directory.path().join("readonly-delete.txt");
    std::fs::write(&file, "contents").expect("seed delete fixture");
    set_windows_readonly(&file, true);

    let mut eval = crate::emacs_core::eval_hook_installers::new_context();
    let variable = "neovm-test-w32-delete-events";
    let callback = install_windows_event_recorder(&mut eval, variable);
    eval.set_variable(
        "neovm-test-w32-delete-file",
        Value::string(file.display().to_string()),
    );
    eval.set_variable("neovm-test-w32-delete-callback", callback);
    let descriptor = eval
        .eval_str(
            "(w32notify-add-watch neovm-test-w32-delete-file '(attributes) neovm-test-w32-delete-callback)",
        )
        .expect("add Windows attribute watch");

    neovm_system::emacs_core::fileio::builtin_delete_file_internal(
        &mut eval,
        vec![Value::string(file.display().to_string())],
    )
    .expect("delete read-only watched file");
    let events = service_until(&mut eval, variable, has_modified_event);
    assert!(
        has_modified_event(&events),
        "deleting the read-only file did not expose GNU's attribute-clear notification"
    );
    assert!(!file.exists(), "read-only file still exists after deletion");

    eval.set_variable("neovm-test-w32-delete-descriptor", descriptor);
    eval.eval_str("(w32notify-rm-watch neovm-test-w32-delete-descriptor)")
        .expect("remove Windows attribute watch");
}

fn set_windows_readonly(path: &Path, readonly: bool) {
    let mut permissions = std::fs::metadata(path)
        .expect("read fixture metadata")
        .permissions();
    permissions.set_readonly(readonly);
    std::fs::set_permissions(path, permissions).expect("set fixture read-only attribute");
}

fn install_windows_event_recorder(
    eval: &mut crate::emacs_core::eval::Context,
    variable: &str,
) -> Value {
    eval.eval_str(&format!(
        r#"
        (progn
          (setq {variable} nil)
          (lambda (event)
            (setq {variable} (cons event {variable}))))
        "#,
    ))
    .expect("create Windows event recorder")
}

fn has_modified_event(events: &[Vec<Value>]) -> bool {
    events
        .iter()
        .any(|fields| fields.get(1) == Some(&Value::symbol("modified")))
}

fn callback_events(eval: &mut crate::emacs_core::eval::Context, variable: &str) -> Vec<Vec<Value>> {
    let events = eval.eval_str(variable).expect("read callback events");
    let mut events: Vec<_> = neovm_value::value::list_to_vec(&events)
        .expect("callback events list")
        .iter()
        .map(|event| neovm_value::value::list_to_vec(event).expect("native event list"))
        .collect();
    // The deliberately minimal callback prepends so this test does not rely
    // on Lisp library functions such as `append'. Restore delivery order at
    // the assertion boundary.
    events.reverse();
    events
}

fn service_until(
    eval: &mut crate::emacs_core::eval::Context,
    variable: &str,
    ready: impl Fn(&[Vec<Value>]) -> bool,
) -> Vec<Vec<Value>> {
    for _ in 0..5 {
        let special_event = eval
            .eval_str("(read-event nil nil 1)")
            .expect("read native file notification");
        if special_event.is_cons() {
            let fields = neovm_value::value::list_to_vec(&special_event)
                .expect("file-notify special event list");
            assert_eq!(fields.first(), Some(&Value::symbol("file-notify")));
            assert_eq!(fields.len(), 3, "file-notify special event shape");
            // A core Context does not load filenotify.el. Apply the same
            // callback and payload fields as `file-notify-handle-event'.
            eval.apply(fields[2], vec![fields[1]])
                .expect("invoke native file-notification callback");
        }
        let events = callback_events(eval, variable);
        if ready(&events) {
            return events;
        }
    }
    let events = callback_events(eval, variable);
    panic!("timed out waiting for native file-notification events: {events:?}");
}
