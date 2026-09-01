use std::time::Duration;

use crate::{ACHIEVEMENTS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACHIEVEMENTS_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// achievements watches which commands are actually run - through `keyfreq',
/// its recommended companion - and unlocks records that it persists to a file.
/// The workflows therefore type real keys with `execute-kbd-macro' into a
/// buffer displayed in the selected window, let the real `keyfreq-mode' record
/// them, and then use the package's own commands.  `keyfreq' counts the
/// *previous* command from `pre-command-hook', so each key sequence ends with
/// one extra command to flush the one before it.  The achievements file is
/// redirected into the per-case sandbox.  Workflows which deliberately invoke
/// an external browser bind GNU Emacs's documented
/// `browse-url-browser-function' boundary to
/// `ach-test-capture-browser-launch', keeping the real command path while
/// preventing a desktop process from escaping the sandbox.
const ACHIEVEMENTS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

;; `achievements' mutates the records in `achievements-list' in place, while
;; unlocking its advanced collection loads another file that appends records.
;; Keep an untouched copy of the package's initial, basic collection so every
;; workflow starts from the same state even though the Rust batch deliberately
;; reuses one editor process.
(defvar ach-test-pristine-basic-achievements nil)
(defvar ach-test-pristine-buffers nil)
(defvar ach-test-opened-urls nil)

(defun ach-test-capture-browser-launch (url &rest _args)
  "Record URL instead of launching a browser outside the test sandbox."
  (push url ach-test-opened-urls))

(function-put 'ach-test-capture-browser-launch 'browse-url-browser-kind 'external)

(defun ach-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ach-test-reset-case-state ()
  "Restore package and editor state changed by an achievements workflow."
  ;; The oracle evaluates this prelude before loading the requested package
  ;; source, so capture the pristine package state lazily at the first case.
  (unless ach-test-pristine-basic-achievements
    (setq ach-test-pristine-basic-achievements
          (mapcar #'copy-sequence achievements-list)
          ach-test-pristine-buffers (buffer-list)))
  (when achievements-mode
    (achievements-mode -1))
  (when (timerp achievements-timer)
    (cancel-timer achievements-timer))
  (setq achievements-mode nil
        achievements-timer nil
        achievements-post-command-list nil
        achievements-score 0
        achievements-total 0
        ach-test-opened-urls nil
        achievements-list
        (mapcar #'copy-sequence ach-test-pristine-basic-achievements)
        command-history nil
        file-name-history nil
        yes-or-no-p-history nil)
  ;; Requiring the advanced collection must append it again after a workflow
  ;; crosses the score threshold.  Removing the feature restores the package's
  ;; original lazy-load boundary without unloading its shared core functions.
  (setq features (delq 'advanced-achievements features))
  (remove-hook 'post-command-hook #'achievements-post-command-function)
  (when (bound-and-true-p keyfreq-mode)
    (keyfreq-mode -1))
  (when (boundp 'keyfreq-table)
    (clrhash keyfreq-table))
  (dolist (buffer (buffer-list))
    (unless (memq buffer ach-test-pristine-buffers)
      (kill-buffer buffer)))
  (when-let ((messages (get-buffer "*Messages*")))
    (with-current-buffer messages
      (let ((inhibit-read-only t))
        (erase-buffer))))
  (dolist (path (list (ach-test-path "achievements.eld")
                      (ach-test-path "state")))
    (when (file-exists-p path)
      (if (file-directory-p path)
          (delete-directory path t)
        (delete-file path)))))

(defmacro ach-test-with-live-buffer (&rest body)
  "Run BODY in a real, window-displayed buffer so typed keys reach it."
  `(progn
     (ach-test-reset-case-state)
     (let ((buffer (generate-new-buffer "*achievements-workflow*")))
       (unwind-protect
           (progn
             (set-window-buffer (selected-window) buffer)
             (set-buffer buffer)
             ,@body)
         (when (buffer-live-p buffer)
           (kill-buffer buffer))))))

(defun ach-test-earned ()
  "Names of every earned achievement, sorted."
  (sort (delq nil
              (mapcar (lambda (achievement)
                        (and (achievements-earned-p achievement)
                             (emacs-achievement-name achievement)))
                      achievements-list))
        #'string<))

(defun ach-test-record (name)
  "Return NAME's stored record, with its predicate reduced to a state."
  (let ((achievement (achievements-get-achievements-by-name name)))
    (and achievement
         (list (emacs-achievement-name achievement)
               (emacs-achievement-description achievement)
               (let ((predicate (emacs-achievement-predicate achievement)))
                 (cond ((eq predicate t) t)
                       ((null predicate) nil)
                       (t :pending)))
               (emacs-achievement-points achievement)
               (emacs-achievement-transient achievement)
               (achievements-earned-p achievement)))))

(defun ach-test-log ()
  "Text of the achievements log buffer, or a marker when there is none."
  (let ((buffer (get-buffer "*achievements-log*")))
    (if buffer
        (with-current-buffer buffer
          (buffer-substring-no-properties (point-min) (point-max)))
      'no-log-buffer)))

(defun ach-test-unlock-messages ()
  "Every ACHIEVEMENT UNLOCKED line the session produced, in order."
  (with-current-buffer (get-buffer-create "*Messages*")
    (let ((lines nil))
      (dolist (line (split-string (buffer-string) "\n" t) (nreverse lines))
        (when (string-prefix-p "ACHIEVEMENT UNLOCKED" line)
          (push line lines))))))

(defun ach-test-rows (&rest names)
  "Return (NAME . ROW) for each NAME in the *Achievements* buffer.
ROW is nil when the achievement is not listed at all."
  (with-current-buffer "*Achievements*"
    (mapcar
     (lambda (name)
       (cons name
             (save-excursion
               (goto-char (point-min))
               (and (re-search-forward (concat "^.*" (regexp-quote name)) nil t)
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position))))))
     names)))
"##;

fn achievements_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACHIEVEMENTS_MELPA_PIN, "achievements.el")
        .expect("prepare pinned achievements source below ./tmp")
        .with_prelude(ACHIEVEMENTS_TEST_PRELUDE)
        .with_timeout(ACHIEVEMENTS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed achievements parity test")
        .into()
}

/// Multi-probe batch for `assert_achievements_parity` cases (2a).
pub(crate) fn assert_achievements_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(achievements_oracle(), &name, "achievements_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn achievements_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_achievements_batch(&cases);
}

// END generated package batch tests
