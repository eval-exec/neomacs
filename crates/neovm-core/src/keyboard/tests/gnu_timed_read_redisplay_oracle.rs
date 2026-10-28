//! GNU keyboard.c:2693-2721 redisplays before the expired-deadline check
//! at :2336-2337. Normal --batch skips the initial frame (xdisp.c:17372-17387);
//! redisplay-skip-initial-frame=nil exposes the ordering in the GNU oracle.
//! Expectations are refreshed exclusively from GNU stdout with UPDATE_EXPECT=1.

const TIMED_READ_REDISPLAY_FORM: &str = r#"(progn
  (defvar neovm--timed-read-redisplay-calls 0)
  (defvar neovm--timed-read-timer-calls 0)
  (let ((redisplay-skip-initial-frame nil)
        (inhibit-redisplay nil)
        (timer-list nil)
        (timer-idle-list nil)
        (unread-command-events nil)
        (pre-redisplay-function
          (lambda (&rest _)
            (setq neovm--timed-read-redisplay-calls
              (1+ neovm--timed-read-redisplay-calls))))
        out)
    (let ((redisplay-skip-initial-frame t))
      (dolist (reader '(read-event read-char read-char-exclusive))
        (setq neovm--timed-read-redisplay-calls 0)
        (let ((result (funcall reader nil nil 0)))
          (push (list reader 'batch result
                      neovm--timed-read-redisplay-calls) out))))
    (dolist (inhibited '(nil t))
      (let ((inhibit-redisplay inhibited))
        (dolist (reader '(read-event read-char read-char-exclusive))
          (setq neovm--timed-read-redisplay-calls 0)
          (let ((result (funcall reader nil nil 0)))
            (push (list reader inhibited result
                        neovm--timed-read-redisplay-calls) out)))))
    (dolist (reader '(read-event read-char read-char-exclusive))
      (let ((unread-command-events nil)
            (pre-redisplay-function
              (lambda (&rest _)
                (setq unread-command-events '(120)))))
        (push (list reader 'queued (funcall reader nil nil 0)
                    unread-command-events) out)))
    (let ((timer-list nil)
          (timer-idle-list nil)
          (timer (timer-create)))
      (setq neovm--timed-read-timer-calls 0)
      (timer-set-function timer
        (lambda ()
          (setq neovm--timed-read-timer-calls
            (1+ neovm--timed-read-timer-calls))))
      (timer-set-time timer '(0 0 0 0))
      (timer-activate timer)
      (unwind-protect
          (let ((result (read-event nil nil 0)))
            (push (list 'due-timer result neovm--timed-read-timer-calls) out))
        (cancel-timer timer)))
    (let ((redisplay-skip-initial-frame t)
          (timer-list nil)
          (timer-idle-list nil)
          (timer (timer-create)))
      (setq neovm--timed-read-timer-calls 0
            neovm--timed-read-redisplay-calls 0)
      (timer-set-function timer
        (lambda ()
          (setq neovm--timed-read-timer-calls
            (1+ neovm--timed-read-timer-calls))))
      (timer-set-time timer '(0 0 0 0))
      (timer-activate timer)
      (unwind-protect
          (let ((result (read-event nil nil 0)))
            (push (list 'batch-due-timer result neovm--timed-read-timer-calls
                        neovm--timed-read-redisplay-calls) out))
        (cancel-timer timer)))
    (nreverse out)))"#;

#[test]
fn gnu_timed_read_zero_timeout_preliminary_redisplay_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let emacs = crate::test_utils::gnu_oracle().executable_path();
    let gnu_form = format!(
        r#"(progn
          (unless (string= emacs-version "31.1")
            (error "timed-read oracle requires GNU Emacs 31.1, got %s" emacs-version))
          (prin1 {TIMED_READ_REDISPLAY_FORM})
          (terpri))"#
    );
    let mut command = if let Some(sandbox) = std::env::var_os("NEOVM_LISP_SANDBOX") {
        let mut command = std::process::Command::new(sandbox);
        command.arg(&emacs);
        command
    } else {
        std::process::Command::new(emacs)
    };
    let output = command
        .args(["-Q", "--batch", "--eval", &gnu_form])
        .output()
        .expect("run explicitly configured GNU Emacs oracle");
    assert!(
        output.status.success(),
        "GNU timed-read oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual_gnu = String::from_utf8(output.stdout.clone()).expect("GNU oracle UTF-8 stdout");
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/keyboard/tests/gnu_timed_read_redisplay.expected");
        // The fixture can only receive GNU's result, never NeoVM's result.
        std::fs::write(&fixture, &output.stdout).expect("refresh fixture exclusively from GNU");
        std::fs::read_to_string(fixture).expect("read freshly written GNU fixture")
    } else {
        include_str!("gnu_timed_read_redisplay.expected").to_owned()
    };
    assert!(
        !expected.trim().is_empty(),
        "refresh the blank fixture from GNU Emacs 31.1 with UPDATE_EXPECT=1"
    );
    assert_eq!(
        actual_gnu.trim_end(),
        expected.trim_end(),
        "GNU oracle fixture"
    );

    let _policy = crate::emacs_core::eval::RedisplayHookPolicyGuard::legacy();
    let mut eval = crate::test_utils::runtime_startup_context();
    // The callback provides a frontend for NeoVM's non-batch redisplay path;
    // GNU exposes the same hooks above by permitting its initial batch frame.
    eval.redisplay_fn = Some(Box::new(|_| {}));
    let actual_neovm = eval
        .eval_str(TIMED_READ_REDISPLAY_FORM)
        .expect("evaluate the timed readers through their Lisp entry points");
    assert_eq!(
        crate::emacs_core::print_value(&actual_neovm),
        expected.trim_end(),
        "NeoVM timed-read redisplay ordering"
    );
}
