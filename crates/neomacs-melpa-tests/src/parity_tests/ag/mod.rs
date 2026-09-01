use std::time::Duration;

use crate::{AG_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AG_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ag.el builds a silver-searcher command line, runs it through
/// `compilation-start` and renders the output in its own compilation-derived
/// mode.  The `ag` binary is absent on this host, so it is the one stand-in: a
/// recording executable on PATH that logs the exact argument vector it was
/// given and replies with realistic ag output, colour escapes included.  ag.el
/// keeps doing its own real work -- assembling and shell-quoting the command
/// line, starting the compilation, filtering the escape sequences, parsing the
/// results into navigable matches and visiting them.
const AG_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

;; ag.el builds a silver-searcher command line, runs it through
;; `compilation-start' and renders the output in its own compilation-derived
;; mode.  The `ag' binary is absent here, so it is the one stand-in: a recording
;; executable on PATH that logs the exact argument vector it was given and
;; replies with realistic ag output, colour escapes included.  ag.el keeps doing
;; its own real work -- building the command line, quoting it, starting the
;; compilation, filtering the escapes and parsing the results into navigable
;; matches.

(setq make-backup-files nil create-lockfiles nil)

(defvar ag-test-root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar ag-test-project (file-name-as-directory (expand-file-name "project" ag-test-root)))

(defun ag-test-write (path text)
  (make-directory (file-name-directory path) t)
  (with-temp-buffer (insert text)
    (write-region (point-min) (point-max) path nil 'silent))
  path)

(defun ag-test-make-project ()
  "Create the fixture tree: Unicode content and a file name with a space."
  (make-directory (expand-file-name ".git" ag-test-project) t)
  (ag-test-write (expand-file-name "src/greeting.el" ag-test-project)
                 ";; Grüße an alle\n(defun greet () \"Grüße\")\n(defun farewell () \"Tschüss\")\n")
  (ag-test-write (expand-file-name "docs/design notes.md" ag-test-project)
                 "# Notes\n\nWe say Grüße in the greeting module.\n")
  (ag-test-write (expand-file-name "README.md" ag-test-project)
                 "Grüße everyone.\n")
  ag-test-project)

(defconst ag-test-agent-script
  "#!/bin/sh
{ printf '%s\\n' '--CALL--'; for a in \"$@\"; do printf '%s\\n' \"$a\"; done; } >> \"$AG_TEST_DIR/ag.log\"
for a in \"$@\"; do
  case \"$a\" in
    NOTHING) exit 1 ;;
    EXPLODE) printf 'ag: unknown option\\n' >&2; exit 2 ;;
  esac
done
cat \"$AG_TEST_DIR/ag-output.txt\"
exit 0
")

(defun ag-test-install-ag ()
  "Install the recording `ag' stand-in and point `ag-executable' at it."
  (let ((path (expand-file-name "bin/ag" ag-test-root)))
    (ag-test-write path ag-test-agent-script)
    (set-file-modes path #o755)
    ;; Realistic grouped, coloured ag output for the fixture tree.
    (ag-test-write
     (expand-file-name "ag-output.txt" ag-test-root)
     (concat "\033[1;32msrc/greeting.el\033[0m\033[K\n"
             "1:4:;; \033[30;43mGrüße\033[0m\033[K an alle\n"
             "2:24:(defun greet () \"\033[30;43mGrüße\033[0m\033[K\")\n"
             "\n"
             "\033[1;32mdocs/design notes.md\033[0m\033[K\n"
             "3:9:We say \033[30;43mGrüße\033[0m\033[K in the greeting module.\n"
             "\n"
             "\033[1;32mREADME.md\033[0m\033[K\n"
             "1:1:\033[30;43mGrüße\033[0m\033[K everyone.\n"))
    (setenv "AG_TEST_DIR" (directory-file-name ag-test-root))
    (setenv "PATH" (concat (expand-file-name "bin" ag-test-root)
                           path-separator (getenv "PATH")))
    (push (expand-file-name "bin" ag-test-root) exec-path)
    (setq ag-executable path)))

(defun ag-test-calls ()
  "Each recorded ag invocation as an argument list, oldest first."
  (let ((path (expand-file-name "ag.log" ag-test-root)))
    (if (file-regular-p path)
        (with-temp-buffer
          (let ((coding-system-for-read 'utf-8)) (insert-file-contents path))
          (let (calls current)
            (dolist (line (split-string (buffer-string) "\n"))
              (cond ((string= line "--CALL--")
                     (when current (push (nreverse current) calls))
                     (setq current nil))
                    ((string= line ""))
                    (t (push line current))))
            (when current (push (nreverse current) calls))
            (nreverse calls)))
      nil)))

(defun ag-test-compilation-complete-p (buffer)
  "Non-nil once `compilation-handle-exit' has written BUFFER's last line.
That line is the causal end of the output rather than a guess about it: Emacs
drains a dying process's remaining reads before it runs the sentinel
\(GNU src/process.c:7896-7910), the sentinel is what calls
`compilation-handle-exit', and that function marks the text it writes with a
`compilation-handle-exit' text property (GNU lisp/progmodes/compile.el:2630).
The property therefore cannot appear until every byte ag wrote has already
been through `compilation-filter'."
  (and (buffer-live-p buffer)
       (with-current-buffer buffer
         (and (text-property-not-all (point-min) (point-max)
                                     'compilation-handle-exit nil)
              t))))

(defun ag-test-result-buffers ()
  "Every live `ag-mode' buffer, in `buffer-list' order."
  (cl-remove-if-not
   (lambda (buffer)
     (with-current-buffer buffer (eq major-mode 'ag-mode)))
   (buffer-list)))

(defun ag-test-wait-for-search ()
  "Wait until every ag-mode buffer holds all of its search's output, or signal.
A workflow can retain both regexp and text result buffers, so do not pick
whichever `*ag search' buffer happens to appear first.  This helper used to
wait for every live ag process to die and then `sit-for' 0.05 -- but a process
being dead is not the same fact as its output having been read, and 0.05
seconds is a statement about the clock, not about the output.  The rendered
text this suite pins ends with the sentinel's own line, which by construction
cannot have been written when a `process-live-p' wait returns.  Signalling
rather than returning means a future edit that goes back to the clock fails on
its first run instead of moving a snapshot months later.  See DIVERGENCES.md
entries 133, 140 and 144."
  (let ((waited 0))
    (while (and (< waited 1200)
                (let ((buffers (ag-test-result-buffers)))
                  (or (null buffers)
                      (cl-find-if-not #'ag-test-compilation-complete-p buffers))))
      (accept-process-output nil 0.05)
      (setq waited (1+ waited)))
    (let ((buffers (ag-test-result-buffers)))
      (unless buffers
        (error "ag-test-wait-for-search: no `ag-mode' buffer exists, so no \
search's output could have ended"))
      (let ((pending (cl-remove-if #'ag-test-compilation-complete-p buffers)))
        (when pending
          (error "ag-test-wait-for-search: %S never reached \
`compilation-handle-exit'; their text records only as much of ag's output as \
had been read" (mapcar #'buffer-name pending)))))
    :finished))

(defun ag-test-results-buffer ()
  (cl-find-if (lambda (b) (string-prefix-p "*ag search" (buffer-name b)))
              (buffer-list)))

(defun ag-test-rendered ()
  "The rendered results buffer: name, mode and text with the header normalised."
  (let ((buffer (ag-test-results-buffer)))
    (if (not buffer)
        'no-results-buffer
      (with-current-buffer buffer
        (list :name (buffer-name)
              :mode major-mode
              :text (replace-regexp-in-string
                     "\\(Compilation \\|Ag \\)\\(started\\|finished\\|exited\\).*"
                     "<STATUS>"
                     (replace-regexp-in-string
                      (regexp-quote (directory-file-name ag-test-root)) "<ROOT>"
                      (buffer-substring-no-properties (point-min) (point-max)))))))))

(defmacro ag-test-with-project (&rest body)
  `(progn
     (ag-test-make-project)
     (ag-test-install-ag)
     (let ((default-directory ag-test-project))
       (unwind-protect (progn ,@body)
         (dolist (buffer (buffer-list))
           (when (string-prefix-p "*ag search" (buffer-name buffer))
             (let ((kill-buffer-query-functions nil))
               (with-current-buffer buffer (set-buffer-modified-p nil))
               (kill-buffer buffer))))
         (dolist (process (process-list))
           (set-process-query-on-exit-flag process nil)
           (delete-process process))))))
"##;

fn ag_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AG_MELPA_PIN, "ag.el")
        .expect("prepare pinned ag source below ./tmp")
        .with_prelude(AG_TEST_PRELUDE)
        .with_timeout(AG_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ag parity test").into()
}

/// Multi-probe batch for `assert_ag_parity` cases (2a).
pub(crate) fn assert_ag_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ag_oracle(), &name, "ag_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ag_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ag_batch(&cases);
}

// END generated package batch tests
