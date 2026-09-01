use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PARENT_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PARENT_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PARENT_MODE_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'parent-mode)

(define-derived-mode neomacs-parent-mode-test-config-mode text-mode
  "Release-Config")
(define-derived-mode neomacs-parent-mode-test-manifest-mode
  neomacs-parent-mode-test-config-mode "Release-Manifest")
(define-derived-mode neomacs-parent-mode-test-deployment-mode
  neomacs-parent-mode-test-manifest-mode "Release-Deployment")

(defalias 'neomacs-parent-mode-test-document-parent 'text-mode)
(define-derived-mode neomacs-parent-mode-test-runbook-mode
  neomacs-parent-mode-test-document-parent "Release-Runbook")

(define-derived-mode neomacs-parent-mode-test-fundamental-mode
  fundamental-mode "Release-Raw")

(defun neomacs-parent-mode-test-capture-signal (function)
  "Run FUNCTION and return complete stable signal information."
  (condition-case error-data
      (progn (funcall function) 'no-signal)
    (error
     (list :symbol (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"###;

fn parent_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PARENT_MODE_MELPA_PIN, "parent-mode.el")
        .expect("prepare revision-pinned Parent Mode source below ./tmp")
        .with_prelude(PARENT_MODE_TEST_PRELUDE)
        .with_timeout(PARENT_MODE_TEST_TIMEOUT)
}

fn deployment_mode_ancestry_drives_real_feature_dispatch() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((mode 'neomacs-parent-mode-test-deployment-mode)
       (ancestry (parent-mode-list mode))
       (feature-rules
        '((text-mode . prose-navigation)
          (neomacs-parent-mode-test-config-mode . key-value-validation)
          (neomacs-parent-mode-test-manifest-mode . schema-completion)
          (neomacs-parent-mode-test-deployment-mode . deploy-actions)))
       (features
        (cl-loop for ancestor in ancestry
                 for feature = (alist-get ancestor feature-rules)
                 when feature collect feature)))
  (list :ancestry ancestry
        :features features
        :is-text (parent-mode-is-derived-p mode 'text-mode)
        :is-config
        (parent-mode-is-derived-p
         mode 'neomacs-parent-mode-test-config-mode)
        :is-prog (parent-mode-is-derived-p mode 'prog-mode)))
"###;
    let expected = expect![[
        r###"OK (:ancestry (text-mode neomacs-parent-mode-test-config-mode neomacs-parent-mode-test-manifest-mode neomacs-parent-mode-test-deployment-mode) :features (prose-navigation key-value-validation schema-completion deploy-actions) :is-text t :is-config t :is-prog nil)"###
    ]];
    ParityBatchCase::value(
        "deployment_mode_ancestry_drives_real_feature_dispatch",
        elisp_form,
        expected,
    )
}

fn built_in_editor_modes_report_their_complete_primary_hierarchies() -> ParityBatchCase {
    let elisp_form = r###"
(progn
  (require 'org)
  (require 'lisp-mode)
  (list
   :org (parent-mode-list 'org-mode)
   :emacs-lisp (parent-mode-list 'emacs-lisp-mode)
   :lisp-interaction (parent-mode-list 'lisp-interaction-mode)
   :special (parent-mode-list 'special-mode)
   :fundamental (parent-mode-list 'fundamental-mode)))
"###;
    let expected = expect![[
        r###"OK (:org (text-mode outline-mode org-mode) :emacs-lisp (prog-mode lisp-data-mode emacs-lisp-mode) :lisp-interaction (prog-mode lisp-data-mode emacs-lisp-mode lisp-interaction-mode) :special (special-mode) :fundamental (fundamental-mode))"###
    ]];
    ParityBatchCase::value(
        "built_in_editor_modes_report_their_complete_primary_hierarchies",
        elisp_form,
        expected,
    )
}

fn aliased_parent_modes_remain_visible_and_follow_runtime_alias_changes() -> ParityBatchCase {
    let elisp_form = r###"
(let ((old-target
       (symbol-function 'neomacs-parent-mode-test-document-parent)))
  (unwind-protect
      (let ((text-chain
             (parent-mode-list 'neomacs-parent-mode-test-runbook-mode)))
        (defalias 'neomacs-parent-mode-test-document-parent 'prog-mode)
        (list
         :text-chain text-chain
         :prog-chain
         (parent-mode-list 'neomacs-parent-mode-test-runbook-mode)
         :now-prog
         (parent-mode-is-derived-p
          'neomacs-parent-mode-test-runbook-mode 'prog-mode)
         :now-text
         (parent-mode-is-derived-p
          'neomacs-parent-mode-test-runbook-mode 'text-mode)))
    (defalias 'neomacs-parent-mode-test-document-parent old-target)))
"###;
    let expected = expect![[
        r###"OK (:text-chain (text-mode neomacs-parent-mode-test-document-parent neomacs-parent-mode-test-runbook-mode) :prog-chain (prog-mode neomacs-parent-mode-test-document-parent neomacs-parent-mode-test-runbook-mode) :now-prog t :now-text nil)"###
    ]];
    ParityBatchCase::value(
        "aliased_parent_modes_remain_visible_and_follow_runtime_alias_changes",
        elisp_form,
        expected,
    )
}

fn runtime_reparenting_updates_package_and_gnu_ancestry_queries_together() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((mode 'neomacs-parent-mode-test-manifest-mode)
       (old-parent (get mode 'derived-mode-parent))
       before after)
  (unwind-protect
      (progn
        (setq before
              (list :parent-mode (parent-mode-list mode)
                    :gnu-text (provided-mode-derived-p mode 'text-mode)
                    :gnu-prog (provided-mode-derived-p mode 'prog-mode)))
        (if (fboundp 'derived-mode-set-parent)
            (derived-mode-set-parent mode 'prog-mode)
          (put mode 'derived-mode-parent 'prog-mode))
        (setq after
              (list :parent-mode (parent-mode-list mode)
                    :gnu-text (provided-mode-derived-p mode 'text-mode)
                    :gnu-prog (provided-mode-derived-p mode 'prog-mode)))
        (list :before before :after after))
    (if (fboundp 'derived-mode-set-parent)
        (derived-mode-set-parent mode old-parent)
      (put mode 'derived-mode-parent old-parent))))
"###;
    let expected = expect![[
        r###"OK (:before (:parent-mode (text-mode neomacs-parent-mode-test-config-mode neomacs-parent-mode-test-manifest-mode) :gnu-text text-mode :gnu-prog nil) :after (:parent-mode (prog-mode neomacs-parent-mode-test-manifest-mode) :gnu-text nil :gnu-prog prog-mode))"###
    ]];
    ParityBatchCase::value(
        "runtime_reparenting_updates_package_and_gnu_ancestry_queries_together",
        elisp_form,
        expected,
    )
}

fn gnu_additional_parents_are_distinguished_from_the_primary_parent_chain() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((mode 'neomacs-parent-mode-test-config-mode)
       (old-extras (copy-sequence (get mode 'derived-mode-extra-parents))))
  (unwind-protect
      (progn
        (derived-mode-add-parents mode '(special-mode))
        (list
         :primary-chain (parent-mode-list mode)
         :package-special
         (parent-mode-is-derived-p mode 'special-mode)
         :gnu-special (provided-mode-derived-p mode 'special-mode)
         :gnu-text (provided-mode-derived-p mode 'text-mode)
         :gnu-all (derived-mode-all-parents mode)))
    (derived-mode-add-parents mode old-extras)))
"###;
    let expected = expect![[
        r###"OK (:primary-chain (text-mode neomacs-parent-mode-test-config-mode) :package-special nil :gnu-special special-mode :gnu-text text-mode :gnu-all (neomacs-parent-mode-test-config-mode text-mode special-mode))"###
    ]];
    ParityBatchCase::value(
        "gnu_additional_parents_are_distinguished_from_the_primary_parent_chain",
        elisp_form,
        expected,
    )
}

fn fundamental_and_missing_modes_preserve_exact_boundary_contracts() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :derived-from-fundamental
 (parent-mode-list 'neomacs-parent-mode-test-fundamental-mode)
 :fundamental (parent-mode-list 'fundamental-mode)
 :missing-list
 (neomacs-parent-mode-test-capture-signal
  (lambda () (parent-mode-list 'neomacs-parent-mode-test-missing-mode)))
 :missing-self
 (parent-mode-is-derived-p 'neomacs-parent-mode-test-missing-mode
                           'neomacs-parent-mode-test-missing-mode)
 :missing-other
 (neomacs-parent-mode-test-capture-signal
  (lambda ()
    (parent-mode-is-derived-p 'neomacs-parent-mode-test-missing-mode
                              'text-mode))))
"###;
    let expected = expect![[
        r###"OK (:derived-from-fundamental (neomacs-parent-mode-test-fundamental-mode) :fundamental (fundamental-mode) :missing-list (:symbol void-function :data (neomacs-parent-mode-test-missing-mode) :message "Symbol’s function definition is void: neomacs-parent-mode-test-missing-mode") :missing-self t :missing-other (:symbol void-function :data (neomacs-parent-mode-test-missing-mode) :message "Symbol’s function definition is void: neomacs-parent-mode-test-missing-mode"))"###
    ]];
    ParityBatchCase::value(
        "fundamental_and_missing_modes_preserve_exact_boundary_contracts",
        elisp_form,
        expected,
    )
}

#[test]
fn parent_mode_package_batch() {
    let cases = vec![
        deployment_mode_ancestry_drives_real_feature_dispatch(),
        built_in_editor_modes_report_their_complete_primary_hierarchies(),
        aliased_parent_modes_remain_visible_and_follow_runtime_alias_changes(),
        runtime_reparenting_updates_package_and_gnu_ancestry_queries_together(),
        gnu_additional_parents_are_distinguished_from_the_primary_parent_chain(),
        fundamental_and_missing_modes_preserve_exact_boundary_contracts(),
    ];
    assert_oracle_batch_cases(
        parent_mode_oracle(),
        "parent-mode-package-batch",
        "Parent Mode",
        &cases,
    );
}
