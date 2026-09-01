//! Practical parity for Rustic's public editing and Cargo workflows.
//!
//! Cargo and rustfmt are owned external boundaries. A closed executable
//! fixture validates every argv, cwd, environment, and formatter input while
//! Rustic's mode setup, command construction, compilation parsing, navigation,
//! buffer edits, process sentinels, and cleanup remain real.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, RUSTIC_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'compile)
(require 'rustic)
(set-window-configuration (current-window-configuration))

(defvar rustic419-test-case-index 0)
(defvar rustic419-test-root nil)
(defvar rustic419-test-root-owned nil)
(defvar rustic419-test-prompt-plan nil)
(defvar rustic419-test-prompt-ledger nil)

(defconst rustic419-test-upstream-tree
  "886c5d478c0ff4223f290f4aa13c36bb0d1bbb14")
(defconst rustic419-test-upstream-main-sha
  "e314ef1cf51c08913d5b47d5c4e3dad1a7b20e86b8aac245e1807218cbfcc913")
(defconst rustic419-test-recorded-tools
  '((:name cargo
     :version "cargo 1.96.1 (356927216 2026-06-26)"
     :sha256 "861a0e077a3810cd82a7e20e955eccbe7e5791fe098b3f433766897deb5433a1")
    (:name rustfmt
     :version "rustfmt 1.9.0-stable (31fca3adb2 2026-06-26)"
     :sha256 "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10")))
(defconst rustic419-test-installed-manifest
  '(("rustic-babel.el" . "b4dbc82b1d04c47e8fa6d88cfb13fcf35b9902f50e55c1462e09233157b8a610")
    ("rustic-cargo.el" . "623762940388878ddb80d05ba1c3a98373dd6bf7a32b4006f8ba001809084019")
    ("rustic-clippy.el" . "4e52fce8b2df66599eb63d3466cef3d93e074b37cde4be890f4015e31891f8e6")
    ("rustic-comint.el" . "e7cedb3a3739955cac28edebe86e1b8f2f8fc822dab7e3e47baa44da98108f0f")
    ("rustic-compile.el" . "dbb3869f17d2d0ef7a6bef09e8aef1bddfdd99c634d912e0839f69a09c4ae0f2")
    ("rustic-doc.el" . "4fc86752bb8a59c27122583d7e5e5986bd9748c964a4227d185ce4d0facee890")
    ("rustic-expand.el" . "d830f982d723a4524f0a39e34c981d9fc127fc8136ca1eb77465aadee26aed99")
    ("rustic-flycheck.el" . "fd03896b895960dad9cb781b3a90e66cd8001183351c1f4a738e5329a407fbdf")
    ("rustic-interaction.el" . "b36403dadfd9a2c08cb2cc85b9a47e6ff179c041895f80ab29c9067ff99a62dd")
    ("rustic-lsp.el" . "8fd736411cfa46dcc8c1296042a28ba1570949d4c982bc47f0580e01dc2a3c34")
    ("rustic-pkg.el" . "e7f966a59e1e777104b87056a0f94944302e8111756fe2dd7ef83569a73e3aec")
    ("rustic-playground.el" . "f161ba9099029b964ef9f33743a565e8b324e4918463075c8d6f410ecc17b80b")
    ("rustic-popup.el" . "4873652dc564904d93b52751bc0b985579ff68e30f25dcebc92c1c71b8ea025d")
    ("rustic-rustfix.el" . "ce28a6be920b2defa6e15c3da57589c36fae64648f139bb3efd3cf8291495743")
    ("rustic-rustfmt.el" . "b8083fd1671fa1bd4a3ff9949ce369c2ff75611c0c5aa80191f4ea0ac6ab4acd")
    ("rustic-spellcheck.el" . "5fa32c8d199f1c9904102ef6f1a45c22b7a6a9eda1d3a456f4efe6c46ded838b")
    ("rustic.el" . "6041a10aa0378620fb76a618d54b6f08d884765575695b392e601b23ad525e6f")))

(defun rustic419-test-file-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun rustic419-test-read-file-if-exists (file)
  (when (file-exists-p file)
    (with-temp-buffer
      (insert-file-contents file)
      (buffer-string))))

(defun rustic419-test-installed-manifest ()
  (let* ((located (locate-library "rustic.el"))
         (_ (unless (and (file-regular-p located) (not (file-symlink-p located)))
              (error "Unsafe installed Rustic main source: %S" located)))
         (directory (file-name-directory (file-truename located))))
    (sort
     (mapcar
      (lambda (file)
        (unless (and (file-regular-p file) (not (file-symlink-p file))
                     (equal (file-name-directory file) directory))
          (error "Unsafe installed Rustic source: %S" file))
        (cons (file-name-nondirectory file) (rustic419-test-file-sha file)))
      (seq-remove
       (lambda (file) (string-suffix-p "-autoloads.el" file))
       (directory-files directory t "\\`rustic\\(?:-[^.]+\\)?\\.el\\'")))
     (lambda (left right) (string< (car left) (car right))))))

(defun rustic419-test-source-state ()
  (let ((manifest (rustic419-test-installed-manifest))
        (advice
         (list
          (cons 'save-some-buffers
                (and (advice-member-p #'rustic-save-some-buffers-advice
                                      'save-some-buffers) t))
          (cons 'compile-goto-error
                (and (advice-member-p #'rustic-compile-goto-error-hook
                                      'compile-goto-error) t)))))
    (unless (and (equal manifest rustic419-test-installed-manifest)
                 (equal advice '((save-some-buffers . t)
                                 (compile-goto-error . t))))
      (error "Rustic source/advice mismatch: %S %S" manifest advice))
    (list :upstream-tree rustic419-test-upstream-tree
          :upstream-main-sha rustic419-test-upstream-main-sha
          :recorded-tools rustic419-test-recorded-tools
          :manifest manifest
          :version
          (package-version-join
           (package-desc-version (cadr (assq 'rustic package-alist))))
          :feature (featurep 'rustic)
          :advice advice)))

(defun rustic419-test-write (relative contents &optional executable)
  (let ((file (expand-file-name relative rustic419-test-root)))
    (unless (and rustic419-test-root-owned
                 (file-in-directory-p file rustic419-test-root))
      (error "Refusing Rustic write outside owned root: %S" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert contents)))
    (when executable (set-file-modes file #o755))
    file))

(defconst rustic419-test-lib-source
  "pub fn add(left: i32, right: i32) -> i32 {\n    left + right\n}\n\n#[cfg(test)]\nmod unit {\n    use super::*;\n\n    #[test]\n    fn works_界() {\n        assert_eq!(add(2, 3), 5);\n    }\n}\n")

(defconst rustic419-test-main-unformatted
  "fn main(){println!(\"café 界\");}\n")

(defconst rustic419-test-main-invalid
  "fn main(){println!(\"café 界\");")

(defconst rustic419-test-main-formatted
  "fn main() {\n    println!(\"café 界\");\n}\n")

(defconst rustic419-test-cargo-script
  "#!/bin/sh
set -eu
index=0
if [ -f \"$RUSTIC419_CLAIM\" ]; then IFS= read -r index < \"$RUSTIC419_CLAIM\"; fi
next=$((index + 1))
expected=
line_number=0
while IFS= read -r line || [ -n \"$line\" ]; do
  line_number=$((line_number + 1))
  if [ \"$line_number\" -eq \"$next\" ]; then expected=$line; fi
done < \"$RUSTIC419_PLAN\"
joined=
separator=
for argument in \"$@\"; do joined=\"${joined}${separator}${argument}\"; separator='|'; done
if [ -z \"$expected\" ] || [ \"$joined\" != \"$expected\" ]; then
  printf 'unexpected cargo[%s]=%s expected=%s\\n' \"$next\" \"$joined\" \"$expected\" >&2
  exit 86
fi
if [ \"$(pwd -P)/\" != \"$RUSTIC419_PROJECT\" ]; then
  printf 'unexpected cargo cwd=%s expected=%s\\n' \"$(pwd -P)/\" \"$RUSTIC419_PROJECT\" >&2
  exit 86
fi
printf '%s\\n' \"$next\" > \"$RUSTIC419_CLAIM\"
printf '%s\\t%s\\t%s\\n' \"$joined\" \"${RUST_BACKTRACE-}\" \"${RUSTFLAGS-}\" >> \"$RUSTIC419_LOG\"
case \"${1-}\" in
  test)
    case \"$joined\" in
      'test|unit::works_界') name='unit::works_界' ;;
      'test|--all-targets|--all-features|--|--nocapture|works_界') name='unit::works_界' ;;
      *) printf 'unrecorded cargo test selection: %s\\n' \"$joined\" >&2; exit 86 ;;
    esac
    printf 'running 1 test\\ntest %s ... ok\\n\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\\n' \"$name\" ;;
  build)
    printf '   Compiling demo-界 v0.1.0 (%s)\\n' \"${RUSTIC419_PROJECT%/}\"
    printf 'error[E0425]: cannot find function `missing_界` in this scope\\n'
    printf ' --> src/lib.rs:2:5\\n'
    printf '  |\\n2 |     missing_界();\\n  |     ^^^^^^^^^^^ not found in this scope\\n'
    printf '\\nFor more information about this error, try `rustc --explain E0425`.\\n'
    printf 'error: could not compile `demo-界` (lib) due to 1 previous error\\n'
    exit 101 ;;
  check)
    printf '    Checking demo-界 v0.1.0 (%s)\\n' \"${RUSTIC419_PROJECT%/}\"
    printf '    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.04s\\n' ;;
  *) printf 'unhandled cargo command: %s\\n' \"$joined\" >&2; exit 86 ;;
esac
")

(defconst rustic419-test-rustfmt-script
  "#!/bin/sh
set -eu
index=0
if [ -f \"$RUSTIC419_CLAIM\" ]; then IFS= read -r index < \"$RUSTIC419_CLAIM\"; fi
next=$((index + 1))
expected=
line_number=0
while IFS= read -r line || [ -n \"$line\" ]; do
  line_number=$((line_number + 1))
  if [ \"$line_number\" -eq \"$next\" ]; then expected=$line; fi
done < \"$RUSTIC419_PLAN\"
joined=
separator=
for argument in \"$@\"; do joined=\"${joined}${separator}${argument}\"; separator='|'; done
case \"$expected\" in
  fail:*) mode=fail; expected_args=${expected#fail:} ;;
  pass:*) mode=pass; expected_args=${expected#pass:} ;;
  *) printf 'unexpected rustfmt plan[%s]=%s\\n' \"$next\" \"$expected\" >&2; exit 86 ;;
esac
if [ \"$joined\" != \"$expected_args\" ] || [ \"$(pwd -P)/\" != \"${RUSTIC419_PROJECT}src/\" ]; then
  printf 'unexpected rustfmt[%s]=%s cwd=%s expected=%ssrc/%s\\n' \"$next\" \"$joined\" \"$(pwd -P)/\" \"$RUSTIC419_PROJECT\" \"$expected_args\" >&2
  exit 86
fi
cat > \"$RUSTIC419_STDIN\"
if [ \"$mode\" = fail ]; then
  expected_stdin=$RUSTIC419_EXPECTED_STDIN_FAIL
else
  expected_stdin=$RUSTIC419_EXPECTED_STDIN_PASS
fi
if ! cmp -s \"$RUSTIC419_STDIN\" \"$expected_stdin\"; then
  printf 'unexpected rustfmt stdin\\n' >&2
  exit 86
fi
printf '%s\\n' \"$next\" > \"$RUSTIC419_CLAIM\"
printf '%s\\t%s\\n' \"$mode:$joined\" \"${RUST_BACKTRACE-}\" >> \"$RUSTIC419_LOG\"
if [ \"$mode\" = fail ]; then
  printf 'error: this file contains an unclosed delimiter\\n'
  printf ' --> <stdin>:1:31\\n'
  printf '  |\\n1 | fn main(){println!(\"café 界\");\\n'
  printf '  |          - unclosed delimiter ^\\n'
  exit 1
fi
printf 'fn main() {\\n    println!(\"café 界\");\\n}\\n'
")

(defun rustic419-test-project ()
  (rustic419-test-write
   "Cargo.toml"
   "[package]\nname = \"demo-界\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
  (rustic419-test-write "src/lib.rs" rustic419-test-lib-source)
  (rustic419-test-write "src/main.rs" rustic419-test-main-unformatted)
  rustic419-test-root)

(defun rustic419-test-install-tools (plan)
  (let ((cargo (rustic419-test-write "bin/cargo" rustic419-test-cargo-script t))
        (rustfmt (rustic419-test-write "bin/rustfmt" rustic419-test-rustfmt-script t)))
    (dolist (entry `((,cargo . ,rustic419-test-cargo-script)
                     (,rustfmt . ,rustic419-test-rustfmt-script)))
      (unless (and (file-regular-p (car entry))
                   (not (file-symlink-p (car entry)))
                   (/= 0 (logand (file-modes (car entry)) #o111))
                   (equal (with-temp-buffer
                            (insert-file-contents (car entry))
                            (buffer-string))
                          (cdr entry)))
        (error "Rustic executable fixture differs from source: %S" (car entry))))
    (rustic419-test-write "plan" (concat (string-join plan "\n") "\n"))
    ;; `rustic-format-start-process' appends one newline to BUFFER's exact
    ;; contents before closing stdin.
    (rustic419-test-write "expected-stdin-fail"
                          (concat rustic419-test-main-invalid "\n"))
    (rustic419-test-write "expected-stdin-pass"
                          (concat rustic419-test-main-unformatted "\n"))
    (setenv "RUSTIC419_PROJECT" rustic419-test-root)
    (setenv "RUSTIC419_PLAN" (expand-file-name "plan" rustic419-test-root))
    (setenv "RUSTIC419_CLAIM" (expand-file-name "claim" rustic419-test-root))
    (setenv "RUSTIC419_LOG" (expand-file-name "calls.log" rustic419-test-root))
    (setenv "RUSTIC419_STDIN" (expand-file-name "stdin" rustic419-test-root))
    (setenv "RUSTIC419_EXPECTED_STDIN_FAIL"
            (expand-file-name "expected-stdin-fail" rustic419-test-root))
    (setenv "RUSTIC419_EXPECTED_STDIN_PASS"
            (expand-file-name "expected-stdin-pass" rustic419-test-root))
    (setq rustic-cargo-bin cargo rustic-rustfmt-bin rustfmt)
    (list cargo rustfmt)))

(defun rustic419-test-project-manifest ()
  (sort
   (mapcar
    (lambda (file)
      (unless (and (file-regular-p file) (not (file-symlink-p file)))
        (error "Unexpected Rustic fixture entry: %S" file))
      (cons (file-relative-name file rustic419-test-root)
            (rustic419-test-file-sha file)))
    (list (expand-file-name "Cargo.toml" rustic419-test-root)
          (expand-file-name "bin/cargo" rustic419-test-root)
          (expand-file-name "bin/rustfmt" rustic419-test-root)
          (expand-file-name "src/lib.rs" rustic419-test-root)
          (expand-file-name "src/main.rs" rustic419-test-root)))
   (lambda (left right) (string< (car left) (car right)))))

(defun rustic419-test-relative (text)
  (when text
    (replace-regexp-in-string
     (regexp-quote (directory-file-name rustic419-test-root)) "[ROOT]"
     (replace-regexp-in-string (regexp-quote rustic419-test-root)
                               "[ROOT]/" text t t)
     t t)))

(defun rustic419-test-calls ()
  (let ((file (expand-file-name "calls.log" rustic419-test-root)))
    (when (file-exists-p file)
      (with-temp-buffer
        (insert-file-contents file)
        (mapcar
         (lambda (line)
           (let* ((fields (split-string line "\t"))
                  (raw (car fields))
                  (rustfmt
                   (string-match "\\`\\(fail\\|pass\\):\\(.*\\)\\'" raw)))
             (append
              (when rustfmt (list :mode (intern (match-string 1 raw))))
              (list :argv
                    (split-string (if rustfmt (match-string 2 raw) raw) "|")
                    :backtrace (cadr fields)
                    :rustflags (caddr fields)))))
         (split-string (string-trim-right (buffer-string)) "\n" t))))))

(defun rustic419-test-compilation-complete-p (buffer)
  "Non-nil once `compilation-handle-exit' has written BUFFER's last line.
That line is the causal end of the output rather than a guess about it: Emacs
drains a dying process's remaining reads before it runs the sentinel
\(GNU src/process.c:7896-7910), the sentinel is what calls
`compilation-handle-exit', and that function marks the text it writes with a
`compilation-handle-exit' text property (GNU lisp/progmodes/compile.el:2630)."
  (and (buffer-live-p buffer)
       (with-current-buffer buffer
         (and (text-property-not-all (point-min) (point-max)
                                     'compilation-handle-exit nil)
              t))))

(defvar rustic419-test-format-sentinel-processes nil
  "Every rustfmt process whose own sentinel has run.")

(defun rustic419-test-note-format-sentinel (process &rest _)
  "Record that `rustic-format-sentinel' has run for PROCESS."
  (cl-pushnew process rustic419-test-format-sentinel-processes :test #'eq))

(advice-add 'rustic-format-sentinel :after
            #'rustic419-test-note-format-sentinel)

(defun rustic419-test-wait-format (process)
  "Wait until rustic's own rustfmt sentinel has run for PROCESS, or signal.
`*rustfmt*' is a `rustic-compilation-mode' buffer, so it looks like a
compilation buffer -- but `rustic-format-buffer' installs
`rustic-format-sentinel' rather than `compilation-sentinel'
\(rustic-rustfmt.el:311-321), so `compilation-handle-exit' never runs for it
and its marker never appears.  The causal fact here is that rustic's own
sentinel has run, which is what the advice above records; recording it from
the sentinel itself, installed once at prelude time, means there is no window
in which the sentinel could have fired before this helper started watching.
Which gate a pin needs is decided by the sentinel that drives the buffer, not
by the major mode it wears.  See DIVERGENCES.md 144."
  (let ((waited 0))
    (while (and (< waited 1200)
                (not (memq process rustic419-test-format-sentinel-processes)))
      (accept-process-output nil 0.05)
      (setq waited (1+ waited)))
    (unless (memq process rustic419-test-format-sentinel-processes)
      (error "rustic419-test-wait-format: %S's sentinel never ran; the buffer \
records only as much of rustfmt's output as had been read" process))
    (list :status (process-status process) :exit (process-exit-status process))))

(defun rustic419-test-wait (process)
  "Wait until PROCESS's buffer holds all of its compilation output, or signal.
Neither `process-live-p' going nil nor N identical samples of the buffer is
that condition, and both were what this helper used to wait for: a process can
be gone with reads still queued, and identical samples only say the sentinel
has not run YET.  The compilation text this suite pins ends with the
sentinel's own line.  See DIVERGENCES.md entries 133, 140 and 144."
  (let ((buffer (process-buffer process))
        (waited 0))
    (while (and (< waited 1200)
                (not (rustic419-test-compilation-complete-p buffer)))
      (accept-process-output nil 0.05)
      (setq waited (1+ waited)))
    (unless (rustic419-test-compilation-complete-p buffer)
      (error "rustic419-test-wait: %S never reached `compilation-handle-exit'; \
its buffer records only as much of the child's output as had been read"
             process))
    (list :status (process-status process) :exit (process-exit-status process))))

(defun rustic419-test-buffer-state (buffer)
  (with-current-buffer buffer
    (list :name (copy-sequence (buffer-name))
          :file (and buffer-file-name
                     (file-relative-name buffer-file-name rustic419-test-root))
          :mode major-mode
          :modified (buffer-modified-p)
          :point (point)
          :mark (and (mark t) (mark t))
          :text (rustic419-test-relative
                 (buffer-substring-no-properties (point-min) (point-max))))))

(defun rustic419-test-compilation-state (buffer)
  (with-current-buffer buffer
    (list :name (buffer-name)
          :mode major-mode
          :directory (file-relative-name default-directory rustic419-test-root)
          :process (and (get-buffer-process buffer) t)
          :errors (and (boundp 'compilation-num-errors-found)
                       compilation-num-errors-found)
          :warnings (and (boundp 'compilation-num-warnings-found)
                         compilation-num-warnings-found)
          :text
          (rustic419-test-relative
           (replace-regexp-in-string
            "at [A-Z][a-z][a-z] [A-Z][a-z][a-z] +[0-9]+ [0-9:]+"
            "at [TIME]"
            (replace-regexp-in-string
             "duration [0-9.]+ s" "duration [DURATION]"
             (buffer-substring-no-properties (point-min) (point-max))))))))

(defun rustic419-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error (list :error (car condition)
                 :data (copy-tree (cdr condition))
                 :message (rustic419-test-relative
                           (error-message-string condition))))))

(defun rustic419-test-read-from-minibuffer
    (prompt &optional initial keymap read history default inherit)
  (unless rustic419-test-prompt-plan
    (error "Unexpected Rustic minibuffer: %S" prompt))
  (let ((answer (pop rustic419-test-prompt-plan)))
    (push (list :prompt prompt :initial initial :keymap keymap :read read
                :history history :default default :inherit inherit
                :answer answer)
          rustic419-test-prompt-ledger)
    (when (symbolp history)
      (set history (cons answer (symbol-value history))))
    answer))

(defun rustic419-test-forbid-external (operation &rest arguments)
  (error "Unexpected Rustic external boundary: %S %S" operation arguments))

(defun rustic419-test-park-buffer (name)
  (when-let ((buffer (get-buffer name)))
    (let ((original (buffer-name buffer)))
      (with-current-buffer buffer
        (rename-buffer (generate-new-buffer-name (concat " *parked " name "*")) t))
      (cons buffer original))))

(defun rustic419-test-project-buffers ()
  (seq-filter
   (lambda (buffer)
     (when-let ((file (buffer-local-value 'buffer-file-name buffer)))
       (file-in-directory-p file rustic419-test-root)))
   (buffer-list)))

(defun rustic419-test-window-state ()
  (mapcar
   (lambda (window)
     (list :window window
           :selected (eq window (selected-window))
           :buffer (window-buffer window)
           :point (window-point window)
           :start (window-start window)
           :hscroll (window-hscroll window)
           :dedicated (window-dedicated-p window)
           :edges (window-edges window)))
   (window-list nil 'no-minibuffer)))

(defun rustic419-test-run (case-name plan body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (index (setq rustic419-test-case-index (1+ rustic419-test-case-index)))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "rustic-%s-%d" case-name index)
                                       sandbox))))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (buffer-before (current-buffer))
         (selected-window-before (selected-window))
         (menu-bar-mode-before menu-bar-mode)
         (menu-bar-lines-before (frame-parameter nil 'menu-bar-lines))
         (window-before (current-window-configuration))
         (window-state-before (rustic419-test-window-state))
         (next-error-before next-error-last-buffer)
         (source-before (rustic419-test-source-state))
         (workspace-default-before (default-value 'rustic--buffer-workspace))
         (rustic419-test-root root)
         (rustic419-test-root-owned nil)
         (rustic419-test-prompt-plan nil)
         (rustic419-test-prompt-ledger nil)
         (process-environment (copy-sequence process-environment))
         (exec-path (copy-sequence exec-path))
         (compilation-in-progress nil)
         (compilation-start-hook nil)
         (compilation-finish-functions nil)
         (compilation-filter-hook nil)
         (compilation-ask-about-save nil)
         (compilation-auto-jump-to-first-error nil)
         (compilation-scroll-output nil)
         (next-error-last-buffer nil)
         (rust-mode-hook nil)
         (rustic-mode-hook '(rustic-setup-lsp))
         (rustic-before-compilation-hook '(rustic-maybe-format-before-compilation))
         (rustic-lsp-client nil)
         (rustic-lsp-setup-p nil)
         (rustic-format-trigger nil)
         (rustic-format-on-save nil)
         (rustic-cargo-clippy-trigger-fix nil)
         (rustic-compile-backtrace "full")
         (rustic-compile-rustflags "--cfg rustic419")
         (rustic-cargo-build-arguments "")
         (rustic-cargo-check-arguments "--all-targets --all-features")
         (rustic-default-test-arguments "--all-targets --all-features")
         (rustic-test-arguments "")
         (rustic-test-history nil)
         (rustic-save-pos nil)
         (rustic-cargo-use-last-stored-arguments nil)
         (rustic-cargo-test-runner 'cargo)
         (rustic-list-project-buffers-function #'rustic419-test-project-buffers)
         (rustic-compile-display-method #'display-buffer)
         (rustic-format-display-method #'display-buffer)
         (enable-dir-local-variables nil)
         parked result source-after fixture-before fixture-after cleanup-errors)
    (unwind-protect
        (progn
          ;; Keep case geometry deterministic after the Prelude has reconciled
          ;; GNU's one-time batch-frame menu margin.
          (menu-bar-mode -1)
          (unless (and root (file-name-absolute-p root))
            (error "Missing absolute Rustic sandbox root"))
          (when (file-exists-p root)
            (error "Rustic sandbox root exists: %S" root))
          (dolist (name '("*rustic-compilation*" "*cargo-test*" "*rustfmt*"))
            (when-let ((entry (rustic419-test-park-buffer name)))
              (push entry parked)))
          (make-directory root)
          (setq rustic419-test-root-owned t)
          (rustic419-test-project)
          (rustic419-test-install-tools plan)
          (set-default 'rustic--buffer-workspace root)
          (setq fixture-before (rustic419-test-project-manifest))
          (let ((original-start-file-process
                 (symbol-function 'start-file-process))
                (original-start-process (symbol-function 'start-process))
                (original-make-process (symbol-function 'make-process)))
            (cl-letf
                (((symbol-function 'start-file-process)
                  (lambda (name buffer program &rest arguments)
                    (unless (member program (list rustic-cargo-bin rustic-rustfmt-bin))
                      (rustic419-test-forbid-external
                       'start-file-process name buffer program arguments))
                    (apply original-start-file-process name buffer program arguments)))
                 ((symbol-function 'process-file)
                  (lambda (&rest arguments)
                    (apply #'rustic419-test-forbid-external 'process-file arguments)))
                 ((symbol-function 'call-process)
                  (lambda (&rest arguments)
                    (apply #'rustic419-test-forbid-external 'call-process arguments)))
                 ((symbol-function 'call-process-region)
                  (lambda (&rest arguments)
                    (apply #'rustic419-test-forbid-external
                           'call-process-region arguments)))
                 ((symbol-function 'make-process)
                  (lambda (&rest arguments)
                    (let ((command (plist-get arguments :command)))
                      (unless (and (consp command)
                                   (member (car command)
                                           (list rustic-cargo-bin rustic-rustfmt-bin)))
                        (rustic419-test-forbid-external
                         'make-process arguments))
                      (apply original-make-process arguments))))
                 ((symbol-function 'start-process)
                  (lambda (name buffer program &rest arguments)
                    (unless (member program (list rustic-cargo-bin rustic-rustfmt-bin))
                      (rustic419-test-forbid-external
                       'start-process name buffer program arguments))
                    (apply original-start-process name buffer program arguments)))
                 ((symbol-function 'url-retrieve)
                  (lambda (&rest arguments)
                    (apply #'rustic419-test-forbid-external 'url-retrieve arguments)))
                 ((symbol-function 'url-retrieve-synchronously)
                  (lambda (&rest arguments)
                    (apply #'rustic419-test-forbid-external
                           'url-retrieve-synchronously arguments)))
                 ((symbol-function 'read-from-minibuffer)
                  #'rustic419-test-read-from-minibuffer)
                 ((symbol-function 'yes-or-no-p)
                  (lambda (&rest arguments)
                    (apply #'rustic419-test-forbid-external 'yes-or-no-p arguments)))
                 ((symbol-function 'y-or-n-p)
                  (lambda (&rest arguments)
                    (apply #'rustic419-test-forbid-external 'y-or-n-p arguments))))
              (setq result (funcall body root))))
          (when rustic419-test-prompt-plan
            (error "Unused Rustic prompt plan: %S" rustic419-test-prompt-plan))
          (let ((claim (expand-file-name "claim" root))
                (log (expand-file-name "calls.log" root)))
            (if plan
                (unless (and (file-exists-p claim)
                             (= (string-to-number
                                 (string-trim
                                  (with-temp-buffer
                                    (insert-file-contents claim)
                                    (buffer-string))))
                                (length plan)))
                  (error "Rustic external plan was not fully consumed: claim=%S calls=%S stdin=%S result=%S"
                         (rustic419-test-read-file-if-exists claim)
                         (rustic419-test-read-file-if-exists log)
                         (let ((stdin (expand-file-name "stdin" root)))
                           (and (file-exists-p stdin)
                                (rustic419-test-file-sha stdin)))
                         result))
              (when (or (file-exists-p claim) (file-exists-p log))
                (error "Unexpected Rustic external call in no-process case"))))
          (setq source-after (rustic419-test-source-state)
                fixture-after (rustic419-test-project-manifest))
          (unless (equal source-before source-after)
            (error "Rustic installed source changed")))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (or (memq buffer buffers-before) (assq buffer parked))
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda () (kill-buffer buffer)))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'menu-bar
                 (lambda ()
                   (unless (eq menu-bar-mode menu-bar-mode-before)
                     (menu-bar-mode (if menu-bar-mode-before 1 -1)))
                   (set-frame-parameter nil 'menu-bar-lines menu-bar-lines-before)))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (dolist (entry parked)
          (attempt (list 'parked (cdr entry))
                   (lambda ()
                     (unless (buffer-live-p (car entry))
                       (error "Parked Rustic buffer died: %S" (cdr entry)))
                     (with-current-buffer (car entry)
                       (rename-buffer (cdr entry) t)))))
        (set-default 'rustic--buffer-workspace workspace-default-before)
        (setq next-error-last-buffer next-error-before)
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))
        (when rustic419-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (let* ((window-state-after (rustic419-test-window-state))
           (window-restored
            (and (eq (selected-window) selected-window-before)
                 (equal window-state-before window-state-after)))
           (cleanup
           (list :source-unchanged (equal source-before source-after)
                 :fixture-accounted (equal fixture-before fixture-after)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :workspace-default-restored
                 (eq (default-value 'rustic--buffer-workspace)
                     workspace-default-before)
                 :next-error-restored (eq next-error-last-buffer next-error-before)
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :menu-bar-restored
                 (and (eq menu-bar-mode menu-bar-mode-before)
                      (equal (frame-parameter nil 'menu-bar-lines)
                             menu-bar-lines-before))
                 :window-restored window-restored
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "Rustic cleanup failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RUSTIC_MELPA_PIN, "rustic.el")
        .expect("prepare exact Rustic source and dependency closure below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_mode_keys_docstring_and_manifest_navigation() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_mode_keys_docstring_and_manifest_navigation",
        r####"
(rustic419-test-run
 "mode" nil
 (lambda (root)
   (let* ((file (expand-file-name "src/lib.rs" root))
          (buffer (find-file-noselect file)))
     (unwind-protect
         (with-current-buffer buffer
           (rustic-mode)
           (goto-char (point-min))
           (search-forward "pub fn add")
           (beginning-of-line)
           (call-interactively #'rustic-docstring-dwim)
           (let ((mode-state
                  (list :mode major-mode
                        :derived (derived-mode-p 'rust-mode)
                        :crate (file-relative-name (rustic-buffer-crate t) root)
                        :keys
                        (mapcar
                         (lambda (key) (cons key (key-binding (kbd key))))
                         '("C-c C-p" "C-c C-c C-b" "C-c C-c C-c"
                           "C-c C-c C-o" "C-c C-c C-,")))))
             (call-interactively #'rustic-open-dependency-file)
             (let ((manifest-buffer (current-buffer)))
               (list :mode mode-state
                     :doc-buffer (rustic419-test-buffer-state buffer)
                     :manifest (rustic419-test-buffer-state manifest-buffer)))))
       (when (buffer-live-p buffer) (kill-buffer buffer))))))
"####,
        expect![[
            r#"OK (:source (:upstream-tree "886c5d478c0ff4223f290f4aa13c36bb0d1bbb14" :upstream-main-sha "e314ef1cf51c08913d5b47d5c4e3dad1a7b20e86b8aac245e1807218cbfcc913" :recorded-tools ((:name cargo :version "cargo 1.96.1 (356927216 2026-06-26)" :sha256 "861a0e077a3810cd82a7e20e955eccbe7e5791fe098b3f433766897deb5433a1") (:name rustfmt :version "rustfmt 1.9.0-stable (31fca3adb2 2026-06-26)" :sha256 "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10")) :manifest (("rustic-babel.el" . "b4dbc82b1d04c47e8fa6d88cfb13fcf35b9902f50e55c1462e09233157b8a610") ("rustic-cargo.el" . "623762940388878ddb80d05ba1c3a98373dd6bf7a32b4006f8ba001809084019") ("rustic-clippy.el" . "4e52fce8b2df66599eb63d3466cef3d93e074b37cde4be890f4015e31891f8e6") ("rustic-comint.el" . "e7cedb3a3739955cac28edebe86e1b8f2f8fc822dab7e3e47baa44da98108f0f") ("rustic-compile.el" . "dbb3869f17d2d0ef7a6bef09e8aef1bddfdd99c634d912e0839f69a09c4ae0f2") ("rustic-doc.el" . "4fc86752bb8a59c27122583d7e5e5986bd9748c964a4227d185ce4d0facee890") ("rustic-expand.el" . "d830f982d723a4524f0a39e34c981d9fc127fc8136ca1eb77465aadee26aed99") ("rustic-flycheck.el" . "fd03896b895960dad9cb781b3a90e66cd8001183351c1f4a738e5329a407fbdf") ("rustic-interaction.el" . "b36403dadfd9a2c08cb2cc85b9a47e6ff179c041895f80ab29c9067ff99a62dd") ("rustic-lsp.el" . "8fd736411cfa46dcc8c1296042a28ba1570949d4c982bc47f0580e01dc2a3c34") ("rustic-pkg.el" . "e7f966a59e1e777104b87056a0f94944302e8111756fe2dd7ef83569a73e3aec") ("rustic-playground.el" . "f161ba9099029b964ef9f33743a565e8b324e4918463075c8d6f410ecc17b80b") ("rustic-popup.el" . "4873652dc564904d93b52751bc0b985579ff68e30f25dcebc92c1c71b8ea025d") ("rustic-rustfix.el" . "ce28a6be920b2defa6e15c3da57589c36fae64648f139bb3efd3cf8291495743") ("rustic-rustfmt.el" . "b8083fd1671fa1bd4a3ff9949ce369c2ff75611c0c5aa80191f4ea0ac6ab4acd") ("rustic-spellcheck.el" . "5fa32c8d199f1c9904102ef6f1a45c22b7a6a9eda1d3a456f4efe6c46ded838b") ("rustic.el" . "6041a10aa0378620fb76a618d54b6f08d884765575695b392e601b23ad525e6f")) :version "20260407.1712" :feature t :advice ((save-some-buffers . t) (compile-goto-error . t))) :result (:mode (:mode rustic-mode :derived rust-mode :crate "./" :keys (("C-c C-p" . rustic-popup) ("C-c C-c C-b" . rustic-cargo-build) ("C-c C-c C-c" . rustic-cargo-current-test) ("C-c C-c C-o" . rustic-format-buffer) ("C-c C-c C-," . rustic-docstring-dwim))) :doc-buffer (:name "lib.rs" :file "src/lib.rs" :mode rustic-mode :modified t :point 48 :mark nil :text "pub fn add(left: i32, right: i32) -> i32 { /// \n    left + right\n}\n\n#[cfg(test)]\nmod unit {\n    use super::*;\n\n    #[test]\n    fn works_界() {\n        assert_eq!(add(2, 3), 5);\n    }\n}\n") :manifest (:name "Cargo.toml" :file "Cargo.toml" :mode conf-toml-mode :modified nil :point 1 :mark nil :text "[package]\nname = \"demo-界\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :workspace-default-restored t :next-error-restored t :buffer-restored t :menu-bar-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_current_test_derives_the_nested_unicode_target() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_current_test_derives_the_nested_unicode_target",
        r####"
(rustic419-test-run
 "current-test" '("test|unit::works_界")
 (lambda (root)
   (let ((buffer (find-file-noselect (expand-file-name "src/lib.rs" root))))
     (unwind-protect
         (with-current-buffer buffer
           (rustic-mode)
           (goto-char (point-min))
           (search-forward "assert_eq!")
           (call-interactively #'rustic-cargo-current-test)
           (let* ((process (get-process rustic-test-process-name))
                  (completion (rustic419-test-wait process))
                  (output (get-buffer rustic-test-buffer-name)))
             (list :target rustic-test-arguments
                   :history (copy-sequence rustic-test-history)
                   :completion completion
                   :output (rustic419-test-compilation-state output)
                   :calls (rustic419-test-calls))))
       (when (buffer-live-p buffer) (kill-buffer buffer))))))
"####,
        expect![[
            r#"OK (:source (:upstream-tree "886c5d478c0ff4223f290f4aa13c36bb0d1bbb14" :upstream-main-sha "e314ef1cf51c08913d5b47d5c4e3dad1a7b20e86b8aac245e1807218cbfcc913" :recorded-tools ((:name cargo :version "cargo 1.96.1 (356927216 2026-06-26)" :sha256 "861a0e077a3810cd82a7e20e955eccbe7e5791fe098b3f433766897deb5433a1") (:name rustfmt :version "rustfmt 1.9.0-stable (31fca3adb2 2026-06-26)" :sha256 "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10")) :manifest (("rustic-babel.el" . "b4dbc82b1d04c47e8fa6d88cfb13fcf35b9902f50e55c1462e09233157b8a610") ("rustic-cargo.el" . "623762940388878ddb80d05ba1c3a98373dd6bf7a32b4006f8ba001809084019") ("rustic-clippy.el" . "4e52fce8b2df66599eb63d3466cef3d93e074b37cde4be890f4015e31891f8e6") ("rustic-comint.el" . "e7cedb3a3739955cac28edebe86e1b8f2f8fc822dab7e3e47baa44da98108f0f") ("rustic-compile.el" . "dbb3869f17d2d0ef7a6bef09e8aef1bddfdd99c634d912e0839f69a09c4ae0f2") ("rustic-doc.el" . "4fc86752bb8a59c27122583d7e5e5986bd9748c964a4227d185ce4d0facee890") ("rustic-expand.el" . "d830f982d723a4524f0a39e34c981d9fc127fc8136ca1eb77465aadee26aed99") ("rustic-flycheck.el" . "fd03896b895960dad9cb781b3a90e66cd8001183351c1f4a738e5329a407fbdf") ("rustic-interaction.el" . "b36403dadfd9a2c08cb2cc85b9a47e6ff179c041895f80ab29c9067ff99a62dd") ("rustic-lsp.el" . "8fd736411cfa46dcc8c1296042a28ba1570949d4c982bc47f0580e01dc2a3c34") ("rustic-pkg.el" . "e7f966a59e1e777104b87056a0f94944302e8111756fe2dd7ef83569a73e3aec") ("rustic-playground.el" . "f161ba9099029b964ef9f33743a565e8b324e4918463075c8d6f410ecc17b80b") ("rustic-popup.el" . "4873652dc564904d93b52751bc0b985579ff68e30f25dcebc92c1c71b8ea025d") ("rustic-rustfix.el" . "ce28a6be920b2defa6e15c3da57589c36fae64648f139bb3efd3cf8291495743") ("rustic-rustfmt.el" . "b8083fd1671fa1bd4a3ff9949ce369c2ff75611c0c5aa80191f4ea0ac6ab4acd") ("rustic-spellcheck.el" . "5fa32c8d199f1c9904102ef6f1a45c22b7a6a9eda1d3a456f4efe6c46ded838b") ("rustic.el" . "6041a10aa0378620fb76a618d54b6f08d884765575695b392e601b23ad525e6f")) :version "20260407.1712" :feature t :advice ((save-some-buffers . t) (compile-goto-error . t))) :result (:target "unit::works_界" :history ("unit::works_界") :completion (:status exit :exit 0) :output (:name "*cargo-test*" :mode rustic-cargo-test-mode :directory "./" :process nil :errors 0 :warnings 0 :text "[ROOT]/bin/cargo test unit::works_界 \nrunning 1 test\ntest unit::works_界 ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\ncargo-test finished at [TIME], duration [DURATION]\n") :calls ((:argv ("test" "unit::works_界") :backtrace "full" :rustflags "--cfg rustic419"))) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :workspace-default-restored t :next-error-restored t :buffer-restored t :menu-bar-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_build_error_navigates_and_check_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_build_error_navigates_and_check_recovers",
        r####"
(rustic419-test-run
 "build-recovery" '("build" "check|--all-targets|--all-features")
 (lambda (root)
   (let ((buffer (find-file-noselect (expand-file-name "src/lib.rs" root))))
     (unwind-protect
         (with-current-buffer buffer
           (rustic-mode)
           (erase-buffer)
           (insert "pub fn broken() {\n    missing_界();\n}\n")
           (save-buffer)
           (call-interactively #'rustic-cargo-build)
           (let* ((build-process (get-process rustic-compilation-process-name))
                  (build-completion (rustic419-test-wait build-process))
                  (compilation (get-buffer rustic-compilation-buffer-name))
                  (build-state (rustic419-test-compilation-state compilation))
                  destination)
             (with-current-buffer compilation
               (goto-char (point-min))
               (search-forward "--> src/lib.rs" nil t)
               (backward-char (length "src/lib.rs"))
               (unless (get-text-property (point) 'compilation-message)
                 (goto-char (previous-single-property-change
                             (point) 'compilation-message nil (point-min))))
               (call-interactively #'compile-goto-error))
             (let ((window (selected-window)))
               (setq destination
                     (with-current-buffer (window-buffer window)
                       (list :file (file-relative-name buffer-file-name root)
                             :line (line-number-at-pos (window-point window))
                             :column (save-excursion
                                       (goto-char (window-point window))
                                       (current-column))
                             :text (buffer-substring-no-properties
                                    (line-beginning-position)
                                    (line-end-position))))))
             (with-current-buffer buffer
               (erase-buffer)
               (insert rustic419-test-lib-source)
               (save-buffer)
               (call-interactively #'rustic-cargo-check))
             (let* ((check-process (get-process rustic-compilation-process-name))
                    (check-completion (rustic419-test-wait check-process))
                    (check-state
                     (rustic419-test-compilation-state
                      (get-buffer rustic-compilation-buffer-name))))
               (list :build build-completion :build-buffer build-state
                     :destination destination
                     :check check-completion :check-buffer check-state
                     :calls (rustic419-test-calls)))))
       (when (buffer-live-p buffer) (kill-buffer buffer))))))
"####,
        expect![[
            r#"OK (:source (:upstream-tree "886c5d478c0ff4223f290f4aa13c36bb0d1bbb14" :upstream-main-sha "e314ef1cf51c08913d5b47d5c4e3dad1a7b20e86b8aac245e1807218cbfcc913" :recorded-tools ((:name cargo :version "cargo 1.96.1 (356927216 2026-06-26)" :sha256 "861a0e077a3810cd82a7e20e955eccbe7e5791fe098b3f433766897deb5433a1") (:name rustfmt :version "rustfmt 1.9.0-stable (31fca3adb2 2026-06-26)" :sha256 "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10")) :manifest (("rustic-babel.el" . "b4dbc82b1d04c47e8fa6d88cfb13fcf35b9902f50e55c1462e09233157b8a610") ("rustic-cargo.el" . "623762940388878ddb80d05ba1c3a98373dd6bf7a32b4006f8ba001809084019") ("rustic-clippy.el" . "4e52fce8b2df66599eb63d3466cef3d93e074b37cde4be890f4015e31891f8e6") ("rustic-comint.el" . "e7cedb3a3739955cac28edebe86e1b8f2f8fc822dab7e3e47baa44da98108f0f") ("rustic-compile.el" . "dbb3869f17d2d0ef7a6bef09e8aef1bddfdd99c634d912e0839f69a09c4ae0f2") ("rustic-doc.el" . "4fc86752bb8a59c27122583d7e5e5986bd9748c964a4227d185ce4d0facee890") ("rustic-expand.el" . "d830f982d723a4524f0a39e34c981d9fc127fc8136ca1eb77465aadee26aed99") ("rustic-flycheck.el" . "fd03896b895960dad9cb781b3a90e66cd8001183351c1f4a738e5329a407fbdf") ("rustic-interaction.el" . "b36403dadfd9a2c08cb2cc85b9a47e6ff179c041895f80ab29c9067ff99a62dd") ("rustic-lsp.el" . "8fd736411cfa46dcc8c1296042a28ba1570949d4c982bc47f0580e01dc2a3c34") ("rustic-pkg.el" . "e7f966a59e1e777104b87056a0f94944302e8111756fe2dd7ef83569a73e3aec") ("rustic-playground.el" . "f161ba9099029b964ef9f33743a565e8b324e4918463075c8d6f410ecc17b80b") ("rustic-popup.el" . "4873652dc564904d93b52751bc0b985579ff68e30f25dcebc92c1c71b8ea025d") ("rustic-rustfix.el" . "ce28a6be920b2defa6e15c3da57589c36fae64648f139bb3efd3cf8291495743") ("rustic-rustfmt.el" . "b8083fd1671fa1bd4a3ff9949ce369c2ff75611c0c5aa80191f4ea0ac6ab4acd") ("rustic-spellcheck.el" . "5fa32c8d199f1c9904102ef6f1a45c22b7a6a9eda1d3a456f4efe6c46ded838b") ("rustic.el" . "6041a10aa0378620fb76a618d54b6f08d884765575695b392e601b23ad525e6f")) :version "20260407.1712" :feature t :advice ((save-some-buffers . t) (compile-goto-error . t))) :result (:build (:status exit :exit 101) :build-buffer (:name "*rustic-compilation*" :mode rustic-compilation-mode :directory "./" :process nil :errors 1 :warnings 0 :text "[ROOT]/bin/cargo build \n   Compiling demo-界 v0.1.0 ([ROOT])\nerror[E0425]: cannot find function `missing_界` in this scope\n --> src/lib.rs:2:5\n  |\n2 |     missing_界();\n  |     ^^^^^^^^^^^ not found in this scope\n\nFor more information about this error, try `rustc --explain E0425`.\nerror: could not compile `demo-界` (lib) due to 1 previous error\n\nrust-compilation exited abnormally with code 101 at [TIME], duration [DURATION]\n") :destination (:file "src/lib.rs" :line 2 :column 4 :text "    missing_界();") :check (:status exit :exit 0) :check-buffer (:name "*rustic-compilation*" :mode rustic-compilation-mode :directory "./" :process nil :errors 0 :warnings 0 :text "[ROOT]/bin/cargo check --all-targets --all-features \n    Checking demo-界 v0.1.0 ([ROOT])\n    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.04s\n\nrust-compilation finished at [TIME], duration [DURATION]\n") :calls ((:argv ("build") :backtrace "full" :rustflags "--cfg rustic419") (:argv ("check" "--all-targets" "--all-features") :backtrace "full" :rustflags "--cfg rustic419"))) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :workspace-default-restored t :next-error-restored t :buffer-restored t :menu-bar-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_test_prompt_and_rerun_preserve_arguments_and_history() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_test_prompt_and_rerun_preserve_arguments_and_history",
        r####"
(rustic419-test-run
 "test-rerun"
 '("test|--all-targets|--all-features|--|--nocapture|works_界"
   "test|--all-targets|--all-features|--|--nocapture|works_界")
 (lambda (root)
   (let ((buffer (find-file-noselect (expand-file-name "src/lib.rs" root))))
     (unwind-protect
         (with-current-buffer buffer
           (rustic-mode)
           (setq rustic419-test-prompt-plan
                 '("--all-targets --all-features -- --nocapture works_界"))
           (let ((current-prefix-arg '(4)))
             (call-interactively #'rustic-cargo-test))
           (let* ((first-process (get-process rustic-test-process-name))
                  (first-completion (rustic419-test-wait first-process))
                  (test-buffer (get-buffer rustic-test-buffer-name))
                  (first-state (rustic419-test-compilation-state test-buffer)))
             (with-current-buffer test-buffer
               (call-interactively #'rustic-cargo-test-rerun))
             (let* ((second-process (get-process rustic-test-process-name))
                    (second-completion (rustic419-test-wait second-process))
                    (second-state (rustic419-test-compilation-state test-buffer)))
               (list :arguments rustic-test-arguments
                     :history (copy-sequence rustic-test-history)
                     :prompts (nreverse rustic419-test-prompt-ledger)
                     :first first-completion :first-buffer first-state
                     :second second-completion :second-buffer second-state
                     :calls (rustic419-test-calls)))))
       (when (buffer-live-p buffer) (kill-buffer buffer))))))
"####,
        expect![[
            r#"OK (:source (:upstream-tree "886c5d478c0ff4223f290f4aa13c36bb0d1bbb14" :upstream-main-sha "e314ef1cf51c08913d5b47d5c4e3dad1a7b20e86b8aac245e1807218cbfcc913" :recorded-tools ((:name cargo :version "cargo 1.96.1 (356927216 2026-06-26)" :sha256 "861a0e077a3810cd82a7e20e955eccbe7e5791fe098b3f433766897deb5433a1") (:name rustfmt :version "rustfmt 1.9.0-stable (31fca3adb2 2026-06-26)" :sha256 "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10")) :manifest (("rustic-babel.el" . "b4dbc82b1d04c47e8fa6d88cfb13fcf35b9902f50e55c1462e09233157b8a610") ("rustic-cargo.el" . "623762940388878ddb80d05ba1c3a98373dd6bf7a32b4006f8ba001809084019") ("rustic-clippy.el" . "4e52fce8b2df66599eb63d3466cef3d93e074b37cde4be890f4015e31891f8e6") ("rustic-comint.el" . "e7cedb3a3739955cac28edebe86e1b8f2f8fc822dab7e3e47baa44da98108f0f") ("rustic-compile.el" . "dbb3869f17d2d0ef7a6bef09e8aef1bddfdd99c634d912e0839f69a09c4ae0f2") ("rustic-doc.el" . "4fc86752bb8a59c27122583d7e5e5986bd9748c964a4227d185ce4d0facee890") ("rustic-expand.el" . "d830f982d723a4524f0a39e34c981d9fc127fc8136ca1eb77465aadee26aed99") ("rustic-flycheck.el" . "fd03896b895960dad9cb781b3a90e66cd8001183351c1f4a738e5329a407fbdf") ("rustic-interaction.el" . "b36403dadfd9a2c08cb2cc85b9a47e6ff179c041895f80ab29c9067ff99a62dd") ("rustic-lsp.el" . "8fd736411cfa46dcc8c1296042a28ba1570949d4c982bc47f0580e01dc2a3c34") ("rustic-pkg.el" . "e7f966a59e1e777104b87056a0f94944302e8111756fe2dd7ef83569a73e3aec") ("rustic-playground.el" . "f161ba9099029b964ef9f33743a565e8b324e4918463075c8d6f410ecc17b80b") ("rustic-popup.el" . "4873652dc564904d93b52751bc0b985579ff68e30f25dcebc92c1c71b8ea025d") ("rustic-rustfix.el" . "ce28a6be920b2defa6e15c3da57589c36fae64648f139bb3efd3cf8291495743") ("rustic-rustfmt.el" . "b8083fd1671fa1bd4a3ff9949ce369c2ff75611c0c5aa80191f4ea0ac6ab4acd") ("rustic-spellcheck.el" . "5fa32c8d199f1c9904102ef6f1a45c22b7a6a9eda1d3a456f4efe6c46ded838b") ("rustic.el" . "6041a10aa0378620fb76a618d54b6f08d884765575695b392e601b23ad525e6f")) :version "20260407.1712" :feature t :advice ((save-some-buffers . t) (compile-goto-error . t))) :result (:arguments "--all-targets --all-features -- --nocapture works_界" :history ("--all-targets --all-features -- --nocapture works_界") :prompts ((:prompt "Cargo test arguments: " :initial "--all-targets --all-features" :keymap nil :read nil :history rustic-test-history :default nil :inherit nil :answer "--all-targets --all-features -- --nocapture works_界")) :first (:status exit :exit 0) :first-buffer (:name "*cargo-test*" :mode rustic-cargo-test-mode :directory "./" :process nil :errors 0 :warnings 0 :text "[ROOT]/bin/cargo test --all-targets --all-features -- --nocapture works_界 \nrunning 1 test\ntest unit::works_界 ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\ncargo-test finished at [TIME], duration [DURATION]\n") :second (:status exit :exit 0) :second-buffer (:name "*cargo-test*" :mode rustic-cargo-test-mode :directory "./" :process nil :errors 0 :warnings 0 :text "[ROOT]/bin/cargo test --all-targets --all-features -- --nocapture works_界 \nrunning 1 test\ntest unit::works_界 ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\ncargo-test finished at [TIME], duration [DURATION]\n") :calls ((:argv ("test" "--all-targets" "--all-features" "--" "--nocapture" "works_界") :backtrace "full" :rustflags "--cfg rustic419") (:argv ("test" "--all-targets" "--all-features" "--" "--nocapture" "works_界") :backtrace "full" :rustflags "--cfg rustic419"))) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :workspace-default-restored t :next-error-restored t :buffer-restored t :menu-bar-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_rustfmt_failure_is_atomic_then_successfully_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_rustfmt_failure_is_atomic_then_successfully_recovers",
        r####"
(rustic419-test-run
 "rustfmt-recovery" '("fail:--" "pass:--")
 (lambda (root)
   (let ((buffer (find-file-noselect (expand-file-name "src/main.rs" root))))
     (unwind-protect
         (with-current-buffer buffer
           (rustic-mode)
           (erase-buffer)
           (insert rustic419-test-main-invalid)
           (goto-char 8)
           (set-buffer-modified-p t)
           (let ((before (rustic419-test-buffer-state buffer)))
             (call-interactively #'rustic-format-buffer)
             (let* ((first-process (get-process rustic-format-process-name))
                    (failure-completion (rustic419-test-wait-format first-process))
                    (after-failure (rustic419-test-buffer-state buffer))
                    (failure-stdin-sha
                     (rustic419-test-file-sha
                      (expand-file-name "stdin" root)))
                    (error-buffer
                     (rustic419-test-compilation-state
                      (get-buffer rustic-format-buffer-name))))
               (erase-buffer)
               (insert rustic419-test-main-unformatted)
               (goto-char 8)
               (set-buffer-modified-p t)
               (let ((repaired (rustic419-test-buffer-state buffer)))
                 (call-interactively #'rustic-format-buffer)
                 (let* ((second-process (get-process rustic-format-process-name))
                        (success-completion (rustic419-test-wait-format second-process))
                        (after-success (rustic419-test-buffer-state buffer)))
                   (list :before before :failure failure-completion
                         :after-failure after-failure
                         :failure-stdin-sha failure-stdin-sha
                         :error-buffer error-buffer :repaired repaired
                         :success success-completion :after-success after-success
                         :stdin-sha
                         (let ((stdin (expand-file-name "stdin" root)))
                           (and (file-exists-p stdin)
                                (rustic419-test-file-sha stdin)))
                         :calls (rustic419-test-calls)))))))
       (when (buffer-live-p buffer) (kill-buffer buffer))))))
"####,
        expect![[
            r#"OK (:source (:upstream-tree "886c5d478c0ff4223f290f4aa13c36bb0d1bbb14" :upstream-main-sha "e314ef1cf51c08913d5b47d5c4e3dad1a7b20e86b8aac245e1807218cbfcc913" :recorded-tools ((:name cargo :version "cargo 1.96.1 (356927216 2026-06-26)" :sha256 "861a0e077a3810cd82a7e20e955eccbe7e5791fe098b3f433766897deb5433a1") (:name rustfmt :version "rustfmt 1.9.0-stable (31fca3adb2 2026-06-26)" :sha256 "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10")) :manifest (("rustic-babel.el" . "b4dbc82b1d04c47e8fa6d88cfb13fcf35b9902f50e55c1462e09233157b8a610") ("rustic-cargo.el" . "623762940388878ddb80d05ba1c3a98373dd6bf7a32b4006f8ba001809084019") ("rustic-clippy.el" . "4e52fce8b2df66599eb63d3466cef3d93e074b37cde4be890f4015e31891f8e6") ("rustic-comint.el" . "e7cedb3a3739955cac28edebe86e1b8f2f8fc822dab7e3e47baa44da98108f0f") ("rustic-compile.el" . "dbb3869f17d2d0ef7a6bef09e8aef1bddfdd99c634d912e0839f69a09c4ae0f2") ("rustic-doc.el" . "4fc86752bb8a59c27122583d7e5e5986bd9748c964a4227d185ce4d0facee890") ("rustic-expand.el" . "d830f982d723a4524f0a39e34c981d9fc127fc8136ca1eb77465aadee26aed99") ("rustic-flycheck.el" . "fd03896b895960dad9cb781b3a90e66cd8001183351c1f4a738e5329a407fbdf") ("rustic-interaction.el" . "b36403dadfd9a2c08cb2cc85b9a47e6ff179c041895f80ab29c9067ff99a62dd") ("rustic-lsp.el" . "8fd736411cfa46dcc8c1296042a28ba1570949d4c982bc47f0580e01dc2a3c34") ("rustic-pkg.el" . "e7f966a59e1e777104b87056a0f94944302e8111756fe2dd7ef83569a73e3aec") ("rustic-playground.el" . "f161ba9099029b964ef9f33743a565e8b324e4918463075c8d6f410ecc17b80b") ("rustic-popup.el" . "4873652dc564904d93b52751bc0b985579ff68e30f25dcebc92c1c71b8ea025d") ("rustic-rustfix.el" . "ce28a6be920b2defa6e15c3da57589c36fae64648f139bb3efd3cf8291495743") ("rustic-rustfmt.el" . "b8083fd1671fa1bd4a3ff9949ce369c2ff75611c0c5aa80191f4ea0ac6ab4acd") ("rustic-spellcheck.el" . "5fa32c8d199f1c9904102ef6f1a45c22b7a6a9eda1d3a456f4efe6c46ded838b") ("rustic.el" . "6041a10aa0378620fb76a618d54b6f08d884765575695b392e601b23ad525e6f")) :version "20260407.1712" :feature t :advice ((save-some-buffers . t) (compile-goto-error . t))) :result (:before (:name "main.rs" :file "src/main.rs" :mode rustic-mode :modified t :point 8 :mark nil :text "fn main(){println!(\"café 界\");") :failure (:status exit :exit 1) :after-failure (:name "main.rs" :file "src/main.rs" :mode rustic-mode :modified t :point 8 :mark nil :text "fn main(){println!(\"café 界\");") :failure-stdin-sha "bb5c0761a2e133d4d2ea283a3163ca002b2b4e65dc83401ffa02f5ad0a937d1f" :error-buffer (:name "*rustfmt*" :mode rustic-format-mode :directory "./" :process nil :errors 1 :warnings 0 :text "error: this file contains an unclosed delimiter\n --> [ROOT]/src/main.rs:1:31\n  |\n1 | fn main(){println!(\"café 界\");\n  |          - unclosed delimiter ^\n") :repaired (:name "main.rs" :file "src/main.rs" :mode rustic-mode :modified t :point 8 :mark nil :text "fn main(){println!(\"café 界\");}\n") :success (:status exit :exit 0) :after-success (:name "main.rs" :file "src/main.rs" :mode rustic-mode :modified t :point 8 :mark nil :text "fn main() {\n    println!(\"café 界\");\n}\n") :stdin-sha "f37a8222a89c572f8b9c5721f36535c28d2c5ae354f4e8f5562867e444b7ef1f" :calls ((:mode fail :argv ("--") :backtrace "full" :rustflags nil) (:mode pass :argv ("--") :backtrace "full" :rustflags nil))) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :workspace-default-restored t :next-error-restored t :buffer-restored t :menu-bar-restored t :window-restored t :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn rustic_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_mode_keys_docstring_and_manifest_navigation(),
        public_current_test_derives_the_nested_unicode_target(),
        public_build_error_navigates_and_check_recovers(),
        public_test_prompt_and_rerun_preserve_arguments_and_history(),
        public_rustfmt_failure_is_atomic_then_successfully_recovers(),
    ];
    assert_oracle_batch_cases(oracle(), "rustic-rank419", "rustic_parity", &cases);
}
