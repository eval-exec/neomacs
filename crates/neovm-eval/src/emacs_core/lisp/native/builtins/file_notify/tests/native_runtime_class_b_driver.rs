//! Host-OS tests for the complete native worker-to-Lisp delivery path.

use super::*;
use std::path::Path;

fn workspace_temp_dir() -> tempfile::TempDir {
    let parent = crate::test_utils::workspace_root()
        .as_path()
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
    let mut events: Vec<_> = neovm_value::value::list_to_vec(&events)
        .expect("callback events list")
        .iter()
        .map(|event| neovm_value::value::list_to_vec(event).expect("native event list"))
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
            let fields = neovm_value::value::list_to_vec(&special_event)
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

#[cfg(target_os = "linux")]
pub(crate) fn run_inotify_rename_delivers_an_ordered_cookie_pair_fixture(
    make_context: fn() -> crate::emacs_core::eval::Context,
) {
    crate::test_utils::init_test_tracing();
    reset_file_notify_thread_locals();
    let directory = workspace_temp_dir();
    let from = directory.path().join("rename-from.txt");
    let to = directory.path().join("rename-to.txt");
    std::fs::write(&from, "contents").expect("seed renamed file");

    let mut eval = make_context();
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
        neovm_value::value::list_to_vec(&pair[0][1]),
        Some(vec![Value::symbol("moved-from")])
    );
    assert_eq!(
        neovm_value::value::list_to_vec(&pair[1][1]),
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

pub(crate) fn run_native_callback_can_remove_its_own_watch_fixture(
    make_context: fn() -> crate::emacs_core::eval::Context,
) {
    crate::test_utils::init_test_tracing();
    reset_file_notify_thread_locals();
    let directory = workspace_temp_dir();
    let mut eval = make_context();
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
