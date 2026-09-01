//! Practical flycheck-rust parity against the exact locked MELPA source.
//!
//! The corpus drives the documented Flycheck hook, public setup command, and
//! real `rust-cargo` checker while an owned Cargo executable supplies exact
//! metadata and diagnostics.  No flycheck-rust function is replaced.

use std::time::Duration;

use expect_test::expect;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, FLYCHECK_MELPA_PIN, FLYCHECK_RUST_MELPA_PIN,
    LET_ALIST_GNU_ELPA_PIN, RUST_MODE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'json)
(require 'seq)
(require 'rust-mode)
(require 'flycheck)
(require 'flycheck-rust)

;; UTF-8 file I/O lazily creates this editor-owned buffer.  Establish it
;; before case baselines so it is not mistaken for package residue.
(get-buffer-create " *code-conversion-work*")

(defvar fcr379-test-root nil)

(defun fcr379-test-path (relative)
  (expand-file-name relative fcr379-test-root))

(defun fcr379-test-write (path bytes)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert bytes)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun fcr379-test-read (path)
  (let ((coding-system-for-read 'utf-8-unix))
    (with-temp-buffer
      (insert-file-contents path)
      (buffer-string))))

(defun fcr379-test-normalize (value)
  (when value
    (replace-regexp-in-string
     (regexp-quote (directory-file-name fcr379-test-root))
     "[ROOT]" value t t)))

(defun fcr379-test-install-cargo ()
  (let ((cargo (fcr379-test-path "bin/cargo")))
    (fcr379-test-write
     cargo
     (concat
      "#!/bin/sh\n"
      "set -eu\n"
      "printf 'BEGIN\\n' >> \"$FCR379_CARGO_LOG\"\n"
      "printf 'cwd=%s\\n' \"$PWD\" >> \"$FCR379_CARGO_LOG\"\n"
      "for arg do printf 'arg=%s\\n' \"$arg\" >> \"$FCR379_CARGO_LOG\"; done\n"
      "printf 'END\\n' >> \"$FCR379_CARGO_LOG\"\n"
      "emit_file () {\n"
      "  while IFS= read -r line || test -n \"$line\"; do\n"
      "    printf '%s\\n' \"$line\"\n"
      "  done < \"$1\"\n"
      "}\n"
      "case \"${1-}\" in\n"
      "  metadata) emit_file \"$FCR379_METADATA\" ;;\n"
      "  check|test)\n"
      "    broken=0\n"
      "    while IFS= read -r line || test -n \"$line\"; do\n"
      "      case \"$line\" in *BROKEN*) broken=1 ;; esac\n"
      "    done < \"$FCR379_SOURCE\"\n"
      "    if test \"$broken\" = 1; then\n"
      "      emit_file \"$FCR379_DIAGNOSTICS\"\n"
      "      exit 1\n"
      "    fi\n"
      "    printf '%s\\n' '{\"reason\":\"build-finished\",\"success\":true}'\n"
      "    ;;\n"
      "  *) printf 'unexpected cargo command: %s\\n' \"$*\" >&2; exit 64 ;;\n"
      "esac\n"))
    (set-file-modes cargo #o755)
    cargo))

(defun fcr379-test-target (kind name source &optional features)
  `((kind . [,kind])
    (crate_types . [,kind])
    (name . ,name)
    (src_path . ,source)
    (required-features . ,(vconcat features))))

(defun fcr379-test-write-metadata (packages)
  (let ((path (fcr379-test-path "metadata.json")))
    (fcr379-test-write
     path
     (concat
      (json-encode
       `((packages . ,(vconcat packages))
         (workspace_root . ,(directory-file-name fcr379-test-root))))
      "\n"))
    path))

(defun fcr379-test-package (name manifest targets)
  `((name . ,name)
    (manifest_path . ,manifest)
    (targets . ,(vconcat targets))))

(defun fcr379-test-ledger ()
  (let ((path (fcr379-test-path "cargo.log")) entries current active)
    (when (file-exists-p path)
      (dolist (line (split-string (fcr379-test-read path) "\n" t))
        (cond
         ((equal line "BEGIN")
          (when active (error "Nested Cargo ledger record"))
          (setq active t current nil))
         ((equal line "END")
          (unless active (error "Cargo ledger END without BEGIN"))
          (push (mapcar #'fcr379-test-normalize (nreverse current)) entries)
          (setq active nil current nil))
         (active (push line current))
         (t (error "Cargo ledger data outside record: %S" line))))
      (when active (error "Truncated Cargo ledger record")))
    (nreverse entries)))

(defun fcr379-test-config-state ()
  (list :crate-type flycheck-rust-crate-type
        :binary-name flycheck-rust-binary-name
        :features flycheck-rust-features
        :local (mapcar #'local-variable-p
                       '(flycheck-rust-crate-type
                         flycheck-rust-binary-name
                         flycheck-rust-features))))

(defun fcr379-test-run-flycheck ()
  (let (finished (rounds 0))
    (add-hook 'flycheck-after-syntax-check-hook
              (lambda () (setq finished t)) nil t)
    (call-interactively #'flycheck-buffer)
    (while (and (not finished) (< rounds 600))
      (let ((process
             (and flycheck-current-syntax-check
                  (flycheck-syntax-check-context
                   flycheck-current-syntax-check))))
        (unless (processp process)
          (error "Flycheck has no owned checker process at round %S: %S"
                 rounds flycheck-current-syntax-check))
        (accept-process-output process 0.05))
      (setq rounds (1+ rounds)))
    (unless finished
      (error "Timed out waiting for Flycheck: %S" flycheck-last-status-change))
    (list :status flycheck-last-status-change)))

(defun fcr379-test-diagnostics ()
  (mapcar
   (lambda (diagnostic)
     (list :file (file-relative-name
                  (flycheck-error-filename diagnostic) fcr379-test-root)
           :line (flycheck-error-line diagnostic)
           :column (flycheck-error-column diagnostic)
           :end-line (flycheck-error-end-line diagnostic)
           :end-column (flycheck-error-end-column diagnostic)
           :level (flycheck-error-level diagnostic)
           :checker (flycheck-error-checker diagnostic)
           :id (flycheck-error-id diagnostic)
           :message (flycheck-error-message diagnostic)))
   flycheck-current-errors))

(defun fcr379-test-overlays ()
  (mapcar
   (lambda (overlay)
     (let ((diagnostic (overlay-get overlay 'flycheck-error)))
       (list :start (overlay-start overlay)
             :end (overlay-end overlay)
             :id (and diagnostic (flycheck-error-id diagnostic))
             :face (overlay-get overlay 'face))))
   (sort (flycheck-overlays-in (point-min) (point-max))
         (lambda (left right) (< (overlay-start left) (overlay-start right))))))

(defun fcr379-test-hash-identity-state (table)
  (let (entries)
    (maphash (lambda (key value) (push (cons key value) entries)) table)
    entries))

(defun fcr379-test-hash-identity-state-p (table state)
  (and (= (hash-table-count table) (length state))
       (let ((missing (make-symbol "missing")))
         (seq-every-p
          (lambda (entry)
            (eq (gethash (car entry) table missing) (cdr entry)))
          state))))

(defun fcr379-test-run (name thunk)
  (let* ((fcr379-test-root
          (file-name-as-directory
           (expand-file-name (concat "flycheck-rust379/" name "/")
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (buffer-list-update-hook-before buffer-list-update-hook)
         (flycheck-last-buffer-before flycheck--last-buffer)
         (flycheck-last-displayed-message-before flycheck--last-displayed-message)
         (next-error-last-buffer-before next-error-last-buffer)
         (project-error-store-before
          (fcr379-test-hash-identity-state flycheck--project-error-store))
         result body-error cleanup-errors)
    (when (file-exists-p fcr379-test-root)
      (delete-directory fcr379-test-root t))
    (make-directory fcr379-test-root t)
    (unwind-protect
        (condition-case error
            (setq result
                  (save-window-excursion
                    (save-current-buffer
                      (funcall thunk))))
          (error (setq body-error error)))
      (dolist (buffer (buffer-list))
        (when (and (not (memq buffer buffers-before))
                   (buffer-live-p buffer))
          (condition-case error
              (progn
                (with-current-buffer buffer
                  (when (bound-and-true-p flycheck-mode)
                    (flycheck-stop)
                    (flycheck-mode -1))
                  (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :kill-buffer (buffer-name buffer) error)
                         cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case error
              (cancel-timer timer)
            (error (push (list :cancel-timer error) cleanup-errors)))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case error
              (progn
                (when (process-live-p process) (delete-process process))
                (unless (memq (process-status process) '(exit signal closed failed))
                  (error "process did not terminate: %S" process)))
            (error (push (list :delete-process (process-name process) error)
                         cleanup-errors)))))
      (unless (equal buffer-list-update-hook buffer-list-update-hook-before)
        (push (list :buffer-list-update-hook
                    buffer-list-update-hook-before buffer-list-update-hook)
              cleanup-errors))
      (unless (fcr379-test-hash-identity-state-p
               flycheck--project-error-store project-error-store-before)
        (push (list :project-error-store
                    :before-count (length project-error-store-before)
                    :after-count (hash-table-count flycheck--project-error-store))
              cleanup-errors))
      (setq flycheck--last-buffer flycheck-last-buffer-before
            flycheck--last-displayed-message
            flycheck-last-displayed-message-before
            next-error-last-buffer next-error-last-buffer-before)
      (condition-case error
          (when (file-exists-p fcr379-test-root)
            (delete-directory fcr379-test-root t))
        (error (push (list :delete-root error) cleanup-errors)))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (push (list :remaining-buffer (buffer-name buffer)) cleanup-errors)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (push (list :remaining-process (process-name process)) cleanup-errors)))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (push (list :remaining-timer t) cleanup-errors))))
    (cond
     ((and body-error cleanup-errors)
      (error "flycheck-rust body failed %S; cleanup failed %S"
             body-error (nreverse cleanup-errors)))
     (body-error (signal (car body-error) (cdr body-error)))
     (cleanup-errors
      (error "flycheck-rust cleanup failed: %S" (nreverse cleanup-errors)))
     (t result))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLYCHECK_RUST_MELPA_PIN, "flycheck-rust.el")
        .expect("prepare pinned flycheck-rust source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned Dash dependency below ./tmp")
        .with_melpa_dependency(FLYCHECK_MELPA_PIN)
        .expect("prepare pinned Flycheck dependency below ./tmp")
        .with_melpa_dependency(RUST_MODE_MELPA_PIN)
        .expect("prepare pinned Rust Mode dependency below ./tmp")
        .with_gnu_elpa_dependency(LET_ALIST_GNU_ELPA_PIN)
        .expect("prepare pinned let-alist dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn documented_flycheck_hook_configures_a_feature_gated_binary_target() -> ParityBatchCase {
    ParityBatchCase::value(
        "documented_flycheck_hook_configures_a_feature_gated_binary_target",
        r####"(fcr379-test-run
 "documented-hook"
 (lambda ()
   (let* ((manifest (fcr379-test-write
                     (fcr379-test-path "workspace space 界/Cargo.toml")
                     "[package]\nname = \"release-cli\"\nversion = \"0.1.0\"\n"))
          (source (fcr379-test-write
                   (fcr379-test-path "workspace space 界/src/bin/release.rs")
                   "fn main() { println!(\"café 界\"); }\n"))
          (metadata
           (fcr379-test-write-metadata
            (list (fcr379-test-package
                   "release-cli" manifest
                   (list (fcr379-test-target
                          "bin" "release-cli" source '("audit" "json")))))))
          (cargo (fcr379-test-install-cargo))
          (log (fcr379-test-path "cargo.log"))
          (diagnostics (fcr379-test-write
                        (fcr379-test-path "diagnostics.jsonl") ""))
          (buffer (find-file-noselect source))
          (process-environment (copy-sequence process-environment))
          (exec-path (cons (file-name-directory cargo) exec-path))
          (flycheck-mode-hook (cons #'flycheck-rust-setup
                                    flycheck-mode-hook))
          (flycheck-check-syntax-automatically nil))
     (setenv "PATH" (concat (file-name-directory cargo)
                            path-separator (or (getenv "PATH") "")))
     (setenv "FCR379_CARGO_LOG" log)
     (setenv "FCR379_METADATA" metadata)
     (setenv "FCR379_DIAGNOSTICS" diagnostics)
     (setenv "FCR379_SOURCE" source)
     (switch-to-buffer buffer)
     (rust-mode)
     (flycheck-mode 1)
     (list :mode major-mode
           :flycheck-mode flycheck-mode
           :config (fcr379-test-config-state)
           :ledger (fcr379-test-ledger)))))"####,
        expect![[
            r#"OK (:mode rust-mode :flycheck-mode t :config (:crate-type "bin" :binary-name "release-cli" :features ("audit" "json") :local (t t t)) :ledger (("cwd=[ROOT]/workspace space 界/src/bin" "arg=metadata" "arg=--no-deps" "arg=--manifest-path" "arg=[ROOT]/workspace space 界/Cargo.toml" "arg=--format-version" "arg=1")))"#
        ]],
    )
}

fn public_setup_selects_exact_closest_and_custom_build_targets() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_setup_selects_exact_closest_and_custom_build_targets",
        r####"(fcr379-test-run
 "target-selection"
 (lambda ()
   (let* ((manifest (fcr379-test-write
                     (fcr379-test-path "Cargo.toml")
                     "[package]\nname = \"workspace-tools\"\nversion = \"0.1.0\"\n"))
          (build (fcr379-test-write
                  (fcr379-test-path "build.rs") "fn main() {}\n"))
          (binary (fcr379-test-write
                   (fcr379-test-path "src/main.rs") "fn main() {}\n"))
          (test-target (fcr379-test-write
                        (fcr379-test-path "tests/a.rs") "#[test] fn a() {}\n"))
          (support (fcr379-test-write
                    (fcr379-test-path "tests/support/mod.rs")
                    "pub fn helper() {}\n"))
          (metadata
           (fcr379-test-write-metadata
            (list
             (fcr379-test-package
              "workspace-tools" manifest
              (list (fcr379-test-target "custom-build" "build-script" build)
                    (fcr379-test-target "bin" "workspace-tools" binary)
                    (fcr379-test-target "test" "integration-a" test-target))))))
          (cargo (fcr379-test-install-cargo))
          (log (fcr379-test-path "cargo.log"))
          (diagnostics (fcr379-test-write
                        (fcr379-test-path "diagnostics.jsonl") ""))
          (process-environment (copy-sequence process-environment))
          (exec-path (cons (file-name-directory cargo) exec-path)))
     (setenv "PATH" (concat (file-name-directory cargo)
                            path-separator (or (getenv "PATH") "")))
     (setenv "FCR379_CARGO_LOG" log)
     (setenv "FCR379_METADATA" metadata)
     (setenv "FCR379_DIAGNOSTICS" diagnostics)
     (setenv "FCR379_SOURCE" binary)
     (let (states)
       (dolist (entry `((exact ,binary)
                        (closest ,support)
                        (custom-build ,build)))
         (let ((buffer (find-file-noselect (cadr entry))))
           (switch-to-buffer buffer)
           (rust-mode)
           (call-interactively #'flycheck-rust-setup)
           (push (list (car entry) (fcr379-test-config-state)) states)))
       (list :targets (nreverse states)
             :ledger (fcr379-test-ledger))))))"####,
        expect![[
            r#"OK (:targets ((exact (:crate-type "bin" :binary-name "workspace-tools" :features nil :local (t t t))) (closest (:crate-type "test" :binary-name "integration-a" :features nil :local (t t t))) (custom-build (:crate-type "bin" :binary-name "workspace-tools" :features nil :local (t t t)))) :ledger (("cwd=[ROOT]/src" "arg=metadata" "arg=--no-deps" "arg=--manifest-path" "arg=[ROOT]/Cargo.toml" "arg=--format-version" "arg=1") ("cwd=[ROOT]/tests/support" "arg=metadata" "arg=--no-deps" "arg=--manifest-path" "arg=[ROOT]/Cargo.toml" "arg=--format-version" "arg=1") ("cwd=[ROOT]" "arg=metadata" "arg=--no-deps" "arg=--manifest-path" "arg=[ROOT]/Cargo.toml" "arg=--format-version" "arg=1")))"#
        ]],
    )
}

fn public_setup_normalizes_proc_macro_and_preserves_required_features() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_setup_normalizes_proc_macro_and_preserves_required_features",
        r####"(fcr379-test-run
 "kind-and-features"
 (lambda ()
   (let* ((manifest (fcr379-test-write
                     (fcr379-test-path "Cargo.toml")
                     "[package]\nname = \"derive-界\"\nversion = \"0.1.0\"\n"))
          (source (fcr379-test-write
                   (fcr379-test-path "src/lib.rs")
                   "extern crate proc_macro;\n"))
          (metadata
           (fcr379-test-write-metadata
            (list (fcr379-test-package
                   "derive-界" manifest
                   (list (fcr379-test-target
                          "proc-macro" "derive-界" source
                          '("derive" "unicode")))))))
          (cargo (fcr379-test-install-cargo))
          (log (fcr379-test-path "cargo.log"))
          (diagnostics (fcr379-test-write
                        (fcr379-test-path "diagnostics.jsonl") ""))
          (buffer (find-file-noselect source))
          (process-environment (copy-sequence process-environment))
          (exec-path (cons (file-name-directory cargo) exec-path)))
     (setenv "PATH" (concat (file-name-directory cargo)
                            path-separator (or (getenv "PATH") "")))
     (setenv "FCR379_CARGO_LOG" log)
     (setenv "FCR379_METADATA" metadata)
     (setenv "FCR379_DIAGNOSTICS" diagnostics)
     (setenv "FCR379_SOURCE" source)
     (switch-to-buffer buffer)
     (rust-mode)
     (call-interactively #'flycheck-rust-setup)
     (list :config (fcr379-test-config-state)
           :ledger (fcr379-test-ledger)))))"####,
        expect![[
            r#"OK (:config (:crate-type "lib" :binary-name "derive-界" :features ("derive" "unicode") :local (t t t)) :ledger (("cwd=[ROOT]/src" "arg=metadata" "arg=--no-deps" "arg=--manifest-path" "arg=[ROOT]/Cargo.toml" "arg=--format-version" "arg=1")))"#
        ]],
    )
}

fn outside_project_and_missing_cargo_are_safe_then_public_setup_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "outside_project_and_missing_cargo_are_safe_then_public_setup_recovers",
        r####"(fcr379-test-run
 "failure-recovery"
 (lambda ()
   (let* ((manifest (fcr379-test-write
                     (fcr379-test-path "project/Cargo.toml")
                     "[package]\nname = \"recoverable\"\nversion = \"0.1.0\"\n"))
          (source (fcr379-test-write
                   (fcr379-test-path "project/src/main.rs")
                   "fn main() {}\n"))
          (metadata
           (fcr379-test-write-metadata
            (list (fcr379-test-package
                   "recoverable" manifest
                   (list (fcr379-test-target
                          "bin" "recoverable" source '("repair")))))))
          (log (fcr379-test-path "cargo.log"))
          (diagnostics (fcr379-test-write
                        (fcr379-test-path "diagnostics.jsonl") ""))
          (outside-buffer (generate-new-buffer " *fcr379-outside-project*"))
          (inside-buffer (find-file-noselect source)))
     (switch-to-buffer outside-buffer)
     (setq buffer-file-name "/__fcr379_manifest_free__/scratch.rs")
     (rust-mode)
     (setq-local flycheck-rust-crate-type "sentinel-kind"
                 flycheck-rust-binary-name "sentinel-name"
                 flycheck-rust-features '("sentinel-feature"))
     (call-interactively #'flycheck-rust-setup)
     (let ((outside-state (fcr379-test-config-state)) missing-state missing-message)
       (switch-to-buffer inside-buffer)
       (rust-mode)
       (let ((process-environment (copy-sequence process-environment))
             (exec-path nil)
             observed-messages)
         (setenv "PATH" "")
         (cl-letf (((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let ((text (apply #'format-message
                                         format-string arguments)))
                        (push text observed-messages)
                        text))))
           (call-interactively #'flycheck-rust-setup))
         (setq missing-state (fcr379-test-config-state)
               missing-message (car observed-messages)))
       (let* ((cargo (fcr379-test-install-cargo))
              (process-environment (copy-sequence process-environment))
              (exec-path (cons (file-name-directory cargo) exec-path)))
         (setenv "PATH" (concat (file-name-directory cargo)
                                path-separator (or (getenv "PATH") "")))
         (setenv "FCR379_CARGO_LOG" log)
         (setenv "FCR379_METADATA" metadata)
         (setenv "FCR379_DIAGNOSTICS" diagnostics)
         (setenv "FCR379_SOURCE" source)
         (call-interactively #'flycheck-rust-setup)
         (list :outside outside-state
               :missing (list :message missing-message :state missing-state)
               :recovered (fcr379-test-config-state)
               :ledger (fcr379-test-ledger)))))))"####,
        expect![[
            r#"OK (:outside (:crate-type "sentinel-kind" :binary-name "sentinel-name" :features ("sentinel-feature") :local (t t t)) :missing (:message "Error in flycheck-rust-setup: (user-error \"flycheck-rust cannot find ‘cargo’.  Please make sure that cargo is installed and on your PATH.  See http://www.flycheck.org/en/latest/user/troubleshooting.html for more information on setting your PATH with Emacs.\")" :state (:crate-type "lib" :binary-name nil :features nil :local (nil nil nil))) :recovered (:crate-type "bin" :binary-name "recoverable" :features ("repair") :local (t t t)) :ledger (("cwd=[ROOT]/project/src" "arg=metadata" "arg=--no-deps" "arg=--manifest-path" "arg=[ROOT]/project/Cargo.toml" "arg=--format-version" "arg=1")))"#
        ]],
    )
}

fn configured_target_drives_rust_cargo_diagnostics_navigation_and_recovery() -> ParityBatchCase {
    ParityBatchCase::value(
        "configured_target_drives_rust_cargo_diagnostics_navigation_and_recovery",
        r####"(fcr379-test-run
 "rust-cargo-integration"
 (lambda ()
   (let* ((manifest (fcr379-test-write
                     (fcr379-test-path "Cargo.toml")
                     "[package]\nname = \"release-cli\"\nversion = \"0.1.0\"\n"))
          (source (fcr379-test-write
                   (fcr379-test-path "src/bin/release.rs")
                   (concat "fn main() {\n"
                           "    let total = BROKEN;\n"
                           "    println!(\"total café 界: {total}\");\n"
                           "}\n")))
          (metadata
           (fcr379-test-write-metadata
            (list (fcr379-test-package
                   "release-cli" manifest
                   (list (fcr379-test-target
                          "bin" "release-cli" source '("audit" "json")))))))
          (diagnostics
           (fcr379-test-write
            (fcr379-test-path "diagnostics.jsonl")
            (concat
             (json-encode
              '((reason . "compiler-message")
                (message
                 (message . "cannot find value `BROKEN` in this scope")
                 (code (code . "E0425"))
                 (level . "error")
                 (spans
                  . [((file_name . "src/bin/release.rs")
                      (line_start . 2)
                      (line_end . 2)
                      (column_start . 17)
                      (column_end . 23)
                      (is_primary . t)
                      (label . "not found in this scope"))])
                 (children . []))))
             "\n")))
          (cargo (fcr379-test-install-cargo))
          (log (fcr379-test-path "cargo.log"))
          (buffer (find-file-noselect source))
          (process-environment (copy-sequence process-environment))
          (exec-path (cons (file-name-directory cargo) exec-path))
          (flycheck-mode-hook (cons #'flycheck-rust-setup
                                    flycheck-mode-hook))
          (flycheck-check-syntax-automatically nil)
          (flycheck-disabled-checkers '(rust rust-clippy))
          (flycheck-rust-check-tests nil))
     (setenv "PATH" (concat (file-name-directory cargo)
                            path-separator (or (getenv "PATH") "")))
     (setenv "FCR379_CARGO_LOG" log)
     (setenv "FCR379_METADATA" metadata)
     (setenv "FCR379_DIAGNOSTICS" diagnostics)
     (setenv "FCR379_SOURCE" source)
     (switch-to-buffer buffer)
     (rust-mode)
     (setq-local flycheck-checker 'rust-cargo)
     (flycheck-mode 1)
     (let ((first-run (fcr379-test-run-flycheck)))
       (goto-char (point-min))
       (call-interactively #'flycheck-next-error)
       (let ((before
              (list :run first-run
                    :config (fcr379-test-config-state)
                    :diagnostics (fcr379-test-diagnostics)
                    :overlays (fcr379-test-overlays)
                    :navigation
                    (list :line (line-number-at-pos)
                          :column (current-column)
                          :messages
                          (mapcar #'flycheck-error-message
                                  (flycheck-overlay-errors-at (point)))))))
         (goto-char (point-min))
         (search-forward "BROKEN")
         (replace-match "42" t t)
         (save-buffer)
         (let ((second-run (fcr379-test-run-flycheck)))
           (list
            :before before
            :after
            (list :run second-run
                  :diagnostics (fcr379-test-diagnostics)
                  :overlays (fcr379-test-overlays)
                  :modified (buffer-modified-p)
                  :text (buffer-substring-no-properties
                         (point-min) (point-max)))
            :ledger (fcr379-test-ledger))))))))"####,
        expect![[
            r#"OK (:before (:run (:status finished) :config (:crate-type "bin" :binary-name "release-cli" :features ("audit" "json") :local (t t t)) :diagnostics ((:file "src/bin/release.rs" :line 2 :column 17 :end-line 2 :end-column 23 :level error :checker rust-cargo :id "E0425" :message "cannot find value `BROKEN` in this scope (not found in this scope)")) :overlays ((:start 29 :end 35 :id "E0425" :face flycheck-error)) :navigation (:line 2 :column 16 :messages ("cannot find value `BROKEN` in this scope (not found in this scope)"))) :after (:run (:status finished) :diagnostics nil :overlays nil :modified nil :text "fn main() {\n    let total = 42;\n    println!(\"total café 界: {total}\");\n}\n") :ledger (("cwd=[ROOT]/src/bin" "arg=metadata" "arg=--no-deps" "arg=--manifest-path" "arg=[ROOT]/Cargo.toml" "arg=--format-version" "arg=1") ("cwd=[ROOT]" "arg=check" "arg=--bin" "arg=release-cli" "arg=--features=audit,json" "arg=--message-format=json") ("cwd=[ROOT]/src/bin" "arg=metadata" "arg=--no-deps" "arg=--format-version" "arg=1") ("cwd=[ROOT]" "arg=check" "arg=--bin" "arg=release-cli" "arg=--features=audit,json" "arg=--message-format=json") ("cwd=[ROOT]/src/bin" "arg=metadata" "arg=--no-deps" "arg=--format-version" "arg=1")))"#
        ]],
    )
}

#[test]
fn flycheck_rust_package_batch() {
    let cases = vec![
        documented_flycheck_hook_configures_a_feature_gated_binary_target(),
        public_setup_selects_exact_closest_and_custom_build_targets(),
        public_setup_normalizes_proc_macro_and_preserves_required_features(),
        outside_project_and_missing_cargo_are_safe_then_public_setup_recovers(),
        configured_target_drives_rust_cargo_diagnostics_navigation_and_recovery(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed flycheck-rust parity test");
    assert_oracle_batch_cases(oracle(), test_name, "flycheck_rust_parity", &cases);
}
