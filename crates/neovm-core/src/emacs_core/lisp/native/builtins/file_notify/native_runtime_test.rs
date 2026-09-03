//! Host-OS tests for the complete native worker-to-Lisp delivery path.

use super::*;
use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;

fn workspace_temp_dir() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_WORKSPACE_DIR"))
        .join("target")
        .join("neovm-core-file-notify-tests");
    std::fs::create_dir_all(&parent).expect("create workspace test directory");
    tempfile::Builder::new()
        .prefix("native-notify-")
        .tempdir_in(parent)
        .expect("create file notification fixture")
}

fn callback_events(eval: &mut crate::emacs_core::eval::Context, variable: &str) -> Vec<Vec<Value>> {
    let events = eval.eval_str(variable).expect("read callback events");
    let mut events: Vec<_> = crate::emacs_core::value::list_to_vec(&events)
        .expect("callback events list")
        .iter()
        .map(|event| crate::emacs_core::value::list_to_vec(event).expect("native event list"))
        .collect();
    // The deliberately minimal callback prepends so these tests do not rely
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
            let fields = crate::emacs_core::value::list_to_vec(&special_event)
                .expect("file-notify special event list");
            assert_eq!(fields.first(), Some(&Value::symbol("file-notify")));
            assert_eq!(fields.len(), 3, "file-notify special event shape");
            // A bare Context seeds the native special-event key, but does not
            // load filenotify.el. Apply the same callback and payload fields as
            // `file-notify-handle-event' so host CI remains independent of
            // generated Lisp sources while still exercising Lisp invocation.
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

fn install_self_removing_callback(eval: &mut crate::emacs_core::eval::Context) -> Value {
    #[cfg(target_os = "linux")]
    let remove_watch = "inotify-rm-watch";
    #[cfg(target_os = "macos")]
    let remove_watch = "kqueue-rm-watch";
    #[cfg(target_os = "windows")]
    let remove_watch = "w32notify-rm-watch";

    eval.eval_str(&format!(
        r#"
        (progn
          (setq neovm-test-self-removing-events nil)
          (lambda (event)
            (setq neovm-test-self-removing-events
                  (cons event neovm-test-self-removing-events))
            ({remove_watch} (car event))))
        "#,
    ))
    .expect("create self-removing callback")
}

#[cfg(target_os = "linux")]
fn add_self_removing_watch(
    eval: &mut crate::emacs_core::eval::Context,
    directory: &Path,
    callback: Value,
) -> Value {
    inotify_add_watch(
        eval,
        vec![
            Value::string(directory.display().to_string()),
            Value::list(vec![Value::symbol("create")]),
            callback,
        ],
    )
    .expect("add self-removing inotify watch")
}

#[cfg(target_os = "macos")]
fn add_self_removing_watch(
    eval: &mut crate::emacs_core::eval::Context,
    directory: &Path,
    callback: Value,
) -> Value {
    kqueue_add_watch(
        eval,
        vec![
            Value::string(directory.display().to_string()),
            Value::list(vec![Value::symbol("create"), Value::symbol("write")]),
            callback,
        ],
    )
    .expect("add self-removing kqueue watch")
}

#[cfg(target_os = "windows")]
fn add_self_removing_watch(
    eval: &mut crate::emacs_core::eval::Context,
    directory: &Path,
    callback: Value,
) -> Value {
    w32notify_add_watch(
        eval,
        vec![
            Value::string(directory.display().to_string()),
            Value::list(vec![Value::symbol("file-name")]),
            callback,
        ],
    )
    .expect("add self-removing Windows watch")
}

#[cfg(target_os = "linux")]
fn native_watch_is_valid(descriptor: Value) -> bool {
    inotify_valid_p(vec![descriptor]).expect("query inotify watch validity") == Value::T
}

#[cfg(target_os = "macos")]
fn native_watch_is_valid(descriptor: Value) -> bool {
    kqueue_valid_p(vec![descriptor]).expect("query kqueue watch validity") == Value::T
}

#[cfg(target_os = "windows")]
fn native_watch_is_valid(descriptor: Value) -> bool {
    w32notify_valid_p(vec![descriptor]).expect("query Windows watch validity") == Value::T
}

/// The Lisp callback may remove the descriptor currently being delivered.
/// This catches worker-join deadlocks, stale registrations, and premature GC
/// unrooting that request/codec-only tests cannot observe.
#[test]
fn native_callback_can_remove_its_own_watch() {
    crate::test_utils::init_test_tracing();
    reset_file_notify_thread_locals();
    let directory = workspace_temp_dir();
    let mut eval = crate::emacs_core::eval::Context::new();
    let callback = install_self_removing_callback(&mut eval);
    let descriptor = add_self_removing_watch(&mut eval, directory.path(), callback);
    assert!(native_watch_is_valid(descriptor));

    std::fs::write(directory.path().join("created.txt"), "created")
        .expect("trigger native file notification");
    let events = service_until(&mut eval, "neovm-test-self-removing-events", |events| {
        !events.is_empty()
    });

    assert!(!events.is_empty(), "the native callback did not run");
    assert!(
        !native_watch_is_valid(descriptor),
        "the descriptor remained live after its callback removed it"
    );
    reset_file_notify_thread_locals();
}

#[test]
#[cfg(target_os = "linux")]
fn inotify_rename_delivers_an_ordered_cookie_pair() {
    crate::test_utils::init_test_tracing();
    reset_file_notify_thread_locals();
    let directory = workspace_temp_dir();
    let from = directory.path().join("rename-from.txt");
    let to = directory.path().join("rename-to.txt");
    std::fs::write(&from, "contents").expect("seed renamed file");

    let mut eval = crate::emacs_core::eval::Context::new();
    let callback = eval
        .eval_str(
            r#"
        (progn
          (setq neovm-test-inotify-rename-events nil)
          (lambda (event)
            (setq neovm-test-inotify-rename-events
                  (cons event neovm-test-inotify-rename-events))))
        "#,
        )
        .expect("create inotify rename callback");
    let descriptor = inotify_add_watch(
        &mut eval,
        vec![
            Value::string(directory.path().display().to_string()),
            Value::list(vec![Value::symbol("moved-from"), Value::symbol("moved-to")]),
            callback,
        ],
    )
    .expect("add inotify rename watch");

    std::fs::rename(&from, &to).expect("rename watched child");
    let events = service_until(&mut eval, "neovm-test-inotify-rename-events", |events| {
        events.len() >= 2
    });
    let pair = &events[..2];
    assert_eq!(pair[0][0], descriptor);
    assert_eq!(pair[1][0], descriptor);
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&pair[0][1]),
        Some(vec![Value::symbol("moved-from")])
    );
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&pair[1][1]),
        Some(vec![Value::symbol("moved-to")])
    );
    assert_eq!(pair[0][2], Value::string("rename-from.txt"));
    assert_eq!(pair[1][2], Value::string("rename-to.txt"));
    assert_eq!(pair[0][3], pair[1][3], "rename cookies must pair");
    assert_ne!(
        pair[0][3],
        Value::fixnum(0),
        "rename cookie must be nonzero"
    );
    inotify_rm_watch(vec![descriptor]).expect("remove inotify rename watch");
    reset_file_notify_thread_locals();
}

#[test]
#[cfg(target_os = "macos")]
fn kqueue_directory_rename_reports_both_relative_names() {
    crate::test_utils::init_test_tracing();
    reset_file_notify_thread_locals();
    let directory = workspace_temp_dir();
    let from = directory.path().join("rename-from.txt");
    let to = directory.path().join("rename-to.txt");
    std::fs::write(&from, "contents").expect("seed renamed file");

    let mut eval = crate::emacs_core::eval::Context::new();
    let callback = eval
        .eval_str(
            r#"
        (progn
          (setq neovm-test-kqueue-rename-events nil)
          (lambda (event)
            (setq neovm-test-kqueue-rename-events
                  (cons event neovm-test-kqueue-rename-events))))
        "#,
        )
        .expect("create kqueue rename callback");
    let descriptor = kqueue_add_watch(
        &mut eval,
        vec![
            Value::string(directory.path().display().to_string()),
            Value::list(vec![Value::symbol("rename"), Value::symbol("write")]),
            callback,
        ],
    )
    .expect("add kqueue rename watch");

    std::fs::rename(&from, &to).expect("rename watched child");
    let events = service_until(&mut eval, "neovm-test-kqueue-rename-events", |events| {
        events.iter().any(|fields| {
            fields.len() == 4
                && crate::emacs_core::value::list_to_vec(&fields[1])
                    .is_some_and(|actions| actions.contains(&Value::symbol("rename")))
        })
    });
    let rename = events
        .iter()
        .find(|fields| fields.len() == 4)
        .expect("kqueue rename event with old and new names");
    assert_eq!(rename[0], descriptor);
    assert_eq!(rename[2], Value::string("rename-from.txt"));
    assert_eq!(rename[3], Value::string("rename-to.txt"));
    kqueue_rm_watch(vec![descriptor]).expect("remove kqueue rename watch");
    reset_file_notify_thread_locals();
}

#[test]
#[cfg(target_os = "windows")]
fn windows_recursive_watch_delivers_an_ordered_rename_pair() {
    crate::test_utils::init_test_tracing();
    reset_file_notify_thread_locals();
    let directory = workspace_temp_dir();
    let nested = directory.path().join("nested");
    std::fs::create_dir(&nested).expect("create nested directory");
    let from = nested.join("rename-from.txt");
    let to = nested.join("rename-to.txt");
    std::fs::write(&from, "contents").expect("seed renamed file");

    let mut eval = crate::emacs_core::eval::Context::new();
    let callback = eval
        .eval_str(
            r#"
        (progn
          (setq neovm-test-w32-rename-events nil)
          (lambda (event)
            (setq neovm-test-w32-rename-events
                  (cons event neovm-test-w32-rename-events))))
        "#,
        )
        .expect("create Windows rename callback");
    let descriptor = w32notify_add_watch(
        &mut eval,
        vec![
            Value::string(directory.path().display().to_string()),
            Value::list(vec![Value::symbol("file-name"), Value::symbol("subtree")]),
            callback,
        ],
    )
    .expect("add recursive Windows rename watch");

    std::fs::rename(&from, &to).expect("rename nested watched child");
    let events = service_until(&mut eval, "neovm-test-w32-rename-events", |events| {
        events
            .iter()
            .filter(|fields| {
                matches!(
                    fields.get(1).and_then(|value| value.as_symbol_name()),
                    Some("renamed-from" | "renamed-to")
                )
            })
            .count()
            >= 2
    });
    let pair: Vec<_> = events
        .iter()
        .filter(|fields| {
            matches!(
                fields.get(1).and_then(|value| value.as_symbol_name()),
                Some("renamed-from" | "renamed-to")
            )
        })
        .collect();
    assert_eq!(pair[0][0], descriptor);
    assert_eq!(pair[1][0], descriptor);
    assert_eq!(pair[0][1], Value::symbol("renamed-from"));
    assert_eq!(pair[1][1], Value::symbol("renamed-to"));
    assert_eq!(
        PathBuf::from(
            pair[0][2]
                .as_lisp_string()
                .and_then(|name| name.as_utf8_str())
                .expect("old Windows path is UTF-8")
        ),
        PathBuf::from("nested").join("rename-from.txt")
    );
    assert_eq!(
        PathBuf::from(
            pair[1][2]
                .as_lisp_string()
                .and_then(|name| name.as_utf8_str())
                .expect("new Windows path is UTF-8")
        ),
        PathBuf::from("nested").join("rename-to.txt")
    );
    w32notify_rm_watch(vec![descriptor]).expect("remove Windows rename watch");
    reset_file_notify_thread_locals();
}

/// GNU's w32notify worker treats deletion of the watched directory as normal
/// descriptor invalidation: the pending native read ends, `w32notify-valid-p'
/// becomes nil, and no asynchronous `file-notify-error' escapes through the
/// command loop (src/w32notify.c:257-274, 686-700).
#[test]
#[cfg(target_os = "windows")]
fn windows_deleted_directory_invalidates_watch_without_a_lisp_error() {
    crate::test_utils::init_test_tracing();
    reset_file_notify_thread_locals();
    let root = workspace_temp_dir();
    let watched = root.path().join("watched");
    std::fs::create_dir(&watched).expect("create watched directory");

    let mut eval = crate::emacs_core::eval::Context::new();
    let callback = eval
        .eval_str("(lambda (_event))")
        .expect("create Windows watch callback");
    let descriptor = w32notify_add_watch(
        &mut eval,
        vec![
            Value::string(watched.display().to_string()),
            Value::list(vec![Value::symbol("file-name")]),
            callback,
        ],
    )
    .expect("add Windows directory watch");
    assert!(native_watch_is_valid(descriptor));

    std::fs::remove_dir(&watched).expect("delete watched directory");
    let read = eval.eval_str("(read-event nil nil 5)");
    assert!(
        read.is_ok(),
        "watch invalidation must not escape as a Lisp error: {read:?}"
    );
    assert!(
        !native_watch_is_valid(descriptor),
        "the descriptor remained valid after its directory was deleted"
    );
    reset_file_notify_thread_locals();
}
