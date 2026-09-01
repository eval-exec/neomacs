use std::time::Duration;

use crate::{CachedMelpaOracle, GIT_MODES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GIT_MODES_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)

(defvar gm353-test-owned-buffers nil)
(defconst gm353-test-components
  '(git-modes gitattributes-mode gitconfig-mode gitignore-mode compat))

(defconst gm353-test-mode-functions
  '(gitattributes-mode gitconfig-mode gitignore-mode))

(defun gm353-test-feature-state ()
  "Return exact package and dependency feature activation state."
  (mapcar (lambda (feature) (cons feature (and (featurep feature) t)))
          gm353-test-components))

(defun gm353-test-autoload-state ()
  "Return whether every public mode command is still an autoload."
  (mapcar (lambda (mode)
            (cons mode (and (autoloadp (symbol-function mode)) t)))
          gm353-test-mode-functions))

(defun gm353-test-registration-state ()
  "Describe Git Modes autoload registrations without cataloging their regexps."
  (let ((entries
         (cl-loop for entry in auto-mode-alist
                  when (memq (cdr entry) gm353-test-mode-functions)
                  collect entry)))
    (list :total (length entries)
          :unique (= (length entries)
                     (length (delete-dups (copy-tree entries))))
          :by-mode
          (mapcar
           (lambda (mode)
             (cons mode (cl-count mode entries :key #'cdr :test #'eq)))
           gm353-test-mode-functions))))

(defun gm353-test-write-file (root relative contents)
  "Write CONTENTS to owned RELATIVE file below ROOT."
  (let ((file (expand-file-name relative root)))
    (unless (string-prefix-p root file)
      (error "GIT-MODES fixture escaped owned root: %s" file))
    (make-directory (file-name-directory file) t)
    (with-temp-file file (insert contents))
    file))

(defun gm353-test-visit (root relative)
  "Visit owned RELATIVE file below ROOT through real file-mode selection."
  (let ((file (expand-file-name relative root)))
    (unless (and (file-exists-p file) (string-prefix-p root file))
      (error "GIT-MODES cannot visit unowned fixture: %s" file))
    (let ((buffer (find-file-noselect file)))
      (push buffer gm353-test-owned-buffers)
      buffer)))

(defun gm353-test-buffer-state (buffer root)
  "Describe BUFFER relative to ROOT without changing its state."
  (with-current-buffer buffer
    (list :file (file-relative-name buffer-file-name root)
          :mode major-mode
          :name mode-name
          :parent
          (cond ((derived-mode-p 'conf-unix-mode) 'conf-unix-mode)
                ((derived-mode-p 'text-mode) 'text-mode)
                (t nil))
          :point (point)
          :modified (and (buffer-modified-p) t)
          :text (buffer-substring-no-properties (point-min) (point-max)))))

(defun gm353-test-run (name function)
  "Run FUNCTION in one owned filesystem/editor world named NAME."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root)
                 (> (length sandbox-root) 0)
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (unless (string-match-p "\\`[a-z0-9-]+\\'" name)
      (error "GIT-MODES invalid owned case name: %S" name))
    (let* ((root (file-name-as-directory (expand-file-name name sandbox-root)))
           (root-owned nil)
           (buffer-baseline (buffer-list))
           (process-baseline (process-list))
           (timer-baseline (copy-sequence timer-list))
           (idle-timer-baseline (copy-sequence timer-idle-list))
           (window-buffer-baseline (window-buffer))
           (auto-mode-baseline (copy-tree auto-mode-alist))
           (gm353-test-owned-buffers nil)
           (enable-local-variables nil)
           (enable-dir-local-variables nil)
           (enable-local-eval nil)
           (default-directory root)
           result body-error cleanup cleanup-errors)
      (when (file-exists-p root)
        (error "GIT-MODES owned case root already exists: %s" root))
      (cl-labels
          ((attempt
            (phase callback)
            (condition-case condition
                (funcall callback)
              (t (push (list phase condition) cleanup-errors) nil))))
        (unwind-protect
            (condition-case condition
                (progn
                  (make-directory root)
                  (setq root-owned t)
                  (save-window-excursion
                    (save-current-buffer
                      (setq result (funcall function root)))))
              (t (setq body-error condition)))
          (dolist (buffer gm353-test-owned-buffers)
            (attempt
             'owned-buffer
             (lambda ()
               (when (buffer-live-p buffer)
                 (with-current-buffer buffer
                   (set-buffer-modified-p nil))
                 (kill-buffer buffer)))))
          (dolist (buffer (seq-difference (buffer-list) buffer-baseline #'eq))
            (attempt
             'late-buffer-first-sweep
             (lambda ()
               (when (buffer-live-p buffer)
                 (with-current-buffer buffer (set-buffer-modified-p nil))
                 (kill-buffer buffer)))))
          (dolist (process (seq-difference (process-list) process-baseline #'eq))
            (attempt
             'process-first-sweep
             (lambda ()
               (set-process-query-on-exit-flag process nil)
               (when (process-live-p process) (delete-process process)))))
          (dolist (timer (seq-difference timer-idle-list idle-timer-baseline #'eq))
            (attempt 'idle-timer-first-sweep (lambda () (cancel-timer timer))))
          (dolist (timer (seq-difference timer-list timer-baseline #'eq))
            (attempt 'timer-first-sweep (lambda () (cancel-timer timer))))
          (dolist (buffer (seq-difference (buffer-list) buffer-baseline #'eq))
            (attempt
             'late-buffer-second-sweep
             (lambda ()
               (when (buffer-live-p buffer)
                 (with-current-buffer buffer (set-buffer-modified-p nil))
                 (kill-buffer buffer)))))
          (dolist (process (seq-difference (process-list) process-baseline #'eq))
            (attempt
             'process-second-sweep
             (lambda ()
               (set-process-query-on-exit-flag process nil)
               (when (process-live-p process) (delete-process process)))))
          (dolist (timer (seq-difference timer-idle-list idle-timer-baseline #'eq))
            (attempt 'idle-timer-second-sweep (lambda () (cancel-timer timer))))
          (dolist (timer (seq-difference timer-list timer-baseline #'eq))
            (attempt 'timer-second-sweep (lambda () (cancel-timer timer))))
          (dolist (buffer (seq-difference (buffer-list) buffer-baseline #'eq))
            (attempt
             'late-buffer-final-sweep
             (lambda ()
               (when (buffer-live-p buffer)
                 (with-current-buffer buffer (set-buffer-modified-p nil))
                 (kill-buffer buffer)))))
          (attempt
           'root
           (lambda ()
             (when root-owned
               (when (file-exists-p root) (delete-directory root t))
               (unless (file-exists-p root) (setq root-owned nil)))))
          (attempt
           'state
           (lambda ()
             (let ((auto-mode-unchanged
                    (equal auto-mode-alist auto-mode-baseline)))
               (unless auto-mode-unchanged
                 (setq auto-mode-alist (copy-tree auto-mode-baseline)))
               (setq cleanup
                     (list
                    :new-buffers
                    (delq nil
                          (mapcar (lambda (buffer)
                                    (and (buffer-live-p buffer)
                                         (buffer-name buffer)))
                                  (seq-difference (buffer-list)
                                                  buffer-baseline #'eq)))
                    :owned-live
                    (and (seq-some #'buffer-live-p gm353-test-owned-buffers) t)
                    :new-processes
                    (mapcar #'process-name
                            (seq-difference (process-list)
                                            process-baseline #'eq))
                    :new-timers
                    (+ (length (seq-difference timer-list timer-baseline #'eq))
                       (length (seq-difference timer-idle-list
                                               idle-timer-baseline #'eq)))
                    :root-exists (file-exists-p root)
                    :root-owned root-owned
                    :window-restored
                    (eq (window-buffer) window-buffer-baseline)
                    :auto-mode-before-restore auto-mode-unchanged
                    :auto-mode-restored
                    (equal auto-mode-alist auto-mode-baseline)
                    :body-error body-error
                    :cleanup-errors (nreverse cleanup-errors)))))))
        (let ((dirty
               (or body-error cleanup-errors
                   (plist-get cleanup :new-buffers)
                   (plist-get cleanup :owned-live)
                   (plist-get cleanup :new-processes)
                   (not (= (plist-get cleanup :new-timers) 0))
                   (plist-get cleanup :root-exists)
                   (plist-get cleanup :root-owned)
                   (not (plist-get cleanup :window-restored))
                   (not (plist-get cleanup :auto-mode-restored))
                   (not (plist-get cleanup :auto-mode-before-restore)))))
          (when dirty
            (error "GIT-MODES world failed: body=%S cleanup=%S"
                   body-error cleanup))
          (list :result result :cleanup cleanup))))))
"####;

fn git_modes_oracle(load_root: bool) -> CachedMelpaOracle {
    let mut prelude = GIT_MODES_TEST_PRELUDE.to_owned();
    if load_root {
        prelude.push_str("\n(require 'git-modes)\n");
    }
    CachedMelpaOracle::new(GIT_MODES_MELPA_PIN, "git-modes.el")
        .expect("prepare exact revision-pinned Git Modes source below ./tmp")
        .with_installed_autoloads()
        .with_prelude(prelude)
        .with_timeout(Duration::from_secs(180))
}

#[test]
fn git_modes_package_batch() {
    let activation_cases = workflows::activation_workflow_cases();
    assert_oracle_batch_cases(
        git_modes_oracle(false),
        "git-modes-autoload-activation",
        "Git Modes autoload activation",
        &activation_cases,
    );

    let loaded_cases = workflows::loaded_workflow_cases();
    assert_oracle_batch_cases(
        git_modes_oracle(true),
        "git-modes-loaded-workflows-batch",
        "Git Modes loaded workflows",
        &loaded_cases,
    );
}
