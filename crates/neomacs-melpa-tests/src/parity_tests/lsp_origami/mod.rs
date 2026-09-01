//! Practical parity for lsp-origami's public folding adapter workflows.
//!
//! The package's boundary is LSP Mode's capability and folding-range API.
//! These cases provide a closed, ordered boundary while keeping the package's
//! public enable commands and Origami's parser, fold tree, overlays, and
//! lifecycle real.

use std::time::Duration;

use expect_test::{Expect, expect};

use crate::{CachedMelpaOracle, LSP_ORIGAMI_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(set-face-attribute 'highlight nil :background "gray80")
(require 'lsp-origami)
(face-spec-set
 'origami-fold-header-face
 '((t (:box (:line-width 1 :color "gray80") :background "gray80")))
 'face-defface-spec)

(defconst lsp-origami390-test-source
  '("lsp-origami.el"
    "cd17a2fec193f46dc5adc795c0ff81098e3351eb7cf43b3fff07da7ea7588b60"))

(let ((file (symbol-file 'lsp-origami-try-enable 'defun)))
  (unless (and (file-regular-p file)
               (equal (file-name-nondirectory file)
                      (car lsp-origami390-test-source))
               (with-temp-buffer
                 (set-buffer-multibyte nil)
                 (insert-file-contents-literally file)
                 (equal (secure-hash 'sha256 (current-buffer))
                        (cadr lsp-origami390-test-source))))
    (error "Unexpected installed LSP Origami source: %S" file)))

(defun lsp-origami390-test-range (beg end &optional children)
  (make-lsp--folding-range :beg beg :end end :children children))

(defun lsp-origami390-test-ranges ()
  (let ((kept (lsp-origami390-test-range 20 55))
        (same-beg (lsp-origami390-test-range 5 30))
        (same-end (lsp-origami390-test-range 60 90)))
    (list (lsp-origami390-test-range
           5 90 (list kept same-beg same-end)))))

(defun lsp-origami390-test-fold-state (node)
  (list :beg (origami-fold-beg node)
        :end (origami-fold-end node)
        :offset (origami-fold-offset node)
        :open (and (origami-fold-open? node) t)
        :children (mapcar #'lsp-origami390-test-fold-state
                          (origami-fold-children node))))

(defun lsp-origami390-test-tree-state (tree)
  (mapcar #'lsp-origami390-test-fold-state (origami-fold-children tree)))

(defun lsp-origami390-test-overlay-state ()
  (mapcar
   (lambda (overlay)
     (list :start (overlay-start overlay)
           :end (overlay-end overlay)
           :invisible (overlay-get overlay 'invisible)))
   (sort (seq-filter
          (lambda (overlay) (overlay-get overlay 'isearch-open-invisible))
          (copy-sequence (overlays-in (point-min) (point-max))))
         (lambda (left right) (< (overlay-start left) (overlay-start right))))))

(defun lsp-origami390-test-condition (thunk)
  (condition-case error
      (list :return (funcall thunk))
    (error (list :error (car error) :data (cdr error)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LSP_ORIGAMI_MELPA_PIN, "lsp-origami.el")
        .expect("prepare exact shallow LSP Origami source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn workflow(name: &'static str, probe: &'static str, expected: Expect) -> ParityBatchCase {
    ParityBatchCase::value(name, probe, expected)
}

fn cases() -> Vec<ParityBatchCase> {
    vec![
        workflow(
            "public_enable_converts_nested_ranges_and_drives_origami_overlays",
            r####"
(with-temp-buffer
  (insert (make-string 100 ?x))
  (let (features closed reopened tree)
    (cl-letf (((symbol-function 'lsp-feature?)
               (lambda (method) (push method features) '(workspace)))
              ((symbol-function 'lsp--get-nested-folding-ranges)
               #'lsp-origami390-test-ranges))
      (call-interactively #'lsp-origami-try-enable)
      (setq tree (origami-get-fold-tree (current-buffer)))
      (goto-char 25)
      (call-interactively #'origami-close-node)
      (setq closed (lsp-origami390-test-overlay-state))
      (call-interactively #'origami-open-node)
      (setq reopened (lsp-origami390-test-overlay-state)))
    (list :origami (and origami-mode t)
          :adapter (and lsp-origami-mode t)
          :style origami-fold-style
          :parser (cdr (assq 'lsp-mode origami-parser-alist))
          :features (nreverse features)
          :tree (lsp-origami390-test-tree-state tree)
          :closed closed
          :reopened reopened)))
"####,
            expect![[
                r#"OK (:origami t :adapter t :style lsp-mode :parser lsp-origami--parser :features ("textDocument/foldingRange" "foldingRangeProvider" "foldingRangeProvider") :tree ((:beg 5 :end 90 :offset 0 :open t :children ((:beg 20 :end 55 :offset 0 :open t :children nil)))) :closed ((:start 5 :end 90 :invisible nil) (:start 5 :end 90 :invisible nil) (:start 20 :end 55 :invisible origami) (:start 20 :end 55 :invisible nil)) :reopened ((:start 5 :end 90 :invisible nil) (:start 5 :end 90 :invisible nil) (:start 20 :end 55 :invisible nil) (:start 20 :end 55 :invisible nil)))"#
            ]],
        ),
        workflow(
            "interactive_unsupported_capability_preserves_origami_then_recovers",
            r####"
(with-temp-buffer
  (let (unsupported recovered)
    (cl-letf (((symbol-function 'lsp-feature?) (lambda (_method) nil)))
      (setq unsupported
            (lsp-origami390-test-condition
             (lambda () (call-interactively #'lsp-origami-try-enable)))))
    (setq unsupported
          (append unsupported
                  (list :origami (and origami-mode t)
                        :adapter (and lsp-origami-mode t))))
    (cl-letf (((symbol-function 'lsp-feature?)
               (lambda (_method) '(workspace))))
      (setq recovered (call-interactively #'lsp-origami-try-enable)))
    (list :unsupported unsupported
          :recovery-return recovered
          :origami (and origami-mode t)
          :adapter (and lsp-origami-mode t)
          :style origami-fold-style)))
"####,
            expect![[
                r#"OK (:unsupported (:error lsp-capability-not-supported :data ("foldingRangeProvider") :origami t :adapter nil) :recovery-return t :origami t :adapter t :style lsp-mode)"#
            ]],
        ),
        workflow(
            "documented_after_open_hook_enables_buffer_local_adapter_state",
            r####"
(with-temp-buffer
  (let ((lsp-after-open-hook nil)
        features)
    (add-hook 'lsp-after-open-hook #'lsp-origami-try-enable)
    (cl-letf (((symbol-function 'lsp-feature?)
               (lambda (method) (push method features) '(workspace))))
      (run-hooks 'lsp-after-open-hook))
    (list :hook lsp-after-open-hook
          :features (nreverse features)
          :origami (and origami-mode t)
          :adapter (and lsp-origami-mode t)
          :style origami-fold-style
          :parser-count
          (length (seq-filter (lambda (entry) (eq (car entry) 'lsp-mode))
                              origami-parser-alist)))))
"####,
            expect![[
                r#"OK (:hook (lsp-origami-try-enable) :features ("textDocument/foldingRange") :origami t :adapter t :style lsp-mode :parser-count 1)"#
            ]],
        ),
        workflow(
            "parser_capability_failure_recovers_and_public_disable_clears_style",
            r####"
(with-temp-buffer
  (insert (make-string 100 ?x))
  (origami-mode 1)
  (lsp-origami-mode 1)
  (let (failure recovery-tree)
    (cl-letf (((symbol-function 'lsp-feature?) (lambda (_method) nil))
              ((symbol-function 'lsp--get-nested-folding-ranges)
               #'lsp-origami390-test-ranges))
      (setq failure
            (lsp-origami390-test-condition
             (lambda () (origami-get-fold-tree (current-buffer))))))
    (origami-reset (current-buffer))
    (cl-letf (((symbol-function 'lsp-feature?)
               (lambda (_method) '(workspace)))
              ((symbol-function 'lsp--get-nested-folding-ranges)
               #'lsp-origami390-test-ranges))
      (setq recovery-tree (origami-get-fold-tree (current-buffer))))
    (lsp-origami-mode -1)
    (list :failure failure
          :recovered (lsp-origami390-test-tree-state recovery-tree)
          :adapter (and lsp-origami-mode t)
          :origami (and origami-mode t)
          :style origami-fold-style
          :parser-still-registered
          (and (assq 'lsp-mode origami-parser-alist) t))))
"####,
            expect![[
                r#"OK (:failure (:error lsp-capability-not-supported :data ("foldingRangeProvider")) :recovered ((:beg 5 :end 90 :offset 0 :open t :children ((:beg 20 :end 55 :offset 0 :open t :children nil)))) :adapter nil :origami t :style nil :parser-still-registered t)"#
            ]],
        ),
    ]
}

#[test]
fn lsp_origami_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "lsp-origami package batch",
        "lsp_origami_parity",
        &cases(),
    );
}
