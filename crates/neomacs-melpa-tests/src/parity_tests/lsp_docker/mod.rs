use std::time::Duration;

use crate::{CachedMelpaOracle, LSP_DOCKER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const LSP_DOCKER_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// Real project and process boundaries shared by the lsp-docker workflows.
///
/// The package's subject is never replaced. Each case creates a real project,
/// a real LSP session and base client, and (when needed) a recording `docker`
/// executable under the Rust-owned sandbox. Public lsp-docker commands still
/// parse YAML, query Docker through `call-process`, clone/register LSP clients,
/// and construct the transport command themselves.
const LSP_DOCKER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(let ((max-lisp-eval-depth 10000))
  (require 'lsp-docker))

(defvar neomacs-lsp-docker-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar neomacs-lsp-docker-test-hook-plists nil)

(defun neomacs-lsp-docker-test-write (path content &optional executable)
  "Write CONTENT to PATH and optionally make it EXECUTABLE."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert content)
      (write-region (point-min) (point-max) path nil 'silent)))
  (when executable (set-file-modes path #o755))
  path)

(defun neomacs-lsp-docker-test-before-register (client)
  "Remember the hook-symbol plist LSP Mode is about to replace for CLIENT."
  (let ((hook
         (intern (format "lsp-%s-after-open-hook"
                         (lsp--client-server-id client)))))
    (unless (assq hook neomacs-lsp-docker-test-hook-plists)
      (push (cons hook (copy-tree (symbol-plist hook)))
            neomacs-lsp-docker-test-hook-plists))))

(defun neomacs-lsp-docker-test-normalize (value project-root)
  "Replace PROJECT-ROOT in string VALUE with a stable marker."
  (replace-regexp-in-string
   (regexp-quote (directory-file-name project-root))
   "<PROJECT>" value t t))

(defun neomacs-lsp-docker-test-normalize-strings (values project-root)
  "Copy and normalize every string in VALUES."
  (mapcar (lambda (value)
            (neomacs-lsp-docker-test-normalize
             (copy-sequence value) project-root))
          values))

(defun neomacs-lsp-docker-test-docker-calls (trace project-root)
  "Read normalized Docker argument vectors from TRACE, oldest first."
  (if (not (file-exists-p trace))
      nil
    (with-temp-buffer
      (insert-file-contents trace)
      (mapcar
       (lambda (line)
         (mapcar
          (lambda (argument)
            (neomacs-lsp-docker-test-normalize
             (copy-sequence argument) project-root))
          (cdr (split-string line "\t"))))
       (split-string (buffer-string) "\n" t)))))

(defconst neomacs-lsp-docker-test-docker-prefix
  (string-join
   '("#!/bin/sh"
     "printf 'CALL' >> \"$NEOMACS_LSP_DOCKER_TRACE\""
     "for argument in \"$@\"; do"
     "  printf '\\t%s' \"$argument\" >> \"$NEOMACS_LSP_DOCKER_TRACE\""
     "done"
     "printf '\\n' >> \"$NEOMACS_LSP_DOCKER_TRACE\"")
   "\n"))

(defmacro neomacs-lsp-docker-test-with-project (name docker-body &rest body)
  "Run BODY in an isolated project with a recording Docker stand-in."
  (declare (indent 2) (debug (form form body)))
  `(let* ((case-root
           (file-name-as-directory
            (expand-file-name ,name neomacs-lsp-docker-test-root)))
          (project-root
           (file-name-as-directory (expand-file-name "project" case-root)))
          (source-file (expand-file-name "src/app.py" project-root))
          (outside-file (expand-file-name "outside/vendor.py" case-root))
          (bin (file-name-as-directory (expand-file-name "bin" case-root)))
          (docker (expand-file-name "docker" bin))
          (trace (expand-file-name "docker-calls.log" case-root))
          (buffers-before (buffer-list))
          (process-environment (copy-sequence process-environment))
          (exec-path (cons (directory-file-name bin) exec-path))
          (lsp-clients (make-hash-table :test 'eq))
          (lsp--session (make-lsp-session))
          (lsp-session-file (expand-file-name "lsp-session" case-root))
          (lsp-workspace-folders-changed-functions nil)
          (lsp-auto-register-remote-clients nil)
          (lsp-docker-command "docker")
          (lsp-docker-container-name-suffix 0)
          (neomacs-lsp-docker-test-hook-plists nil))
     (when (file-directory-p case-root)
       (delete-directory case-root t))
     (make-directory (file-name-directory source-file) t)
     (neomacs-lsp-docker-test-write
      source-file "from service import deploy\n\ndeploy(\"release-42\")\n")
     (neomacs-lsp-docker-test-write
      docker
      (concat neomacs-lsp-docker-test-docker-prefix "\n" ,docker-body "\n")
      t)
     (setenv "NEOMACS_LSP_DOCKER_TRACE" trace)
     (setenv "PATH" (concat bin path-separator (getenv "PATH")))
     (puthash
      'pylsp
      (make-lsp-client
       :new-connection (lsp-stdio-connection '("pylsp" "--stdio"))
       :major-modes '(python-mode python-ts-mode)
       :server-id 'pylsp
       :priority 5
       :activation-fn
       (lambda (file mode)
         (and (memq mode '(python-mode python-ts-mode))
              (string-suffix-p ".py" file))))
      lsp-clients)
     (advice-add 'lsp-register-client :before
                 #'neomacs-lsp-docker-test-before-register)
     (unwind-protect
         (progn
           (lsp-workspace-folders-add project-root)
           (with-current-buffer (find-file-noselect source-file)
             (setq default-directory project-root)
             ,@body))
       (advice-remove 'lsp-register-client
                      #'neomacs-lsp-docker-test-before-register)
       (dolist (entry neomacs-lsp-docker-test-hook-plists)
         (setplist (car entry) (cdr entry)))
       (dolist (buffer (buffer-list))
         (when (and (not (memq buffer buffers-before))
                    (buffer-live-p buffer))
           (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer)))
       (when (file-directory-p case-root)
         (delete-directory case-root t)))))
"####;

fn lsp_docker_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LSP_DOCKER_MELPA_PIN, "lsp-docker.el")
        .expect("prepare exact shallow lsp-docker source graph below ./tmp")
        .with_prelude(LSP_DOCKER_TEST_PRELUDE)
        .with_timeout(LSP_DOCKER_TEST_TIMEOUT)
}

fn assert_lsp_docker_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        lsp_docker_oracle(),
        "lsp-docker-package-batch",
        "lsp_docker_parity",
        cases,
    );
}

#[test]
fn lsp_docker_package_batch() {
    assert_lsp_docker_batch(&workflows::workflow_batch_cases());
}
