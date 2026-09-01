use super::*;

fn workspace_temp_dir() -> tempfile::TempDir {
    let parent = std::path::Path::new(env!("CARGO_WORKSPACE_DIR"))
        .join("tmp")
        .join("neovm-core-file-notify-tests");
    std::fs::create_dir_all(&parent).expect("create workspace test directory");
    tempfile::Builder::new()
        .prefix("notify-")
        .tempdir_in(parent)
        .expect("create file notification fixture")
}

#[test]
fn filesystem_changes_reach_the_lisp_callback_through_the_special_event_queue() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let watched_file = directory.path().join("watched.txt");
    std::fs::write(&watched_file, "before").expect("seed watched file");

    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        r#"
        (progn
          (setq neovm-test-file-notify-event nil)
          (defun neovm-test-file-notify-callback (event)
            (setq neovm-test-file-notify-event event)))
        "#,
    )
    .expect("install callback");

    let descriptor = builtin_inotify_add_watch(
        &mut eval,
        vec![
            Value::string(directory.path().display().to_string()),
            Value::list(vec![Value::symbol("modify")]),
            Value::symbol("neovm-test-file-notify-callback"),
        ],
    )
    .expect("add watch");

    std::fs::write(&watched_file, "after").expect("modify watched file");
    eval.eval_str("(read-event nil nil 1)")
        .expect("service file notification");
    let event = eval
        .eval_str("neovm-test-file-notify-event")
        .expect("read callback event");
    let fields = crate::emacs_core::value::list_to_vec(&event).expect("callback event list");
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[0], descriptor);
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&fields[1]),
        Some(vec![Value::symbol("modify")])
    );
    assert_eq!(fields[2], Value::string("watched.txt"));
    assert!(fields[3].as_fixnum().is_some());

    builtin_inotify_rm_watch(vec![descriptor]).expect("remove watch");
}
