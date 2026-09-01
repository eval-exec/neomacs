use std::time::Duration;

use crate::{AC_INF_RUBY_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_INF_RUBY_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ac-inf-ruby is an auto-complete source that asks a live Ruby REPL what to
/// complete, so every workflow runs a real `run-ruby' session in a
/// window-displayed buffer and drives it with real keys.
///
/// The REPL is the one true external boundary.  `ac-inf-ruby-test-irb' is a
/// small shell program speaking the protocol inf-ruby expects: it prints an
/// `irb(main):NNN:0> ' prompt inf-ruby recognises as top level, a
/// `irb(main):NNN:1* ' continuation prompt while a block is open, and answers
/// the completion snippet inf-ruby sends by extracting the expression from its
/// `.call("EXPR", "LINE")' tail and printing every entry of a real completion
/// table that starts with it, followed by the evaluation result and the next
/// prompt -- the shape `inf-ruby-completions' parses.  It records every line it
/// receives, so the exact request the package makes is observable.  inf-ruby
/// keeps doing its own real process handling, filter swapping and parsing, and
/// auto-complete keeps building real menus.
const AC_INF_RUBY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'comint)

(defun ac-inf-ruby-test-path (name)
  "Return the absolute sandbox path of NAME."
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ac-inf-ruby-test-write (name text)
  "Write TEXT to sandbox file NAME and return its path."
  (let ((path (ac-inf-ruby-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

;; What the REPL's completer knows, in the order it reports matches.
(defconst ac-inf-ruby-test-completions
  "Str\nString\nStringIO\nStruct\nstr.to_s\nstr.to_str\nstr.to_sym\nstr.size\n")

(defconst ac-inf-ruby-test-irb "#!/bin/sh
printf 'irb(main):001:0> '
count=1
level=0
while IFS= read -r line; do
  printf '%s\\n' \"$line\" >> \"$ACIR_LOG\"
  count=$((count + 1))
  case \"$line\" in
    *'.call('*)
      expr=$(printf '%s' \"$line\" | sed 's/.*}\\.call(\"//; s/\", \".*//')
      printf 'expr=%s\\n' \"$expr\" >> \"$ACIR_LOG\"
      grep -E \"^$(printf '%s' \"$expr\" | sed 's/[.[\\*^$]/\\\\&/g')\" \"$ACIR_TABLE\" || true
      printf '=> nil\\n' ;;
    def\ *|*do) level=1; printf '' ;;
    end) level=0; printf '=> :done\\n' ;;
    *) printf '=> nil\\n' ;;
  esac
  if [ \"$level\" = 1 ]; then
    printf 'irb(main):%03d:1* ' \"$count\"
  else
    printf 'irb(main):%03d:0> ' \"$count\"
  fi
done
")

(defun ac-inf-ruby-test-install-irb ()
  "Install the recording REPL and its completion table, return its path."
  (ac-inf-ruby-test-write "completions.txt" ac-inf-ruby-test-completions)
  (setenv "ACIR_LOG" (ac-inf-ruby-test-path "irb.log"))
  (setenv "ACIR_TABLE" (ac-inf-ruby-test-path "completions.txt"))
  (let ((path (ac-inf-ruby-test-write "bin/irb" ac-inf-ruby-test-irb)))
    (set-file-modes path #o755)
    path))

(defun ac-inf-ruby-test-last-line ()
  "Return the last line of the REPL buffer, prompt included.
`line-beginning-position' stops at comint's prompt field, so field motion
has to be inhibited to see the prompt itself."
  (let ((inhibit-field-text-motion t))
    (save-excursion
      (goto-char (point-max))
      (buffer-substring-no-properties (line-beginning-position) (point-max)))))

(defun ac-inf-ruby-test-wait-for-prompt ()
  "Wait until the REPL has printed a prompt, then return that line."
  (let ((process (get-buffer-process (current-buffer))))
    (cl-loop repeat 200
             until (string-match inf-ruby-prompt-pattern
                                 (ac-inf-ruby-test-last-line))
             do (accept-process-output process 0.05))
    (ac-inf-ruby-test-last-line)))

(defun ac-inf-ruby-test-start-repl ()
  "Run the recording REPL, display it, and wait for its first prompt."
  (let ((command (ac-inf-ruby-test-install-irb)))
    (save-window-excursion (run-ruby command "ruby"))
    (set-buffer "*ruby*")
    (set-window-buffer (selected-window) (current-buffer))
    (set-process-query-on-exit-flag (get-buffer-process (current-buffer)) nil)
    (ac-inf-ruby-test-wait-for-prompt)
    (current-buffer)))

(defun ac-inf-ruby-test-stop-repl ()
  "Kill the REPL process and wait until it is really gone."
  (let ((process (get-buffer-process (current-buffer))))
    (when process
      (kill-process process)
      (cl-loop repeat 200
               until (not (process-live-p process))
               do (accept-process-output nil 0.05)))))

(defmacro ac-inf-ruby-test-with-repl (&rest body)
  "Run BODY in a live REPL buffer, then shut the session down."
  `(let ((existing (buffer-list)))
     (unwind-protect
         (progn
           (ac-inf-ruby-test-start-repl)
           ,@body)
       (dolist (buffer (buffer-list))
         (unless (memq buffer existing)
           (with-current-buffer buffer
             (ignore-errors (ac-abort))
             (ac-inf-ruby-test-stop-repl)
             (set-buffer-modified-p nil))
           (kill-buffer buffer))))))

(defun ac-inf-ruby-test-submit (text)
  "Type TEXT at the REPL prompt, send it, and return the new prompt line."
  (goto-char (point-max))
  (insert text)
  (comint-send-input)
  (ac-inf-ruby-test-wait-for-prompt))

(defun ac-inf-ruby-test-requests ()
  "Return every expression the package asked the REPL to complete."
  (let ((path (ac-inf-ruby-test-path "irb.log")))
    (if (file-exists-p path)
        (let (requests)
          (with-temp-buffer
            (insert-file-contents path)
            (dolist (line (split-string (buffer-string) "\n" t))
              (when (string-prefix-p "expr=" line)
                (push (substring line 5) requests))))
          (nreverse requests))
      'nothing-recorded)))

(defun ac-inf-ruby-test-menu ()
  "Report every candidate auto-complete built, in menu order."
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (popup-item-symbol candidate)))
          ac-candidates))

(defun ac-inf-ruby-test-session ()
  "Report the completion state auto-complete is holding."
  (list :prefix ac-prefix
        :prefix-start (and ac-point (- ac-point (point-min)))
        :common (and (stringp ac-common-part)
                     (substring-no-properties ac-common-part))
        :menu-live (and (ac-menu-live-p) t)
        :selected (and (ac-menu-live-p)
                       (substring-no-properties (popup-selected-item ac-menu)))))

(defun ac-inf-ruby-test-buffer-state ()
  "Report the editing state the user can see."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (- (point) (point-min))
        :mode major-mode
        :top-level inf-ruby-at-top-level-prompt-p
        :auto-complete auto-complete-mode
        :sources ac-sources))
"##;

fn ac_inf_ruby_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_INF_RUBY_MELPA_PIN, "ac-inf-ruby.el")
        .expect("prepare pinned ac-inf-ruby source below ./tmp")
        .with_prelude(AC_INF_RUBY_TEST_PRELUDE)
        .with_timeout(AC_INF_RUBY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-inf-ruby parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_inf_ruby_parity` cases (2a).
pub(crate) fn assert_ac_inf_ruby_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_inf_ruby_oracle(), &name, "ac_inf_ruby_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_inf_ruby_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_inf_ruby_batch(&cases);
}

// END generated package batch tests
