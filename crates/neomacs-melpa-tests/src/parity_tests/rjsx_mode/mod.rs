//! Practical parity for rjsx-mode's public JSX editing commands.
//!
//! These cases open real `.jsx` files, parse nested tags/spreads/fragments,
//! indent JSX, fontify tag and attribute names, expand electric `<`/`>`,
//! rename and jump enclosing tags, and recover after a mismatched closer.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, JS2_MODE_MELPA_PIN, RJSX_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'js2-mode)
(require 'rjsx-mode)
(set-window-configuration (current-window-configuration))

(defconst rx447-test-tree
  "9cb7f6493c3d4942e0dfd78b3f2f3f0cd8d86c73")
(defconst rx447-test-manifest
  '(("rjsx-mode-pkg.el" . "41ab56f77afb8846f635450b90f9af632d27a6006ba1542fbb49e4e872fc6afa")
    ("rjsx-mode.el" . "d613561494977c81e6f31183dd4467a6121b67df570204b5173e73fa8d92976f")))

(defvar rx447-test-case-index 0)
(defvar rx447-test-root nil)
(defvar rx447-test-root-owned nil)

(defun rx447-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun rx447-test-source-state ()
  (let* ((located (locate-library "rjsx-mode.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (rx447-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/rjsx-mode.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car rx447-test-manifest)))
      (error "Unexpected installed rjsx-mode payload: %S" (or manifest files)))
    (dolist (entry rx447-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (rx447-test-sha file) expected))
          (error "Unexpected installed rjsx-mode source: %S"
                 (cons entry manifest)))))
    (list :tree rx447-test-tree
          :manifest manifest
          :feature (featurep 'rjsx-mode)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'rjsx-mode package-alist)))))))

(defun rx447-test-forbid-external (operation &rest arguments)
  (error "Unexpected rjsx-mode external boundary: %S %S" operation arguments))

(defun rx447-test-cancel-parse-timer ()
  (when (and (boundp 'js2-mode-parse-timer)
             (timerp js2-mode-parse-timer))
    (cancel-timer js2-mode-parse-timer)
    (setq js2-mode-parse-timer nil)))

(defun rx447-test-activate ()
  (let ((js2-idle-timer-delay 3600))
    (unless (derived-mode-p 'rjsx-mode)
      (rjsx-mode)))
  (rx447-test-cancel-parse-timer)
  (js2-reparse 'force)
  (rx447-test-cancel-parse-timer))

(defun rx447-test-visit (root name code)
  (let ((file (expand-file-name name root))
        (js2-idle-timer-delay 3600))
    (make-directory (file-name-directory file) t)
    (write-region code nil file nil 'silent)
    (find-file file)
    (rx447-test-activate)
    file))

(defun rx447-test-face-at (pattern)
  (save-excursion
    (goto-char (point-min))
    (re-search-forward pattern)
    (goto-char (match-beginning 0))
    (list :at (match-string-no-properties 0)
          :font-lock (get-text-property (point) 'font-lock-face)
          :face (face-at-point))))

(defun rx447-test-diagnostics ()
  (mapcar
   (lambda (diagnostic)
     (let ((position (nth 1 diagnostic))
           (length (or (nth 2 diagnostic) 0)))
       (list :msg (copy-sequence (js2-get-msg (car diagnostic)))
             :line (line-number-at-pos position)
             :col (save-excursion
                    (goto-char position)
                    (current-column))
             :text (buffer-substring-no-properties
                    position
                    (min (point-max) (+ position length)))
             :face (nth 3 diagnostic))))
   (js2-errors-and-warnings)))

(defun rx447-test-run (body)
  (let* ((index (cl-incf rx447-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "rjsx-mode-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (rx447-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (max-reparse-before rjsx-max-size-for-frequent-reparse)
         (rx447-test-root root)
         (rx447-test-root-owned nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute rjsx-mode sandbox root"))
              (when (file-exists-p root)
                (error "rjsx-mode sandbox root exists: %S" root))
              (make-directory root)
              (setq rx447-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'rx447-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (apply #'rx447-test-forbid-external
                                  'call-process-region args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'rx447-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'rx447-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'rx447-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'rx447-test-forbid-external
                                  'url-retrieve-synchronously args))))
                (setq result (funcall body root)))
              (setq source-after (rx447-test-source-state))
              (unless (equal source-before source-after)
                (error "rjsx-mode source changed")))
          (error (setq body-error
                       (list (car condition)
                             (copy-tree (cdr condition))))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error (push (list label (car condition)
                                  (copy-tree (cdr condition)))
                            cleanup-errors)))))
        (rx447-test-cancel-parse-timer)
        (setq rjsx-max-size-for-frequent-reparse max-reparse-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              default-directory directory-before)
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (with-current-buffer buffer
                           (set-buffer-modified-p nil))
                         (kill-buffer buffer))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window
                 (lambda () (set-window-configuration window-before)))
        (when (window-live-p selected-window-before)
          (attempt 'selected
                   (lambda () (select-window selected-window-before))))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer
                   (lambda () (set-buffer buffer-before))))
        (when rx447-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "rjsx-mode body failed: %S" body-error))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers (mapcar #'buffer-name
                                      (seq-remove
                                       (lambda (buffer)
                                         (memq buffer buffers-before))
                                       (buffer-list)))
                 :new-processes (length
                                 (seq-remove
                                  (lambda (process)
                                    (memq process processes-before))
                                  (process-list)))
                 :new-timers (length
                              (seq-remove
                               (lambda (timer)
                                 (memq timer timers-before))
                               (append timer-list timer-idle-list)))
                 :new-frames (length
                              (seq-remove
                               (lambda (frame)
                                 (memq frame frames-before))
                               (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored (eq (selected-window)
                                      selected-window-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "rjsx-mode cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RJSX_MODE_MELPA_PIN, "rjsx-mode.el")
        .expect("prepare pinned rjsx-mode source below ./tmp")
        .with_melpa_dependency(JS2_MODE_MELPA_PIN)
        .expect("prepare pinned js2-mode dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn opens_jsx_parses_nested_and_fontifies_tags() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_jsx_parses_nested_and_fontifies_tags",
        r####"
(rx447-test-run
 (lambda (root)
   (rx447-test-visit
    root "app.jsx"
    "const App = ({ok, extra}) => (
  <>
    <div className={ok ? \"on\" : \"off\"} {...extra}>
      café 界
      {ok && <Module.Child required/>}
    </div>
  </>
);
")
   (list :file (file-relative-name buffer-file-name root)
         :mode major-mode
         :derived (and (derived-mode-p 'js2-mode) t)
         :indent indent-line-function
         :jsx-auto (cdr (assoc "\\.jsx\\'" auto-mode-alist))
         :dirty js2-mode-buffer-dirty-p
         :diagnostics (rx447-test-diagnostics)
         :tag (rx447-test-face-at "div")
         :attr (rx447-test-face-at "className")
         :text (rx447-test-face-at "café")
         :member (rx447-test-face-at "Module"))))
"####,
        expect![[
            r#"OK (:source (:tree "9cb7f6493c3d4942e0dfd78b3f2f3f0cd8d86c73" :manifest (("rjsx-mode-pkg.el" . "41ab56f77afb8846f635450b90f9af632d27a6006ba1542fbb49e4e872fc6afa") ("rjsx-mode.el" . "d613561494977c81e6f31183dd4467a6121b67df570204b5173e73fa8d92976f")) :feature t :version "20200224.2149") :result (:file "app.jsx" :mode rjsx-mode :derived t :indent rjsx-indent-line :jsx-auto rjsx-mode :dirty nil :diagnostics ((:msg "Undeclared variable or function 'Module'" :line 5 :col 14 :text "Module" :face js2-external-variable)) :tag (:at "div" :font-lock rjsx-tag :face nil) :attr (:at "className" :font-lock rjsx-attr :face nil) :text (:at "café" :font-lock rjsx-text :face nil) :member (:at "Module" :font-lock rjsx-tag :face nil)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn indents_nested_jsx_region() -> ParityBatchCase {
    ParityBatchCase::value(
        "indents_nested_jsx_region",
        r####"
(rx447-test-run
 (lambda (root)
   (rx447-test-visit
    root "nested.jsx"
    "(
<App>
    <div>
        {variable1}
        <Component/>
</div>
</App>
)
")
   (let ((js-indent-level 4)
         (js2-basic-offset 4)
         (sgml-basic-offset 2)
         (indent-tabs-mode nil))
     (indent-region (point-min) (point-max))
     (rx447-test-cancel-parse-timer)
     (list :text (buffer-substring-no-properties (point-min) (point-max))))))
"####,
        expect![[
            r#"OK (:source (:tree "9cb7f6493c3d4942e0dfd78b3f2f3f0cd8d86c73" :manifest (("rjsx-mode-pkg.el" . "41ab56f77afb8846f635450b90f9af632d27a6006ba1542fbb49e4e872fc6afa") ("rjsx-mode.el" . "d613561494977c81e6f31183dd4467a6121b67df570204b5173e73fa8d92976f")) :feature t :version "20200224.2149") :result (:text "(\n    <App>\n      <div>\n        {variable1}\n        <Component/>\n      </div>\n    </App>\n)\n") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn electric_lt_expands_gt_renames_and_jumps_tag() -> ParityBatchCase {
    ParityBatchCase::value(
        "electric_lt_expands_gt_renames_and_jumps_tag",
        r####"
(rx447-test-run
 (lambda (root)
   (rx447-test-visit root "electric.jsx" "return ")
   (goto-char (point-max))
   (rjsx-electric-lt 1)
   (let ((after-lt
          (list :before (buffer-substring-no-properties (point-min) (point))
                :after (buffer-substring-no-properties (point) (point-max))))
         grounded slash-class expanded renamed closing)
     (erase-buffer)
     (insert "let c = 3 ")
     (rjsx-electric-lt 1)
     (setq grounded (buffer-substring-no-properties (point-min) (point-max)))
     (erase-buffer)
     (insert "let c = <Component/>")
     (rx447-test-activate)
     (goto-char (point-min))
     (search-forward "/>")
     (backward-char 2)
     (setq slash-class (get-char-property (point) 'rjsx-class))
     (rjsx-electric-gt 1)
     (setq expanded (buffer-substring-no-properties (point-min) (point-max)))
     (erase-buffer)
     (insert "let c = (\n  <div>\n    <Compo")
     (save-excursion (insert "nent a=\"123\"/>\n  </div>)"))
     (rx447-test-activate)
     (rjsx-rename-tag-at-point "NewName")
     (setq renamed (buffer-substring-no-properties (point-min) (point-max)))
     (erase-buffer)
     (insert "let c = <div")
     (save-excursion (insert ">\n</div>"))
     (rx447-test-activate)
     (rjsx-jump-closing-tag)
     (setq closing (list :line (line-number-at-pos) :col (current-column)))
     (rjsx-jump-opening-tag)
     (list :electric-lt after-lt
           :grounded grounded
           :expanded expanded
           :slash-class slash-class
           :renamed renamed
           :closing closing
           :opening (list :line (line-number-at-pos)
                          :col (current-column))))))
"####,
        expect![[
            r#"OK (:source (:tree "9cb7f6493c3d4942e0dfd78b3f2f3f0cd8d86c73" :manifest (("rjsx-mode-pkg.el" . "41ab56f77afb8846f635450b90f9af632d27a6006ba1542fbb49e4e872fc6afa") ("rjsx-mode.el" . "d613561494977c81e6f31183dd4467a6121b67df570204b5173e73fa8d92976f")) :feature t :version "20200224.2149") :result (:electric-lt (:before "return <" :after "/>") :grounded "let c = 3 <" :expanded "let c = <Component></Component>" :slash-class self-closing-slash :renamed "let c = (\n  <div>\n    <NewName a=\"123\"/>\n  </div>)" :closing (:line 2 :col 1) :opening (:line 1 :col 9)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn reports_mismatch_undeclared_and_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "reports_mismatch_undeclared_and_recovers",
        r####"
(rx447-test-run
 (lambda (root)
   (rx447-test-visit
    root "bad.jsx"
    "const C = function() {
  return <div></span>;
};
")
   (let ((mismatch (rx447-test-diagnostics)))
     (erase-buffer)
     (insert "const C = function() {  return <Component abc={123}/>;\n};\n")
     (rx447-test-activate)
     (let ((undeclared (rx447-test-diagnostics)))
       (erase-buffer)
       (insert "const C = function() {  return <component abc={123}/>;\n};\n")
       (rx447-test-activate)
       (let ((lowercase (rx447-test-diagnostics)))
         (erase-buffer)
         (insert "const C = function() {\n  return <div></span>;\n};\n")
         (rx447-test-activate)
         (goto-char (point-min))
         (search-forward "span")
         (replace-match "div")
         (rx447-test-activate)
         (list :mismatch mismatch
               :undeclared undeclared
               :lowercase lowercase
               :recovered (rx447-test-diagnostics)))))))
"####,
        expect![[
            r#"OK (:source (:tree "9cb7f6493c3d4942e0dfd78b3f2f3f0cd8d86c73" :manifest (("rjsx-mode-pkg.el" . "41ab56f77afb8846f635450b90f9af632d27a6006ba1542fbb49e4e872fc6afa") ("rjsx-mode.el" . "d613561494977c81e6f31183dd4467a6121b67df570204b5173e73fa8d92976f")) :feature t :version "20200224.2149") :result (:mismatch ((:msg "mismatched closing JSX tag; expected `div'" :line 2 :col 14 :text "</span>" :face nil)) :undeclared ((:msg "Undeclared variable or function 'Component'" :line 1 :col 32 :text "Component" :face js2-external-variable)) :lowercase nil :recovered nil) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn rjsx_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        opens_jsx_parses_nested_and_fontifies_tags(),
        indents_nested_jsx_region(),
        electric_lt_expands_gt_renames_and_jumps_tag(),
        reports_mismatch_undeclared_and_recovers(),
    ];
    assert_oracle_batch_cases(oracle(), "rjsx-mode-rank447", "rjsx_mode_parity", &cases);
}
