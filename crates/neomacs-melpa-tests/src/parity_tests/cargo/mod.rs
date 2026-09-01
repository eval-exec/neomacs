//! Practical parity for cargo.el's public process and navigation workflows.
//!
//! Cargo and rustc are owned external boundaries. A closed shell fixture
//! validates every argv before replaying deterministic compiler, metadata,
//! search, project-creation, and explanation results. The package's public
//! commands, project discovery, command construction, compilation mode,
//! buttons, sentinels, prompts, Xref objects, and cleanup remain real.

use std::time::Duration;

use expect_test::{Expect, expect};

use crate::{CARGO_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'cargo)

(defconst cargo389-test-installed-sources
  '((cargo-find-dependency "cargo.el"
                           "123d2f692485fb292ea79c8fe3084f2571bdd2c2fa0572f9af2766000f9b7068")
    (cargo-process-build "cargo-process.el"
                         "5abdc35334bbb39646137c35b386357a1cb990ef1506042a9411becf6f4cfe3c")))
(defvar cargo389-test-case-index 0)
(defvar cargo389-test-owned-roots nil)

(defun cargo389-test-source-hash (symbol)
  (let ((file (symbol-file symbol 'defun)))
    (unless (file-regular-p file)
      (error "Missing installed Cargo source for %S: %S" symbol file))
    (list (file-name-nondirectory file)
          (with-temp-buffer
            (set-buffer-multibyte nil)
            (insert-file-contents-literally file)
            (secure-hash 'sha256 (current-buffer))))))

(dolist (source cargo389-test-installed-sources)
  (unless (equal (cargo389-test-source-hash (nth 0 source)) (cdr source))
    (error "Unexpected installed Cargo source: %S" source)))

(defun cargo389-test-write (path text &optional executable)
  (make-directory (file-name-directory path) t)
  (write-region text nil path nil 'silent)
  (when executable (set-file-modes path #o755))
  path)

(defun cargo389-test-project (root)
  (make-directory (expand-file-name "src" root) t)
  (cargo389-test-write
   (expand-file-name "Cargo.toml" root)
   "[package]\nname = \"demo-界\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n")
  (cargo389-test-write
   (expand-file-name "src/main.rs" root)
   "fn main() {\n        #[cfg(not(test))]\n        missing_name();\n}\n\n#[cfg(test)]\nmod unit {\n    #[test]\n    fn works_界() {\n        assert_eq!(2 + 2, 4);\n    }\n}\n")
  root)

(defconst cargo389-test-tool-script
  "#!/bin/sh
set -eu
claim=0
if [ -f \"$CARGO389_CLAIM\" ]; then IFS= read -r claim < \"$CARGO389_CLAIM\"; fi
next=$((claim + 1))
expected=
i=0
while IFS= read -r line || [ -n \"$line\" ]; do
  i=$((i + 1))
  if [ \"$i\" -eq \"$next\" ]; then expected=$line; fi
done < \"$CARGO389_PLAN\"
joined=
separator=
for argument in \"$@\"; do joined=\"${joined}${separator}${argument}\"; separator='|'; done
if [ -z \"$expected\" ] || [ \"$joined\" != \"$expected\" ]; then
  printf 'unexpected[%s]=%s expected=%s\\n' \"$next\" \"$joined\" \"$expected\" >&2
  exit 86
fi
printf '%s\\n' \"$next\" > \"$CARGO389_CLAIM\"
printf '%s\\t%s\\n' \"$joined\" \"${RUST_BACKTRACE-}\" >> \"$CARGO389_LOG\"
case \"${1-}\" in
  metadata)
    case \"$joined\" in
      *no-deps) printf '{\"packages\":[],\"workspace_root\":\"%s\"}\\n' \"$CARGO389_PROJECT\" ;;
      *) printf '{\"packages\":[{\"name\":\"serde\",\"manifest_path\":\"%sdeps/serde/Cargo.toml\"},{\"name\":\"serde\",\"manifest_path\":\"%svendor/serde/Cargo.toml\"},{\"name\":\"unicode-界\",\"manifest_path\":\"%sdeps/unicode/Cargo.toml\"}],\"workspace_root\":\"%s\"}\\n' \"$CARGO389_PROJECT\" \"$CARGO389_PROJECT\" \"$CARGO389_PROJECT\" \"$CARGO389_PROJECT\" ;;
    esac ;;
  build)
    printf '   Compiling demo-界 v0.1.0 (%s)\\n' \"$CARGO389_PROJECT\"
    printf 'error[E0425]: cannot find function `missing_name` in this scope\\n'
    printf ' --> %ssrc/main.rs:3:9\\n' \"$CARGO389_PROJECT\"
    printf '  |\\n3 |         missing_name();\\n  |         ^^^^^^^^^^^^ not found in this scope\\n'
    printf '\\nFor more information about this error, try `rustc --explain E0425`.\\n'
    printf 'error: could not compile `demo-界` (bin \"demo-界\") due to 1 previous error\\n'
    exit 101 ;;
  test)
    printf 'running 1 test\\ntest unit::works_界 ... ok\\n\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\\n' ;;
  check)
    printf 'Finished dev profile [unoptimized] target(s) in 0.01s\\n' ;;
  search)
    printf 'serde = \"1.0.219\"    # serialization framework\\n' ;;
  new)
    mkdir -p \"$2/src\"
    printf '[package]\\nname = \"%s\"\\nversion = \"0.1.0\"\\n' \"$2\" > \"$2/Cargo.toml\"
    printf 'fn main() { println!(\"hello 界\"); }\\n' > \"$2/src/main.rs\"
    printf 'Created binary (application) `%s` package\\n' \"$2\" ;;
  *) printf 'unhandled command: %s\\n' \"$joined\" >&2; exit 86 ;;
esac
")

(defconst cargo389-test-rustc-script
  "#!/bin/sh
set -eu
if [ \"$#\" -ne 1 ] || [ \"$1\" != '--explain=E0425' ]; then
  printf 'unexpected rustc argv: <%s>\\n' \"$*\" >&2
  exit 86
fi
printf '<%s>\\n' \"$1\" >> \"$CARGO389_RUSTC_LOG\"
printf '# Error code E0425\\n\\nAn unresolved name was used.\\n\\n```rust\\nmissing_name();\\n```\\n'
")

(defun cargo389-test-install-tools (root plan)
  (let* ((bin (expand-file-name "bin" root))
         (cargo (expand-file-name "cargo" bin))
         (rustc (expand-file-name "rustc" bin))
         (plan-file (expand-file-name "plan" root)))
    (cargo389-test-write cargo cargo389-test-tool-script t)
    (cargo389-test-write rustc cargo389-test-rustc-script t)
    (cargo389-test-write plan-file (concat (string-join plan "\n") "\n"))
    (setenv "CARGO389_PLAN" plan-file)
    (setenv "CARGO389_CLAIM" (expand-file-name "claim" root))
    (setenv "CARGO389_LOG" (expand-file-name "cargo.log" root))
    (setenv "CARGO389_RUSTC_LOG" (expand-file-name "rustc.log" root))
    (setenv "CARGO389_PROJECT" root)
    (list cargo rustc)))

(defun cargo389-test-log (root name)
  (let ((file (expand-file-name name root)))
    (when (file-exists-p file)
      (with-temp-buffer
        (insert-file-contents file)
        (mapcar
         (lambda (line)
           (let ((fields (split-string line "\t")))
             (list :argv (cargo389-test-relative (car fields) root)
                   :backtrace
                   (unless (string-empty-p (or (cadr fields) ""))
                     (cadr fields)))))
         (split-string (string-trim-right (buffer-string)) "\n" t))))))

(defmacro cargo389-test-piped (&rest body)
  "Evaluate BODY with the Cargo child on a pipe, not on a PTY.
`cargo-process--add-errno-buttons' runs from `compilation-filter-hook' and
searches only `compilation-filter-start' .. `(point)' for
`cargo-process--errno-regex' (cargo-process.el:469-479, installed at :292).
A match straddling that boundary is never found again, so whether the E0425
button exists at all is decided by where a read landed -- and this suite pins
the button.  `compilation-start' gives the child a PTY by default
\(GNU src/process.c:8923-8929), and a PTY's line discipline is the only
topology here that can hand Emacs half a line; over a pipe each of the
stand-in's `printf' writes is atomic and below PIPE_BUF.  See DIVERGENCES.md
entries 133 and 144."
  (declare (indent 0))
  `(let ((process-connection-type nil)) ,@body))

(defun cargo389-test-assert-piped (process)
  "Signal unless PROCESS is connected through a pipe.
`cargo389-test-wait' is the only way this suite reaches a finished Cargo
buffer, so checking here means no call site can skip the guard: one that
forgets `cargo389-test-piped' fails on its first run, in both editors."
  (unless (processp process)
    (error "cargo389-test-assert-piped: %S is not a process, so the pipe \
guard could not be checked" process))
  (when (process-tty-name process)
    (error "cargo389-test-assert-piped: %S is PTY-connected (%s); its output \
would arrive in scheduling-dependent chunks"
           process (process-tty-name process))))

(defun cargo389-test-compilation-complete-p (buffer)
  "Non-nil once `compilation-handle-exit' has written BUFFER's last line.
That line is the causal end of the output rather than a guess about it: Emacs
drains a dying process's remaining reads before it runs the sentinel
\(GNU src/process.c:7896-7910), the sentinel is what calls
`compilation-handle-exit', and that function marks the text it writes with a
`compilation-handle-exit' text property (GNU lisp/progmodes/compile.el:2630)."
  (and (buffer-live-p (get-buffer buffer))
       (with-current-buffer buffer
         (and (text-property-not-all (point-min) (point-max)
                                     'compilation-handle-exit nil)
              t))))

(defun cargo389-test-wait-sentinel (process)
  "Wait until PROCESS's own sentinel has run, or signal.
`cargo-process-new' replaces the compilation sentinel with a lambda of its own
\(cargo-process.el:606-618) that never calls `compilation-handle-exit', so
`*Cargo New*' wears `cargo-process-mode' but that marker never appears in it.
Which gate a pin needs is decided by the sentinel that drives the buffer, not
by the major mode it wears; for a buffer driven by the package's own sentinel
the causal fact is that this sentinel has run.  Attaching the observer here
cannot miss it -- Emacs runs process sentinels only from the event loop, and
nothing has pumped it between the spawn and this call."
  (cargo389-test-assert-piped process)
  (let ((seen nil) (waited 0))
    (add-function :after (process-sentinel process)
                  (lambda (&rest _) (setq seen t)))
    (while (and (< waited 1200) (not seen))
      (accept-process-output nil 0.05)
      (setq waited (1+ waited)))
    (unless seen
      (error "cargo389-test-wait-sentinel: %S's own sentinel never ran, so \
whatever it was going to write is not in the buffer yet" process))
    (list :status (process-status process)
          :exit (process-exit-status process))))

(defun cargo389-test-wait (process)
  "Wait until PROCESS's buffer holds all of its compilation output, or signal.
Neither `process-live-p' going nil nor N identical samples of the buffer is
that condition: a process can be gone with reads still queued, and identical
samples only say the sentinel has not run YET.  Both were what this helper
used to wait for, and the pin it guards records the sentinel's own closing
line."
  (let ((buffer (process-buffer process))
        (waited 0))
    (cargo389-test-assert-piped process)
    (while (and (< waited 1200)
                (not (cargo389-test-compilation-complete-p buffer)))
      (accept-process-output nil 0.05)
      (setq waited (1+ waited)))
    (unless (cargo389-test-compilation-complete-p buffer)
      (error "cargo389-test-wait: %S never reached `compilation-handle-exit'; \
its buffer records only as much of the child's output as had been read"
             process))
    (when (and (buffer-live-p buffer) (get-buffer-process buffer))
      (error "Cargo process remained attached after exit: %S" process))
    (list :status (process-status process)
          :exit (process-exit-status process))))

(defun cargo389-test-relative (text root)
  (replace-regexp-in-string (regexp-quote root) "[ROOT]/" text t t))

(defun cargo389-test-compilation-state (buffer root)
  (with-current-buffer buffer
    (let ((text (buffer-substring-no-properties (point-min) (point-max))))
      (list :name (buffer-name)
            :mode major-mode
            :truncate truncate-lines
            :text
            (cargo389-test-relative
             (replace-regexp-in-string
              "at [A-Z][a-z][a-z] [A-Z][a-z][a-z] +[0-9]+ [0-9:]+"
              "at [TIME]"
              (replace-regexp-in-string "duration [0-9.]+ s"
                                        "duration [DURATION]" text))
             root)))))

(defun cargo389-test-buffer-state (buffer root)
  (with-current-buffer buffer
    (list :name (buffer-name)
          :file (and buffer-file-name (file-relative-name buffer-file-name root))
          :mode major-mode
          :modified (buffer-modified-p)
          :point (point)
          :text (buffer-substring-no-properties (point-min) (point-max)))))

(defun cargo389-test-xrefs (xrefs root)
  (mapcar
   (lambda (xref)
     (let ((location (xref-item-location xref)))
       (list :summary (xref-item-summary xref)
             :file (file-relative-name (xref-file-location-file location) root)
             :line (xref-file-location-line location)
             :column (xref-file-location-column location))))
   xrefs))

(defun cargo389-test-park-buffer (name suffix)
  (when-let* ((buffer (get-buffer name)))
    (let ((old-name (buffer-name buffer)))
      (with-current-buffer buffer
        (rename-buffer (format " %s-%s" suffix (sxhash-eq buffer)) t))
      (cons buffer old-name))))

(defun cargo389-test-run (body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (index (setq cargo389-test-case-index (1+ cargo389-test-case-index)))
         (root (file-name-as-directory
                (expand-file-name (format "cargo-%d" index) sandbox)))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (buffer-before (current-buffer))
         (windows-before (current-window-configuration))
         (parked (delq nil
                       (mapcar (lambda (name)
                                 (cargo389-test-park-buffer name "cargo389"))
                               '("*Cargo Build*" "*Cargo Test*" "*Cargo Check*"
                                 "*Cargo Search*" "*Cargo New*" "*rust errno*"))))
         (process-environment (copy-sequence process-environment))
         (exec-path (copy-sequence exec-path))
         (compilation-ask-about-save nil)
         (compilation-scroll-output nil)
         (cargo-process-last-command nil)
         (cargo-process--enable-rust-backtrace nil)
         (next-error-last-buffer next-error-last-buffer)
         (message-log-max nil)
         (inhibit-message t)
         (xref-marker-ring nil)
         (cargo389-test-owned-roots nil)
         result body-error cleanup-errors window-restored)
    (unless (and sandbox (file-name-absolute-p sandbox))
      (error "Missing Cargo sandbox root"))
    (when (file-exists-p root)
      (error "Cargo case root already exists: %s" root))
    (make-directory root t)
    (push root cargo389-test-owned-roots)
    (unwind-protect
        (condition-case error
            (cl-letf (((symbol-function 'url-retrieve)
                       (lambda (&rest args)
                         (error "Unexpected Cargo network request: %S" args)))
                      ((symbol-function 'make-network-process)
                       (lambda (&rest args)
                         (error "Unexpected Cargo network process: %S" args))))
              (setq result (funcall body root)))
          (error (setq body-error
                       (list :type (car error) :message (error-message-string error)))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case error
              (progn (set-process-query-on-exit-flag process nil)
                     (delete-process process))
            (error (push (list :process error) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (when (buffer-live-p buffer)
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :buffer error) cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case error (cancel-timer timer)
            (error (push (list :timer error) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case error (delete-frame frame t)
            (error (push (list :frame error) cleanup-errors)))))
      (condition-case error
          (progn (set-window-configuration windows-before)
                 (when (buffer-live-p buffer-before) (set-buffer buffer-before))
                 (setq window-restored t))
        (error (push (list :windows error) cleanup-errors)))
      (dolist (entry parked)
        (condition-case error
            (if (buffer-live-p (car entry))
                (with-current-buffer (car entry) (rename-buffer (cdr entry) t))
              (error "Parked Cargo buffer died: %S" (cdr entry)))
          (error (push (list :parked error) cleanup-errors))))
      (dolist (owned-root cargo389-test-owned-roots)
        (condition-case error
            (when (file-exists-p owned-root) (delete-directory owned-root t))
          (error (push (list :root error) cleanup-errors))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (when (buffer-live-p buffer)
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :reaction-buffer error) cleanup-errors))))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter (lambda (buffer) (not (memq buffer buffers-before)))
                                     (buffer-list)))
                 :new-processes
                 (mapcar #'process-name
                         (seq-filter (lambda (process) (not (memq process processes-before)))
                                     (process-list)))
                 :new-timers
                 (length (seq-filter (lambda (timer) (not (memq timer timers-before)))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-filter (lambda (frame) (not (memq frame frames-before)))
                                     (frame-list)))
                 :roots-exist (seq-some #'file-exists-p cargo389-test-owned-roots)
                 :window-restored window-restored
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "cargo389 failure: %S" (list result cleanup))
        (list :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CARGO_MELPA_PIN, "cargo.el")
        .expect("prepare exact shallow cargo.el source graph below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn workflow(name: &'static str, probe: &'static str, expected: Expect) -> ParityBatchCase {
    ParityBatchCase::value(name, probe, expected)
}

fn cases() -> Vec<ParityBatchCase> {
    vec![
        workflow(
            "public_build_repeat_and_errno_help_preserve_compilation_contract",
            r####"
(cargo389-test-run
 (lambda (root)
   (cargo389-test-project root)
   (let* ((tools (cargo389-test-install-tools
                  root
                  (list
                   "metadata|--format-version|1|--no-deps"
                   (concat "build|--manifest-path|" root "Cargo.toml|--locked")
                   "metadata|--format-version|1|--no-deps"
                   (concat "build|--manifest-path|" root "Cargo.toml|--locked"))))
          (cargo-process--custom-path-to-bin (nth 0 tools))
          (cargo-process--rustc-cmd (nth 1 tools))
          (cargo-process--command-flags "--locked")
          (source (find-file-noselect (expand-file-name "src/main.rs" root)))
          first-outcome first-state button-state explain-state repeat-outcome)
     (with-current-buffer source
       (cargo-minor-mode 1)
       (setq first-outcome (cargo389-test-wait (cargo389-test-piped (cargo-process-build)))))
     (let ((build-buffer (get-buffer "*Cargo Build*")))
       (setq first-state (cargo389-test-compilation-state build-buffer root))
       (with-current-buffer build-buffer
         (goto-char (point-min))
         (search-forward "E0425")
         (let ((button (button-at (1- (point)))))
           (setq button-state
                 (list :label (button-label button)
                       :type (button-type button)))
           (push-button (button-start button)))))
     (setq explain-state
           (cargo389-test-buffer-state (get-buffer "*rust errno*") root))
     (with-current-buffer source
       (setq repeat-outcome (cargo389-test-wait (cargo389-test-piped (cargo-process-repeat)))))
     (list :mode (with-current-buffer source cargo-minor-mode)
           :keys (with-current-buffer source
                   (list (key-binding (kbd "C-c C-c C-b"))
                         (key-binding (kbd "C-c C-c C-c"))))
           :first first-outcome
           :first-buffer first-state
           :button button-state
           :explain explain-state
           :repeat repeat-outcome
           :last-command
           (with-current-buffer source
             (cargo389-test-relative
              (prin1-to-string cargo-process-last-command) root))
           :cargo-log (cargo389-test-log root "cargo.log")
           :rustc-log (cargo389-test-log root "rustc.log")))))
"####,
            expect![[
                r##"OK (:result (:mode t :keys (cargo-process-build cargo-process-repeat) :first (:status exit :exit 101) :first-buffer (:name "*Cargo Build*" :mode cargo-process-mode :truncate t :text "-*- mode: cargo-process; default-directory: \"[ROOT]/\" -*-\nCargo-Process started at [TIME]\n\n[ROOT]/bin/cargo build --manifest-path [ROOT]/Cargo.toml --locked \n   Compiling demo-界 v0.1.0 ([ROOT]/)\nerror[E0425]: cannot find function `missing_name` in this scope\n --> [ROOT]/src/main.rs:3:9\n  |\n3 |         missing_name();\n  |         ^^^^^^^^^^^^ not found in this scope\n\nFor more information about this error, try `rustc --explain E0425`.\nerror: could not compile `demo-界` (bin \"demo-界\") due to 1 previous error\n\nCargo-Process exited abnormally with code 101 at [TIME], duration [DURATION]\n") :button (:label "E0425" :type rustc-errno) :explain (:name "*rust errno*" :file nil :mode markdown-view-mode :modified t :point 1 :text "# Error code E0425\n\nAn unresolved name was used.\n\n```rust\nmissing_name();\n```\n") :repeat (:status exit :exit 101) :last-command "(\"Build\" \"build\" \"[ROOT]/bin/cargo build --manifest-path [ROOT]/Cargo.toml --locked \")" :cargo-log ((:argv "metadata|--format-version|1|--no-deps" :backtrace nil) (:argv "build|--manifest-path|[ROOT]/Cargo.toml|--locked" :backtrace nil) (:argv "metadata|--format-version|1|--no-deps" :backtrace nil) (:argv "build|--manifest-path|[ROOT]/Cargo.toml|--locked" :backtrace nil)) :rustc-log ((:argv "<--explain=E0425>" :backtrace nil))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"##
            ]],
        ),
        workflow(
            "public_test_routes_and_prefix_edit_preserve_names_flags_and_backtrace",
            r####"
(cargo389-test-run
 (lambda (root)
   (cargo389-test-project root)
   (let* ((manifest (concat root "Cargo.toml"))
          (tools (cargo389-test-install-tools
                  root
                  (list
                   "metadata|--format-version|1|--no-deps"
                   (concat "test|unit::works_界|--manifest-path|" manifest "|--|--nocapture")
                   "metadata|--format-version|1|--no-deps"
                   (concat "test|unit|--manifest-path|" manifest "|--|--nocapture")
                   "metadata|--format-version|1|--no-deps"
                   (concat "check|--manifest-path|" manifest "|--release"))))
          (cargo-process--custom-path-to-bin (nth 0 tools))
          (cargo-process--rustc-cmd (nth 1 tools))
          (cargo-process--enable-rust-backtrace t)
          (cargo-process--command-test--additional-args "-- --nocapture")
          (source (find-file-noselect (expand-file-name "src/main.rs" root)))
          current-test current-file edited prompt default)
     (with-current-buffer source
       (goto-char (point-min))
       (search-forward "fn works_界")
       (beginning-of-line)
       (setq current-test (cargo389-test-wait (cargo389-test-piped (cargo-process-current-test))))
       (setq current-file (cargo389-test-wait (cargo389-test-piped (cargo-process-current-file-tests))))
       (let ((current-prefix-arg '(4)))
         (cl-letf (((symbol-function 'read-shell-command)
                    (lambda (actual-prompt actual-default &rest _)
                      (setq prompt actual-prompt default actual-default)
                      (concat cargo-process--custom-path-to-bin
                              " check --manifest-path " manifest " --release"))))
           (setq edited (cargo389-test-wait (cargo389-test-piped (call-interactively #'cargo-process-check)))))))
     (list :current-test current-test
           :current-file current-file
           :edited edited
           :prompt prompt
           :default (cargo389-test-relative default root)
           :test-buffer (cargo389-test-compilation-state (get-buffer "*Cargo Test*") root)
           :check-buffer (cargo389-test-compilation-state (get-buffer "*Cargo Check*") root)
           :last-command
           (with-current-buffer source
             (cargo389-test-relative (prin1-to-string cargo-process-last-command) root))
           :cargo-log (cargo389-test-log root "cargo.log")))))
"####,
            expect![[
                r#"OK (:result (:current-test (:status exit :exit 0) :current-file (:status exit :exit 0) :edited (:status exit :exit 0) :prompt "Cargo command: " :default "[ROOT]/bin/cargo check --manifest-path [ROOT]/Cargo.toml  " :test-buffer (:name "*Cargo Test*" :mode cargo-process-mode :truncate t :text "-*- mode: cargo-process; default-directory: \"[ROOT]/\" -*-\nCargo-Process started at [TIME]\n\n[ROOT]/bin/cargo test unit --manifest-path [ROOT]/Cargo.toml  -- --nocapture\nrunning 1 test\ntest unit::works_界 ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\nCargo-Process finished at [TIME], duration [DURATION]\n") :check-buffer (:name "*Cargo Check*" :mode cargo-process-mode :truncate t :text "-*- mode: cargo-process; default-directory: \"[ROOT]/\" -*-\nCargo-Process started at [TIME]\n\n[ROOT]/bin/cargo check --manifest-path [ROOT]/Cargo.toml --release\nFinished dev profile [unoptimized] target(s) in 0.01s\n\nCargo-Process finished at [TIME], duration [DURATION]\n") :last-command "(\"Check\" \"check\" \"[ROOT]/bin/cargo check --manifest-path [ROOT]/Cargo.toml --release\")" :cargo-log ((:argv "metadata|--format-version|1|--no-deps" :backtrace "1") (:argv "test|unit::works_界|--manifest-path|[ROOT]/Cargo.toml|--|--nocapture" :backtrace "1") (:argv "metadata|--format-version|1|--no-deps" :backtrace "1") (:argv "test|unit|--manifest-path|[ROOT]/Cargo.toml|--|--nocapture" :backtrace "1") (:argv "metadata|--format-version|1|--no-deps" :backtrace nil) (:argv "check|--manifest-path|[ROOT]/Cargo.toml|--release" :backtrace nil))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
            ]],
        ),
        workflow(
            "public_dependency_lookup_uses_metadata_and_xref_then_reports_missing",
            r####"
(cargo389-test-run
 (lambda (root)
   (cargo389-test-project root)
   (let* ((tools (cargo389-test-install-tools
                  root '("metadata|--format-version|1"
                         "metadata|--format-version|1")))
          (cargo-process--custom-path-to-bin (nth 0 tools))
          (source (find-file-noselect (expand-file-name "src/main.rs" root)))
          captured metadata missing-message)
     (with-current-buffer source
       (let ((xref-show-definitions-function
              (lambda (fetcher _display)
                (setq captured (cargo389-test-xrefs (funcall fetcher) root)))))
         (cargo-find-dependency "serde"))
       (setq metadata (cargo-process--get-metadata))
       (let ((inhibit-message nil))
         (setq missing-message (cargo-find-dependency "absent" metadata))))
     (list :xrefs captured
           :missing missing-message
           :cargo-log (cargo389-test-log root "cargo.log")))))
"####,
            expect![[
                r#"OK (:result (:xrefs ((:summary "serde" :file "deps/serde/Cargo.toml" :line 1 :column 0) (:summary "serde" :file "vendor/serde/Cargo.toml" :line 1 :column 0)) :missing "Can’t find: absent" :cargo-log ((:argv "metadata|--format-version|1" :backtrace "1") (:argv "metadata|--format-version|1" :backtrace "1"))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
            ]],
        ),
        workflow(
            "public_new_and_interactive_search_preserve_files_prompts_and_no_manifest_route",
            r####"
(cargo389-test-run
 (lambda (root)
   (cargo389-test-write (expand-file-name "Cargo.toml" root) "[workspace]\n")
   (let* ((opened-path (expand-file-name "demo-界/src/main.rs" root))
          (tools (cargo389-test-install-tools
                  root
                  (list
                   "metadata|--format-version|1|--no-deps"
                   "new|demo-界|--bin"
                   "metadata|--format-version|1|--no-deps"
                   "search|serde")))
          (cargo-process--custom-path-to-bin (nth 0 tools))
          (cargo-process--rustc-cmd (nth 1 tools))
          (cargo-process--open-file-after-new t)
          (default-directory root)
          new-process new-outcome opened selected search-outcome prompt)
     (cargo389-test-piped (cargo-process-new "demo-界" t))
     (setq new-process (get-buffer-process "*Cargo New*")
           new-outcome (cargo389-test-wait-sentinel new-process)
           opened (get-file-buffer opened-path)
           selected (eq (window-buffer (selected-window)) opened))
     (unless (buffer-live-p opened)
       (error "Public Cargo new did not visit %s" opened-path))
     (with-current-buffer opened
       (goto-char (point-min))
       (search-forward "main")
       (cl-letf (((symbol-function 'read-string)
                  (lambda (actual-prompt &rest _)
                    (setq prompt actual-prompt)
                    "serde")))
         (setq search-outcome
               (cargo389-test-wait (cargo389-test-piped (call-interactively #'cargo-process-search))))))
     (list :new new-outcome
           :selected selected
           :opened (cargo389-test-buffer-state opened root)
           :prompt prompt
           :search search-outcome
           :search-buffer
           (cargo389-test-compilation-state (get-buffer "*Cargo Search*") root)
           :cargo-log (cargo389-test-log root "cargo.log")))))
"####,
            expect![[
                r#"OK (:result (:new (:status exit :exit 0) :selected t :opened (:name "main.rs" :file "demo-界/src/main.rs" :mode fundamental-mode :modified nil :point 8 :text "fn main() { println!(\"hello 界\"); }\n") :prompt "Search (default main): " :search (:status exit :exit 0) :search-buffer (:name "*Cargo Search*" :mode cargo-process-mode :truncate t :text "-*- mode: cargo-process; default-directory: \"[ROOT]/\" -*-\nCargo-Process started at [TIME]\n\n[ROOT]/bin/cargo search serde   \nserde = \"1.0.219\"    # serialization framework\n\nCargo-Process finished at [TIME], duration [DURATION]\n") :cargo-log ((:argv "metadata|--format-version|1|--no-deps" :backtrace nil) (:argv "new|demo-界|--bin" :backtrace nil) (:argv "metadata|--format-version|1|--no-deps" :backtrace nil) (:argv "search|serde" :backtrace nil))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
            ]],
        ),
    ]
}

#[test]
fn cargo_package_batch() {
    assert_oracle_batch_cases(oracle(), "cargo package batch", "cargo_parity", &cases());
}
