use super::*;

fn workspace_temp_dir() -> tempfile::TempDir {
    let parent = std::path::Path::new(env!("CARGO_WORKSPACE_DIR"))
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
    reset_file_notify_thread_locals();
    let directory = workspace_temp_dir();
    let file = directory.path().join("regular-file");
    std::fs::write(&file, "contents").expect("seed regular file");
    let mut eval = crate::test_utils::runtime_startup_context();

    let result = inotify_add_watch(
        &mut eval,
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
    reset_file_notify_thread_locals();
}

/// Linux appends `IN_IGNORED` when the kernel removes a watch after its inode
/// is deleted.  GNU consumes that terminal event and removes every logical
/// registration sharing the native descriptor; `inotify-valid-p` must not
/// remain true for a watch that can never produce another event.
#[test]
fn inotify_kernel_removal_invalidates_the_logical_watch() {
    reset_file_notify_thread_locals();
    let directory = workspace_temp_dir();
    let file = directory.path().join("removed-file");
    std::fs::write(&file, "contents").expect("seed watched file");
    let mut eval = crate::test_utils::runtime_startup_context();

    let descriptor = inotify_add_watch(
        &mut eval,
        vec![
            Value::string(file.display().to_string()),
            Value::symbol("all-events"),
            Value::symbol("ignore"),
        ],
    )
    .expect("add file watch");
    std::fs::remove_file(&file).expect("remove watched inode");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while inotify_valid_p(vec![descriptor])
        .expect("query watch")
        .is_truthy()
        && std::time::Instant::now() < deadline
    {
        eval.eval_str("(read-event nil nil 0.05)")
            .expect("service terminal inotify event");
    }

    assert!(
        inotify_valid_p(vec![descriptor])
            .expect("query terminal watch")
            .is_nil(),
        "a kernel-removed inotify descriptor must not stay logically valid"
    );
    reset_file_notify_thread_locals();
}

/// GNU forwards `dont-follow` as `IN_DONT_FOLLOW`; it watches the symlink
/// inode, not its target.  A target write therefore must not satisfy a
/// `modify` watch registered on the link itself.
#[test]
fn inotify_dont_follow_does_not_observe_target_writes() {
    use std::os::unix::fs::symlink;

    reset_file_notify_thread_locals();
    let directory = workspace_temp_dir();
    let target = directory.path().join("target");
    let link = directory.path().join("link");
    std::fs::write(&target, "before").expect("seed symlink target");
    symlink(&target, &link).expect("create symlink");
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str("(setq neovm-test-dont-follow-event nil)")
        .expect("initialize event sink");
    let callback = eval
        .eval_str("(lambda (event) (setq neovm-test-dont-follow-event event))")
        .expect("create callback");

    let descriptor = inotify_add_watch(
        &mut eval,
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

    inotify_rm_watch(vec![descriptor]).expect("remove symlink watch");
    reset_file_notify_thread_locals();
}
