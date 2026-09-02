use super::*;

#[cfg(unix)]
#[test]
fn first_text_change_locks_a_clean_file_visiting_buffer_like_gnu() {
    crate::test_utils::init_test_tracing();
    let root = std::env::current_dir()
        .expect("workspace directory")
        .join("tmp/neovm-core-test-artifacts")
        .join(format!("first-change-lock-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create workspace-local fixture directory");
    let visited = root.join("visited.txt");
    let lock = root.join(".#visited.txt");
    fs::write(&visited, b"before\n").expect("write visited file");
    let visited_value = Value::string(visited.to_string_lossy());

    let mut eval = super::super::eval::Context::new();
    eval.set_variable("create-lockfiles", Value::T);
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_file_name(current, visited_value)
        .expect("set buffer-file-name");
    eval.buffers
        .set_buffer_file_truename(current, visited_value)
        .expect("set buffer-file-truename");

    super::super::editfns::insert_lisp_string_with_change_hooks_in_buffer(
        &mut eval,
        current,
        &LispString::from_utf8("changed"),
    )
    .expect("modify visiting buffer");

    assert!(
        fs::symlink_metadata(&lock).is_ok(),
        "GNU locks a clean file-visiting buffer before its first text change"
    );

    let _ = fs::remove_file(&lock);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn lock_and_unlock_file_dispatch_matching_file_name_handlers_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();

    let result = eval.eval_str(
        r#"(progn
             (setq neovm-file-lock-handler-calls nil)
             (setq file-name-handler-alist
                   (list
                    (cons "\\`/remote:"
                          (lambda (operation &rest arguments)
                            (setq neovm-file-lock-handler-calls
                                  (cons (cons operation arguments)
                                        neovm-file-lock-handler-calls))
                            (if (eq operation 'file-locked-p)
                                :remote-owner
                              :handled)))))
             (list (lock-file "/remote:host:/work/note.txt")
                   (file-locked-p "/remote:host:/work/note.txt")
                   (unlock-file "/remote:host:/work/note.txt")
                   (reverse neovm-file-lock-handler-calls)))"#,
    );

    assert_eq!(
        crate::emacs_core::format_eval_result(&result),
        "OK (:handled :remote-owner nil ((lock-file \"/remote:host:/work/note.txt\") (file-locked-p \"/remote:host:/work/note.txt\") (unlock-file \"/remote:host:/work/note.txt\")))"
    );
}

#[test]
fn new_lock_info_contains_gnu_boot_time_suffix_when_available() {
    let lock_info = current_lock_info_string("me", "testhost");
    let parsed = parse_lock_info(&lock_info).expect("parse current lock info");
    assert_eq!(parsed.user, "me");
    assert_eq!(parsed.host, "testhost");
    assert_eq!(parsed.pid, std::process::id());
    assert_eq!(parsed.boot_time, system_boot_time_sec());
}

/// Read a Lisp string expression from a context, "" when absent.
#[cfg(unix)]
fn lisp_string(eval: &mut super::super::eval::Context, expr: &str) -> String {
    eval.eval_str(expr)
        .ok()
        .and_then(|v| v.as_utf8_str())
        .unwrap_or_default()
        .to_string()
}

#[test]
fn zero_is_never_a_valid_lock_owner_pid() {
    assert!(!process_is_alive(0));
}

#[cfg(windows)]
#[test]
fn windows_process_probe_recognizes_current_process() {
    assert!(process_is_alive(std::process::id()));
}

#[cfg(unix)]
#[test]
fn current_lock_owner_recognizes_dangling_symlink_lockfiles() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = dir.path().join(".#probe");
    std::os::unix::fs::symlink(current_lock_info_string("me", "testhost"), &lock_path)
        .expect("create lock symlink");

    assert!(matches!(
        current_lock_owner(&lock_path, "testhost").expect("read lock owner"),
        LockOwner::Current
    ));
}

#[cfg(unix)]
#[test]
fn dead_pid_lock_on_this_host_is_zapped_and_reported_free() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = dir.path().join(".#stale");
    // A pid from a crashed session: pid 1 is init (alive, but use an
    // impossible one). Recycle-proof choice: our own pid is alive, so use
    // a pid that cannot exist (> pid_max default of 4194304).
    let contents = "someone@testhost.999999999";
    std::os::unix::fs::symlink(contents, &lock_path).expect("symlink lock");
    match current_lock_owner(&lock_path, "testhost").expect("owner check") {
        LockOwner::None => {}
        LockOwner::Current => panic!("stale lock cannot be ours"),
        LockOwner::Other(clasher) => panic!("stale lock must be zapped, got owner {clasher:?}"),
    }
    assert!(
        std::fs::symlink_metadata(&lock_path).is_err(),
        "GNU unlinks the stale lockfile in current_lock_owner"
    );
}

#[cfg(unix)]
#[test]
fn live_pid_lock_on_this_host_names_the_other_owner() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = dir.path().join(".#live");
    // pid 1 is always alive; kill(1, 0) fails with EPERM for non-root,
    // which GNU treats as alive.
    let contents = "someone@testhost.1";
    std::os::unix::fs::symlink(contents, &lock_path).expect("symlink lock");
    match current_lock_owner(&lock_path, "testhost").expect("owner check") {
        LockOwner::Other(clasher) => {
            assert_eq!(clasher.user, "someone");
            assert_eq!(clasher.pid, 1);
            assert_eq!(clasher.opponent(), "someone@testhost (pid 1)");
        }
        _ => panic!("live-pid lock must report the other owner"),
    }
    assert!(
        std::fs::symlink_metadata(&lock_path).is_ok(),
        "live locks are never zapped"
    );
}

/// Spawn a definitely-live process this user owns, so liveness probes never
/// depend on pid-1 EPERM subtleties.
#[cfg(unix)]
fn spawn_live_owner() -> std::process::Child {
    std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep child")
}

#[cfg(unix)]
fn visit_file_in_current_buffer(eval: &mut super::super::eval::Context, visited: &Path) {
    let visited_value = Value::string(visited.to_string_lossy());
    eval.set_variable("create-lockfiles", Value::T);
    let current = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .set_buffer_file_name(current, visited_value)
        .expect("set buffer-file-name");
    eval.buffers
        .set_buffer_file_truename(current, visited_value)
        .expect("set buffer-file-truename");
}

/// GNU lock_file (src/filelock.c) calls ask-user-about-lock when another
/// process owns the lock, and any signal it raises — the batch-mode
/// file-locked signal from userlock.el in particular — propagates and
/// aborts the modification.
#[cfg(unix)]
#[test]
fn modifying_externally_locked_file_propagates_file_locked_and_leaves_buffer_untouched() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");
    let lock_path = dir.path().join(".#note.txt");
    let mut owner = spawn_live_owner();

    let mut eval = super::super::eval::Context::new();
    visit_file_in_current_buffer(&mut eval, &visited);
    // Load the visited contents like find-file would, then mark the buffer
    // clean so the contested lock governs the NEXT (first) modification.
    eval.eval_str(r#"(progn (insert "hello\n") (set-buffer-modified-p nil))"#)
        .expect("seed visited buffer contents");
    let host = lisp_string(&mut eval, "(system-name)");
    let contents = format!("someone@{}.{}", host, owner.id());
    std::os::unix::fs::symlink(&contents, &lock_path).expect("symlink lock");
    eval.eval_str(
        r#"(fset 'ask-user-about-lock
               (lambda (file opponent)
                 (signal 'file-locked
                         (list file opponent "Cannot resolve lock conflict in batch mode"))))"#,
    )
    .expect("define batch ask-user-about-lock");

    let result = eval.eval_str(r#"(insert "EDIT")"#);
    let formatted = crate::emacs_core::format_eval_result(&result);
    assert_eq!(
        formatted,
        format!(
            "ERR (file-locked (\"{}\" \"someone@{} (pid {})\" \"Cannot resolve lock conflict in batch mode\"))",
            visited.display(),
            host,
            owner.id(),
        ),
        "GNU propagates the ask-user-about-lock signal and refuses the edit"
    );

    let buffer_after = eval.eval_str("(buffer-string)");
    assert_eq!(
        crate::emacs_core::format_eval_result(&buffer_after),
        "OK \"hello\n\"",
        "a refused edit must not modify the buffer"
    );
    assert_eq!(
        fs::read_link(&lock_path)
            .expect("lock survives")
            .to_string_lossy(),
        contents,
        "a refused edit must not steal the other process's lock"
    );

    let _ = owner.kill();
    let _ = owner.wait();
}

/// GNU lock_file rewrites the clasher info USER@HOST.PID:BOOT into
/// "USER@HOST (pid PID)" before handing it to ask-user-about-lock.
#[cfg(unix)]
#[test]
fn ask_user_about_lock_steal_and_proceed_answers_match_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");
    let lock_path = dir.path().join(".#note.txt");
    let mut owner = spawn_live_owner();

    // Answer nil: proceed without taking the lock.
    let mut eval = super::super::eval::Context::new();
    let host = lisp_string(&mut eval, "(system-name)");
    let contents = format!(
        "someone@{}.{}:{}",
        host,
        owner.id(),
        system_boot_time_sec().max(1),
    );
    std::os::unix::fs::symlink(&contents, &lock_path).expect("symlink lock");
    visit_file_in_current_buffer(&mut eval, &visited);
    eval.eval_str(
        r#"(progn
             (setq neovm-lock-args nil)
             (fset 'ask-user-about-lock
                   (lambda (file opponent)
                     (setq neovm-lock-args (list file opponent))
                     nil)))"#,
    )
    .expect("define recording ask-user-about-lock");
    eval.eval_str(r#"(insert "EDIT")"#)
        .expect("proceed answer edits anyway");
    assert_eq!(
        crate::emacs_core::format_eval_result(&eval.eval_str("neovm-lock-args")),
        format!(
            "OK (\"{}\" \"someone@{} (pid {})\")",
            visited.display(),
            host,
            owner.id(),
        ),
        "opponent string must be USER@HOST (pid PID) with the boot time stripped"
    );
    assert_eq!(
        fs::read_link(&lock_path)
            .expect("lock survives")
            .to_string_lossy(),
        contents,
        "answer nil edits the file but leaves the other lock in place"
    );

    // Answer t: steal the lock, then edit.
    let mut eval = super::super::eval::Context::new();
    let our_lock_info = current_lock_info_string(
        &lisp_string(&mut eval, "(user-login-name)"),
        &lisp_string(&mut eval, "(system-name)"),
    );
    visit_file_in_current_buffer(&mut eval, &visited);
    eval.eval_str(r#"(fset 'ask-user-about-lock (lambda (file opponent) t))"#)
        .expect("define stealing ask-user-about-lock");
    eval.eval_str(r#"(insert "EDIT")"#)
        .expect("steal answer edits");
    assert_eq!(
        fs::read_link(&lock_path)
            .expect("stolen lock")
            .to_string_lossy(),
        our_lock_info,
        "answer t forces the lock over to us"
    );

    let _ = owner.kill();
    let _ = owner.wait();
}

/// GNU current_lock_owner returns EINVAL for unparseable lock contents;
/// lock_file deliberately ignores that errno (no prompt, edit proceeds),
/// while file-locked-p reports it as a file-error.
#[cfg(unix)]
#[test]
fn unparseable_lock_contents_are_an_error_not_another_owner() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");
    let lock_path = dir.path().join(".#note.txt");
    std::os::unix::fs::symlink("complete garbage", &lock_path).expect("symlink lock");

    let mut eval = super::super::eval::Context::new();
    visit_file_in_current_buffer(&mut eval, &visited);
    eval.eval_str(
        r#"(fset 'ask-user-about-lock
               (lambda (file opponent)
                 (signal 'file-locked (list file opponent))))"#,
    )
    .expect("define signalling ask-user-about-lock");

    eval.eval_str(r#"(insert "EDIT")"#)
        .expect("GNU ignores the EINVAL from lock_if_free and never prompts");

    let locked_p = eval.eval_str(&format!("(file-locked-p \"{}\")", visited.display()));
    assert!(
        crate::emacs_core::format_eval_result(&locked_p).starts_with("ERR (file-error"),
        "GNU file-locked-p reports EINVAL via report_file_errno, got {}",
        crate::emacs_core::format_eval_result(&locked_p),
    );
}

/// GNU zaps an empty lock file (buggy-filesystem leftover) and reports the
/// file free.
#[cfg(unix)]
#[test]
fn empty_lock_file_is_zapped_and_reported_free() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = dir.path().join(".#empty");
    fs::write(&lock_path, b"").expect("write empty lock");
    match current_lock_owner(&lock_path, "testhost").expect("owner check") {
        LockOwner::None => {}
        _ => panic!("empty lock file must be zapped and reported free"),
    }
    assert!(
        fs::symlink_metadata(&lock_path).is_err(),
        "GNU unlinks the empty lock file"
    );
}

/// GNU lock_file_1 and current_lock_owner take the host from Lisp
/// `(system-name)` and the user from `(user-login-name)`, not from the OS:
/// a rebound system-name (the oracle sandbox, --no-build-details) must make
/// a matching-host lock with a dead pid read as STALE, not as another host's
/// lock that can never be verified.
#[cfg(unix)]
#[test]
fn lock_host_comes_from_lisp_system_name_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");
    let lock_path = dir.path().join(".#note.txt");
    // Dead pid (over default pid_max) on the FAKED host.
    std::os::unix::fs::symlink("someone@faked-host.999999999", &lock_path).expect("symlink lock");

    let mut eval = super::super::eval::Context::new();
    eval.eval_str(r#"(setq system-name "faked-host")"#)
        .expect("rebind system-name");
    let locked_p = eval.eval_str(&format!("(file-locked-p \"{}\")", visited.display()));
    assert_eq!(
        crate::emacs_core::format_eval_result(&locked_p),
        "OK nil",
        "a dead-pid lock on the (system-name) host is stale, like GNU"
    );
    assert!(
        fs::symlink_metadata(&lock_path).is_err(),
        "the stale lock must be zapped"
    );

    // And the lock we CREATE must carry the Lisp system-name.
    visit_file_in_current_buffer(&mut eval, &visited);
    eval.eval_str(r#"(insert "EDIT")"#)
        .expect("edit locks the file");
    let contents = fs::read_link(&lock_path)
        .expect("our lock")
        .to_string_lossy()
        .into_owned();
    assert!(
        contents.contains("@faked-host."),
        "created lock must use (system-name), got {contents}"
    );
}

/// GNU Ffile_locked_p returns only the USER part for another owner.
#[cfg(unix)]
#[test]
fn file_locked_p_names_only_the_user_for_another_owner() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");
    let lock_path = dir.path().join(".#note.txt");
    let mut owner = spawn_live_owner();

    let mut eval = super::super::eval::Context::new();
    let host = lisp_string(&mut eval, "(system-name)");
    let contents = format!("someone@{}.{}", host, owner.id());
    std::os::unix::fs::symlink(&contents, &lock_path).expect("symlink lock");
    let locked_p = eval.eval_str(&format!("(file-locked-p \"{}\")", visited.display()));
    assert_eq!(
        crate::emacs_core::format_eval_result(&locked_p),
        "OK \"someone\"",
    );

    let _ = owner.kill();
    let _ = owner.wait();
}

#[cfg(unix)]
#[test]
fn stale_boot_time_zaps_even_a_live_pid() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = dir.path().join(".#reboot");
    // pid 1 alive, but a boot time of 12 (1970) cannot match this boot.
    let contents = "someone@testhost.1:12";
    std::os::unix::fs::symlink(contents, &lock_path).expect("symlink lock");
    if system_boot_time_sec() == 0 {
        return; // GNU also omits the comparison when boot time is unavailable.
    }
    match current_lock_owner(&lock_path, "testhost").expect("owner check") {
        LockOwner::None => {}
        _ => panic!("previous-boot lock must be stale"),
    }
}

/// Seed the current buffer as a clean visitor of VISITED whose recorded
/// modtime is older than the file on disk, with a batch-mode
/// userlock--ask-user-about-supersession-threat that signals like
/// userlock.el's does when it cannot prompt.
#[cfg(unix)]
fn seed_superseded_visiting_buffer(eval: &mut super::super::eval::Context, visited: &Path) {
    visit_file_in_current_buffer(eval, visited);
    eval.eval_str(
        r#"(progn
             (insert "hello\n")
             (set-buffer-modified-p nil)
             (set-visited-file-modtime '(0 0))
             ;; userlock.el's (define-error 'file-supersession nil 'file-error).
             (put 'file-supersession 'error-conditions
                  '(file-supersession file-error error))
             (put 'file-supersession 'error-message "File supersession")
             (fset 'userlock--ask-user-about-supersession-threat
                   (lambda (file)
                     (signal 'file-supersession
                             (list "File changed on disk" file)))))"#,
    )
    .expect("seed a superseded visiting buffer");
}

/// GNU lock_file (src/filelock.c:601-608) calls
/// userlock--ask-user-about-supersession-threat with calln, so the
/// file-supersession signal it raises propagates out of Flock_file and
/// aborts the modification that asked for the lock.
#[cfg(unix)]
#[test]
fn supersession_threat_signal_propagates_out_of_lock_file_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");

    let mut eval = super::super::eval::Context::new();
    seed_superseded_visiting_buffer(&mut eval, &visited);

    let result = eval.eval_str(&format!("(lock-file \"{}\")", visited.display()));
    assert_eq!(
        crate::emacs_core::format_eval_result(&result),
        format!(
            "ERR (file-supersession (\"File changed on disk\" \"{}\"))",
            visited.display()
        ),
        "GNU lock_file uses calln, so the supersession signal propagates"
    );
}

/// The same threat must abort the buffer modification that triggered the
/// lock, leaving the buffer text untouched — GNU insdel.c:2174 calls
/// Flock_file from prepare_to_modify_buffer_1, before any text is inserted.
#[cfg(unix)]
#[test]
fn supersession_threat_aborts_the_first_text_change_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");

    let mut eval = super::super::eval::Context::new();
    seed_superseded_visiting_buffer(&mut eval, &visited);

    let result = eval.eval_str(r#"(insert "EDIT")"#);
    assert_eq!(
        crate::emacs_core::format_eval_result(&result),
        format!(
            "ERR (file-supersession (\"File changed on disk\" \"{}\"))",
            visited.display()
        ),
    );
    assert_eq!(
        crate::emacs_core::format_eval_result(&eval.eval_str("(buffer-string)")),
        "OK \"hello\n\"",
        "a refused edit must not modify the buffer"
    );
}

/// GNU computes the lock file name only when create-lockfiles is non-nil
/// (filelock.c:593-599) but runs the supersession check unconditionally
/// (filelock.c:601-608) — the lock-file docstring says so explicitly.
#[cfg(unix)]
#[test]
fn supersession_threat_is_checked_even_when_create_lockfiles_is_nil_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");

    let mut eval = super::super::eval::Context::new();
    seed_superseded_visiting_buffer(&mut eval, &visited);
    eval.eval_str("(setq create-lockfiles nil)")
        .expect("opt out of lock files");

    let result = eval.eval_str(&format!("(lock-file \"{}\")", visited.display()));
    assert_eq!(
        crate::emacs_core::format_eval_result(&result),
        format!(
            "ERR (file-supersession (\"File changed on disk\" \"{}\"))",
            visited.display()
        ),
        "create-lockfiles nil suppresses the lock file, never the threat check"
    );
    assert!(
        fs::symlink_metadata(dir.path().join(".#note.txt")).is_err(),
        "create-lockfiles nil must still suppress the lock file itself"
    );
}

/// GNU skips the threat entirely when this Emacs already owns the lock
/// (filelock.c:607) — the file on disk was changed by us.
#[cfg(unix)]
#[test]
fn supersession_threat_is_skipped_when_we_own_the_lock_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");

    let mut eval = super::super::eval::Context::new();
    seed_superseded_visiting_buffer(&mut eval, &visited);
    let host = lisp_string(&mut eval, "(system-name)");
    std::os::unix::fs::symlink(
        current_lock_info_string("me", &host),
        dir.path().join(".#note.txt"),
    )
    .expect("symlink our own lock");

    assert_eq!(
        crate::emacs_core::format_eval_result(
            &eval.eval_str(&format!("(lock-file \"{}\")", visited.display()))
        ),
        "OK nil",
        "GNU never raises the threat for a file this Emacs already locked"
    );
}

/// GNU calls make-lock-file-name with calln (filelock.c:558), so an error
/// from it propagates out of Flock_file.  Swallowing it is worse than the
/// lost signal alone: we then invented a ".#NAME" lock the Lisp layer had
/// just declined to produce.
#[cfg(unix)]
#[test]
fn make_lock_file_name_errors_propagate_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");

    let mut eval = super::super::eval::Context::new();
    eval.set_variable("create-lockfiles", Value::T);
    eval.eval_str(
        r#"(fset 'make-lock-file-name
                 (lambda (_f) (signal 'error (list "make-lock-file-name exploded"))))"#,
    )
    .expect("install an exploding make-lock-file-name");

    assert_eq!(
        crate::emacs_core::format_eval_result(
            &eval.eval_str(&format!("(lock-file \"{}\")", visited.display()))
        ),
        "ERR (error (\"make-lock-file-name exploded\"))",
    );
    assert!(
        fs::symlink_metadata(dir.path().join(".#note.txt")).is_err(),
        "a refused lock file name must not be second-guessed with a fallback"
    );
}

/// GNU expands FN in exactly one place: `make_lock_file_name`
/// (filelock.c:543, `fn = Fexpand_file_name (fn, Qnil)`).  Every other
/// consumer — Fget_truename_buffer (:603), Fverify_visited_file_modtime
/// (:605), Ffile_exists_p (:606), the supersession calln (:608) — receives
/// the caller's string verbatim.  Expanding earlier breaks the truename
/// lookup, because find-file stores `buffer-file-truename` abbreviated
/// ("~/..."), so the expanded name never matches its own buffer.
#[cfg(unix)]
#[test]
fn only_the_lock_file_name_expands_the_filename_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");

    let mut eval = super::super::eval::Context::new();
    eval.set_variable("create-lockfiles", Value::T);
    eval.eval_str(&format!(
        r#"(progn
             (setq default-directory "{}/")
             (setq neovm-lock-args nil)
             (fset 'make-lock-file-name
                   (lambda (f)
                     (setq neovm-lock-args
                           (cons (cons 'make-lock-file-name f) neovm-lock-args))
                     (concat "{}/.#note.txt")))
             (fset 'get-truename-buffer
                   (lambda (f)
                     (setq neovm-lock-args
                           (cons (cons 'get-truename-buffer f) neovm-lock-args))
                     nil)))"#,
        dir.path().display(),
        dir.path().display(),
    ))
    .expect("install recording stubs");

    assert_eq!(
        crate::emacs_core::format_eval_result(&eval.eval_str(r#"(lock-file "note.txt")"#)),
        "OK nil",
    );
    assert_eq!(
        crate::emacs_core::format_eval_result(&eval.eval_str("(reverse neovm-lock-args)")),
        format!(
            "OK ((make-lock-file-name . \"{}/note.txt\") (get-truename-buffer . \"note.txt\"))",
            dir.path().display()
        ),
        "only make-lock-file-name sees an expanded name"
    );

    let _ = fs::remove_file(dir.path().join(".#note.txt"));
}

/// GNU reports lock failures through report_file_errno, whose DATA is
/// (ACTION STRERROR FILENAME) — fileio.c get_file_errno_data, filelock.c:648.
/// The bare strerror text carries no Rust "(os error N)" suffix.
#[cfg(unix)]
#[test]
fn lock_error_data_matches_gnu_report_file_errno_shape() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");
    fs::write(dir.path().join(".#note.txt"), b"garbage-not-a-lock").expect("write bogus lock");

    let mut eval = super::super::eval::Context::new();
    let result = eval.eval_str(&format!("(file-locked-p \"{}\")", visited.display()));
    assert_eq!(
        crate::emacs_core::format_eval_result(&result),
        format!(
            "ERR (file-error (\"Testing file lock\" \"Invalid argument\" \"{}\"))",
            visited.display()
        ),
    );
}

/// GNU Funlock_file (filelock.c:717-720) wraps unlock_file in
/// internal_condition_case_1 for file-error and routes the error to
/// userlock--handle-unlock-error, returning nil: unlock-file never signals.
#[cfg(unix)]
#[test]
fn unlock_file_routes_lock_errors_to_the_userlock_handler_like_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let visited = dir.path().join("note.txt");
    fs::write(&visited, b"hello\n").expect("write visited file");
    fs::write(dir.path().join(".#note.txt"), b"garbage-not-a-lock").expect("write bogus lock");

    let mut eval = super::super::eval::Context::new();
    eval.eval_str(
        r#"(progn
             (setq neovm-unlock-errors nil)
             (fset 'userlock--handle-unlock-error
                   (lambda (err)
                     (setq neovm-unlock-errors (cons err neovm-unlock-errors)))))"#,
    )
    .expect("define the unlock error handler");

    assert_eq!(
        crate::emacs_core::format_eval_result(
            &eval.eval_str(&format!("(unlock-file \"{}\")", visited.display()))
        ),
        "OK nil",
        "GNU unlock-file swallows file-errors and returns nil"
    );
    assert_eq!(
        crate::emacs_core::format_eval_result(&eval.eval_str("neovm-unlock-errors")),
        format!(
            "OK ((file-error \"Unlocking file\" \"Invalid argument\" \"{}\"))",
            visited.display()
        ),
        "the error is handed to userlock--handle-unlock-error"
    );
}
