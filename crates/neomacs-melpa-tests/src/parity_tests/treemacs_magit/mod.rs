use std::time::Duration;

use crate::{
    CachedMelpaOracle, MAGIT_MELPA_PIN, PFUTURE_MELPA_PIN, TREEMACS_MAGIT_MELPA_PIN,
    TREEMACS_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TREEMACS_MAGIT_TEST_TIMEOUT: Duration = Duration::from_secs(240);

const TREEMACS_MAGIT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

;; Give every real Git process deterministic identity and branch behavior.
(setq magit-git-global-arguments
      (append
       '("-c" "init.defaultBranch=master"
         "-c" "core.quotePath=false"
         "-c" "user.name=Parity User"
         "-c" "user.email=parity@example.invalid")
       (and (boundp 'magit-git-global-arguments)
            magit-git-global-arguments)))

(require 'treemacs-magit)

(defvar neomacs-treemacs-magit-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun neomacs-treemacs-magit-test-write (path text)
  "Write TEXT to PATH inside the case sandbox."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region text nil path nil 'silent))
  path)

(defun neomacs-treemacs-magit-test-project (name)
  "Create a committed real Git project named NAME in the case sandbox."
  (let ((root (file-name-as-directory
               (expand-file-name name neomacs-treemacs-magit-test-root))))
    (when (file-directory-p root)
      (delete-directory root t))
    (make-directory root t)
    (let ((default-directory root))
      (magit-git "init" ".")
      (neomacs-treemacs-magit-test-write
       (expand-file-name "release.txt" root)
       "version=1\nchannel=stable\n")
      (magit-git "add" "release.txt")
      (magit-git "commit" "-m" "baseline"))
    root))

(defun neomacs-treemacs-magit-test-new-timers (current baseline)
  "Return timers in CURRENT that are absent from BASELINE."
  (cl-remove-if (lambda (timer) (memq timer baseline)) current))

(defun neomacs-treemacs-magit-test-await-idle-timers (baseline)
  "Wait until an idle timer appears beyond BASELINE and return all new ones."
  (let ((deadline (+ (float-time) 20.0))
        new)
    (while (and
            (null
             (setq new
                   (neomacs-treemacs-magit-test-new-timers
                    timer-idle-list baseline)))
            (< (float-time) deadline))
      (accept-process-output nil 0.02))
    (unless new
      (error "Treemacs-Magit idle update was not scheduled"))
    new))

(defun neomacs-treemacs-magit-test-await-processes (baseline)
  "Wait until every process created after BASELINE has completed."
  (let ((deadline (+ (float-time) 20.0))
        live)
    (while (and
            (setq live
                  (cl-remove-if-not
                   (lambda (process)
                     (and (not (memq process baseline))
                          (process-live-p process)))
                   (process-list)))
            (< (float-time) deadline))
      (accept-process-output nil 0.02))
    (when live
      (error "Treemacs-Magit processes did not finish: %S" live))))

(defun neomacs-treemacs-magit-test-await-process (process)
  "Wait for Magit's concrete PROCESS without waiting on editor servers."
  (let ((deadline (+ (float-time) 20.0)))
    (while (and (process-live-p process)
                (< (float-time) deadline))
      (accept-process-output nil 0.02))
    (when (process-live-p process)
      (error "Treemacs-Magit process did not finish: %S" process))))

(defun neomacs-treemacs-magit-test-node-state (buffer path root)
  "Describe PATH's live node in BUFFER relative to ROOT."
  (with-current-buffer buffer
    (save-excursion
      (let ((position (treemacs-find-visible-node path)))
        (when position
          (goto-char position)
          (let ((node (treemacs-node-at-point)))
            (list :path (file-relative-name path root)
                  :label (treemacs--get-label-of node)
                  :state (treemacs-button-get node :state)
                  :face (get-text-property
                         (treemacs-button-start node) 'face))))))))

(defun neomacs-treemacs-magit-test-refresh-flags (buffer root)
  "Describe BUFFER's refresh flags for ROOT using sandbox-relative paths."
  (let ((canonical-root (treemacs-canonical-path root)))
    (with-current-buffer buffer
      (let ((node (treemacs-find-in-dom canonical-root)))
        (mapcar
         (lambda (flag)
           (cons (file-relative-name (car flag) canonical-root) (cdr flag)))
         (copy-tree
          (and node (treemacs-dom-node->refresh-flag node))))))))

(defun neomacs-treemacs-magit-test-complete-filewatch-refresh
    (buffer root)
  "Dispatch the real queued filewatch timer and describe completion."
  (let* ((timer treemacs--refresh-timer)
         (queued
          (list
           :refresh-flags
           (neomacs-treemacs-magit-test-refresh-flags buffer root)
           :timer-created (timerp timer)
           :timer-active (and (memq timer timer-list) t))))
    (when (timerp timer)
      (timer-event-handler timer))
    (list
     :queued queued
     :completed
     (list
      :refresh-flags
      (neomacs-treemacs-magit-test-refresh-flags buffer root)
      :timer-cleared (null treemacs--refresh-timer)
      :timer-active (and (memq timer timer-list) t)))))

(defmacro neomacs-treemacs-magit-test-with-project (name git-kind &rest body)
  "Run BODY with a real Git project and isolated live Treemacs workspace."
  (declare (indent 2) (debug (form form body)))
  `(save-window-excursion
     (let* ((origin-buffer (current-buffer))
            (buffers-before (buffer-list))
            (root (neomacs-treemacs-magit-test-project ,name))
            (default-directory root)
            (default-workspace (treemacs-workspace->create! :name "Release"))
            (treemacs--workspaces (list default-workspace))
            (treemacs--disabled-workspaces nil)
            (treemacs--scope-storage nil)
            (treemacs-persist-file
             (expand-file-name "state/treemacs-persist" root))
            (treemacs-last-error-persist-file
             (expand-file-name "state/treemacs-persist-error" root))
            (treemacs-magit--timers nil)
            (treemacs--refresh-timer nil)
            (timer-idle-list timer-idle-list)
            (timer-list timer-list)
            (processes-before (process-list))
            (prior-git-mode treemacs-git-mode)
            (prior-git-kind treemacs--git-mode)
            (prior-status-process
             (symbol-function 'treemacs--git-status-process-function))
            (prior-status-parse
             (symbol-function 'treemacs--git-status-parse-function))
            treemacs-buffer)
       (unwind-protect
           (progn
             (setf (treemacs-current-workspace) default-workspace)
             (treemacs-do-add-project-to-workspace root "Release Service")
             (let ((treemacs-collapse-dirs 0)
                   (treemacs-expand-after-init t)
                   (treemacs-follow-after-init nil)
                   (treemacs-filewatch-mode nil)
                   (treemacs-space-between-root-nodes nil))
               (if ,git-kind
                   (treemacs-git-mode ,git-kind)
                 (treemacs-git-mode -1))
               (treemacs)
               (setq treemacs-buffer (treemacs-get-local-buffer))
               (neomacs-treemacs-magit-test-await-processes
                processes-before))
             ,@body)
         (when (timerp treemacs--refresh-timer)
           (cancel-timer treemacs--refresh-timer))
         (dolist (timer (copy-sequence timer-idle-list))
           (when (timerp timer)
             (cancel-timer timer)))
         (setq treemacs-git-mode prior-git-mode
               treemacs--git-mode prior-git-kind)
         (fset 'treemacs--git-status-process-function prior-status-process)
         (fset 'treemacs--git-status-parse-function prior-status-parse)
         (dolist (process (process-list))
           (when (and (not (memq process processes-before))
                      (process-live-p process))
             (delete-process process)))
         (when (buffer-live-p origin-buffer)
           (switch-to-buffer origin-buffer))
         (dolist (buffer (buffer-list))
           (when (and (not (memq buffer buffers-before))
                      (buffer-live-p buffer))
             (with-current-buffer buffer
               (set-buffer-modified-p nil))
             (kill-buffer buffer)))
         (when (file-directory-p root)
           (delete-directory root t))))))
"####;

fn treemacs_magit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TREEMACS_MAGIT_MELPA_PIN, "treemacs-magit.el")
        .expect("prepare exact shallow Treemacs-Magit source below ./tmp")
        .with_melpa_dependency(MAGIT_MELPA_PIN)
        .expect("prepare exact shallow Magit dependency below ./tmp")
        .with_melpa_dependency(PFUTURE_MELPA_PIN)
        .expect("prepare exact shallow Pfuture dependency below ./tmp")
        .with_melpa_dependency(TREEMACS_MELPA_PIN)
        .expect("prepare exact shallow Treemacs dependency below ./tmp")
        .with_prelude(TREEMACS_MAGIT_TEST_PRELUDE)
        .with_timeout(TREEMACS_MAGIT_TEST_TIMEOUT)
}

fn assert_treemacs_magit_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        treemacs_magit_oracle(),
        "treemacs-magit-package-batch",
        "Treemacs Magit",
        cases,
    );
}

#[test]
fn treemacs_magit_package_batch() {
    assert_treemacs_magit_batch(&workflows::workflow_batch_cases());
}
