//! Practical parity for Racket Mode's process-free classic editing surface.
//!
//! Upstream deliberately implements `racket-mode` without requiring its Racket
//! back end. These cases exercise that documented boundary through the public
//! mode and editing commands while keeping font-lock, indentation, completion,
//! Imenu, Xref, hideshow overlays, and failure recovery real.

use std::time::Duration;

use expect_test::{Expect, expect};

use crate::{CachedMelpaOracle, RACKET_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'racket-mode)

(defconst racket391-test-source
  '("racket-mode.el"
    "fa46b0ce8b7d1b5a9b176fc4cecc5bcb63642b7f606c7568f31405023f98a640"
    38
    "e4f1cc98d1d82ee6f3e6f55f4e909c2d04ffa60e061380a97e7c0239accfa4da"))

(defun racket391-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((file (symbol-file 'racket-mode 'defun))
       (directory (and file (file-name-directory file)))
       (files (and directory
                   (seq-remove
                    (lambda (path)
                      (member (file-name-nondirectory path)
                              '("racket-mode-autoloads.el"
                                "racket-mode-pkg.el")))
                    (directory-files directory t "\\`racket-.*\\.el\\'"))))
       (files (sort files #'string-lessp))
       (manifest
        (mapconcat
         (lambda (path)
           (format "%s\t%s\n"
                   (file-name-nondirectory path)
                   (racket391-test-file-sha256 path)))
         files
         "")))
  (unless (and (file-regular-p file)
               (equal (file-name-nondirectory file)
                      (nth 0 racket391-test-source))
               (equal (racket391-test-file-sha256 file)
                      (nth 1 racket391-test-source))
               (= (length files) (nth 2 racket391-test-source))
               (equal (secure-hash 'sha256 manifest)
                      (nth 3 racket391-test-source)))
    (error "Unexpected installed Racket Mode sources: %S" file)))

(defun racket391-test-with-mode (text thunk)
  "Run THUNK in an owned classic Racket buffer containing TEXT."
  (let ((hs-special-modes-alist (copy-tree hs-special-modes-alist))
        (racket-mode-map (copy-keymap racket-mode-map))
        (racket-submodules-at-point-function
         racket-submodules-at-point-function))
    (with-temp-buffer
      (insert text)
      (racket-mode)
      (funcall thunk))))

(defun racket391-test-face-runs ()
  (let ((pos (point-min)) result)
    (while (< pos (point-max))
      (let ((end (or (next-single-property-change
                      pos 'face nil (point-max))
                     (point-max))))
        (push (list (buffer-substring-no-properties pos end)
                    (get-text-property pos 'face)
                    pos end)
              result)
        (setq pos end)))
    (nreverse result)))

(defun racket391-test-index-state (index)
  (mapcar
   (lambda (entry)
     (if (markerp (cdr entry))
         (cons (car entry) (marker-position (cdr entry)))
       (cons (car entry)
             (mapcar (lambda (child)
                       (cons (car child) (marker-position (cdr child))))
                     (cdr entry)))))
   index))

(defun racket391-test-overlay-state ()
  (mapcar
   (lambda (overlay)
     (list :start (overlay-start overlay)
           :end (overlay-end overlay)
           :invisible (overlay-get overlay 'invisible)
           :hs (overlay-get overlay 'hs)))
   (sort (copy-sequence (overlays-in (point-min) (point-max)))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun racket391-test-condition (thunk)
  (condition-case error
      (list :return (funcall thunk))
    (error (list :error (car error) :data (cdr error)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RACKET_MODE_MELPA_PIN, "racket-mode.el")
        .expect("prepare exact shallow Racket Mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn workflow(name: &'static str, probe: &'static str, expected: Expect) -> ParityBatchCase {
    ParityBatchCase::value(name, probe, expected)
}

fn cases() -> Vec<ParityBatchCase> {
    vec![
        workflow(
            "public_mode_installs_font_lock_keys_and_editor_integrations",
            r####"
(racket391-test-with-mode
 "#lang racket\n;; café λ\n(define (square x)\n  (match x\n    [(? number? n) (* n n)]))\n"
 (lambda ()
   (font-lock-ensure)
   (list :mode major-mode
         :mode-name mode-name
         :syntax (syntax-table-p (syntax-table))
         :indent indent-line-function
         :completion completion-at-point-functions
         :xref xref-backend-functions
         :comment (list comment-start comment-add comment-column)
         :keys (mapcar (lambda (key)
                         (lookup-key racket-mode-map (kbd key)))
                       '("C-c C-c" "C-c C-t" "C-c C-p" "M-C-y"))
         :faces (racket391-test-face-runs))))
"####,
            expect![[
                r##"OK (:mode racket-mode :mode-name "Racket" :syntax t :indent racket-indent-line :completion (racket-complete-at-point) :xref (racket-mode-xref-backend-function t) :comment (";" 1 40) :keys (racket-run-module-at-point racket-test racket-cycle-paren-shapes racket-insert-lambda) :faces (("#lang" font-lock-keyword-face 1 6) (" " nil 6 7) ("racket" font-lock-variable-name-face 7 13) ("\n" nil 13 14) (";; " font-lock-comment-delimiter-face 14 17) ("café λ\n" font-lock-comment-face 17 24) ("(" nil 24 25) ("define" font-lock-keyword-face 25 31) (" (" nil 31 33) ("square" font-lock-function-name-face 33 39) (" x)\n  (" nil 39 46) ("match" font-lock-builtin-face 46 51) (" x\n    [(? " nil 51 62) ("number?" font-lock-builtin-face 62 69) (" n) (" nil 69 74) ("*" font-lock-builtin-face 74 75) (" n n)]))\n" nil 75 84)))"##
            ]],
        ),
        workflow(
            "public_indent_align_parens_and_closing_commands_edit_real_text",
            r####"
(racket391-test-with-mode
 "(let ([a 12]\n[long-name 23])\n(cond\n[(positive? a) 'yes]\n[else 'no]))"
 (lambda ()
   (indent-region (point-min) (point-max))
   (let ((indented (buffer-string)) aligned unaligned cycled closing)
     (goto-char (point-min))
     (search-forward "[a")
     (backward-char 2)
     (call-interactively #'racket-align)
     (setq aligned (buffer-string))
     (call-interactively #'racket-unalign)
     (setq unaligned (buffer-string))
     (goto-char (point-min))
     (call-interactively #'racket-cycle-paren-shapes)
     (setq cycled (buffer-string))
     (erase-buffer)
     (insert "(list 1 2")
     (let ((last-command-event ?}))
       (call-interactively #'racket-insert-closing))
     (setq closing (buffer-string))
     (list :indented indented
           :aligned aligned
           :unaligned unaligned
           :cycled cycled
           :closing closing))))
"####,
            expect![[
                r##"OK (:indented "(let ([a 12]\n      [long-name 23])\n  (cond\n    [(positive? a) 'yes]\n    [else 'no]))" :aligned "(let ([a         12]\n      [long-name 23])\n  (cond\n    [(positive? a) 'yes]\n    [else 'no]))" :unaligned "(let ([a 12]\n      [long-name 23])\n  (cond\n    [(positive? a) 'yes]\n    [else 'no]))" :cycled "[let ([a 12]\n      [long-name 23])\n  (cond\n    [(positive? a) 'yes]\n    [else 'no])]" :closing "(list 1 2)")"##
            ]],
        ),
        workflow(
            "documented_completion_at_point_uses_static_racket_candidates",
            r####"
(racket391-test-with-mode
 "#lang racket\n(for/f"
 (lambda ()
   (let (observed)
     (let ((completion-in-region-function
            (lambda (beg end table &optional predicate)
              (setq observed
                    (list :bounds
                          (list (if (markerp beg) (marker-position beg) beg)
                                (if (markerp end) (marker-position end) end))
                          :prefix (buffer-substring-no-properties beg end)
                          :candidates
                          (all-completions
                           (buffer-substring-no-properties beg end)
                           table predicate)))
              t)))
       (call-interactively #'completion-at-point))
     (list :capf completion-at-point-functions
           :observed observed
           :buffer (buffer-string)
           :point (point)))))
"####,
            expect![[
                r##"OK (:capf (racket-complete-at-point) :observed (:bounds (15 20) :prefix "for/f" :candidates ("for/first" "for/first:" "for/flvector:" "for/fold" "for/fold/derived" "for/fold:" "for/foldr" "for/foldr/derived" "for/foldr:")) :buffer "#lang racket\n(for/f" :point 20)"##
            ]],
        ),
        workflow(
            "mode_hooks_build_nested_imenu_and_plain_xref_results",
            r####"
(racket391-test-with-mode
 "#lang racket\n(define top 1)\n(module+ test\n  (define (inner x) (+ x 1)))\n(require \"peer.rkt\")\n"
 (lambda ()
   (let* ((index (funcall imenu-create-index-function))
          (backend (run-hook-with-args-until-success
                    'xref-backend-functions))
          (bogus (car (xref-backend-definitions backend "top")))
          (location (xref-item-location bogus)))
     (goto-char (point-max))
     (imenu (car index))
     (list :index (racket391-test-index-state index)
           :selected (list (line-number-at-pos)
                           (current-column)
                           (thing-at-point 'symbol t))
           :backend backend
           :identifier
           (let ((id (progn
                       (goto-char (point-min))
                       (search-forward "\"peer.rkt\"")
                       (backward-char 1)
                       (xref-backend-identifier-at-point backend))))
             (substring-no-properties id))
           :bogus (list (xref-item-summary bogus)
                        (xref-location-group location)
                        (xref-location-line location))))))
"####,
            expect![[
                r##"OK (:index (("top" . 22) ("Module: test" ("inner" . 54))) :selected (2 8 "top") :backend racket-mode-xref :identifier "peer.rkt" :bogus ("top" "(No location)" nil))"##
            ]],
        ),
        workflow(
            "public_test_folding_creates_and_removes_exact_hideshow_overlays",
            r####"
(racket391-test-with-mode
 "#lang racket\n(define (live x) (+ x 1))\n(module+ test\n  (check-equal? (live 1) 2)\n  (check-equal? (live 2) 3))\n(module test racket\n  (displayln 'ok))\n"
 (lambda ()
   (let (fold-message folded unfold-message unfolded)
     (cl-letf (((symbol-function 'message)
                (lambda (fmt &rest args)
                  (setq fold-message (apply #'format fmt args)))))
       (call-interactively #'racket-fold-all-tests))
     (setq folded (racket391-test-overlay-state))
     (cl-letf (((symbol-function 'message)
                (lambda (fmt &rest args)
                  (setq unfold-message (apply #'format fmt args)))))
       (call-interactively #'racket-unfold-all-tests))
     (setq unfolded (racket391-test-overlay-state))
     (list :hs (and hs-minor-mode t)
           :fold-message fold-message
           :folded folded
           :unfold-message unfold-message
           :unfolded unfolded))))
"####,
            expect![[
                r##"OK (:hs t :fold-message "Folded 2 test submodules" :folded ((:start 53 :end 109 :invisible hs :hs code) (:start 130 :end 148 :invisible hs :hs code)) :unfold-message "Unfolded 2 test submodules" :unfolded nil)"##
            ]],
        ),
        workflow(
            "alignment_failure_is_atomic_and_a_corrected_buffer_recovers",
            r####"
(racket391-test-with-mode
 "(let ([a 1][long-name 2])\n  (+ a long-name))"
 (lambda ()
   (goto-char (point-min))
   (search-forward "[a")
   (backward-char 2)
   (let* ((before (buffer-string))
          (failure
           (racket391-test-condition
            (lambda () (call-interactively #'racket-align))))
          (after-failure (buffer-string)))
     (erase-buffer)
     (insert "(let ([a 1]\n      [long-name 2])\n  (+ a long-name))")
     (goto-char (point-min))
     (search-forward "[a")
     (backward-char 2)
     (let ((recovery
            (racket391-test-condition
             (lambda () (call-interactively #'racket-align)))))
       (list :before before
             :failure failure
             :after-failure after-failure
             :recovery recovery
             :buffer (buffer-string))))))
"####,
            expect![[
                r##"OK (:before "(let ([a 1][long-name 2])\n  (+ a long-name))" :failure (:error user-error :data ("Can’t align if any couples are on same line")) :after-failure "(let ([a 1][long-name 2])\n  (+ a long-name))" :recovery (:return nil) :buffer "(let ([a         1]\n      [long-name 2])\n  (+ a long-name))")"##
            ]],
        ),
    ]
}

#[test]
fn racket_mode_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "racket-mode package batch",
        "racket_mode_parity",
        &cases(),
    );
}
