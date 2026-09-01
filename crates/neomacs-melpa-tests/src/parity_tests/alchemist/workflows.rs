use expect_test::expect;

use super::ParityBatchCase;

fn running_the_test_suite_renders_real_mix_failures_as_buttons_over_the_source_locations()
-> ParityBatchCase {
    ParityBatchCase::value(
        "running_the_test_suite_renders_real_mix_failures_as_buttons_over_the_source_locations",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (recordings (file-name-as-directory (expand-file-name "recordings" root)))
       (log (expand-file-name "invocations" recordings))
       (mismatched (alchemist-test-install-recordings recordings))
       (standins (alchemist-test-install-standins recordings log))
       (project (alchemist-test-make-project root))
       (test-file (expand-file-name "test/parity_project_test.exs" project))
       ;; The sandbox is inside the neomacs worktree, whose .dir-locals.el
       ;; would otherwise reach every file this suite visits.
       (enable-dir-local-variables nil)
       buffer)
  (setq buffer (find-file-noselect test-file))
  (unwind-protect
      (with-current-buffer buffer
        (setq alchemist-mix-command (car standins)
              alchemist-execute-command (cdr standins)
              ;; A real customization, and it is what makes the argument
              ;; vector match the seed the recording was captured with.
              alchemist-mix-test-default-options '("--seed" "0"))
        (alchemist-mix-test)
        (alchemist-test-await-report)
        (list
         ;; A fixture that is another language's output can be mangled by
         ;; escaping without anything signalling, so the bytes that landed
         ;; on disk are checked against the constants every run.
         :recordings-intact (null mismatched)
         :mode (with-current-buffer alchemist-test-report-buffer-name major-mode)
         :report (alchemist-test-report-text)
         ;; Four buttons: the two "N)" failure headers and the two ": (test)"
         ;; stacktrace lines.  They name DIFFERENT lines (13/14 and 17/18),
         ;; so a renderer that paired them up wrongly could not agree with
         ;; this snapshot.  The library stacktrace line carries no
         ;; ": (test)" suffix and must NOT become a button.
         :buttons (alchemist-test-report-buttons)
         :mode-name-face alchemist-test--mode-name-face
         :invocations (alchemist-test-invocations log project)))
    (when (buffer-live-p buffer) (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:recordings-intact t :mode alchemist-test-report-mode :report "Compiling 1 file (.ex)\nRunning ExUnit with seed: 0, max_cases: 64\n\n...\n\n  1) test detects a wrong total (ParityProjectTest)\n     test/parity_project_test.exs:13\n     Assertion with == failed\n     code:  assert Enum.sum([1, 2, 3]) == 7\n     left:  6\n     right: 7\n     stacktrace:\n       test/parity_project_test.exs:14: (test)\n\n\n\n  2) test raises on bad input (ParityProjectTest)\n     test/parity_project_test.exs:17\n     ** (ArgumentError) deliberate failure for the parity fixture\n     code: ParityProject.explode()\n     stacktrace:\n       (parity_project 0.1.0) lib/parity_project.ex:20: ParityProject.explode/0\n       test/parity_project_test.exs:18: (test)\n\n\nFinished in 0.08 seconds (0.00s async, 0.08s sync)\n1 doctest, 4 tests, 2 failures\n" :buttons (("test/parity_project_test.exs:13" alchemist-test--test-file-and-location-face "test/parity_project_test.exs:13") ("test/parity_project_test.exs:14" alchemist-test--stacktrace-file-and-location-face "test/parity_project_test.exs:14") ("test/parity_project_test.exs:17" alchemist-test--test-file-and-location-face "test/parity_project_test.exs:17") ("test/parity_project_test.exs:18" alchemist-test--stacktrace-file-and-location-face "test/parity_project_test.exs:18")) :mode-name-face alchemist-test--failed-face :invocations "cwd=[PROJECT]\nargv: [test] [--seed] [0]\n")"#
        ]],
    )
}

fn pressing_a_rendered_failure_button_opens_the_test_file_at_that_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "pressing_a_rendered_failure_button_opens_the_test_file_at_that_line",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (recordings (file-name-as-directory (expand-file-name "recordings" root)))
       (log (expand-file-name "invocations" recordings))
       (mismatched (alchemist-test-install-recordings recordings))
       (standins (alchemist-test-install-standins recordings log))
       (project (alchemist-test-make-project root))
       (test-file (expand-file-name "test/parity_project_test.exs" project))
       ;; The sandbox is inside the neomacs worktree, whose .dir-locals.el
       ;; would otherwise reach every file this suite visits.
       (enable-dir-local-variables nil)
       buffer opened)
  (setq buffer (find-file-noselect test-file))
  (unwind-protect
      (with-current-buffer buffer
        (setq alchemist-mix-command (car standins)
              alchemist-execute-command (cdr standins)
              alchemist-mix-test-default-options '("--seed" "0"))
        (alchemist-mix-test)
        (alchemist-test-await-report)
        (with-current-buffer alchemist-test-report-buffer-name
          (let ((default-directory project)
                (stacktrace-button
                 ;; Locate the button by the text it carries, never by
                 ;; counting: a button is a button wherever it is taken from.
                 (let ((position (point-min)) found)
                   (while (and (not found) (setq position (next-button position)))
                     (when (equal (button-get position 'file)
                                  "test/parity_project_test.exs:14")
                       (setq found position))
                     (setq position (button-end position)))
                   found)))
            (list
             :found (and stacktrace-button t)
             :label (copy-sequence (button-label stacktrace-button))
             :opened
             (progn
               (push-button (button-start stacktrace-button))
               ;; `alchemist-test--open-file' does its work inside
               ;; `with-current-buffer', so it leaves the report buffer
               ;; current; the visited buffer is where the effect landed.
               (setq opened (get-file-buffer test-file))
               (with-current-buffer opened
                 (list (file-relative-name (buffer-file-name) project)
                       (line-number-at-pos)
                       ;; Report the line's text beside its number, so a
                       ;; miscounted position shows up as different text.
                       (copy-sequence
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position))))))))))
    (dolist (live (list buffer opened))
      (when (and live (buffer-live-p live)) (kill-buffer live)))))"##,
        expect![[
            r#"OK (:found t :label "test/parity_project_test.exs:14" :opened ("test/parity_project_test.exs" 14 "    assert Enum.sum([1, 2, 3]) == 7"))"#
        ]],
    )
}

fn testing_at_point_and_testing_stale_build_different_commands_and_render_their_own_reports()
-> ParityBatchCase {
    ParityBatchCase::value(
        "testing_at_point_and_testing_stale_build_different_commands_and_render_their_own_reports",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (recordings (file-name-as-directory (expand-file-name "recordings" root)))
       (log (expand-file-name "invocations" recordings))
       (mismatched (alchemist-test-install-recordings recordings))
       (standins (alchemist-test-install-standins recordings log))
       (project (alchemist-test-make-project root))
       (test-file (expand-file-name "test/parity_project_test.exs" project))
       ;; The sandbox is inside the neomacs worktree, whose .dir-locals.el
       ;; would otherwise reach every file this suite visits.
       (enable-dir-local-variables nil)
       buffer)
  (setq buffer (find-file-noselect test-file))
  (unwind-protect
      (with-current-buffer buffer
        (setq alchemist-mix-command (car standins)
              alchemist-execute-command (cdr standins)
              alchemist-mix-test-default-options '("--seed" "0"))
        ;; Put point inside the failing test, the way a user does before
        ;; running just that one.
        (goto-char (point-min))
        (search-forward "assert Enum.sum([1, 2, 3]) == 7")
        (let ((at-point-line (line-number-at-pos)))
          (alchemist-mix-test-at-point)
          (alchemist-test-await-report)
          (let ((at-point-report (alchemist-test-report-text))
                (at-point-buttons (alchemist-test-report-buttons)))
            ;; `alchemist-mix-test-stale' first asks the toolchain for its
            ;; version through `alchemist-execute-command', so the elixir
            ;; stand-in is exercised here too.
            (alchemist-mix-test-stale)
            (alchemist-test-await-report)
            (list :at-point-line at-point-line
                  :at-point-report at-point-report
                  :at-point-buttons at-point-buttons
                  :stale-report (alchemist-test-report-text)
                  :stale-buttons (alchemist-test-report-buttons)
                  :last-run (copy-sequence
                             (file-relative-name alchemist-last-run-test project))
                  ;; Both invocations, plus the version query in between.
                  :invocations (alchemist-test-invocations log project)))))
    (when (buffer-live-p buffer) (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:at-point-line 14 :at-point-report "Running ExUnit with seed: 0, max_cases: 64\nExcluding tags: [:test]\nIncluding tags: [location: {\"test/parity_project_test.exs\", 14}]\n\n\n\n  1) test detects a wrong total (ParityProjectTest)\n     test/parity_project_test.exs:13\n     Assertion with == failed\n     code:  assert Enum.sum([1, 2, 3]) == 7\n     left:  6\n     right: 7\n     stacktrace:\n       test/parity_project_test.exs:14: (test)\n\n\nFinished in 0.06 seconds (0.00s async, 0.06s sync)\n1 doctest, 4 tests, 1 failure, 4 excluded\n" :at-point-buttons (("test/parity_project_test.exs:13" alchemist-test--test-file-and-location-face "test/parity_project_test.exs:13") ("test/parity_project_test.exs:14" alchemist-test--stacktrace-file-and-location-face "test/parity_project_test.exs:14")) :stale-report "Running ExUnit with seed: 0, max_cases: 64\n\n...\n\n  1) test detects a wrong total (ParityProjectTest)\n     test/parity_project_test.exs:13\n     Assertion with == failed\n     code:  assert Enum.sum([1, 2, 3]) == 7\n     left:  6\n     right: 7\n     stacktrace:\n       test/parity_project_test.exs:14: (test)\n\n\n\n  2) test raises on bad input (ParityProjectTest)\n     test/parity_project_test.exs:17\n     ** (ArgumentError) deliberate failure for the parity fixture\n     code: ParityProject.explode()\n     stacktrace:\n       (parity_project 0.1.0) lib/parity_project.ex:20: ParityProject.explode/0\n       test/parity_project_test.exs:18: (test)\n\n\nFinished in 0.08 seconds (0.00s async, 0.08s sync)\n1 doctest, 4 tests, 2 failures\n" :stale-buttons (("test/parity_project_test.exs:13" alchemist-test--test-file-and-location-face "test/parity_project_test.exs:13") ("test/parity_project_test.exs:14" alchemist-test--stacktrace-file-and-location-face "test/parity_project_test.exs:14") ("test/parity_project_test.exs:17" alchemist-test--test-file-and-location-face "test/parity_project_test.exs:17") ("test/parity_project_test.exs:18" alchemist-test--stacktrace-file-and-location-face "test/parity_project_test.exs:18")) :last-run "test/--stale" :invocations "cwd=[PROJECT]\nargv: [test] [[PROJECT]test/parity_project_test.exs:14] [--seed] [0]\nelixir argv: [--version]\ncwd=[PROJECT]\nargv: [test] [--stale] [--seed] [0]\n")"#
        ]],
    )
    .fresh_process()
}

fn a_green_suite_leaves_the_success_face_and_produces_no_buttons() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_green_suite_leaves_the_success_face_and_produces_no_buttons",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (recordings (file-name-as-directory (expand-file-name "recordings" root)))
       (log (expand-file-name "invocations" recordings))
       (mismatched (alchemist-test-install-recordings recordings))
       (standins (alchemist-test-install-standins recordings log))
       (project (alchemist-test-make-project root))
       (test-file (expand-file-name "test/parity_project_test.exs" project))
       ;; The sandbox is inside the neomacs worktree, whose .dir-locals.el
       ;; would otherwise reach every file this suite visits.
       (enable-dir-local-variables nil)
       buffer)
  (setenv "ALCHEMIST_MIX_REPLY" "pass")
  (setq buffer (find-file-noselect test-file))
  (unwind-protect
      (with-current-buffer buffer
        (setq alchemist-mix-command (car standins)
              alchemist-execute-command (cdr standins)
              alchemist-mix-test-default-options '("--seed" "0"))
        (alchemist-mix-test)
        (alchemist-test-await-report)
        (prog1
            (list :report (alchemist-test-report-text)
                  ;; Nothing in a green run matches either renderer regex.
                  :buttons (alchemist-test-report-buttons)
                  :mode-name-face alchemist-test--mode-name-face
                  :invocations (alchemist-test-invocations log project))
          (setenv "ALCHEMIST_MIX_REPLY" nil)))
    (setenv "ALCHEMIST_MIX_REPLY" nil)
    (when (buffer-live-p buffer) (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:report "Running ExUnit with seed: 0, max_cases: 64\n\n...\nFinished in 0.07 seconds (0.00s async, 0.07s sync)\n1 doctest, 2 tests, 0 failures\n" :buttons nil :mode-name-face alchemist-test--success-face :invocations "cwd=[PROJECT]\nargv: [test] [--seed] [0]\n")"#
        ]],
    )
    .fresh_process()
}

fn the_compilation_output_filter_no_longer_matches_what_modern_mix_prints() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_compilation_output_filter_no_longer_matches_what_modern_mix_prints",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (recordings (file-name-as-directory (expand-file-name "recordings" root)))
       (log (expand-file-name "invocations" recordings))
       (mismatched (alchemist-test-install-recordings recordings))
       (standins (alchemist-test-install-standins recordings log))
       (project (alchemist-test-make-project root))
       (test-file (expand-file-name "test/parity_project_test.exs" project))
       ;; The sandbox is inside the neomacs worktree, whose .dir-locals.el
       ;; would otherwise reach every file this suite visits.
       (enable-dir-local-variables nil))
  (cl-flet
      ((run (display-compilation)
         (let ((buffer (find-file-noselect test-file)))
           (unwind-protect
               (with-current-buffer buffer
                 (setq alchemist-mix-command (car standins)
                       alchemist-execute-command (cdr standins)
                       alchemist-mix-test-default-options '("--seed" "0")
                       alchemist-test-display-compilation-output display-compilation)
                 (alchemist-mix-test)
                 (alchemist-test-await-report)
                 (let ((text (alchemist-test-report-text)))
                   (list :compiling-line-survives
                         (and (string-match-p "^Compiling 1 file" text) t)
                         :generated-line-survives
                         (and (string-match-p "^Generated parity_project app" text) t)
                         :first-line
                         (copy-sequence (car (split-string text "\n"))))))
             (when (buffer-live-p buffer) (kill-buffer buffer))))))
    (list :recordings-intact (null mismatched)
          ;; What the recording really contains, so the two arms below can be
          ;; read without the fixture in hand.
          :recorded-first-lines
          (mapcar #'copy-sequence
                  (seq-take (split-string alchemist-test-recording-full "\n") 2))
          :filter-on (run nil)
          :filter-off (run t))))"##,
        expect![[
            r#"OK (:recordings-intact t :recorded-first-lines ("Compiling 1 file (.ex)" "Generated parity_project app") :filter-on (:compiling-line-survives t :generated-line-survives nil :first-line "Compiling 1 file (.ex)") :filter-off (:compiling-line-survives t :generated-line-survives t :first-line "Compiling 1 file (.ex)"))"#
        ]],
    )
}

fn alchemist_key_bindings_reach_their_commands_only_once_their_mode_is_on() -> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_key_bindings_reach_their_commands_only_once_their_mode_is_on",
        r##"(let ((buffer (generate-new-buffer "*alchemist-keys*")))
  (unwind-protect
      (progn
        (set-window-buffer (selected-window) buffer)
        (set-buffer buffer)
        (cl-letf (((symbol-function 'alchemist-server-start-if-not-running)
                   (lambda () nil)))
          (alchemist-mode 1))
        (list
         :prefix (copy-sequence alchemist-key-command-prefix)
         ;; What the map itself answers: a number, for every one of them.
         :direct-lookup
         (mapcar (lambda (key)
                   (list key (lookup-key alchemist-mode-keymap (kbd key))))
                 '("C-c a x" "C-c a t" "C-c a r"))
         ;; What the user actually gets.
         :dispatched
         (mapcar (lambda (key) (list key (key-binding (kbd key))))
                 '("C-c a x" "C-c a t" "C-c a r" "C-c a c b" "C-c a e b"))
         ;; The test-mode keys belong to `alchemist-test-mode', which
         ;; `alchemist-test-enable-mode' turns on for Elixir buffers holding
         ;; tests -- so they are absent until that mode is on, and present
         ;; afterwards.
         :test-keys-before
         (mapcar (lambda (key) (list key (key-binding (kbd key))))
                 '("C-c , s" "C-c , a" "C-c , n"))
         :test-keys-after
         (progn
           (alchemist-test-mode 1)
           (mapcar (lambda (key) (list key (key-binding (kbd key))))
                   '("C-c , s" "C-c , a" "C-c , n")))))
    (when (buffer-live-p buffer) (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:prefix "\3a" :direct-lookup (("C-c a x" 1) ("C-c a t" 1) ("C-c a r" 1)) :dispatched (("C-c a x" alchemist-mix) ("C-c a t" alchemist-mix-test) ("C-c a r" alchemist-mix-rerun-last-test) ("C-c a c b" alchemist-compile-this-buffer) ("C-c a e b" alchemist-execute-this-buffer)) :test-keys-before (("C-c , s" nil) ("C-c , a" nil) ("C-c , n" nil)) :test-keys-after (("C-c , s" alchemist-mix-test-at-point) ("C-c , a" alchemist-mix-test) ("C-c , n" alchemist-test-mode-jump-to-next-test)))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        running_the_test_suite_renders_real_mix_failures_as_buttons_over_the_source_locations(),
        pressing_a_rendered_failure_button_opens_the_test_file_at_that_line(),
        testing_at_point_and_testing_stale_build_different_commands_and_render_their_own_reports(),
        a_green_suite_leaves_the_success_face_and_produces_no_buttons(),
        the_compilation_output_filter_no_longer_matches_what_modern_mix_prints(),
        alchemist_key_bindings_reach_their_commands_only_once_their_mode_is_on(),
    ]
}
