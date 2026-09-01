use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_MELPA_PIN, HELM_MODE_MANAGER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'helm-mode-manager)

(defun neomacs-helm-mode-manager-test-source (arguments)
  "Return the single legacy Helm source from ARGUMENTS."
  (plist-get arguments :sources))

(defun neomacs-helm-mode-manager-test-source-value (source property)
  "Return PROPERTY from SOURCE, resolving a dynamically bound symbol."
  (let ((value (cdr (assq property source))))
    (if (symbolp value) (symbol-value value) value)))
"####;

fn major_mode_discovery_accepts_zero_argument_commands_and_rejects_minor_modes() -> ParityBatchCase
{
    let elisp_form = r####"
(let ((symbols '(text-mode whitespace-mode emacs-lisp-mode
                 forward-char special-mode auto-fill-mode)))
  (cl-letf (((symbol-function 'mapatoms)
             (lambda (function &optional _obarray)
               (mapc function symbols))))
    (list :discovered (helm-mode-manager-list-major-modes)
          :contracts
          (mapcar
           (lambda (symbol)
             (list symbol
                   :command (commandp symbol)
                   :arguments (help-function-arglist symbol)))
           symbols))))
"####;
    let expected = expect![[
        r#"OK (:discovered ("special-mode" "emacs-lisp-mode" "text-mode") :contracts ((text-mode :command t :arguments nil) (whitespace-mode :command t :arguments "[Arg list not available until function definition is loaded.]") (emacs-lisp-mode :command t :arguments nil) (forward-char :command t :arguments (&optional arg1)) (special-mode :command t :arguments nil) (auto-fill-mode :command t :arguments (&optional arg1))))"#
    ]];
    ParityBatchCase::value(
        "major_mode_discovery_accepts_zero_argument_commands_and_rejects_minor_modes",
        elisp_form,
        expected,
    )
}

fn major_mode_helm_source_switches_the_buffer_and_previews_documentation() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (text-mode)
  (let (source-name candidates described)
    (cl-letf (((symbol-function 'helm-mode-manager-list-major-modes)
               (lambda () '("text-mode" "emacs-lisp-mode")))
              ((symbol-function 'describe-function)
               (lambda (function) (setq described function)))
              ((symbol-function 'helm)
               (lambda (&rest arguments)
                 (let* ((source
                         (neomacs-helm-mode-manager-test-source arguments))
                        (action (cdr (assq 'action source)))
                        (preview (cdr (assq 'persistent-action source))))
                   (setq source-name (cdr (assq 'name source)))
                   (setq candidates
                         (neomacs-helm-mode-manager-test-source-value
                          source 'candidates))
                   (funcall preview "text-mode")
                   (funcall action "emacs-lisp-mode")
                   'helm-completed))))
      (let ((result (helm-switch-major-mode)))
        (list :result result
              :source source-name
              :candidates candidates
              :described described
              :major-mode major-mode
              :derived-prog (derived-mode-p 'prog-mode)
              :comment-start comment-start)))))
"####;
    let expected = expect![[
        r#"OK (:result helm-completed :source "Major modes" :candidates ("text-mode" "emacs-lisp-mode") :described text-mode :major-mode emacs-lisp-mode :derived-prog prog-mode :comment-start ";")"#
    ]];
    ParityBatchCase::value(
        "major_mode_helm_source_switches_the_buffer_and_previews_documentation",
        elisp_form,
        expected,
    )
}

fn enabling_marked_minor_modes_uses_the_public_mode_commands() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (abbrev-mode -1)
  (overwrite-mode -1)
  (let ((minor-mode-list '(abbrev-mode overwrite-mode))
        source-name candidates described marked)
    (cl-letf (((symbol-function 'helm-marked-candidates)
               (lambda () '("abbrev-mode" "overwrite-mode")))
              ((symbol-function 'describe-function)
               (lambda (function) (setq described function)))
              ((symbol-function 'helm)
               (lambda (&rest arguments)
                 (let* ((source
                         (neomacs-helm-mode-manager-test-source arguments))
                        (action (cdr (assq 'action source)))
                        (preview (cdr (assq 'persistent-action source))))
                   (setq source-name (cdr (assq 'name source)))
                   (setq candidates
                         (neomacs-helm-mode-manager-test-source-value
                          source 'candidates))
                   (setq marked (helm-marked-candidates))
                   (funcall preview "abbrev-mode")
                   (funcall action nil)
                   'helm-completed))))
      (let ((result (helm-enable-minor-mode)))
        (prog1
            (list :result result
                  :source source-name
                  :candidates candidates
                  :marked marked
                  :described described
                  :enabled (list abbrev-mode overwrite-mode))
          (abbrev-mode -1)
          (overwrite-mode -1))))))
"####;
    let expected = expect![[
        r#"OK (:result helm-completed :source "Minor modes" :candidates (abbrev-mode overwrite-mode) :marked ("abbrev-mode" "overwrite-mode") :described abbrev-mode :enabled (t overwrite-mode-textual))"#
    ]];
    ParityBatchCase::value(
        "enabling_marked_minor_modes_uses_the_public_mode_commands",
        elisp_form,
        expected,
    )
}

fn disabling_filters_active_modes_ignores_unbound_entries_and_passes_minus_one() -> ParityBatchCase
{
    let elisp_form = r####"
(with-temp-buffer
  (abbrev-mode 1)
  (overwrite-mode 1)
  (let ((minor-mode-list
         '(abbrev-mode neomacs-unbound-minor-mode overwrite-mode))
        source-name candidates described)
    (cl-letf (((symbol-function 'helm-marked-candidates)
               (lambda () '("overwrite-mode" "abbrev-mode")))
              ((symbol-function 'describe-function)
               (lambda (function) (setq described function)))
              ((symbol-function 'helm)
               (lambda (&rest arguments)
                 (let* ((source
                         (neomacs-helm-mode-manager-test-source arguments))
                        (action (cdr (assq 'action source)))
                        (preview (cdr (assq 'persistent-action source))))
                   (setq source-name (cdr (assq 'name source)))
                   (setq candidates
                         (neomacs-helm-mode-manager-test-source-value
                          source 'candidates))
                   (funcall preview "overwrite-mode")
                   (funcall action nil)
                   'helm-completed))))
      (let ((result (helm-disable-minor-mode)))
        (list :result result
              :source source-name
              :candidates candidates
              :described described
              :enabled-after (list abbrev-mode overwrite-mode))))))
"####;
    let expected = expect![[
        r#"OK (:result helm-completed :source "Active minor modes" :candidates (overwrite-mode abbrev-mode) :described overwrite-mode :enabled-after (nil nil))"#
    ]];
    ParityBatchCase::value(
        "disabling_filters_active_modes_ignores_unbound_entries_and_passes_minus_one",
        elisp_form,
        expected,
    )
}

fn helm_mode_manager_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_MODE_MANAGER_MELPA_PIN, "helm-mode-manager.el")
        .expect("prepare pinned Helm-Mode-Manager source below ./tmp")
        .with_melpa_dependency(HELM_MELPA_PIN)
        .expect("prepare pinned Helm dependency below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn helm_mode_manager_practical_workflows_batch() {
    let cases = vec![
        major_mode_discovery_accepts_zero_argument_commands_and_rejects_minor_modes(),
        major_mode_helm_source_switches_the_buffer_and_previews_documentation(),
        enabling_marked_minor_modes_uses_the_public_mode_commands(),
        disabling_filters_active_modes_ignores_unbound_entries_and_passes_minus_one(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("helm-mode-manager parity batch");
    assert_oracle_batch_cases(
        helm_mode_manager_oracle(),
        test_name,
        "helm-mode-manager parity",
        &cases,
    );
}
