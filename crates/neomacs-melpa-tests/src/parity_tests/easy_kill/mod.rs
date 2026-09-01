//! Practical parity for easy-kill and easy-mark selections.
//!
//! These cases select a region, word, sexp and enclosing list, mark a
//! sexp, append to the kill-ring, and stay empty in an empty buffer.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EASY_KILL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'easy-kill)
(set-window-configuration (current-window-configuration))
(setq interprogram-cut-function nil
      interprogram-paste-function nil
      kill-ring nil
      kill-ring-yank-pointer nil)

(defconst ek457-test-tree
  "371f35effa5b385d3f2debab4aeb087957e3684e")
(defconst ek457-test-manifest
  '(("easy-kill-pkg.el" . "d0b7f267ba92e5a71f654262b95e508937eb437be3576fb6e929cc59933cf102")
    ("easy-kill.el" . "5990df098447f19f792571b15646d0a8ac1a5ce713169e4be55f8730e2e66739")))

(defun ek457-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun ek457-test-source-state ()
  (let* ((located (locate-library "easy-kill.el"))
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
                         (cons file (ek457-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/easy-kill.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car ek457-test-manifest)))
      (error "Unexpected installed easy-kill payload: %S"
             (or manifest files)))
    (dolist (entry ek457-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (ek457-test-sha file) expected))
          (error "Unexpected installed easy-kill source: %S"
                 (cons entry manifest)))))
    (list :tree ek457-test-tree
          :manifest manifest
          :feature (featurep 'easy-kill)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'easy-kill package-alist)))))))

(defun ek457-test-snapshot ()
  (list :thing (and (overlayp easy-kill-candidate)
                    (easy-kill-get thing))
        :text (and (overlayp easy-kill-candidate)
                   (substring-no-properties (easy-kill-candidate)))
        :bounds (and (overlayp easy-kill-candidate)
                     (easy-kill--bounds))))

(defun ek457-test-cleanup ()
  (when (overlayp easy-kill-candidate)
    (let ((i (overlay-get easy-kill-candidate 'origin-indicator)))
      (when (overlayp i) (delete-overlay i)))
    (delete-overlay easy-kill-candidate)
    (setq easy-kill-candidate nil))
  (setq kill-ring nil kill-ring-yank-pointer nil deactivate-mark t)
  (deactivate-mark t))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EASY_KILL_MELPA_PIN, "easy-kill.el")
        .expect("prepare pinned easy-kill source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn active_region_is_copied_and_line_is_tried_otherwise() -> ParityBatchCase {
    ParityBatchCase::value(
        "active_region_is_copied_and_line_is_tried_otherwise",
        r####"
(unwind-protect
    (with-temp-buffer
      (insert "hello café\nsecond line\n")
      (goto-char (point-min))
      (set-mark (point))
      (forward-word)
      (activate-mark)
      (easy-kill)
      (let ((region-kill (car kill-ring)))
        (deactivate-mark t)
        (setq kill-ring nil)
        (goto-char (point-min))
        (forward-line 1)
        (forward-word)
        (easy-kill)
        (list :source (ek457-test-source-state)
              :region (substring-no-properties region-kill)
              :line (ek457-test-snapshot))))
  (ek457-test-cleanup))
"####,
        expect![[
            r#"OK (:source (:tree "371f35effa5b385d3f2debab4aeb087957e3684e" :manifest (("easy-kill-pkg.el" . "d0b7f267ba92e5a71f654262b95e508937eb437be3576fb6e929cc59933cf102") ("easy-kill.el" . "5990df098447f19f792571b15646d0a8ac1a5ce713169e4be55f8730e2e66739")) :feature t :version "20260121.752") :region "hello" :line (:thing line :text "second line\n" :bounds (12 . 24)))"#
        ]],
    )
}

fn word_sexp_and_list_expand_in_elisp() -> ParityBatchCase {
    ParityBatchCase::value(
        "word_sexp_and_list_expand_in_elisp",
        r####"
(unwind-protect
    (with-temp-buffer
      (emacs-lisp-mode)
      (insert "(defun greet (name)\n  (message \"hello %s\" name))\n")
      (goto-char (point-min))
      (search-forward "name")
      (backward-char)
      (easy-kill)
      (easy-kill-thing 'word)
      (let ((word (ek457-test-snapshot)))
        (easy-kill-thing 'sexp)
        (let ((sexp (ek457-test-snapshot)))
          (goto-char (point-min))
          (search-forward "message")
          (easy-kill-thing 'list)
          (list :source (ek457-test-source-state)
                :word word
                :sexp sexp
                :list (ek457-test-snapshot)))))
  (ek457-test-cleanup))
"####,
        expect![[
            r#"OK (:source (:tree "371f35effa5b385d3f2debab4aeb087957e3684e" :manifest (("easy-kill-pkg.el" . "d0b7f267ba92e5a71f654262b95e508937eb437be3576fb6e929cc59933cf102") ("easy-kill.el" . "5990df098447f19f792571b15646d0a8ac1a5ce713169e4be55f8730e2e66739")) :feature t :version "20260121.752") :word (:thing word :text "name" :bounds (15 . 19)) :sexp (:thing sexp :text "name" :bounds (15 . 19)) :list (:thing list :text "(message \"hello %s\" name)" :bounds (23 . 48)))"#
        ]],
    )
}

fn easy_mark_selects_the_sexp_as_an_active_region() -> ParityBatchCase {
    ParityBatchCase::value(
        "easy_mark_selects_the_sexp_as_an_active_region",
        r####"
(unwind-protect
    (with-temp-buffer
      (emacs-lisp-mode)
      (insert "(list 1 2 3)")
      (goto-char (point-min))
      (search-forward "2")
      (easy-mark)
      (list :source (ek457-test-source-state)
            :mark (ek457-test-snapshot)
            :region (and (use-region-p)
                         (buffer-substring-no-properties
                          (region-beginning) (region-end)))
            :active (and (use-region-p) t)))
  (ek457-test-cleanup))
"####,
        expect![[
            r#"OK (:source (:tree "371f35effa5b385d3f2debab4aeb087957e3684e" :manifest (("easy-kill-pkg.el" . "d0b7f267ba92e5a71f654262b95e508937eb437be3576fb6e929cc59933cf102") ("easy-kill.el" . "5990df098447f19f792571b15646d0a8ac1a5ce713169e4be55f8730e2e66739")) :feature t :version "20260121.752") :mark (:thing sexp :text "2" :bounds (9 . 10)) :region "2" :active t)"#
        ]],
    )
}

fn append_joins_kills_and_empty_buffer_has_empty_candidate() -> ParityBatchCase {
    ParityBatchCase::value(
        "append_joins_kills_and_empty_buffer_has_empty_candidate",
        r####"
(unwind-protect
    (list
     :source (ek457-test-source-state)
     :empty
     (with-temp-buffer
       (easy-kill)
       (prog1 (ek457-test-snapshot)
         (ek457-test-cleanup)))
     :append
     (with-temp-buffer
       (insert "alpha beta")
       (goto-char (point-min))
       (forward-word)
       (easy-kill)
       (easy-kill-thing 'word)
       (easy-kill-save-candidate)
       (forward-word)
       (easy-kill)
       (easy-kill-thing 'word)
       (setf (easy-kill-get append) t)
       (easy-kill-save-candidate)
       (list :kill (mapcar #'substring-no-properties kill-ring)
             :candidate (ek457-test-snapshot))))
  (ek457-test-cleanup))
"####,
        expect![[
            r#"OK (:source (:tree "371f35effa5b385d3f2debab4aeb087957e3684e" :manifest (("easy-kill-pkg.el" . "d0b7f267ba92e5a71f654262b95e508937eb437be3576fb6e929cc59933cf102") ("easy-kill.el" . "5990df098447f19f792571b15646d0a8ac1a5ce713169e4be55f8730e2e66739")) :feature t :version "20260121.752") :empty (:thing nil :text "" :bounds (1 . 1)) :append (:kill ("alpha beta") :candidate (:thing word :text "beta" :bounds (7 . 11))))"#
        ]],
    )
}

#[test]
fn easy_kill_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        active_region_is_copied_and_line_is_tried_otherwise(),
        word_sexp_and_list_expand_in_elisp(),
        easy_mark_selects_the_sexp_as_an_active_region(),
        append_joins_kills_and_empty_buffer_has_empty_candidate(),
    ];
    assert_oracle_batch_cases(oracle(), "easy-kill-rank457", "easy_kill_parity", &cases);
}
