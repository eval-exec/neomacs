//! GNU keyboard.c:2693-2721 runs ripe timers before preliminary redisplay.
//! Timer-created unread events remain visible to that redisplay, before the
//! later unread-event check (:2992-3010). Batch/inhibit guards only skip display.

const READ_TIMER_ORDER_FORM: &str = r#"(progn
  (defvar neovm--read-timer-log nil)
  (defvar neovm--read-timer-display nil)
  (let (out)
    (dolist (reader '(read-event read-char read-char-exclusive))
      (dolist (mode '(display batch inhibited))
        (dolist (queue-input '(nil t))
          (let ((redisplay-skip-initial-frame (eq mode 'batch))
                (inhibit-redisplay (eq mode 'inhibited))
                (timer-list nil)
                (timer-idle-list nil)
                (unread-command-events nil)
                (neovm--read-timer-log nil)
                (neovm--read-timer-display 'original)
                (pre-redisplay-function
                  (lambda (&rest _)
                    (push (list 'redisplay neovm--read-timer-display
                                (copy-sequence unread-command-events))
                          neovm--read-timer-log)))
                (timer (timer-create)))
            (timer-set-function timer
              (lambda ()
                (push 'timer neovm--read-timer-log)
                (setq neovm--read-timer-display 'changed)
                (when queue-input (setq unread-command-events '(120)))))
            (timer-set-time timer '(0 0 0 0))
            (timer-activate timer)
            (unwind-protect
                (let ((result (funcall reader nil nil 0)))
                  (push (list reader mode queue-input result
                              (nreverse neovm--read-timer-log)
                              unread-command-events) out))
              (cancel-timer timer))))))
    (nreverse out)))"#;

const READ_TIMER_UNREAD_FORM: &str = r#"(progn
  (defvar neovm--read-timer-log nil)
  (let ((redisplay-skip-initial-frame nil)
        (inhibit-redisplay nil)
        out)
    (dolist (reader '(read-event read-char read-char-exclusive))
      (let ((timer-list nil)
            (timer-idle-list nil)
            (unread-command-events '(121))
            (neovm--read-timer-log nil)
            (pre-redisplay-function
              (lambda (&rest _) (push 'redisplay neovm--read-timer-log)))
            (timer (timer-create)))
        (timer-set-function timer
          (lambda () (push 'timer neovm--read-timer-log)))
        (timer-set-time timer '(0 0 0 0))
        (timer-activate timer)
        (unwind-protect
            (let ((result (funcall reader nil nil 0)))
              (push (list reader result (nreverse neovm--read-timer-log)) out))
          (cancel-timer timer))))
    (nreverse out)))"#;

const READ_TIMER_THROW_FORM: &str = r#"(progn
  (defvar neovm--read-timer-log nil)
  (let ((redisplay-skip-initial-frame nil)
        (inhibit-redisplay nil)
        out)
    (dolist (reader '(read-event read-char read-char-exclusive))
      (let ((timer-list nil)
            (timer-idle-list nil)
            (unread-command-events nil)
            (neovm--read-timer-log nil)
            (pre-redisplay-function
              (lambda (&rest _) (push 'redisplay neovm--read-timer-log)))
            (timer (timer-create)))
        (timer-set-function timer
          (lambda ()
            (push 'timer neovm--read-timer-log)
            (throw 'read-timer-exit 'from-timer)))
        (timer-set-time timer '(0 0 0 0))
        (timer-activate timer)
        (unwind-protect
            (let ((result (catch 'read-timer-exit (funcall reader nil nil 0))))
              (push (list reader result (nreverse neovm--read-timer-log)) out))
          (cancel-timer timer))))
    (nreverse out)))"#;

const READ_TIMER_GENERATIONS_FORM: &str = r#"(progn
  (defvar neovm--read-timer-log nil)
  (let ((redisplay-skip-initial-frame nil)
        (inhibit-redisplay nil)
        out)
    (dolist (queue-input '(nil t))
      (let ((timer-list nil)
            (timer-idle-list nil)
            (unread-command-events nil)
            (neovm--read-timer-log nil)
            (pre-redisplay-function
              (lambda (&rest _) (push 'redisplay neovm--read-timer-log)))
            (timer (timer-create))
            (later (timer-create)))
        (timer-set-function later
          (lambda ()
            (push 'later-timer neovm--read-timer-log)
            (setq unread-command-events '(121))))
        (timer-set-time later '(0 0 0 0))
        (timer-set-function timer
          (lambda ()
            (push 'timer neovm--read-timer-log)
            (when queue-input (setq unread-command-events '(120)))
            (timer-activate later)))
        (timer-set-time timer '(0 0 0 0))
        (timer-activate timer)
        (unwind-protect
            (let ((result (read-event nil nil 0)))
              (push (list queue-input result (nreverse neovm--read-timer-log)
                          (and (memq later timer-list) t)) out))
          (cancel-timer timer)
          (cancel-timer later))))
    (nreverse out)))"#;

fn assert_gnu_oracle(form: &str, fixture_name: &str, fixture: &str) {
    crate::test_utils::init_test_tracing();
    let mut command = crate::test_utils::gnu_probe_command();
    let output = command
        .args([
            "-Q",
            "--batch",
            "--eval",
            &format!(
                r#"(progn
                  (unless (string= emacs-version "31.1")
                    (error "read-timer oracle requires GNU Emacs 31.1"))
                  (prin1 {form}) (terpri))"#
            ),
        ])
        .output()
        .expect("run configured GNU Emacs oracle");
    assert!(
        output.status.success(),
        "GNU read-timer oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let gnu = String::from_utf8(output.stdout.clone()).expect("GNU UTF-8 stdout");
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/keyboard/tests")
            .join(fixture_name);
        if std::fs::read(&path).ok().as_deref() != Some(output.stdout.as_slice()) {
            std::fs::write(&path, &output.stdout).expect("refresh exclusively from GNU stdout");
        }
        std::fs::read_to_string(path).expect("read GNU fixture")
    } else {
        fixture.to_owned()
    };
    assert!(
        !expected.trim().is_empty(),
        "refresh fixture with UPDATE_EXPECT=1"
    );
    assert_eq!(gnu.trim_end(), expected.trim_end(), "GNU oracle fixture");

    let _policy = crate::emacs_core::eval::RedisplayHookPolicyGuard::legacy();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.redisplay_fn = Some(Box::new(|_| {}));
    let neovm = eval.eval_str(form).expect("evaluate read-timer Lisp");
    assert_eq!(
        crate::emacs_core::print_value(&neovm),
        expected.trim_end(),
        "NeoVM timer/preliminary redisplay order"
    );
}

#[test]
fn gnu_read_timer_precedes_preliminary_redisplay_matches_oracle() {
    assert_gnu_oracle(
        READ_TIMER_ORDER_FORM,
        "gnu_read_timer_order.expected",
        include_str!("gnu_read_timer_order.expected"),
    );
}

#[test]
fn gnu_read_timer_preexisting_unread_input_matches_oracle() {
    assert_gnu_oracle(
        READ_TIMER_UNREAD_FORM,
        "gnu_read_timer_unread.expected",
        include_str!("gnu_read_timer_unread.expected"),
    );
}

#[test]
fn gnu_read_timer_throw_precedes_preliminary_redisplay_matches_oracle() {
    assert_gnu_oracle(
        READ_TIMER_THROW_FORM,
        "gnu_read_timer_throw.expected",
        include_str!("gnu_read_timer_throw.expected"),
    );
}

#[test]
fn gnu_read_timer_defers_callback_created_timer_matches_oracle() {
    assert_gnu_oracle(
        READ_TIMER_GENERATIONS_FORM,
        "gnu_read_timer_generations.expected",
        include_str!("gnu_read_timer_generations.expected"),
    );
}
