//! Re-homed from eval's file_notify/tests/linux_test.rs (P1 F1).
//!
//! Driver calls are re-pointed to the GNU-visible Lisp subrs
//! (`inotify-add-watch`, `inotify-valid-p`, `inotify-rm-watch`, registered by
//! file_notify/subrs.rs) through `Context::funcall_general` with
//! already-evaluated Value arguments (GNU Ffuncall semantics), per peer
//! 12:57Z (c) and the 14:4xZ eval_form hold: the test must not synthesize a
//! source form, because that would evaluate the aspect list and callback
//! symbol as code.  The thread-local reset is the eval authority fence.
//! Test names and assertions are unchanged.
use crate::test_utils::runtime_startup_context;
use crate::test_utils::workspace_root;
use neovm_eval::test_support::authority::reset_file_notify_thread_locals_for_test;
use neovm_value::value::Value;

fn workspace_temp_dir() -> tempfile::TempDir {
    let parent = workspace_root()
        .as_path()
        .join("target")
        .join("neovm-core-file-notify-tests");
    std::fs::create_dir_all(&parent).expect("create workspace test directory");
    tempfile::Builder::new()
        .prefix("inotify-")
        .tempdir_in(parent)
        .expect("create file notification fixture")
}

/// GNU passes `IN_ONLYDIR` to the kernel, which rejects a regular file with
/// ENOTDIR.  Accepting the flag but ignoring it is observably incompatible and
/// can make callers believe a directory-only invariant is enforced when it is
/// not.
#[test]
fn inotify_onlydir_rejects_a_regular_file() {
    reset_file_notify_thread_locals_for_test();
    let directory = workspace_temp_dir();
    let file = directory.path().join("regular-file");
    std::fs::write(&file, "contents").expect("seed regular file");
    let mut eval = runtime_startup_context();

    let result = eval.funcall_general(
        Value::symbol("inotify-add-watch"),
        vec![
            Value::string(file.display().to_string()),
            Value::list(vec![Value::symbol("onlydir")]),
            Value::symbol("ignore"),
        ],
    );

    assert!(
        result.is_err(),
        "GNU inotify-add-watch rejects a non-directory when onlydir is requested"
    );
    // The hold at peer 14:4xZ: the call must reach the real subr, not fail
    // because the aspect list was evaluated as code.  A void-function error
    // would satisfy the plain is_err assertion vacuously.
    match &result {
        Err(flow) => match flow.kind() {
            neovm_value::error::FlowRef::Signal(sig) => assert_eq!(
                sig.symbol_name(),
                "file-notify-error",
                "the onlydir rejection must come from the inotify subr"
            ),
            _ => panic!("expected a signal, got another flow"),
        },
        Ok(_) => panic!("expected an error"),
    }
    reset_file_notify_thread_locals_for_test();
}

/// Linux appends `IN_IGNORED` when the kernel removes a watch after its inode
/// is deleted.  GNU consumes that terminal event and removes every logical
/// registration sharing the native descriptor; `inotify-valid-p` must not
/// remain true for a watch that can never produce another event.
#[test]
fn inotify_kernel_removal_invalidates_the_logical_watch() {
    reset_file_notify_thread_locals_for_test();
    let directory = workspace_temp_dir();
    let file = directory.path().join("removed-file");
    std::fs::write(&file, "contents").expect("seed watched file");
    let mut eval = runtime_startup_context();

    let descriptor = eval
        .funcall_general(
            Value::symbol("inotify-add-watch"),
            vec![
                Value::string(file.display().to_string()),
                Value::symbol("all-events"),
                Value::symbol("ignore"),
            ],
        )
        .expect("add file watch");
    std::fs::remove_file(&file).expect("remove watched inode");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while eval
        .funcall_general(Value::symbol("inotify-valid-p"), vec![descriptor])
        .expect("query watch")
        .is_truthy()
        && std::time::Instant::now() < deadline
    {
        eval.eval_str("(read-event nil nil 0.05)")
            .expect("service terminal inotify event");
    }

    assert!(
        eval.funcall_general(Value::symbol("inotify-valid-p"), vec![descriptor])
            .expect("query terminal watch")
            .is_nil(),
        "a kernel-removed inotify descriptor must not stay logically valid"
    );
    reset_file_notify_thread_locals_for_test();
}

/// GNU forwards `dont-follow` as `IN_DONT_FOLLOW`; it watches the symlink
/// inode, not its target.  A target write therefore must not satisfy a
/// `modify` watch registered on the link itself.
#[test]
fn inotify_dont_follow_does_not_observe_target_writes() {
    use std::os::unix::fs::symlink;

    reset_file_notify_thread_locals_for_test();
    let directory = workspace_temp_dir();
    let target = directory.path().join("target");
    let link = directory.path().join("link");
    std::fs::write(&target, "before").expect("seed symlink target");
    symlink(&target, &link).expect("create symlink");
    let mut eval = runtime_startup_context();
    eval.eval_str("(setq neovm-test-dont-follow-event nil)")
        .expect("initialize event sink");
    let callback = eval
        .eval_str("(lambda (event) (setq neovm-test-dont-follow-event event))")
        .expect("create callback");

    let descriptor = eval
        .funcall_general(
            Value::symbol("inotify-add-watch"),
            vec![
                Value::string(link.display().to_string()),
                Value::list(vec![Value::symbol("modify"), Value::symbol("dont-follow")]),
                callback,
            ],
        )
        .expect("watch symlink itself");

    std::fs::write(&target, "after").expect("modify symlink target");
    eval.eval_str("(read-event nil nil 0.1)")
        .expect("service possible event");
    assert!(
        eval.eval_str("neovm-test-dont-follow-event")
            .expect("read event sink")
            .is_nil(),
        "a dont-follow watch must not observe modifications of the symlink target"
    );

    eval.funcall_general(Value::symbol("inotify-rm-watch"), vec![descriptor])
        .expect("remove symlink watch");
    reset_file_notify_thread_locals_for_test();
}
