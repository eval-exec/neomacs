use std::time::Duration;

use crate::{CachedMelpaOracle, ESH_HELP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ESH_HELP_TEST_TIMEOUT: Duration = Duration::from_secs(120);

// Captured from man-db 2.13.1 / coreutils 9.8 with:
//   LANG=C man printf
const PRINTF_MAN_RECORDING: &str = include_str!("printf-man-db-2.13.1-coreutils-9.8.txt");
// Captured from the same page through util-linux col 2.41.4 with:
//   LANG=C man printf | col -b
const PRINTF_COL_RECORDING: &str = include_str!("printf-col-b-man-db-2.13.1-coreutils-9.8.txt");

const ESH_HELP_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'esh-help)
(require 'em-alias)
(require 'em-banner)
(require 'em-dirs)
(require 'em-hist)
(require 'em-prompt)
(require 'em-script)

(defun printf (lisp-only-argument)
  "Deliberate Lisp-name collision for testing Esh Help's PATH precedence."
  (list :lisp-branch-should-not-win lisp-only-argument))

(defun neomacs-esh-help-test-root (case-name)
  "Return CASE-NAME's directory below the Rust-owned sandbox."
  (file-name-as-directory
   (expand-file-name case-name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-esh-help-test-write-executable (path contents)
  "Write executable CONTENTS to PATH."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  (set-file-modes path #o755))

(defun neomacs-esh-help-test-type-input (input)
  "Type INPUT through the selected Eshell window after a fresh prompt."
  (let ((inhibit-read-only t))
    (erase-buffer)
    (eshell-emit-prompt))
  (switch-to-buffer (current-buffer))
  (execute-kbd-macro (string-to-vector input)))

(defun neomacs-esh-help-test-reset-eldoc-idle-timer ()
  "Start one semantic workflow without an earlier command's pending request."
  (when (and (timerp eldoc-timer)
             (memq eldoc-timer timer-idle-list))
    (cancel-timer eldoc-timer))
  (setq eldoc-timer nil))

(defun neomacs-esh-help-test-visible-eldoc (input)
  "Enter INPUT and await its automatically scheduled Eldoc presentation."
  (setq-local eldoc-display-functions '(eldoc-display-in-echo-area))
  (unless eldoc-mode
    (eldoc-mode 1))
  ;; Commands between semantic probes (for example M-x cache clearing) may
  ;; leave their own pending request.  Consume that test-owned pending state
  ;; before proving the newly typed input schedules exactly one request.
  (neomacs-esh-help-test-reset-eldoc-idle-timer)
  (let ((origin (current-buffer))
        (timer-baseline (copy-sequence timer-idle-list))
        delivery-timers
        cleanup-timers)
    (unwind-protect
        (progn
          (neomacs-esh-help-test-type-input input)
          (setq delivery-timers
                (seq-difference timer-idle-list timer-baseline #'eq))
          (setq cleanup-timers delivery-timers)
          (unless (and (= (length delivery-timers) 1)
                       (eq (car delivery-timers) eldoc-timer))
            (error "Eldoc scheduled unexpected idle timers: owned=%S pointer=%S hook=%S idle=%S"
                   delivery-timers eldoc-timer
                   (memq #'eldoc-schedule-timer post-command-hook)
                   timer-idle-list))
          ;; Batch editors never enter command-loop idle.  Deliver only the
          ;; identity-isolated timer created by the typed command through
          ;; GNU's real timer event handler.  The handler itself consumes the
          ;; one-shot timer, just as it does in the interactive scheduler.  The
          ;; command bindings reproduce idle command-loop state; ElDoc's
          ;; update, provider, strategy, and echo-area display remain real.
          (let ((this-command nil)
                (last-command 'self-insert-command))
            (timer-event-handler (car delivery-timers)))
          (with-current-buffer origin
            (list :input input
                  :eldoc-message
                  (when eldoc-last-message
                    (substring-no-properties eldoc-last-message))
                  :point (point)
                  :text
                  (buffer-substring-no-properties (point-min) (point-max))
                  :eldoc-mode eldoc-mode
                  :timer-deliveries (length delivery-timers)
                  :scheduled
                  (not (null
                        (memq #'eldoc-schedule-timer post-command-hook))))))
      ;; Synchronous external commands may pass through nested command/event
      ;; processing and schedule another ElDoc timer.  It is still owned by
      ;; this request, so discover it against the same identity baseline and
      ;; tear it down even when the provider signals.
      (setq cleanup-timers
            (delete-dups
             (append cleanup-timers
                     (seq-difference timer-idle-list timer-baseline #'eq))))
      (dolist (timer cleanup-timers)
        (when (timerp timer)
          (cancel-timer timer)))
      ;; `timer-event-handler' has consumed the one-shot timer, but ElDoc's
      ;; buffer-local pointer is normally cleared by the command-loop path
      ;; which batch mode does not enter.  Release only the timer this helper
      ;; proved it owns so the next typed command schedules a fresh request.
      (with-current-buffer origin
        (when (memq eldoc-timer cleanup-timers)
          (setq eldoc-timer nil))))))

(defun neomacs-esh-help-test-request-eldoc (input)
  "Interactively request Eldoc for INPUT through its public refresh command."
  (unless eldoc-mode
    (eldoc-mode 1))
  (neomacs-esh-help-test-reset-eldoc-idle-timer)
  (let ((timer-baseline (copy-sequence timer-idle-list))
        owned-timers)
    (unwind-protect
        (progn
          (neomacs-esh-help-test-type-input input)
          (setq owned-timers
                (seq-difference timer-idle-list timer-baseline #'eq))
          (eldoc-print-current-symbol-info t))
      (dolist (timer owned-timers)
        (when (timerp timer)
          (cancel-timer timer)))
      (when (memq eldoc-timer owned-timers)
        (setq eldoc-timer nil)))))

(defun neomacs-esh-help-test-run-help (input)
  "Type INPUT, invoke `M-x esh-help-run-help', and return typed state."
  (neomacs-esh-help-test-type-input input)
  (let ((typed-state
         (list :text (buffer-substring-no-properties (point-min) (point-max))
               :point (point)
               :selected
               (eq (window-buffer (selected-window)) (current-buffer)))))
    (execute-kbd-macro (kbd "M-x esh-help-run-help RET"))
    typed-state))

(defun neomacs-esh-help-test-command-execute-run-help (input)
  "Type INPUT and interactively dispatch Esh Help for asynchronous Man UI.

`execute-kbd-macro' is retained by the non-async workflows.  GNU's batch macro
loop tries to reselect its pre-command buffer after asynchronous Man display,
which is not a user-visible package boundary.  `command-execute' preserves the
real command, interactive specification, argument discovery, and Man path."
  (neomacs-esh-help-test-type-input input)
  (let ((typed-state
         (list :text (buffer-substring-no-properties (point-min) (point-max))
               :point (point)
               :selected
               (eq (window-buffer (selected-window)) (current-buffer)))))
    (command-execute #'esh-help-run-help)
    typed-state))

(defun neomacs-esh-help-test-read-lines (path)
  "Read nonempty lines from PATH, or return nil if it does not exist."
  (when (file-exists-p path)
    (with-temp-buffer
      (insert-file-contents path)
      (split-string (buffer-string) "\n" t))))

(defun neomacs-esh-help-test-drop-synopsis (recording)
  "Derive a malformed fixture by removing RECORDING's SYNOPSIS section."
  (with-temp-buffer
    (insert recording)
    (goto-char (point-min))
    (unless (re-search-forward "^SYNOPSIS\n" nil t)
      (error "Recorded man page has no SYNOPSIS heading"))
    (let ((start (match-beginning 0)))
      (unless (re-search-forward "^DESCRIPTION\n" nil t)
        (error "Recorded man page has no DESCRIPTION after SYNOPSIS"))
      (delete-region start (match-beginning 0)))
    (buffer-string)))

(defun neomacs-esh-help-test-install-man-peers (root)
  "Install fail-closed man-db 2.13.1 replay peers below ROOT.

The successful payload is a byte-for-byte recording of the coreutils 9.8
printf(1) page from man-db 2.13.1, plus the exact bytes produced by util-linux
col 2.41.4.  The missing-page route reproduces man-db's real stderr and exit
status behavior.  Unknown requests are logged and fail nonzero so they cannot
accidentally satisfy a package workflow."
  (let* ((bin (expand-file-name "bin/" root))
         (recording (expand-file-name "printf.man" root))
         (col-recording (expand-file-name "printf.col" root))
         (malformed-recording (expand-file-name "printf-no-synopsis.man" root))
         (malformed-col-recording
          (expand-file-name "printf-no-synopsis.col" root))
         (man-program (expand-file-name "man" bin))
         (man-log (expand-file-name "man.log" root))
         (col-log (expand-file-name "col.log" root))
         (col-input-base (expand-file-name "col.input" root))
         (miss-log (expand-file-name "fixture-miss.log" root)))
    (with-temp-file recording
      (insert neomacs-esh-help-test-printf-man-recording))
    (with-temp-file col-recording
      (insert neomacs-esh-help-test-printf-col-recording))
    ;; The malformed boundary is a single documented corruption of the exact
    ;; real-tool recordings, not invented man output.  Removing only SYNOPSIS
    ;; exercises Esh Help's parser failure against all other real page bytes.
    (with-temp-file malformed-recording
      (insert
       (neomacs-esh-help-test-drop-synopsis
        neomacs-esh-help-test-printf-man-recording)))
    (with-temp-file malformed-col-recording
      (insert
       (neomacs-esh-help-test-drop-synopsis
        neomacs-esh-help-test-printf-col-recording)))
    (neomacs-esh-help-test-write-executable
     (expand-file-name "printf" bin)
     "#!/bin/sh\nexit 0\n")
    (neomacs-esh-help-test-write-executable
     man-program
     (concat
      "#!/bin/sh\n"
      "printf 'LANG=<%s> ARGC=<%s> ARGV=<%s>\\n' \"$LANG\" \"$#\" \"$*\" >> \"$NEOMACS_ESH_HELP_MAN_LOG\"\n"
      "if [ \"$#\" -ne 1 ]; then\n"
      "  printf 'man ARGC=<%s> ARGV=<%s>\\n' \"$#\" \"$*\" >> \"$NEOMACS_ESH_HELP_MISS_LOG\"\n"
      "  printf '%s\\n' 'UNRECORDED MAN REQUEST' >&2\n"
      "  exit 97\n"
      "fi\n"
      "case \"$1\" in\n"
      "  printf) cat \"$NEOMACS_ESH_HELP_MAN_RECORDING\" ;;\n"
      "  missing-tool) printf '%s\\n' 'No manual entry for missing-tool' >&2; exit 16 ;;\n"
      "  malformed-tool) cat \"$NEOMACS_ESH_HELP_MALFORMED_MAN_RECORDING\" ;;\n"
      "  *) printf 'man ARGC=<%s> ARGV=<%s>\\n' \"$#\" \"$*\" >> \"$NEOMACS_ESH_HELP_MISS_LOG\"; printf '%s\\n' 'UNRECORDED MAN REQUEST' >&2; exit 97 ;;\n"
      "esac\n"))
    (neomacs-esh-help-test-write-executable
     (expand-file-name "col" bin)
     (concat
      "#!/bin/sh\n"
      "printf 'ARGC=<%s> ARGV=<%s>\\n' \"$#\" \"$*\" >> \"$NEOMACS_ESH_HELP_COL_LOG\"\n"
      "input=\"$NEOMACS_ESH_HELP_COL_INPUT_BASE.$$\"\n"
      "trap 'rm -f \"$input\"' EXIT HUP INT TERM\n"
      "cat > \"$input\"\n"
      "if [ \"$#\" -eq 1 ] && [ \"$1\" = '-b' ]; then\n"
      "  if cmp -s \"$NEOMACS_ESH_HELP_MAN_RECORDING\" \"$input\"; then\n"
      "    cat \"$NEOMACS_ESH_HELP_COL_RECORDING\"\n"
      "    exit 0\n"
      "  elif [ ! -s \"$input\" ]; then\n"
      "    exit 0\n"
      "  elif cmp -s \"$NEOMACS_ESH_HELP_MALFORMED_MAN_RECORDING\" \"$input\"; then\n"
      "    cat \"$NEOMACS_ESH_HELP_MALFORMED_COL_RECORDING\"\n"
      "    exit 0\n"
      "  fi\n"
      "fi\n"
      "printf 'col ARGC=<%s> ARGV=<%s>\\n' \"$#\" \"$*\" >> \"$NEOMACS_ESH_HELP_MISS_LOG\"\n"
      "printf '%s\\n' 'UNRECORDED COL REQUEST' >&2\n"
      "exit 98\n"))
    (setq exec-path (cons bin exec-path))
    (setenv "PATH" (concat bin path-separator (getenv "PATH")))
    (setenv "NEOMACS_ESH_HELP_MAN_RECORDING" recording)
    (setenv "NEOMACS_ESH_HELP_COL_RECORDING" col-recording)
    (setenv "NEOMACS_ESH_HELP_MALFORMED_MAN_RECORDING"
            malformed-recording)
    (setenv "NEOMACS_ESH_HELP_MALFORMED_COL_RECORDING"
            malformed-col-recording)
    (setenv "NEOMACS_ESH_HELP_MAN_LOG" man-log)
    (setenv "NEOMACS_ESH_HELP_COL_LOG" col-log)
    (setenv "NEOMACS_ESH_HELP_COL_INPUT_BASE" col-input-base)
    (setenv "NEOMACS_ESH_HELP_MISS_LOG" miss-log)
    (setq manual-program man-program)
    (eshell-set-path exec-path)
    (list
     :man-log man-log :col-log col-log
     :miss-log miss-log
     :malformed-fixture
     (list
      :source
      "man-db 2.13.1/coreutils 9.8 printf(1), util-linux col 2.41.4 recordings"
      :transformation 'delete-synopsis-section
      :raw-sha256
      (secure-hash 'sha256
                   (neomacs-esh-help-test-drop-synopsis
                    neomacs-esh-help-test-printf-man-recording))
      :post-col-sha256
      (secure-hash 'sha256
                   (neomacs-esh-help-test-drop-synopsis
                    neomacs-esh-help-test-printf-col-recording))))))

(defun neomacs-esh-help-test-wait-for-man (topic)
  "Wait conditionally for GNU Man to finish TOPIC and return its buffer."
  (let* ((name (format "*Man %s*" topic))
         (buffer (get-buffer name))
         (deadline (+ (float-time) 10.0)))
    (unless buffer
      (error "Man did not create %s" name))
    (while (and (< (float-time) deadline)
                (with-current-buffer buffer
                  (or mode-line-process (null Man-page-list))))
      (accept-process-output (get-buffer-process buffer) 0.05))
    (with-current-buffer buffer
      (when (or mode-line-process (null Man-page-list))
        (error "Man did not finish cooking %s" topic)))
    buffer))

(defun neomacs-esh-help-test-buffer-ui-state (buffer)
  "Return BUFFER's shared mode, editing, point, and window state."
  (with-current-buffer buffer
    (list :mode major-mode
          :read-only buffer-read-only
          :modified (buffer-modified-p)
          :point (point)
          :visible (not (null (get-buffer-window buffer)))
          :selected (eq (window-buffer (selected-window)) buffer))))

(defun neomacs-esh-help-test-buffer-process-state (buffer)
  "Return BUFFER's live or completed process state."
  (when-let* ((process (get-buffer-process buffer)))
    (list (process-status process) (process-exit-status process))))

(defun neomacs-esh-help-test-buffer-state (buffer)
  "Return strict visible state for BUFFER."
  (with-current-buffer buffer
    (append
     (list :name (buffer-name)
           :text (buffer-substring-no-properties (point-min) (point-max)))
     (neomacs-esh-help-test-buffer-ui-state buffer)
     (list
      :buttons
      (let ((position (point-min)) button labels)
        (while (setq button (next-button position))
          (push (substring-no-properties (button-label button)) labels)
          (setq position (button-end button)))
        (nreverse labels))
      :process (neomacs-esh-help-test-buffer-process-state buffer)))))

(defun neomacs-esh-help-test-buffer-digest-state (buffer)
  "Return strict compact content and UI state for BUFFER."
  (with-current-buffer buffer
    (let ((text (buffer-substring-no-properties (point-min) (point-max))))
      (append
       (list :name (buffer-name)
             :characters (length text)
             :sha256 (secure-hash 'sha256 text)
             :prefix (substring text 0 (min 180 (length text)))
             :suffix (substring text (max 0 (- (length text) 180))))
       (neomacs-esh-help-test-buffer-ui-state buffer)
       (list :process
             (neomacs-esh-help-test-buffer-process-state buffer))))))

(defmacro neomacs-esh-help-test-with-sandbox (case-name &rest body)
  "Run BODY in an isolated environment named CASE-NAME."
  (declare (indent 1) (debug t))
  `(let* ((root (neomacs-esh-help-test-root ,case-name))
          (default-directory root)
          (eshell-directory-name (expand-file-name "state/" root))
          (eshell-history-file-name nil)
          (eshell-last-dir-ring-file-name nil)
          (eshell-aliases-file nil)
          (eshell-login-script nil)
          (eshell-rc-script nil)
          (eshell-banner-message "")
          (eshell-prompt-function (lambda () "OPS> "))
          (eshell-prompt-regexp "^OPS> ")
          (eshell-mode-hook nil)
          (esh-help-man-cache (make-hash-table :test #'equal))
          (manual-program "man")
          (Man-topic-history nil)
          (Man-mode-hook nil)
          (Man-width 80)
          (enable-dir-local-variables nil)
          (process-environment (copy-sequence process-environment))
          (exec-path (copy-sequence exec-path))
          (buffer-baseline (buffer-list))
          (process-baseline (process-list))
          (timer-baseline (copy-sequence timer-list))
          (idle-timer-baseline (copy-sequence timer-idle-list)))
     (make-directory root t)
     (unwind-protect
         (progn ,@body)
       ;; Every Esh Help case is a fresh editor process because real Eshell
       ;; initialization installs global control paths.  Still tear down every
       ;; process, timer, buffer, and filesystem object owned by the case so an
       ;; early assertion or process failure cannot race sandbox deletion.
       (dolist (process
                (seq-difference (process-list) process-baseline #'eq))
         (condition-case nil
             (progn
               (set-process-query-on-exit-flag process nil)
               (set-process-sentinel process nil)
               (set-process-filter process nil)
               (when (process-live-p process)
                 (delete-process process)))
           (error nil)))
       (dolist (timer
                (seq-difference timer-idle-list idle-timer-baseline #'eq))
         (when (timerp timer)
           (cancel-timer timer)))
       (dolist (timer (seq-difference timer-list timer-baseline #'eq))
         (when (timerp timer)
           (cancel-timer timer)))
       (dolist (buffer
                (seq-difference (buffer-list) buffer-baseline #'eq))
         (when (buffer-live-p buffer)
           (condition-case nil
               (kill-buffer buffer)
             (error nil))))
       (when (file-directory-p root)
         (delete-directory root t)))))

(defmacro neomacs-esh-help-test-with-eshell-buffer (&rest body)
  "Run BODY in a selected disposable Eshell buffer.

Every caller runs in a fresh editor process, so the fixture deliberately owns
the current-buffer transition.  Restoring an oracle implementation buffer
after asynchronous Man display is not a package behavior and that buffer may
legitimately have been deleted by the command loop."
  (declare (indent 0) (debug t))
  `(let ((session
          (generate-new-buffer " *neomacs-esh-help-eshell*")))
     (set-buffer session)
     (eshell-mode)
     (switch-to-buffer session)
     ,@body))

(defmacro neomacs-esh-help-test-with-eshell (case-name &rest body)
  "Run BODY in an isolated configured Eshell session named CASE-NAME."
  (declare (indent 1) (debug t))
  `(neomacs-esh-help-test-with-sandbox ,case-name
     (setup-esh-help-eldoc)
     (neomacs-esh-help-test-with-eshell-buffer
       ,@body)))

(defmacro neomacs-esh-help-test-with-man-eshell (case-name &rest body)
  "Run BODY with deterministic Man peers exported before Eshell starts.

Eshell gives each session a buffer-local copy of `process-environment'.  Real
GNU Man starts its process from a separate Man buffer, so fixture variables
must be present in the inherited default environment before Eshell makes that
copy.  BODY can inspect the lexically visible `peers' manifest."
  (declare (indent 1) (debug t))
  `(neomacs-esh-help-test-with-sandbox ,case-name
     (let ((peers (neomacs-esh-help-test-install-man-peers root)))
       (setenv "LANG" "fr_FR.UTF-8")
       (setup-esh-help-eldoc)
       (neomacs-esh-help-test-with-eshell-buffer
         ,@body))))
"####;

fn esh_help_oracle() -> CachedMelpaOracle {
    let recording = format!("{PRINTF_MAN_RECORDING:?}");
    let col_recording = format!("{PRINTF_COL_RECORDING:?}");
    let prelude = format!(
        "(defconst neomacs-esh-help-test-printf-man-recording {recording})\n\
         (defconst neomacs-esh-help-test-printf-col-recording {col_recording})\n\
         {ESH_HELP_TEST_PRELUDE}"
    );
    CachedMelpaOracle::new(ESH_HELP_MELPA_PIN, "esh-help.el")
        .expect("prepare exact shallow Esh Help source below ./tmp")
        .with_prelude(prelude)
        .with_timeout(ESH_HELP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Esh Help parity test")
        .into()
}

fn assert_esh_help_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        esh_help_oracle(),
        &current_test_name(),
        "esh_help_parity",
        cases,
    );
}

#[test]
fn esh_help_package_batch() {
    assert_esh_help_batch(&workflows::workflow_batch_cases());
}
