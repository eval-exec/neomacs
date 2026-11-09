//! Re-homed from eval's file_notify/tests/file_notify_test.rs (P1 F1).
//!
//! Only these re-points: the boot/workspace helpers come from core's
//! test_utils; the driver calls go through the GNU-visible Lisp subrs
//! (`inotify-*`, `kqueue-*`) with Context::funcall_general and
//! already-evaluated Value arguments (GNU Ffuncall semantics; an eval_form
//! source form would evaluate the aspect list and callback symbol as code --
//! peer 14:4xZ hold); the event-injection and thread-local-registry fixtures
//! go through the eval authority fences.  Test names and assertions are
//! unchanged.
use crate::test_utils::init_test_tracing;
use crate::test_utils::runtime_startup_context;
use crate::test_utils::workspace_root;
use neovm_eval::emacs_core::builtins::file_notify::SUBRS;
use neovm_eval::emacs_core::builtins::file_notify::{
    FileNotifyEvent, WatchId, WatchRegistration, WatchRegistry, prepare_deliveries,
    prepare_deliveries_for_test,
};
use neovm_eval::test_support::authority::{
    drain_batch_for_test, file_notify_register_for_test, file_notify_unregister_for_test,
    reset_file_notify_thread_locals_for_test,
};
#[cfg(target_os = "macos")]
use neovm_value::intern::intern;
use neovm_value::value::Value;

fn workspace_temp_dir() -> tempfile::TempDir {
    let parent = workspace_root()
        .as_path()
        .join("target")
        .join("neovm-core-file-notify-tests");
    std::fs::create_dir_all(&parent).expect("create workspace test directory");
    tempfile::Builder::new()
        .prefix("notify-")
        .tempdir_in(parent)
        .expect("create file notification fixture")
}

#[test]
fn compiled_file_notification_subrs_match_the_target_backend() {
    let names: Vec<_> = SUBRS.specs().iter().map(|spec| spec.name()).collect();
    #[cfg(target_os = "linux")]
    assert_eq!(
        names,
        ["inotify-add-watch", "inotify-rm-watch", "inotify-valid-p"]
    );
    #[cfg(target_os = "macos")]
    assert_eq!(
        names,
        ["kqueue-add-watch", "kqueue-rm-watch", "kqueue-valid-p"]
    );
    #[cfg(target_os = "windows")]
    assert_eq!(
        names,
        [
            "w32notify-add-watch",
            "w32notify-rm-watch",
            "w32notify-valid-p"
        ]
    );
}

#[derive(Debug)]
struct LifecycleTestEvent(WatchId);

impl FileNotifyEvent for LifecycleTestEvent {
    fn watch_id(&self) -> &WatchId {
        &self.0
    }

    fn into_lisp(
        self,
        _ctx: &crate::emacs_core::eval::Context,
        _registration: WatchRegistration,
    ) -> Value {
        unreachable!("the lifecycle test does not encode events")
    }
}

#[test]
fn terminal_watch_stays_rooted_until_delivery_finishes() {
    // Post-split re-point: no-Context Value fixtures install the canonical
    // test-owned heap scope (the AUTHORITY fence's TestHeapScope::install);
    // the scope restores/uninstalls the thread's heap on Drop.
    let _heap = neovm_value::test_support::authority::TestHeapScope::install();
    let watch_id = WatchId::new(7, 3);
    let mut registry = WatchRegistry::default();
    registry.register(
        watch_id.clone(),
        Value::fixnum(42),
        Value::string("watched"),
    );

    let (deliveries, terminated, failure) = prepare_deliveries(
        &registry,
        drain_batch_for_test::<LifecycleTestEvent>(Vec::new(), vec![watch_id], None),
    );

    assert!(deliveries.is_empty());
    assert!(failure.is_none());
    let mut roots = Vec::new();
    registry.collect_gc_roots(&mut roots);
    assert!(
        roots.contains(&Value::fixnum(42)),
        "terminal callback lost its GC root before delivery completed"
    );

    for watch_id in terminated {
        registry.unregister(&watch_id);
    }
    roots.clear();
    registry.collect_gc_roots(&mut roots);
    assert!(
        roots.is_empty(),
        "completed registration remained GC-rooted"
    );
}

#[test]
fn terminal_delivery_values_survive_exact_gc_until_queued() {
    reset_file_notify_thread_locals_for_test();
    // Post-split re-point: the fixture Values must live on the evaluator's
    // heap for the exact GC below to trace them (pre-split they were
    // allocated through the value crate's cfg(test) fallback heap, which a
    // dependency crate does not have).  The Context is created first for
    // that reason; the assertions are unchanged.
    let mut eval = runtime_startup_context();
    let watch_id = WatchId::new(9, 0);
    let callback = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(
        b"callback-root".to_vec(),
    ));
    let file_name = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(
        b"file-name-root".to_vec(),
    ));
    file_notify_register_for_test(watch_id.clone(), callback, file_name);
    let (deliveries, terminated, failure) = prepare_deliveries_for_test(drain_batch_for_test(
        vec![LifecycleTestEvent(watch_id)],
        vec![WatchId::new(9, 0)],
        None,
    ));
    assert!(failure.is_none());

    eval.eval_str("(garbage-collect)")
        .expect("run an exact collection while terminal delivery is pending");

    let registration = deliveries[0].1;
    assert_eq!(
        registration
            .callback()
            .as_lisp_string()
            .expect("callback remained a string")
            .as_bytes(),
        b"callback-root"
    );
    assert_eq!(
        registration
            .registered_file_name()
            .as_lisp_string()
            .expect("file name remained a string")
            .as_bytes(),
        b"file-name-root"
    );

    for watch_id in terminated {
        file_notify_unregister_for_test(&watch_id);
    }
}

#[test]
fn watch_registry_roots_every_lisp_object_needed_for_delivery() {
    // Post-split re-point: no-Context Value fixture -> the canonical
    // test-owned heap scope (AUTHORITY fence `TestHeapScope::install`).
    let _heap = neovm_value::test_support::authority::TestHeapScope::install();
    let watch_id = WatchId::new(8, 0);
    let callback = Value::symbol("file-notify-test-callback");
    let file_name = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
        b'f', 0xff, b'o',
    ]));
    let mut registry = WatchRegistry::default();
    registry.register(watch_id.clone(), callback, file_name);

    let registration = registry
        .registration(&watch_id)
        .expect("registered watch has evaluator state");
    assert_eq!(registration.callback(), callback);
    assert_eq!(registration.registered_file_name(), file_name);

    let mut roots = Vec::new();
    registry.collect_gc_roots(&mut roots);
    assert!(roots.contains(&callback));
    assert!(roots.contains(&file_name));
}

/// The kqueue tests below drive the GNU-visible Lisp subrs through the
/// evaluator's `funcall_general` with already-evaluated Value arguments
/// (peer 12:57Z (c) and the 14:4xZ eval_form hold); these thin wrappers keep
/// each test body's call shape while the symbol resolves to the registered
/// `kqueue-*` subr, exactly as GNU's Ffuncall does.
#[cfg(target_os = "macos")]
fn kqueue_call(
    eval: &mut neovm_eval::emacs_core::eval::Context,
    name: &str,
    args: Vec<Value>,
) -> neovm_value::error::EvalResult {
    eval.funcall_general(Value::symbol(name), args)
}

#[cfg(target_os = "macos")]
fn kqueue_add_watch(
    eval: &mut neovm_eval::emacs_core::eval::Context,
    args: Vec<Value>,
) -> neovm_value::error::EvalResult {
    kqueue_call(eval, "kqueue-add-watch", args)
}

#[cfg(target_os = "macos")]
fn kqueue_rm_watch(
    eval: &mut neovm_eval::emacs_core::eval::Context,
    args: Vec<Value>,
) -> neovm_value::error::EvalResult {
    kqueue_call(eval, "kqueue-rm-watch", args)
}

#[cfg(target_os = "macos")]
fn kqueue_valid_p(
    eval: &mut neovm_eval::emacs_core::eval::Context,
    args: Vec<Value>,
) -> neovm_value::error::EvalResult {
    kqueue_call(eval, "kqueue-valid-p", args)
}

/// Destructure a `Flow` into its signal payload; Debug-printing a `SymId`
/// resolves the name best-effort and is not stable under parallel tests, so
/// error assertions compare interned symbols structurally.
#[cfg(target_os = "macos")]
fn expect_signal(err: neovm_value::error::Flow) -> Box<neovm_value::error::SignalData> {
    let signal = match err.into_kind() {
        neovm_value::error::FlowKind::Signal(signal) => signal,
        err => {
            panic!("expected a signal, got {err:?}");
        }
    };
    signal
}

#[test]
#[cfg(target_os = "linux")]
fn filesystem_changes_reach_the_lisp_callback_through_the_special_event_queue() {
    init_test_tracing();
    let directory = workspace_temp_dir();
    let watched_file = directory.path().join("watched.txt");
    std::fs::write(&watched_file, "before").expect("seed watched file");

    let mut eval = runtime_startup_context();
    eval.eval_str(
        r#"
        (progn
          (setq neovm-test-file-notify-event nil)
          (defun neovm-test-file-notify-callback (event)
            (setq neovm-test-file-notify-event event)))
        "#,
    )
    .expect("install callback");

    let descriptor = eval
        .funcall_general(
            Value::symbol("inotify-add-watch"),
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
    let fields = neovm_value::value::list_to_vec(&event).expect("callback event list");
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[0], descriptor);
    assert_eq!(
        neovm_value::value::list_to_vec(&fields[1]),
        Some(vec![Value::symbol("modify")])
    );
    assert_eq!(fields[2], Value::string("watched.txt"));
    assert!(fields[3].as_fixnum().is_some());

    eval.funcall_general(Value::symbol("inotify-rm-watch"), vec![descriptor])
        .expect("remove watch");
}

/// GNU `Fkqueue_add_watch` (src/kqueue.c:338) returns a bare opaque fixnum
/// descriptor where inotify descriptors are conses, and a
/// kqueue event is `(DESCRIPTOR ACTIONS FILE [FILE1])` with NO trailing
/// cookie (`kqueue_generate_event`, src/kqueue.c:71-105).  For a plain file
/// watch the reported FILE is the watched file's own name, and ACTIONS is
/// filtered to the requested flags by exact `Fmember` (:84-90).
#[test]
#[cfg(target_os = "macos")]
fn kqueue_file_watch_reports_a_write_action_with_gnus_event_shape() {
    init_test_tracing();
    let directory = workspace_temp_dir();
    let watched_file = directory.path().join("watched.txt");
    std::fs::write(&watched_file, "before").expect("seed watched file");

    let mut eval = runtime_startup_context();
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
    let descriptor = kqueue_add_watch(
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
    let events = neovm_value::value::list_to_vec(&events).expect("events list");
    let write_event = events
        .iter()
        .map(|event| neovm_value::value::list_to_vec(event).expect("event list"))
        .find(|fields| {
            neovm_value::value::list_to_vec(&fields[1])
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

    kqueue_rm_watch(&mut eval, vec![descriptor]).expect("remove watch");
}

/// GNU generates directory events by diffing directory listings
/// (`kqueue_compare_dir_list`, src/kqueue.c:110-273): a new file inside the
/// watched directory is a `create' with the file's RELATIVE name, and
/// kqueue has no NOTE_CREATE, so GNU observes NOTE_WRITE on the directory and
/// reconstructs a child `create' with its relative name from two snapshots.
#[test]
#[cfg(target_os = "macos")]
fn kqueue_directory_watch_reports_relative_names_from_snapshot_diffs() {
    init_test_tracing();
    let directory = workspace_temp_dir();
    let existing = directory.path().join("existing.txt");
    std::fs::write(&existing, "seed").expect("seed existing file");

    let mut eval = runtime_startup_context();
    eval.eval_str(
        r#"
        (progn
          (setq neovm-test-kqueue-events nil)
          (defun neovm-test-kqueue-callback (event)
            (push event neovm-test-kqueue-events)))
        "#,
    )
    .expect("install callback");

    let descriptor = kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(directory.path().display().to_string()),
            Value::list(vec![Value::symbol("create"), Value::symbol("write")]),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect("add kqueue directory watch");

    std::fs::write(&existing, "rewritten").expect("write existing file");
    std::fs::write(directory.path().join("created.txt"), "new").expect("create new file");
    eval.eval_str("(read-event nil nil 1)")
        .expect("service file notification");

    let events = eval
        .eval_str("neovm-test-kqueue-events")
        .expect("read callback events");
    let events: Vec<Vec<Value>> = neovm_value::value::list_to_vec(&events)
        .expect("events list")
        .iter()
        .map(|event| neovm_value::value::list_to_vec(event).expect("event list"))
        .collect();

    assert!(
        events.iter().any(|fields| {
            neovm_value::value::list_to_vec(&fields[1])
                .is_some_and(|actions| actions == vec![Value::symbol("create")])
                && fields[2] == Value::string("created.txt")
        }),
        "a directory watch reports the created file's relative name: {events:?}"
    );
    kqueue_rm_watch(&mut eval, vec![descriptor]).expect("remove watch");
}

/// `Fkqueue_rm_watch` (src/kqueue.c:475) answers t and unregisters; a
/// descriptor that is not in the watch list signals `(file-notify-error
/// "Not a watch descriptor" DESCRIPTOR)` -- unlike inotify's errno-shaped
/// message.  `Fkqueue_valid_p' (:505) never signals.  And `kqueue_callback'
/// (:330-333) removes the watch itself when the watched file is deleted, so
/// validity dies with the file.
#[test]
#[cfg(target_os = "macos")]
fn kqueue_rm_watch_and_valid_p_follow_gnu_and_a_deleted_file_invalidates_its_watch() {
    init_test_tracing();
    let directory = workspace_temp_dir();
    let removable = directory.path().join("removable.txt");
    std::fs::write(&removable, "doomed").expect("seed removable file");

    let mut eval = runtime_startup_context();
    eval.eval_str(
        r#"
        (progn
          (setq neovm-test-kqueue-events nil)
          (defun neovm-test-kqueue-callback (event)
            (push event neovm-test-kqueue-events)))
        "#,
    )
    .expect("install callback");

    let descriptor = kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(removable.display().to_string()),
            Value::list(vec![Value::symbol("delete"), Value::symbol("write")]),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect("add kqueue watch");

    assert_eq!(
        kqueue_valid_p(&mut eval, vec![descriptor]).unwrap(),
        Value::T
    );
    assert_eq!(
        kqueue_rm_watch(&mut eval, vec![descriptor]).unwrap(),
        Value::T
    );
    assert_eq!(
        kqueue_valid_p(&mut eval, vec![descriptor]).unwrap(),
        Value::NIL
    );

    let signal =
        expect_signal(kqueue_rm_watch(&mut eval, vec![descriptor]).expect_err("stale descriptor"));
    assert_eq!(signal.symbol, intern("file-notify-error"), "{signal:?}");
    assert_eq!(
        signal.data[0]
            .as_lisp_string()
            .and_then(|message| message.as_utf8_str()),
        Some("Not a watch descriptor"),
        "{signal:?}"
    );
    assert_eq!(signal.data[1], descriptor, "GNU's data is the descriptor");

    let descriptor = kqueue_add_watch(
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
    let events = neovm_value::value::list_to_vec(&events).expect("events list");
    assert!(
        events.iter().any(|event| {
            neovm_value::value::list_to_vec(event).is_some_and(|fields| {
                fields[0] == descriptor
                    && neovm_value::value::list_to_vec(&fields[1])
                        .is_some_and(|actions| actions.contains(&Value::symbol("delete")))
            })
        }),
        "deleting the watched file reports a delete action: {events:?}"
    );
    assert_eq!(
        kqueue_valid_p(&mut eval, vec![descriptor]).unwrap(),
        Value::NIL,
        "GNU cancels the monitor when the watched file is deleted (src/kqueue.c:330-333)"
    );
}

/// `Fkqueue_add_watch`'s own checks, in GNU's order (src/kqueue.c:380-389):
/// a missing FILE is a file error (`report_file_error', ENOENT ->
/// `file-missing'); FLAGS must satisfy `CHECK_LIST'; CALLBACK must satisfy
/// `FUNCTIONP' or it is `(wrong-type-argument invalid-function ...)'.  A
/// symbol in FLAGS that kqueue does not know is simply ignored -- the native
/// flag assembly is seven `Fmember' probes (:440-446), while `create' is
/// synthesized from a directory diff; neither path is a validation pass --
/// unlike inotify's `Unknown aspect' error.
#[test]
#[cfg(target_os = "macos")]
fn kqueue_add_watch_checks_arguments_like_gnu_and_ignores_unknown_flags() {
    init_test_tracing();
    let directory = workspace_temp_dir();
    let watched_file = directory.path().join("checked.txt");
    std::fs::write(&watched_file, "content").expect("seed watched file");

    let mut eval = runtime_startup_context();
    eval.eval_str("(defun neovm-test-kqueue-callback (_event) nil)")
        .expect("install callback");

    let err = kqueue_add_watch(
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

    let err = kqueue_add_watch(
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

    let err = kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(watched_file.display().to_string()),
            Value::cons(Value::symbol("write"), Value::symbol("dotted-tail")),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect_err("GNU CHECK_LIST rejects an improper flags list");
    let signal = expect_signal(err);
    assert_eq!(signal.symbol, intern("wrong-type-argument"), "{signal:?}");
    assert_eq!(signal.data[0], Value::symbol("listp"), "{signal:?}");

    let err = kqueue_add_watch(
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

    let descriptor = kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(watched_file.display().to_string()),
            Value::list(vec![Value::symbol("frobnicate"), Value::symbol("write")]),
            Value::symbol("neovm-test-kqueue-callback"),
        ],
    )
    .expect("an unknown flag symbol is ignored, not an error");
    kqueue_rm_watch(&mut eval, vec![descriptor]).expect("remove watch");
}

/// GNU normalizes kqueue FILE before checking or storing it:
/// `Fdirectory_file_name (Fexpand_file_name (file, Qnil))`
/// (`src/kqueue.c:380-381`). Relative names therefore resolve against the
/// current buffer's `default-directory`, and file events report the stored
/// absolute name.
#[test]
#[cfg(target_os = "macos")]
fn kqueue_watch_expands_relative_file_against_default_directory() {
    init_test_tracing();
    let directory = workspace_temp_dir();
    let watched_file = directory.path().join("relative.txt");
    std::fs::write(&watched_file, "before").expect("seed watched file");

    let mut eval = runtime_startup_context();
    let current_buffer = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.edit
        .buffers
        .get_mut(current_buffer)
        .expect("current buffer")
        .set_buffer_local(
            "default-directory",
            Value::string(format!(
                "{}{}",
                directory.path().display(),
                std::path::MAIN_SEPARATOR
            )),
        );
    eval.eval_str(
        r#"(progn
             (setq neovm-test-relative-kqueue-events nil)
             (defun neovm-test-relative-kqueue-callback (event)
               (push event neovm-test-relative-kqueue-events)))"#,
    )
    .expect("install relative watch environment");

    let descriptor = kqueue_add_watch(
        &mut eval,
        vec![
            Value::string("relative.txt"),
            Value::list(vec![Value::symbol("write")]),
            Value::symbol("neovm-test-relative-kqueue-callback"),
        ],
    )
    .expect("relative kqueue watch resolves through default-directory");

    std::fs::write(&watched_file, "after").expect("modify watched file");
    eval.eval_str("(read-event nil nil 1)")
        .expect("service relative file notification");
    let events = eval
        .eval_str("neovm-test-relative-kqueue-events")
        .expect("read callback events");
    let events = neovm_value::value::list_to_vec(&events).expect("events list");
    assert!(
        events.iter().any(|event| {
            neovm_value::value::list_to_vec(event).is_some_and(|fields| {
                fields[0] == descriptor
                    && fields[2] == Value::string(watched_file.display().to_string())
            })
        }),
        "kqueue stores and reports GNU's normalized absolute FILE: {events:?}"
    );

    kqueue_rm_watch(&mut eval, vec![descriptor]).expect("remove watch");
}
