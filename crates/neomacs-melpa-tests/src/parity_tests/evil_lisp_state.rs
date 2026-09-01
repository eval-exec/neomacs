use std::time::Duration;

use expect_test::expect;

use crate::{
    BIND_MAP_MELPA_PIN, CachedMelpaOracle, EVIL_LISP_STATE_MELPA_PIN, EVIL_MELPA_PIN,
    SMARTPARENS_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'smartparens-config)

(defun neomacs-evil-lisp-state-test-balanced-p ()
  "Return non-nil when the accessible buffer has balanced delimiters."
  (condition-case nil
      (progn (check-parens) t)
    (error nil)))

(defun neomacs-evil-lisp-state-test-state ()
  "Capture practical buffer and Evil state after a structural edit."
  (list
   :buffer (buffer-substring-no-properties (point-min) (point-max))
   :point (point)
   :line (line-number-at-pos)
   :column (current-column)
   :char (char-after)
   :evil-state evil-state
   :previous evil-previous-state
   :lisp-state (not (null (evil-lisp-state-p)))
   :smartparens (bound-and-true-p smartparens-mode)
   :balanced (neomacs-evil-lisp-state-test-balanced-p)))

(defun neomacs-evil-lisp-state-test-command (command)
  "Invoke COMMAND with the pre/post hooks of a real command cycle."
  (let ((this-command command)
        (real-this-command command))
    (run-hooks 'pre-command-hook)
    (unwind-protect
        (call-interactively command)
      (run-hooks 'post-command-hook))))
"####;

fn lisp_state_lifecycle_forces_structural_editing_and_returns_to_normal() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (evil-local-mode 1)
  (evil-normal-state)
  (let ((before
         (list :state evil-state
               :lisp (evil-lisp-state-p)
               :smartparens (bound-and-true-p smartparens-mode)))
        entered toggled-back toggled-again quit)
    (evil-lisp-state)
    (setq entered
          (list
           :state evil-state
           :previous evil-previous-state
           :lisp (evil-lisp-state-p)
           :smartparens smartparens-mode
           :mode (evil-state-property 'lisp :mode)
           :tag (evil-state-property 'lisp :tag)
           :cursor (evil-state-property 'lisp :cursor)
           :local-map (keymapp evil-lisp-state-local-map)
           :toggle (lookup-key evil-lisp-state-map ".")
           :escape (lookup-key evil-lisp-state-map [escape])
           :slurp (lookup-key evil-lisp-state-major-mode-map "s")
           :wrap (lookup-key evil-lisp-state-major-mode-map "w")))
    (lisp-state-toggle-lisp-state)
    (setq toggled-back (list evil-state evil-previous-state))
    (lisp-state-toggle-lisp-state)
    (setq toggled-again (list evil-state evil-previous-state))
    (evil-lisp-state/quit)
    (setq quit (list evil-state evil-previous-state smartparens-mode))
    (evil-local-mode -1)
    (list :before before
          :entered entered
          :toggled-back toggled-back
          :toggled-again toggled-again
          :quit quit
          :disabled (list evil-local-mode evil-state))))
"####;
    let expected = expect![
        "OK (:before (:state normal :lisp nil :smartparens nil) :entered (:state lisp :previous normal :lisp t :smartparens t :mode evil-lisp-state-minor-mode :tag evil-lisp-state-tag :cursor evil-lisp-state-cursor :local-map t :toggle lisp-state-toggle-lisp-state :escape evil-lisp-state/quit :slurp evil-lisp-state-sp-forward-slurp-sexp :wrap evil-lisp-state-wrap) :toggled-back (normal lisp) :toggled-again (lisp normal) :quit (normal lisp t) :disabled (nil nil))"
    ];
    ParityBatchCase::value(
        "lisp_state_lifecycle_forces_structural_editing_and_returns_to_normal",
        elisp_form,
        expected,
    )
}

fn mapped_commands_slurp_barf_and_transpose_a_real_pipeline() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (evil-local-mode 1)
  (evil-normal-state)
  (insert "(pipeline (fetch source)) (validate config) (publish result)")
  (goto-char (point-min))
  (search-forward "pipeline")
  (let ((slurp-command
         (lookup-key evil-lisp-state-major-mode-map (kbd "s")))
        (barf-command
         (lookup-key evil-lisp-state-major-mode-map (kbd "b")))
        (transpose-command
         (lookup-key evil-lisp-state-major-mode-map (kbd "t")))
        states)
    (call-interactively slurp-command)
    (push (cons :slurp-validate
                (neomacs-evil-lisp-state-test-state))
          states)
    (call-interactively slurp-command)
    (push (cons :slurp-publish
                (neomacs-evil-lisp-state-test-state))
          states)
    (call-interactively barf-command)
    (push (cons :barf-publish
                (neomacs-evil-lisp-state-test-state))
          states)
    (goto-char (point-min))
    (search-forward "(fetch source)")
    (call-interactively transpose-command)
    (push (cons :transpose-stages
                (neomacs-evil-lisp-state-test-state))
          states)
    (prog1
        (list :commands
              (list slurp-command barf-command transpose-command)
              :states (nreverse states))
      (evil-local-mode -1))))
"####;
    let expected = expect![[
        r#"OK (:commands (evil-lisp-state-sp-forward-slurp-sexp evil-lisp-state-sp-forward-barf-sexp evil-lisp-state-sp-transpose-sexp) :states ((:slurp-validate :buffer "(pipeline (fetch source) (validate config)) (publish result)" :point 10 :line 1 :column 9 :char 32 :evil-state lisp :previous normal :lisp-state t :smartparens t :balanced t) (:slurp-publish :buffer "(pipeline (fetch source) (validate config) (publish result))" :point 10 :line 1 :column 9 :char 32 :evil-state lisp :previous lisp :lisp-state t :smartparens t :balanced t) (:barf-publish :buffer "(pipeline (fetch source) (validate config)) (publish result)" :point 10 :line 1 :column 9 :char 32 :evil-state lisp :previous lisp :lisp-state t :smartparens t :balanced t) (:transpose-stages :buffer "(pipeline (validate config) (fetch source)) (publish result)" :point 43 :line 1 :column 42 :char 41 :evil-state lisp :previous lisp :lisp-state t :smartparens t :balanced t)))"#
    ]];
    ParityBatchCase::value(
        "mapped_commands_slurp_barf_and_transpose_a_real_pipeline",
        elisp_form,
        expected,
    )
}

fn mode_scoped_leader_resolves_and_runs_the_real_evil_wrap_binding() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (evil-lisp-state-leader ", l")
  (let (elisp-result text-result)
    (with-temp-buffer
      (emacs-lisp-mode)
      (evil-local-mode 1)
      (evil-normal-state)
      (insert "(total price tax)")
      (goto-char (point-min))
      (search-forward "total")
      (goto-char (match-beginning 0))
      (evil-normalize-keymaps)
      (let ((binding (key-binding (kbd ", l w"))))
        (neomacs-evil-lisp-state-test-command binding)
        (setq elisp-result
              (list :binding binding
                    :state (neomacs-evil-lisp-state-test-state)
                    :root-active evil-lisp-state-major-mode-map-active)))
      (evil-local-mode -1))
    (with-temp-buffer
      (text-mode)
      (evil-local-mode 1)
      (evil-normal-state)
      (evil-normalize-keymaps)
      (setq text-result
            (list :binding (key-binding (kbd ", l w"))
                  :root-active
                  (and (boundp 'evil-lisp-state-major-mode-map-active)
                       evil-lisp-state-major-mode-map-active)))
      (evil-local-mode -1))
    (list :elisp elisp-result :text text-result)))
"####;
    let expected = expect![[
        r#"OK (:elisp (:binding evil-lisp-state-wrap :state (:buffer "((total) price tax)" :point 3 :line 1 :column 2 :char 116 :evil-state lisp :previous normal :lisp-state t :smartparens t :balanced t) :root-active t) :text (:binding nil :root-active nil))"#
    ]];
    ParityBatchCase::value(
        "mode_scoped_leader_resolves_and_runs_the_real_evil_wrap_binding",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn insert_before_and_after_commands_build_balanced_sibling_forms() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (evil-local-mode 1)
  (evil-normal-state)
  (insert "(progn\n  (prepare)\n  (deploy))")
  (goto-char (point-min))
  (search-forward "prepare")
  (goto-char (match-beginning 0))
  (let ((after-command
         (lookup-key evil-lisp-state-major-mode-map (kbd ")")))
        (before-command
         (lookup-key evil-lisp-state-major-mode-map (kbd "(")))
        after-insert)
    (evil-lisp-state)
    (neomacs-evil-lisp-state-test-command after-command)
    (insert "validate")
    (evil-normal-state)
    (setq after-insert (neomacs-evil-lisp-state-test-state))
    (goto-char (point-min))
    (search-forward "deploy")
    (goto-char (match-beginning 0))
    (neomacs-evil-lisp-state-test-command before-command)
    (insert "audit")
    (evil-normal-state)
    (indent-region (point-min) (point-max))
    (prog1
        (list :commands (list before-command after-command)
              :after-insert after-insert
              :completed (neomacs-evil-lisp-state-test-state))
      (evil-local-mode -1))))
"####;
    let expected = expect![[
        r#"OK (:commands (evil-lisp-state-insert-sexp-before evil-lisp-state-insert-sexp-after) :after-insert (:buffer "(progn\n  (prepare)\n  (validate)\n  (deploy))" :point 30 :line 3 :column 10 :char 101 :evil-state normal :previous insert :lisp-state nil :smartparens t :balanced t) :completed (:buffer "(progn\n  (prepare)\n  (validate)\n  (audit)\n  (deploy))" :point 40 :line 4 :column 7 :char 116 :evil-state normal :previous insert :lisp-state nil :smartparens t :balanced t))"#
    ]];
    ParityBatchCase::value(
        "insert_before_and_after_commands_build_balanced_sibling_forms",
        elisp_form,
        expected,
    )
}

fn lisp_navigation_and_end_of_line_evaluation_follow_nested_code() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (smartparens-mode 1)
  (evil-local-mode 1)
  (evil-normal-state)
  (insert
   "(let ((subtotal 40))\n"
   "  (+ subtotal 2))\n"
   "(setq neomacs-evil-lisp-state-total (+ 20 22))\n")
  (goto-char (point-max))
  (forward-line -1)
  (lisp-state-eval-sexp-end-of-line)
  (let ((evaluated neomacs-evil-lisp-state-total)
        closing opening forward-symbol beginning)
    (goto-char (point-min))
    (search-forward "subtotal 40")
    (lisp-state-next-closing-paren)
    (setq closing
          (list :point (point) :char (char-after)
                :depth (car (syntax-ppss))))
    (search-forward "+ subtotal")
    (lisp-state-prev-opening-paren)
    (setq opening
          (list :point (point) :char (char-after)
                :depth (car (syntax-ppss))))
    (goto-char (point-min))
    (lisp-state-forward-symbol)
    (setq forward-symbol
          (list :point (point)
                :symbol (symbol-at-point)
                :depth (car (syntax-ppss))))
    (search-forward "subtotal 2")
    (backward-char 2)
    (lisp-state-beginning-of-sexp)
    (setq beginning
          (list :point (point) :char (char-after)
                :depth (car (syntax-ppss))))
    (prog1
        (list :evaluated evaluated
              :closing closing
              :opening opening
              :forward-symbol forward-symbol
              :beginning beginning
              :state (neomacs-evil-lisp-state-test-state))
      (makunbound 'neomacs-evil-lisp-state-total)
      (evil-local-mode -1))))
"####;
    let expected = expect![[
        r#"OK (:evaluated 42 :closing (:point 20 :char 41 :depth 2) :opening (:point 24 :char 40 :depth 1) :forward-symbol (:point 2 :symbol let :depth 1) :beginning (:point 24 :char 40 :depth 1) :state (:buffer "(let ((subtotal 40))\n  (+ subtotal 2))\n(setq neomacs-evil-lisp-state-total (+ 20 22))\n" :point 24 :line 2 :column 2 :char 40 :evil-state normal :previous normal :lisp-state nil :smartparens t :balanced t))"#
    ]];
    ParityBatchCase::value(
        "lisp_navigation_and_end_of_line_evaluation_follow_nested_code",
        elisp_form,
        expected,
    )
}

fn evil_lisp_state_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_LISP_STATE_MELPA_PIN, "evil-lisp-state.el")
        .expect("prepare pinned Evil Lisp State source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare pinned Evil dependency")
        .with_melpa_dependency(BIND_MAP_MELPA_PIN)
        .expect("prepare pinned Bind Map dependency")
        .with_melpa_dependency(SMARTPARENS_MELPA_PIN)
        .expect("prepare pinned Smartparens dependency")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn evil_lisp_state_practical_workflows_batch() {
    let cases = vec![
        lisp_state_lifecycle_forces_structural_editing_and_returns_to_normal(),
        mapped_commands_slurp_barf_and_transpose_a_real_pipeline(),
        mode_scoped_leader_resolves_and_runs_the_real_evil_wrap_binding(),
        insert_before_and_after_commands_build_balanced_sibling_forms(),
        lisp_navigation_and_end_of_line_evaluation_follow_nested_code(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("evil-lisp-state parity batch");
    assert_oracle_batch_cases(
        evil_lisp_state_oracle(),
        test_name,
        "evil-lisp-state parity",
        &cases,
    );
}
