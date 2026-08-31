use super::*;
use crate::emacs_core::intern::intern;

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

/// Destructure a `Flow` into its signal payload; Debug-printing a `SymId`
/// resolves the name best-effort and is not stable under parallel tests, so
/// error assertions compare interned symbols structurally.
fn expect_signal(err: crate::emacs_core::error::Flow) -> Box<crate::emacs_core::error::SignalData> {
    let crate::emacs_core::error::Flow::Signal(signal) = err else {
        panic!("expected a signal, got {err:?}");
    };
    signal
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

/// GNU `Fkqueue_add_watch` (src/kqueue.c:338) returns a bare fixnum
/// descriptor -- the open fd -- where inotify descriptors are conses, and a
/// kqueue event is `(DESCRIPTOR ACTIONS FILE [FILE1])` with NO trailing
/// cookie (`kqueue_generate_event`, src/kqueue.c:71-105).  For a plain file
/// watch the reported FILE is the watched file's own name, and ACTIONS is
/// filtered to the requested flags by exact `Fmember` (:84-90).
#[test]
fn kqueue_file_watch_reports_a_write_action_with_gnus_event_shape() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let watched_file = directory.path().join("watched.txt");
    std::fs::write(&watched_file, "before").expect("seed watched file");

    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        r#"
        (progn
          (setq neovm-test-kqueue-events nil)
          (defun neovm-test-kqueue-callback (event)
            (push event neovm-test-kqueue-events)))
        "#,
    )
    .expect("install callback");

    // The flags filenotify.el's kqueue adapter sends for `(change)'
    // (lisp/filenotify.el:361-372).
    let descriptor = builtin_kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(watched_file.display().to_string()),
            Value::list(vec![
                Value::symbol("revoke"),
                Value::symbol("create"),
                Value::symbol("delete"),
                Value::symbol("write"),
                Value::symbol("extend"),
                Value::symbol("rename"),
            ]),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect("add kqueue watch");
    assert!(
        descriptor.as_fixnum().is_some(),
        "GNU kqueue descriptors are fixnums, got {descriptor:?}"
    );

    std::fs::write(&watched_file, "after-longer-content").expect("modify watched file");
    eval.eval_str("(read-event nil nil 1)")
        .expect("service file notification");

    let events = eval
        .eval_str("neovm-test-kqueue-events")
        .expect("read callback events");
    let events = crate::emacs_core::value::list_to_vec(&events).expect("events list");
    let write_event = events
        .iter()
        .map(|event| crate::emacs_core::value::list_to_vec(event).expect("event list"))
        .find(|fields| {
            crate::emacs_core::value::list_to_vec(&fields[1])
                .is_some_and(|actions| actions.contains(&Value::symbol("write")))
        })
        .unwrap_or_else(|| panic!("no write event among {events:?}"));

    assert_eq!(
        write_event.len(),
        3,
        "a kqueue event is (DESCRIPTOR ACTIONS FILE) with no cookie"
    );
    assert_eq!(write_event[0], descriptor);
    assert_eq!(
        write_event[2],
        Value::string(watched_file.display().to_string()),
        "a file watch reports the watched file's own name"
    );

    builtin_kqueue_rm_watch(vec![descriptor]).expect("remove watch");
}

/// GNU generates directory events by diffing directory listings
/// (`kqueue_compare_dir_list`, src/kqueue.c:110-273): a new file inside the
/// watched directory is a `create' with the file's RELATIVE name, and
/// `kqueue_generate_event' (:84-90) drops any action the caller did not list
/// in FLAGS -- so a plain write to an existing file is silent for a watch
/// that only asked for `create'.
#[test]
fn kqueue_directory_watch_reports_relative_names_and_filters_unrequested_actions() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let existing = directory.path().join("existing.txt");
    std::fs::write(&existing, "seed").expect("seed existing file");

    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        r#"
        (progn
          (setq neovm-test-kqueue-events nil)
          (defun neovm-test-kqueue-callback (event)
            (push event neovm-test-kqueue-events)))
        "#,
    )
    .expect("install callback");

    let descriptor = builtin_kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(directory.path().display().to_string()),
            Value::list(vec![Value::symbol("create")]),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect("add kqueue directory watch");

    // The unrequested action first, so its delivery (a bug) would be visible
    // by the time the requested one arrives.
    std::fs::write(&existing, "rewritten").expect("write existing file");
    std::fs::write(directory.path().join("created.txt"), "new").expect("create new file");
    eval.eval_str("(read-event nil nil 1)")
        .expect("service file notification");

    let events = eval
        .eval_str("neovm-test-kqueue-events")
        .expect("read callback events");
    let events: Vec<Vec<Value>> = crate::emacs_core::value::list_to_vec(&events)
        .expect("events list")
        .iter()
        .map(|event| crate::emacs_core::value::list_to_vec(event).expect("event list"))
        .collect();

    assert!(
        events.iter().any(|fields| {
            crate::emacs_core::value::list_to_vec(&fields[1])
                .is_some_and(|actions| actions == vec![Value::symbol("create")])
                && fields[2] == Value::string("created.txt")
        }),
        "a directory watch reports the created file's relative name: {events:?}"
    );
    // The write to existing.txt would deliver a `write' action if the
    // `Fmember' filter (src/kqueue.c:84-90) were not applied.  (Asserting on
    // the file NAME instead would be unsound: FSEvents coalesces per-path
    // flags, so existing.txt may legitimately surface a stale `create'.)
    for fields in &events {
        assert_eq!(
            crate::emacs_core::value::list_to_vec(&fields[1]),
            Some(vec![Value::symbol("create")]),
            "an action absent from FLAGS is never delivered: {events:?}"
        );
    }

    builtin_kqueue_rm_watch(vec![descriptor]).expect("remove watch");
}

/// `Fkqueue_rm_watch` (src/kqueue.c:475) answers t and unregisters; a
/// descriptor that is not in the watch list signals `(file-notify-error
/// "Not a watch descriptor" DESCRIPTOR)` -- unlike inotify's errno-shaped
/// message.  `Fkqueue_valid_p' (:505) never signals.  And `kqueue_callback'
/// (:330-333) removes the watch itself when the watched file is deleted, so
/// validity dies with the file.
#[test]
fn kqueue_rm_watch_and_valid_p_follow_gnu_and_a_deleted_file_invalidates_its_watch() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let removable = directory.path().join("removable.txt");
    std::fs::write(&removable, "doomed").expect("seed removable file");

    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        r#"
        (progn
          (setq neovm-test-kqueue-events nil)
          (defun neovm-test-kqueue-callback (event)
            (push event neovm-test-kqueue-events)))
        "#,
    )
    .expect("install callback");

    let descriptor = builtin_kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(removable.display().to_string()),
            Value::list(vec![Value::symbol("delete"), Value::symbol("write")]),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect("add kqueue watch");

    assert_eq!(builtin_kqueue_valid_p(vec![descriptor]).unwrap(), Value::T);
    assert_eq!(builtin_kqueue_rm_watch(vec![descriptor]).unwrap(), Value::T);
    assert_eq!(
        builtin_kqueue_valid_p(vec![descriptor]).unwrap(),
        Value::NIL
    );

    let signal =
        expect_signal(builtin_kqueue_rm_watch(vec![descriptor]).expect_err("stale descriptor"));
    assert_eq!(signal.symbol, intern("file-notify-error"), "{signal:?}");
    assert_eq!(
        signal.data[0]
            .as_lisp_string()
            .and_then(|message| message.as_utf8_str()),
        Some("Not a watch descriptor"),
        "{signal:?}"
    );
    assert_eq!(signal.data[1], descriptor, "GNU's data is the descriptor");

    let descriptor = builtin_kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(removable.display().to_string()),
            Value::list(vec![Value::symbol("delete"), Value::symbol("write")]),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect("re-add kqueue watch");
    std::fs::remove_file(&removable).expect("delete watched file");
    eval.eval_str("(read-event nil nil 1)")
        .expect("service file notification");

    let events = eval
        .eval_str("neovm-test-kqueue-events")
        .expect("read callback events");
    let events = crate::emacs_core::value::list_to_vec(&events).expect("events list");
    assert!(
        events.iter().any(|event| {
            crate::emacs_core::value::list_to_vec(event).is_some_and(|fields| {
                fields[0] == descriptor
                    && crate::emacs_core::value::list_to_vec(&fields[1])
                        .is_some_and(|actions| actions.contains(&Value::symbol("delete")))
            })
        }),
        "deleting the watched file reports a delete action: {events:?}"
    );
    assert_eq!(
        builtin_kqueue_valid_p(vec![descriptor]).unwrap(),
        Value::NIL,
        "GNU cancels the monitor when the watched file is deleted (src/kqueue.c:330-333)"
    );
}

/// `Fkqueue_add_watch`'s own checks, in GNU's order (src/kqueue.c:380-389):
/// a missing FILE is a file error (`report_file_error', ENOENT ->
/// `file-missing'); FLAGS must satisfy `CHECK_LIST'; CALLBACK must satisfy
/// `FUNCTIONP' or it is `(wrong-type-argument invalid-function ...)'.  A
/// symbol in FLAGS that kqueue does not know is simply ignored -- the flag
/// assembly is eight `Fmember' probes (:440-446), not a validation pass --
/// unlike inotify's `Unknown aspect' error.
#[test]
fn kqueue_add_watch_checks_arguments_like_gnu_and_ignores_unknown_flags() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let watched_file = directory.path().join("checked.txt");
    std::fs::write(&watched_file, "content").expect("seed watched file");

    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str("(defun neovm-test-kqueue-callback (_event) nil)")
        .expect("install callback");

    let err = builtin_kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(directory.path().join("missing.txt").display().to_string()),
            Value::list(vec![Value::symbol("write")]),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect_err("a missing file is a file error");
    let signal = expect_signal(err);
    assert_eq!(signal.symbol, intern("file-missing"), "{signal:?}");

    let err = builtin_kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(watched_file.display().to_string()),
            Value::fixnum(5),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect_err("FLAGS must be a list");
    let signal = expect_signal(err);
    assert_eq!(signal.symbol, intern("wrong-type-argument"), "{signal:?}");
    assert_eq!(signal.data[0], Value::symbol("listp"), "{signal:?}");

    let err = builtin_kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(watched_file.display().to_string()),
            Value::list(vec![Value::symbol("write")]),
            Value::fixnum(42),
        ],
    )
    .expect_err("CALLBACK must be a function");
    let signal = expect_signal(err);
    assert_eq!(signal.symbol, intern("wrong-type-argument"), "{signal:?}");
    assert_eq!(
        signal.data[0],
        Value::symbol("invalid-function"),
        "{signal:?}"
    );

    let descriptor = builtin_kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(watched_file.display().to_string()),
            Value::list(vec![Value::symbol("frobnicate"), Value::symbol("write")]),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect("an unknown flag symbol is ignored, not an error");
    builtin_kqueue_rm_watch(vec![descriptor]).expect("remove watch");
}
