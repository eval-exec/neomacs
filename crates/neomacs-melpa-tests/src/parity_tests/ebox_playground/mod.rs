//! Behavioural parity for the pinned `ebox-playground` renderer.
//!
//! Each probe writes its own small DSL file under the temporary directory,
//! binds a fixed viewport width, and calls the public `open-file`/`close`
//! entry points, so window width, the bundled examples (which the MELPA build
//! does not install) and the user's files do not affect the result.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'ebox-playground)

(defun neomacs-ebox-write (suffix content)
  "Write CONTENT to a fresh temporary file with SUFFIX and return its name."
  (let ((file (make-temp-file "neomacs-ebox-" nil suffix)))
    (with-temp-file file
      (insert content))
    file))

(defun neomacs-ebox-render (file buffer)
  "Render FILE at viewport 20 in BUFFER and return its observable lifecycle."
  (let ((ebox-viewport-width 20))
    (unwind-protect
        (let* ((mounted (ebox-playground-open-file file buffer))
               (name (buffer-name mounted))
               (text (with-current-buffer mounted
                       (buffer-substring-no-properties (point-min) (point-max))))
               (closed (ebox-playground-close buffer)))
          (list name text (buffer-live-p closed) (get-buffer buffer)))
      (ignore-errors (ebox-playground-close buffer)))))

(defun neomacs-ebox-error (file)
  "Return FILE's render error as (TYPE MESSAGE)."
  (condition-case err
      (neomacs-ebox-render file "*neomacs-ebox-error*")
    (error (list (car err) (cadr err)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("ebox-playground", "0.1.1"), "ebox-playground.el")
        .expect("prepare pinned ebox-playground source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn ebox_playground_behavior_batch() {
    let cases = vec![
        // A box renders its text inside the border, and closing the preview
        // kills the buffer.
        ParityBatchCase::value(
            "render_box",
            r####"
(neomacs-ebox-render
 (neomacs-ebox-write
  ".ebox"
  "'(box :width (vw 100) :height (lh 1) :border \"#000000\" \"hello world\")\n")
 "*neomacs-ebox-box*")
"####,
            expect![[r#"OK ("*neomacs-ebox-box*" "│hello world │" nil nil)"#]],
        ),
        // Nested column layout stacks both children in source order.
        ParityBatchCase::value(
            "render_column",
            r####"
(neomacs-ebox-render
 (neomacs-ebox-write
  ".ebox"
  "`(column :width (vw 100)\n   (box :height (lh 1) :border \"#000000\" \"first\")\n   (box :height (lh 1) :border \"#000000\" \"second\"))\n")
 "*neomacs-ebox-column*")
"####,
            expect![[r#"OK ("*neomacs-ebox-column*" "│first │\n│second │" nil nil)"#]],
        ),
        // A same-basename .el companion is loaded before the DSL form is
        // evaluated, so its definitions are visible to the form.
        ParityBatchCase::value(
            "render_companion",
            r####"
(let* ((file (neomacs-ebox-write
              ".ebox"
              "`(box :width (vw 100) :height (lh 1) ,neomacs-ebox-label)\n")))
  (with-temp-file (concat (file-name-sans-extension file) ".el")
    (insert "(defvar neomacs-ebox-label \"from-companion\")\n"))
  (neomacs-ebox-render file "*neomacs-ebox-companion*"))
"####,
            expect![[r#"OK ("*neomacs-ebox-companion*" "from-companion " nil nil)"#]],
        ),
        // Rendering again into the same preview name replaces the previous
        // mount instead of appending to it.
        ParityBatchCase::value(
            "render_replaces_previous",
            r####"
(let* ((first (neomacs-ebox-write
               ".ebox"
               "'(box :width (vw 100) :height (lh 1) \"first preview\")\n"))
       (second (neomacs-ebox-write
                ".ebox"
                "'(box :width (vw 100) :height (lh 1) \"second preview\")\n")))
  (let ((ebox-viewport-width 20))
    (unwind-protect
        (progn
          (ebox-playground-open-file first "*neomacs-ebox-replace*")
          (let ((mounted (ebox-playground-open-file second "*neomacs-ebox-replace*")))
            (list (buffer-name mounted)
                  (with-current-buffer mounted
                    (buffer-substring-no-properties (point-min) (point-max)))
                  (eq (get-buffer "*neomacs-ebox-replace*") mounted))))
      (ebox-playground-close "*neomacs-ebox-replace*"))))
"####,
            expect![[r#"OK ("*neomacs-ebox-replace*" "second preview " t)"#]],
        ),
        // Empty, multi-form, missing and invalid sources report their
        // documented errors instead of rendering a partial tree.
        ParityBatchCase::value(
            "invalid_sources",
            r####"
(let* ((empty (neomacs-ebox-write ".ebox" ""))
       (two (neomacs-ebox-write ".ebox" "'(box \"one\")\n'(box \"two\")\n"))
       (directory (file-name-directory empty))
       (missing (expand-file-name "neomacs-ebox-missing.ebox" directory))
       (bad (neomacs-ebox-write
             ".ebox"
             "'(box :width (vw 100) (void-function-xyz \"x\"))\n"))
       (missing-error (neomacs-ebox-error missing)))
  (list (neomacs-ebox-error empty)
        (neomacs-ebox-error two)
        (list (car missing-error)
              (replace-regexp-in-string
               (regexp-quote directory) "<dir>/" (cadr missing-error)))
        (neomacs-ebox-error bad)))
"####,
            expect![[
                r#"OK ((user-error "The Ebox DSL source is empty") (user-error "The Ebox source must contain one Elisp expression") (user-error "Ebox DSL file is not readable: <dir>/neomacs-ebox-missing.ebox") (ebox-error "Unknown Ebox author tag"))"#
            ]],
        ),
        // Closing a preview returns the former buffer once and then reports
        // that no such buffer exists.
        ParityBatchCase::value(
            "close_lifecycle",
            r####"
(let* ((file (neomacs-ebox-write
              ".ebox"
              "'(box :width (vw 100) :height (lh 1) \"close me\")\n"))
       (ebox-viewport-width 20)
       (mounted (ebox-playground-open-file file "*neomacs-ebox-close*"))
       (closed (ebox-playground-close "*neomacs-ebox-close*")))
  (list (eq closed mounted)
        (buffer-live-p closed)
        (get-buffer "*neomacs-ebox-close*")
        (ebox-playground-close "*neomacs-ebox-close*")))
"####,
            expect![[r#"OK (t nil nil nil)"#]],
        ),
        // .ebox files open in the package's derived mode with its render
        // binding and header line.
        ParityBatchCase::value(
            "file_mode",
            r####"
(with-temp-buffer
  (setq buffer-file-name "/tmp/neomacs-ebox-mode.ebox")
  (set-auto-mode)
  (list major-mode
        (key-binding (kbd "C-c C-c"))
        header-line-format
        (assoc "\\.ebox\\'" auto-mode-alist)))
"####,
            expect![[
                r#"OK (ebox-playground-mode ebox-playground-render "Ebox DSL  |  C-c C-c  Render preview" ("\\.ebox\\'" . ebox-playground-mode))"#
            ]],
        ),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "ebox-playground-behavior",
        "ebox-playground",
        &cases,
    );
}
