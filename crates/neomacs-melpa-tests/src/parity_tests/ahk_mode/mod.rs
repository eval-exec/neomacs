use std::time::Duration;

use crate::{AHK_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AHK_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ahk-mode is a major mode for AutoHotkey scripts: it claims `.ahk', sets up
/// a syntax table where `#', `_', `@' and `\' are word constituents and a
/// backtick escapes, highlights commands, functions, directives, variables and
/// hotkeys by kind, indents blocks, comments in line and block notation,
/// completes from its own keyword tables and indexes the script for imenu.
/// All of that is local, so these workflows write a real `.ahk' script into the
/// per-case sandbox, open it so the mode is selected the way a user gets it,
/// and read back faces, indentation, buffer text, completions and the imenu
/// index.
///
/// The one thing that is not local is running the script.  `ahk-run-script'
/// and `ahk-lookup-chm' call `w32-shell-execute', which exists only on
/// Windows, and the package looks for `AutoHotkey.chm' under two hard-coded
/// `c:/Program Files' paths.  Nothing is stood in for: the workflow drives
/// those commands as they are and records what a user on this platform
/// actually gets, which is the honest answer for a Windows-only feature.
const AHK_MODE_TEST_PRELUDE: &str = r##"(require 'cl-lib)
(require 'imenu)

(defun ahk-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ahk-test-copy (value)
  (if (stringp value) (copy-sequence value) value))

(defconst ahk-test-script "\
; Inventory helper hotkeys
#NoEnv
#SingleInstance force
SetWorkingDir %A_ScriptDir%

global WidgetCount := 0

CountWidgets(name, price) {
    MsgBox, Counting %name%
    if (price > 10) {
        WidgetCount += 1
    } else {
        WidgetCount := 0
    }
    return WidgetCount
}

FormatWidget(widget)
{
    return widget
}

^!w::
    InputBox, widget, Widget, Enter a widget name
    if ErrorLevel
        return
    CountWidgets(widget, 12)
    FileAppend, %widget%`n, log.txt
return

::btw::by the way

ReportLabel:
    MsgBox, % \"Total: \" . WidgetCount
return

/*
   A block comment describing
   the report label above.
*/
")

(defun ahk-test-visit (&optional text name)
  (let* ((path (ahk-test-path (or name "scripts/inventory.ahk")))
         (buffer nil))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert (or text ahk-test-script))
      (write-region (point-min) (point-max) path nil 'silent))
    (setq buffer (find-file-noselect path))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (font-lock-ensure)
    buffer))

(defun ahk-test-faces-where (text)
  "Every face run on the line containing TEXT.
Located by content rather than by a line number counted by hand, which is how
a fixture ends up asserting the wrong construct."
  (save-excursion
    (goto-char (point-min))
    (if (not (search-forward text nil t))
        (list :missing (ahk-test-copy text))
      (goto-char (match-beginning 0))
      (let ((end (line-end-position)) (position (line-beginning-position)) runs)
        (while (< position end)
          (let ((next (next-single-property-change position 'face nil end)))
            (push (list (get-text-property position 'face)
                        (buffer-substring-no-properties position next))
                  runs)
            (setq position next)))
        (nreverse runs)))))

(defun ahk-test-faces-on-line (line)
  (save-excursion
    (goto-char (point-min))
    (forward-line (1- line))
    (let ((end (line-end-position)) (position (point)) runs)
      (while (< position end)
        (let ((next (next-single-property-change position 'face nil end)))
          (push (list (get-text-property position 'face)
                      (buffer-substring-no-properties position next))
                runs)
          (setq position next)))
      (nreverse runs))))

(defun ahk-test-tokens-with-face (face)
  (let ((position (point-min)) seen)
    (while (< position (point-max))
      (let ((next (next-single-property-change position 'face nil (point-max))))
        (when (equal (get-text-property position 'face) face)
          (let ((text (buffer-substring-no-properties position next)))
            (unless (member text seen) (push text seen))))
        (setq position next)))
    (sort seen #'string<)))

(defun ahk-test-syntax-of (character)
  (list character (char-to-string (char-syntax character))))

(defun ahk-test-candidates ()
  (let ((capf (run-hook-with-args-until-success 'completion-at-point-functions)))
    (when capf
      (cl-destructuring-bind (start end table &rest properties) capf
        (list :prefix (buffer-substring-no-properties start end)
              :exclusive (plist-get properties :exclusive)
              :candidates (sort (mapcar #'ahk-test-copy
                                        (all-completions
                                         (buffer-substring-no-properties start end)
                                         table))
                                #'string<)
              :annotations (mapcar (lambda (candidate)
                                     (list (ahk-test-copy candidate)
                                           (ahk-test-copy
                                            (funcall (plist-get properties :annotation-function)
                                                     candidate))))
                                   (sort (mapcar #'ahk-test-copy
                                                 (all-completions
                                                  (buffer-substring-no-properties start end)
                                                  table))
                                         #'string<)))))))

(defun ahk-test-messages (regexp)
  (let (matches)
    (with-current-buffer "*Messages*"
      (save-excursion
        (goto-char (point-min))
        (while (re-search-forward regexp nil t)
          (push (ahk-test-copy (match-string-no-properties 0)) matches))))
    (nreverse matches)))
"##;

fn ahk_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AHK_MODE_MELPA_PIN, "ahk-mode.el")
        .expect("prepare pinned ahk-mode source below ./tmp")
        .with_prelude(AHK_MODE_TEST_PRELUDE)
        .with_timeout(AHK_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ahk-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_ahk_mode_parity` cases (2a).
pub(crate) fn assert_ahk_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ahk_mode_oracle(), &name, "ahk_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ahk_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ahk_mode_batch(&cases);
}

// END generated package batch tests
