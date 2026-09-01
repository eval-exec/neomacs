use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_CORE_MELPA_PIN, HELM_THEMES_SOURCE_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'helm-themes)

(defun neomacs-helm-themes-test-run-session (initial preview accepted)
  "Run a simulated Helm theme session with INITIAL, PREVIEW, and ACCEPTED."
  (let ((custom-enabled-themes (copy-sequence initial))
        disable-calls
        load-calls
        helm-contract)
    (cl-letf (((symbol-function 'disable-theme)
               (lambda (theme)
                 (setq disable-calls (append disable-calls (list theme)))
                 (setq custom-enabled-themes
                       (delq theme custom-enabled-themes))))
              ((symbol-function 'load-theme)
               (lambda (theme &optional no-confirm _no-enable)
                 (setq load-calls
                       (append load-calls (list (list theme no-confirm))))
                 (setq custom-enabled-themes
                       (cons theme (delq theme custom-enabled-themes)))
                 t))
              ((symbol-function 'helm)
               (lambda (&rest arguments)
                 (setq helm-contract
                       (list :prompt (plist-get arguments :prompt)
                             :preselect (plist-get arguments :preselect)
                             :source-is-package-source
                             (eq (plist-get arguments :sources)
                                 helm-themes-source)
                             :buffer (plist-get arguments :buffer)))
                 (when preview (helm-themes--load-theme preview))
                 accepted)))
      (let ((result (helm-themes)))
        (list :result result
              :contract helm-contract
              :disabled disable-calls
              :loaded load-calls
              :enabled custom-enabled-themes)))))
"####;

fn candidates_and_helm_source_expose_default_plus_every_available_theme() -> ParityBatchCase {
    let elisp_form = r####"
(cl-letf (((symbol-function 'custom-available-themes)
           (lambda () '(modus-operandi wombat deeper-blue))))
  (list :candidates (helm-themes--candidates)
        :source
        (list :name (helm-get-attr 'name helm-themes-source)
              :group (helm-get-attr 'group helm-themes-source)
              :candidates-function
              (helm-get-attr 'candidates helm-themes-source)
              :action-is-loader
              (eq (helm-get-attr 'action helm-themes-source)
                  'helm-themes--load-theme)
              :persistent-is-loader
              (eq (helm-get-attr 'persistent-action helm-themes-source)
                  'helm-themes--load-theme))))
"####;
    let expected = expect![[
        r#"OK (:candidates (default modus-operandi wombat deeper-blue) :source (:name "Selection Theme" :group helm :candidates-function helm-themes--candidates :action-is-loader t :persistent-is-loader t))"#
    ]];
    ParityBatchCase::value(
        "candidates_and_helm_source_expose_default_plus_every_available_theme",
        elisp_form,
        expected,
    )
}

fn direct_loading_disables_all_prior_themes_and_default_clears_the_preview() -> ParityBatchCase {
    let elisp_form = r####"
(let ((custom-enabled-themes '(night-theme day-theme))
      disabled loaded after-preview)
  (cl-letf (((symbol-function 'disable-theme)
             (lambda (theme)
               (push theme disabled)
               (setq custom-enabled-themes
                     (delq theme custom-enabled-themes))))
            ((symbol-function 'load-theme)
             (lambda (theme &optional no-confirm _no-enable)
               (push (list theme no-confirm) loaded)
               (push theme custom-enabled-themes)
               t)))
    (let ((preview-result (helm-themes--load-theme "preview-theme")))
      (setq after-preview (copy-sequence custom-enabled-themes))
      (let ((default-result (helm-themes--load-theme "default")))
        (list :preview-result preview-result
              :after-preview after-preview
              :default-result default-result
              :after-default custom-enabled-themes
              :disabled (nreverse disabled)
              :loaded (nreverse loaded))))))
"####;
    let expected = expect![
        "OK (:preview-result t :after-preview (preview-theme) :default-result t :after-default nil :disabled (night-theme day-theme preview-theme) :loaded ((preview-theme t)))"
    ];
    ParityBatchCase::value(
        "direct_loading_disables_all_prior_themes_and_default_clears_the_preview",
        elisp_form,
        expected,
    )
}

fn cancelling_a_preview_restores_the_original_theme() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-helm-themes-test-run-session
 '(original-theme) "preview-theme" nil)
"####;
    let expected = expect![[
        r#"OK (:result nil :contract (:prompt "pattern (current theme: original-theme): " :preselect "original-theme$" :source-is-package-source t :buffer "*helm-themes*") :disabled (original-theme preview-theme) :loaded ((preview-theme t) (original-theme t)) :enabled (original-theme))"#
    ]];
    ParityBatchCase::value(
        "cancelling_a_preview_restores_the_original_theme",
        elisp_form,
        expected,
    )
}

fn accepting_a_preview_commits_it_and_accepting_default_leaves_no_theme() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :preview
 (neomacs-helm-themes-test-run-session
  '(original-theme) "preview-theme" t)
 :default
 (neomacs-helm-themes-test-run-session
  '(original-theme) "default" t)
 :cancel-without-original
 (neomacs-helm-themes-test-run-session
  nil "preview-theme" nil))
"####;
    let expected = expect![[
        r#"OK (:preview (:result t :contract (:prompt "pattern (current theme: original-theme): " :preselect "original-theme$" :source-is-package-source t :buffer "*helm-themes*") :disabled (original-theme) :loaded ((preview-theme t)) :enabled (preview-theme)) :default (:result t :contract (:prompt "pattern (current theme: original-theme): " :preselect "original-theme$" :source-is-package-source t :buffer "*helm-themes*") :disabled (original-theme) :loaded nil :enabled nil) :cancel-without-original (:result nil :contract (:prompt "pattern (current theme: default): " :preselect "nil$" :source-is-package-source t :buffer "*helm-themes*") :disabled (preview-theme) :loaded ((preview-theme t)) :enabled nil))"#
    ]];
    ParityBatchCase::value(
        "accepting_a_preview_commits_it_and_accepting_default_leaves_no_theme",
        elisp_form,
        expected,
    )
}

fn helm_themes_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_THEMES_SOURCE_PIN, "helm-themes.el")
        .expect("prepare pinned archived Helm-Themes source below ./tmp")
        .with_melpa_dependency(HELM_CORE_MELPA_PIN)
        .expect("prepare pinned Helm-Core dependency below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn helm_themes_practical_workflows_batch() {
    let cases = vec![
        candidates_and_helm_source_expose_default_plus_every_available_theme(),
        direct_loading_disables_all_prior_themes_and_default_clears_the_preview(),
        cancelling_a_preview_restores_the_original_theme(),
        accepting_a_preview_commits_it_and_accepting_default_leaves_no_theme(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("helm-themes parity batch");
    assert_oracle_batch_cases(
        helm_themes_oracle(),
        test_name,
        "helm-themes parity",
        &cases,
    );
}
