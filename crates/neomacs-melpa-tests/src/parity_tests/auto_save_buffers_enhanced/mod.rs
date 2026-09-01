use std::time::Duration;

use crate::{AUTO_SAVE_BUFFERS_ENHANCED_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUTO_SAVE_BUFFERS_ENHANCED_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const AUTO_SAVE_BUFFERS_ENHANCED_TEST_PRELUDE: &str = r####"
(defvar neomacs-asbe-test--events nil)
(defvar elscreen-create-hook)

(defun neomacs-asbe-test--load-package ()
  "Load the package through its documented autoloaded entry point."
  (unless (featurep 'auto-save-buffers-enhanced)
    (auto-save-buffers-enhanced nil)))

(defun neomacs-asbe-test--package-timers ()
  "Return registered idle timers created by auto-save-buffers-enhanced."
  (let (matches)
    (dolist (timer (copy-sequence timer-idle-list) (nreverse matches))
      (when (eq (timer--function timer)
                'auto-save-buffers-enhanced-save-buffers)
        (setq matches (cons timer matches))))))

(defun neomacs-asbe-test--cancel-package-timers ()
  "Cancel every idle timer created by auto-save-buffers-enhanced."
  (dolist (timer (neomacs-asbe-test--package-timers))
    (cancel-timer timer)))

(defun neomacs-asbe-test--fire-idle-tick ()
  "Run each registered package timer as the command loop would after idling."
  (dolist (timer (neomacs-asbe-test--package-timers))
    (when (memq timer timer-idle-list)
      (timer-event-handler timer))))

(defun neomacs-asbe-test--timer-state ()
  "Return a deterministic description of every registered package timer."
  (let ((timers (neomacs-asbe-test--package-timers)))
    (list
     :count (length timers)
     :timers
     (mapcar
      (lambda (timer)
        (list
         :idle-seconds (float-time (timer--time timer))
         :repeat (timer--repeat-delay timer)
         :function (timer--function timer)
         :arguments (timer--args timer)
         :registered (and (memq timer timer-idle-list) t)))
      timers))))

(defun neomacs-asbe-test--write-file (path contents)
  "Create PATH's parent directories and write exact UTF-8 CONTENTS."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file path
      (insert contents))))

(defun neomacs-asbe-test--file-text (path)
  "Read PATH as exact UTF-8 text."
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path))
    (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-asbe-test--buffer-state (buffer)
  "Return the exact user-visible state of BUFFER."
  (with-current-buffer buffer
    (list
     :text (buffer-substring-no-properties (point-min) (point-max))
     :point (point)
     :modified (buffer-modified-p)
     :read-only buffer-read-only)))

(defun neomacs-asbe-test--hook-count (function hook)
  "Count entries exactly equal to FUNCTION in HOOK."
  (let ((count 0))
    (dolist (entry hook count)
      (when (eq entry function)
        (setq count (1+ count))))))

(defun neomacs-asbe-test--messages (start)
  "Return exact non-empty message lines written after START."
  (with-current-buffer (messages-buffer)
    (split-string
     (buffer-substring-no-properties (min start (point-max)) (point-max))
     "\n" t)))

(defun neomacs-asbe-test--cleanup-buffers (buffers)
  "Kill BUFFERS without save prompts."
  (dolist (buffer buffers)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (setq buffer-read-only nil)
        (set-buffer-modified-p nil))
      (kill-buffer buffer))))

(defun neomacs-asbe-test--cleanup-root (root)
  "Remove ROOT and its contents when it exists."
  (when (and root (file-exists-p root))
    (delete-directory root t)))

(defun neomacs-asbe-test--outermost-checkout (path)
  "Return the OUTERMOST ancestor directory of PATH holding a checkout marker.

`auto-save-buffers-enhanced-add-checkout-path-into-include-regexps'
walks from the visited buffer's `default-directory' up to `/' and keeps
the LAST marker it sees rather than the first
\(auto-save-buffers-enhanced.el:288-305), so the include rule it records
names the OUTERMOST checkout.  `locate-dominating-file' answers the
INNERMOST one, and the two coincide only when PATH has exactly one
checkout ancestor.

That is false in a nested Git worktree, which is how this suite is run
during development: the worktree root carries a `.git' FILE and the main
checkout above it a `.git' DIRECTORY, both of which `file-exists-p'
accepts.  Deriving the expectation from the innermost marker therefore
made the case fail in BOTH editors with the same value -- a
path-dependent pin, not a divergence -- and leaked a raw host path into
the snapshot, because only paths at or below the workspace root are
normalized.

Signal rather than return nil when no ancestor carries a marker: the
case's `:duplicate-rules' pin needs both visited files to resolve to the
same checkout root, which requires a marker at or above the sandbox."
  (let ((directory (file-name-as-directory (expand-file-name path)))
        (outermost nil))
    (catch 'root
      (while t
        (when (or (file-exists-p (expand-file-name ".svn" directory))
                  (file-exists-p (expand-file-name ".cvs" directory))
                  (file-exists-p (expand-file-name ".git" directory)))
          (setq outermost directory))
        (let ((parent (file-name-directory (directory-file-name directory))))
          (when (equal parent directory)
            (throw 'root t))
          (setq directory parent))))
    (unless outermost
      (error "No checkout marker above %s; the sandbox needs one" path))
    outermost))
"####;

fn auto_save_buffers_enhanced_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        AUTO_SAVE_BUFFERS_ENHANCED_MELPA_PIN,
        "auto-save-buffers-enhanced.el",
    )
    .expect("prepare pinned auto-save-buffers-enhanced source below ./tmp")
    .with_prelude(AUTO_SAVE_BUFFERS_ENHANCED_TEST_PRELUDE)
    .with_installed_autoloads()
    .with_timeout(AUTO_SAVE_BUFFERS_ENHANCED_TEST_TIMEOUT)
}

#[test]
fn auto_save_buffers_enhanced_package_batch() {
    assert_oracle_batch_cases(
        auto_save_buffers_enhanced_oracle(),
        "auto_save_buffers_enhanced_package_batch",
        "auto_save_buffers_enhanced_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
