use std::time::Duration;

use crate::{CachedMelpaOracle, SPACELINE_ALL_THE_ICONS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SPACELINE_ALL_THE_ICONS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// The package's renderer is a real Spaceline-generated function.  GNU Emacs
/// deliberately makes `format-mode-line' a no-op under `--batch', so this
/// fixture supplies only the small set of built-in mode-spec values consumed
/// by the package while leaving its segment selection, icon lookup,
/// propertization, composition, maps, and public commands untouched.  A TUI
/// test separately exercises GNU's real display engine without this boundary.
const SPACELINE_ALL_THE_ICONS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'spaceline)

(defun neomacs-spaceline-icons-test-format-mode-line (format &rest _)
  "Render the GNU mode specs needed by the package in a batch editor."
  (cond
   ((equal format "%*")
    (cond (buffer-read-only "%")
          ((buffer-modified-p) "*")
          (t "-")))
   ((equal format "%I")
    (let ((size (buffer-size)))
      (if (< size 1000)
          (number-to-string size)
        (error "Fixture only models GNU %%I below 1000 bytes, got %d" size))))
   ((equal format "%b") (buffer-name))
   ((equal format "%l:%c")
    (format "%d:%d" (line-number-at-pos) (current-column)))
   ((equal format "%m")
    (if (stringp mode-name) mode-name (format "%s" mode-name)))
   ((equal format "%p") "All")
   ((null format) "")
   ((stringp format) format)
   (t (error "Unexpected format-mode-line input at the display boundary: %S"
             format))))

(defun neomacs-spaceline-icons-test-property-runs (string property)
  "Return every non-nil PROPERTY run in STRING."
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((value (get-text-property position property string))
             (next (or (next-single-property-change
                        position property string (length string))
                       (length string))))
        (when value
          (push (list position next (copy-tree value)) runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-spaceline-icons-test-action-runs (string)
  "Describe every mode-line mouse-1 action carried by STRING."
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((map (get-text-property position 'local-map string))
             (next (or (next-single-property-change
                        position 'local-map string (length string))
                       (length string)))
             (binding (and (keymapp map)
                           (lookup-key map [mode-line mouse-1]))))
        (when binding
          (push (list position next
                      (substring-no-properties string position next)
                      (if (symbolp binding) binding 'lambda))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-spaceline-icons-test-summary (string)
  "Describe exact visible and semantic output of a rendered mode line."
  (list :text (substring-no-properties string)
        :codepoints (string-to-list (substring-no-properties string))
        :width (string-width string)
        :faces (neomacs-spaceline-icons-test-property-runs string 'face)
        :font-lock-faces
        (neomacs-spaceline-icons-test-property-runs string 'font-lock-face)
        :display (neomacs-spaceline-icons-test-property-runs string 'display)
        :mouse-faces
        (neomacs-spaceline-icons-test-property-runs string 'mouse-face)
        :help (neomacs-spaceline-icons-test-property-runs string 'help-echo)
        :mouse-1 (neomacs-spaceline-icons-test-action-runs string)))

(defun neomacs-spaceline-icons-test-visual-summary (string)
  "Describe STRING's complete visible output without repeating all properties."
  (list :text (substring-no-properties string)
        :codepoints (string-to-list (substring-no-properties string))
        :width (string-width string)))

(defun neomacs-spaceline-icons-test-action-position (string binding help)
  "Find the first position in STRING matching mouse-1 BINDING or HELP."
  (let ((position 0)
        found)
    (while (and (< position (length string)) (not found))
      (let* ((map (get-text-property position 'local-map string))
             (candidate (and (keymapp map)
                             (lookup-key map [mode-line mouse-1])))
             (candidate-help (get-text-property position 'help-echo string)))
        (when (and candidate
                   (or (and binding (eq candidate binding))
                       (and help (equal candidate-help help))))
          (setq found position))
        (setq position
              (or (next-single-property-change
                   position 'local-map string (length string))
                  (length string)))))
    (or found (error "No mouse-1 action matched %S / %S" binding help))))

(defun neomacs-spaceline-icons-test-action-segment (string binding help)
  "Describe the exact glyph segment in STRING matching BINDING or HELP."
  (let* ((position
          (neomacs-spaceline-icons-test-action-position string binding help))
         (end (or (next-single-property-change
                   position 'local-map string (length string))
                  (length string)))
         (map (get-text-property position 'local-map string))
         (action (lookup-key map [mode-line mouse-1]))
         (text (substring-no-properties string position end)))
    (list :range (list position end)
          :text text
          :codepoints (string-to-list text)
          :face (copy-tree (get-text-property position 'face string))
          :font-lock-face
          (copy-tree (get-text-property position 'font-lock-face string))
          :display (copy-tree (get-text-property position 'display string))
          :mouse-face (copy-tree (get-text-property position 'mouse-face string))
          :help (get-text-property position 'help-echo string)
          :mouse-1 (if (symbolp action) action 'lambda))))

(defun neomacs-spaceline-icons-test-find-action (string binding help)
  "Find STRING's mouse-1 action matching BINDING or HELP."
  (let* ((position
          (neomacs-spaceline-icons-test-action-position string binding help))
         (map (get-text-property position 'local-map string)))
    (lookup-key map [mode-line mouse-1])))

(defun neomacs-spaceline-icons-test-snapshot-default (symbol)
  "Capture whether SYMBOL is bound and its copied default value."
  (list symbol (boundp symbol)
        (and (boundp symbol) (copy-tree (default-value symbol)))))

(defun neomacs-spaceline-icons-test-restore-default (snapshot)
  "Restore one default-value SNAPSHOT."
  (pcase-let ((`(,symbol ,was-bound ,value) snapshot))
    (if was-bound
        (set-default symbol value)
      (makunbound symbol))))

(defmacro neomacs-spaceline-icons-test-with-theme-state (&rest body)
  "Run BODY while owning and restoring the compiled all-icons theme."
  (declare (indent 0) (debug t))
  `(let* ((target-function 'spaceline-ml-all-the-icons)
          (function-was-bound (fboundp target-function))
          (old-function (and function-was-bound
                             (symbol-function target-function)))
          (old-mode-line-format (copy-tree (default-value 'mode-line-format)))
          (runtime-symbols
           '(spaceline--segments-code-all-the-icons-left
             spaceline--segments-code-all-the-icons-right
             spaceline--runtime-data-all-the-icons))
          (runtime-snapshots
           (mapcar #'neomacs-spaceline-icons-test-snapshot-default
                   runtime-symbols))
          (spaceline--mode-lines (copy-tree spaceline--mode-lines))
          (spaceline-byte-compile nil)
          (spaceline-responsive nil))
     (unwind-protect
         (progn ,@body)
       (set-default 'mode-line-format old-mode-line-format)
       (dolist (snapshot runtime-snapshots)
         (neomacs-spaceline-icons-test-restore-default snapshot))
       (if function-was-bound
           (fset target-function old-function)
         (fmakunbound target-function)))))
"####;

fn spaceline_all_the_icons_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        SPACELINE_ALL_THE_ICONS_MELPA_PIN,
        "spaceline-all-the-icons.el",
    )
    .expect("prepare exact shallow Spaceline All The Icons source below ./tmp")
    .with_prelude(SPACELINE_ALL_THE_ICONS_TEST_PRELUDE)
    .with_timeout(SPACELINE_ALL_THE_ICONS_TEST_TIMEOUT)
}

#[test]
fn spaceline_all_the_icons_package_batch() {
    assert_oracle_batch_cases(
        spaceline_all_the_icons_oracle(),
        "spaceline-all-the-icons-package-batch",
        "Spaceline All The Icons",
        &workflows::workflow_batch_cases(),
    );
}
