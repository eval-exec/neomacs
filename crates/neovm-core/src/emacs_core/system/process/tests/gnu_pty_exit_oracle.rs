//! GNU process.c:7896-7911 drains output before updating status at :7914-7915
//! and invoking the sentinel at :7937. Refresh expectations only from GNU.

const PTY_EXIT_FORM: &str = r#"(let ((payloads '("process-output\n" "café世界\n")) results)
  (while payloads
    (let* ((payload (car payloads))
           (buffer (get-buffer-create " *pty-eof-oracle*"))
           (process (make-process
                      :name "pty-eof-oracle"
                      :buffer buffer
                      :connection-type 'pty
                      :coding 'utf-8-unix
                      :command (list "/bin/sh" "-c" "printf '%s' \"$1\""
                                     "pty-eof-oracle" payload)))
           (attempts 0)
           complete)
      (unwind-protect
          (progn
            (set-process-query-on-exit-flag process nil)
            ;; A single accept-process-output can return before child exit.
            ;; Wait for the default sentinel itself, so the fixture has one
            ;; outcome regardless of child scheduling. The bound only guards
            ;; broken implementations; it does not synchronize child exit.
            (while (and (null complete) (< attempts 100))
              (accept-process-output process 0.1)
              (setq attempts (1+ attempts))
              (setq complete (null (memq process (process-list)))))
            (if (null complete) (error "PTY exit sentinel was not delivered"))
            (setq results (cons (list (process-status process)
                        (save-current-buffer
                          (set-buffer buffer)
                          (buffer-string)))
                  results)))
        (if (memq process (process-list)) (delete-process process))
        (kill-buffer buffer)))
    (setq payloads (cdr payloads)))
  (nreverse results))"#;

#[test]
fn accept_process_output_pty_immediate_exit_matches_gnu_oracle() {
    assert_exit_oracle(
        PTY_EXIT_FORM,
        "gnu_pty_exit.expected",
        include_str!("gnu_pty_exit.expected"),
    );
}

const PIPE_EXIT_FORM: &str = r#"(let ((shapes '((t utf-8-dos) (t utf-8-unix) (nil utf-8-dos) (t utf-8-dos filter))) results)
  (while shapes
    (let* ((shape (car shapes))
           (buffer (get-buffer-create " *pipe-exit-oracle*"))
           (unibyte (car shape))
           (coding (car (cdr shape)))
           (filtered (car (cdr (cdr shape))))
           (seen "")
           snapshot
           process)
      (unwind-protect
          (progn
            (save-current-buffer (set-buffer buffer) (set-buffer-multibyte (null unibyte)))
            (setq process (make-process :name "pipe-exit-oracle" :buffer buffer
              :connection-type 'pipe :coding coding
              :command '("/bin/sh" "-c" "printf 'caf\\303\\251\\r\\n'")
              :filter (if filtered (lambda (_p s) (setq seen (concat seen s))) #'internal-default-process-filter)
              :sentinel (lambda (p message)
                (setq snapshot (list (process-status p) message
                  (if (memq p (process-list)) t nil)
                  (append (if filtered seen (save-current-buffer (set-buffer buffer) (buffer-string))) nil))))))
            (set-process-query-on-exit-flag process nil)
            (let ((attempts 0))
              (while (and (null snapshot) (< attempts 100))
                (accept-process-output process 0.1)
                (setq attempts (1+ attempts))))
            (if (null snapshot) (error "Pipe sentinel did not run"))
            (setq results (cons snapshot results)))
        (if (and process (memq process (process-list))) (delete-process process))
        (kill-buffer buffer)))
    (setq shapes (cdr shapes)))
  (nreverse results))"#;

#[test]
fn accept_process_output_pipe_immediate_exit_matches_gnu_oracle() {
    assert_exit_oracle(
        PIPE_EXIT_FORM,
        "gnu_pipe_exit.expected",
        include_str!("gnu_pipe_exit.expected"),
    );
}

fn assert_exit_oracle(form: &str, fixture_name: &str, compiled_expectation: &str) {
    crate::test_utils::init_test_tracing();
    let emacs = crate::test_utils::gnu_oracle().executable_path();
    // Campaign runs wrap nextest itself in sandbox-run.sh and unset this
    // variable inside it; a standalone refresh can supply the wrapper here.
    let mut command = if let Some(sandbox) = std::env::var_os("NEOVM_LISP_SANDBOX") {
        let mut command = std::process::Command::new(sandbox);
        command.arg(&emacs);
        command
    } else {
        std::process::Command::new(emacs)
    };
    let gnu_form = format!(
        r#"(progn
          (unless (string= emacs-version "31.1")
            (error "process exit oracle requires GNU Emacs 31.1, got %s" emacs-version))
          (let ((print-escape-newlines t)) (prin1 {form}))
          (terpri))"#
    );
    let output = command
        .args(["-Q", "--batch", "--eval", &gnu_form])
        .output()
        .expect("run the explicitly configured GNU Emacs oracle in its sandbox");
    assert!(
        output.status.success(),
        "GNU process exit oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual_gnu = String::from_utf8(output.stdout.clone()).expect("GNU oracle UTF-8 stdout");
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/emacs_core/system/process/tests")
            .join(fixture_name);
        // Expectations come exclusively from GNU, including in refresh mode.
        std::fs::write(&fixture, &output.stdout).expect("refresh fixture exclusively from GNU");
        std::fs::read_to_string(fixture).expect("read freshly written GNU fixture")
    } else {
        compiled_expectation.to_owned()
    };
    assert!(
        !expected.trim().is_empty(),
        "refresh the blank process exit fixture from GNU Emacs 31.1 with UPDATE_EXPECT=1"
    );
    assert_eq!(
        actual_gnu.trim_end(),
        expected.trim_end(),
        "GNU oracle fixture"
    );

    let mut eval = crate::emacs_core::Context::new();
    let actual_neovm = eval
        .eval_str(form)
        .expect("read immediate-exit process output through accept-process-output");
    let mut options = crate::emacs_core::print::PrintOptions::default();
    options.print_escape_newlines = true;
    assert_eq!(
        crate::emacs_core::print::print_value_with_options(&actual_neovm, options),
        expected.trim_end(),
        "Process output must precede the default finished sentinel"
    );
}
