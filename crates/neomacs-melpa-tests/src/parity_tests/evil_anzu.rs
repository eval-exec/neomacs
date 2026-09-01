use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_ANZU_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-anzu)

(defun neomacs-evil-anzu-test-buffer (text body)
  "Run BODY in a temporary Evil buffer containing TEXT."
  (with-temp-buffer
    (insert text)
    (goto-char (point-min))
    (evil-local-mode 1)
    (unwind-protect
        (let ((evil-search-wrap t)
              (evil-search-wrap-ring-bell nil)
              (evil-regexp-search nil)
              (evil-ex-search-interactive nil)
              (evil-ex-search-highlight-all nil)
              (evil-ex-search-pattern nil)
              (evil-ex-search-direction 'forward)
              (evil-ex-search-offset "")
              (evil-ex-search-history nil)
              (evil-search-forward-history nil)
              (evil-search-backward-history nil)
              (search-ring nil)
              (regexp-search-ring nil)
              (isearch-string "")
              (isearch-regexp nil)
              (isearch-forward t)
              (evil-flash-timer nil))
          (evil-normal-state)
          (funcall body))
      (when anzu-mode (anzu-mode -1))
      (evil-local-mode -1))))

(defun neomacs-evil-anzu-test-state (&optional length)
  "Return visible search and Anzu state, including LENGTH characters at point."
  (list
   :point (point)
   :line (line-number-at-pos)
   :text (and length
              (<= (+ (point) length) (1+ (point-max)))
              (buffer-substring-no-properties (point) (+ (point) length)))
   :current anzu--current-position
   :total anzu--total-matched
   :state anzu--state
   :indicator (substring-no-properties (or (anzu--update-mode-line) ""))
   :mode-line-entry (and (member anzu--mode-line-format mode-line-format) t)
   :forward-history (copy-tree evil-search-forward-history)
   :backward-history (copy-tree evil-search-backward-history)))

(defun neomacs-evil-anzu-test-capture (function)
  "Return FUNCTION's value or exact error data."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"####;

fn repeated_forward_and_backward_evil_searches_track_the_visible_match() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-anzu-test-buffer
 "release draft\nrelease candidate\nrelease approved\nrelease archived\n"
 (lambda ()
   (anzu-mode 1)
   (cl-letf (((symbol-function 'evil-flash-search-pattern)
              (lambda (&rest _) nil)))
     (evil-search "release" t nil (point-min))
     (let ((first (neomacs-evil-anzu-test-state 7)))
       (evil-search-next 1)
       (let ((second (neomacs-evil-anzu-test-state 7)))
         (evil-search-next 2)
         (let ((fourth (neomacs-evil-anzu-test-state 7)))
           (evil-search-previous 1)
           (list :first first
                 :second second
                 :fourth fourth
                 :backward (neomacs-evil-anzu-test-state 7))))))))
"####;
    let expected = expect![[
        r#"OK (:first (:point 1 :line 1 :text "release" :current 1 :total 4 :state search :indicator "(1/4)" :mode-line-entry t :forward-history nil :backward-history nil) :second (:point 15 :line 2 :text "release" :current 2 :total 4 :state search :indicator "(2/4)" :mode-line-entry t :forward-history nil :backward-history nil) :fourth (:point 50 :line 4 :text "release" :current 4 :total 4 :state search :indicator "(4/4)" :mode-line-entry t :forward-history nil :backward-history nil) :backward (:point 33 :line 3 :text "release" :current 3 :total 4 :state search :indicator "(3/4)" :mode-line-entry t :forward-history nil :backward-history nil))"#
    ]];
    ParityBatchCase::value(
        "repeated_forward_and_backward_evil_searches_track_the_visible_match",
        elisp_form,
        expected,
    )
}

fn ex_regexp_search_counts_matches_and_reports_wrapping_through_public_commands() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-evil-anzu-test-buffer
 "INFO boot\nERROR-17 api\nWARN retry\nERROR-42 worker\nERROR-99 edge\n"
 (lambda ()
   (anzu-mode 1)
   (setq evil-ex-search-pattern
         (evil-ex-make-search-pattern "ERROR-[[:digit:]]+")
         evil-ex-search-direction 'forward)
   (goto-char (point-min))
   (evil-ex-search 1)
   (let ((first
          (list :match (match-string-no-properties 0)
                :state (neomacs-evil-anzu-test-state))))
     (evil-ex-search 1)
     (let ((second
            (list :match (match-string-no-properties 0)
                  :state (neomacs-evil-anzu-test-state))))
       (evil-ex-search 2)
       (list :first first
             :second second
             :after-wrap
             (list :match (match-string-no-properties 0)
                   :state (neomacs-evil-anzu-test-state)))))))
"####;
    let expected = expect![[
        r#"OK (:first (:match "ERROR-17" :state (:point 11 :line 2 :text nil :current 1 :total 3 :state search :indicator "(1/3)" :mode-line-entry t :forward-history nil :backward-history nil)) :second (:match "ERROR-42" :state (:point 35 :line 4 :text nil :current 2 :total 3 :state search :indicator "(2/3)" :mode-line-entry t :forward-history nil :backward-history nil)) :after-wrap (:match "ERROR-17" :state (:point 11 :line 2 :text nil :current 1 :total 3 :state search :indicator "(1/3)" :mode-line-entry t :forward-history nil :backward-history nil)))"#
    ]];
    ParityBatchCase::value(
        "ex_regexp_search_counts_matches_and_reports_wrapping_through_public_commands",
        elisp_form,
        expected,
    )
}

fn nohighlight_clears_the_anzu_indicator_after_a_real_evil_search() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-anzu-test-buffer
 "deploy api\ndeploy worker\ndeploy edge\n"
 (lambda ()
   (let ((initial-mode-line (copy-tree mode-line-format)))
     (anzu-mode 1)
     (cl-letf (((symbol-function 'evil-flash-search-pattern)
                (lambda (&rest _) nil)))
       (evil-search "deploy" t nil (point-min)))
     (let ((during (neomacs-evil-anzu-test-state 6)))
       (evil-ex-nohighlight)
       (list :during during
             :after (neomacs-evil-anzu-test-state 6)
             :mode-line-restored (equal mode-line-format initial-mode-line))))))
"####;
    let expected = expect![[
        r#"OK (:during (:point 1 :line 1 :text "deploy" :current 1 :total 3 :state search :indicator "(1/3)" :mode-line-entry t :forward-history nil :backward-history nil) :after (:point 1 :line 1 :text "deploy" :current 0 :total 0 :state nil :indicator "" :mode-line-entry nil :forward-history nil :backward-history nil) :mode-line-restored t)"#
    ]];
    ParityBatchCase::value(
        "nohighlight_clears_the_anzu_indicator_after_a_real_evil_search",
        elisp_form,
        expected,
    )
}

fn failed_and_disabled_searches_preserve_the_expected_user_state() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :failed
 (neomacs-evil-anzu-test-buffer
  "release alpha\nrelease beta\n"
  (lambda ()
    (anzu-mode 1)
    (cl-letf (((symbol-function 'evil-flash-search-pattern)
               (lambda (&rest _) nil)))
      (evil-search "release" t nil (point-min))
      (let ((before (neomacs-evil-anzu-test-state 7))
            (point-before (point)))
        (list
         :before before
         :outcome
         (neomacs-evil-anzu-test-capture
          (lambda () (evil-search "missing" t nil)))
         :point-preserved (= point-before (point))
         :after (neomacs-evil-anzu-test-state 7))))))
 :disabled
 (neomacs-evil-anzu-test-buffer
  "release alpha\nrelease beta\n"
  (lambda ()
    (anzu-mode -1)
    (let ((initial-mode-line (copy-tree mode-line-format)))
      (cl-letf (((symbol-function 'evil-flash-search-pattern)
                 (lambda (&rest _) nil)))
        (evil-search "release" t nil (point-min)))
      (list :anzu anzu-mode
            :point (point)
            :text (buffer-substring-no-properties (point) (+ (point) 7))
            :state anzu--state
            :current anzu--current-position
            :total anzu--total-matched
            :mode-line-unchanged (equal mode-line-format initial-mode-line))))))
"####;
    let expected = expect![[
        r#"OK (:failed (:before (:point 1 :line 1 :text "release" :current 1 :total 2 :state search :indicator "(1/2)" :mode-line-entry t :forward-history nil :backward-history nil) :outcome (:error user-error :data ("\"missing\": string not found") :message "\"missing\": string not found") :point-preserved t :after (:point 1 :line 1 :text "release" :current 1 :total 2 :state search :indicator "(1/2)" :mode-line-entry t :forward-history nil :backward-history nil)) :disabled (:anzu nil :point 1 :text "release" :state nil :current 0 :total 0 :mode-line-unchanged t))"#
    ]];
    ParityBatchCase::value(
        "failed_and_disabled_searches_preserve_the_expected_user_state",
        elisp_form,
        expected,
    )
}

fn package_load_unload_and_reload_owns_exactly_the_four_evil_advices() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((descriptor (cadr (assq 'evil-anzu package-alist)))
       (targets
        '((evil-search . evil-anzu-start-search)
          (evil-ex-find-next . evil-anzu-search-next)
          (evil-flash-hook . evil-anzu-prevent-flicker)
          (evil-ex-delete-hl . evil-anzu-reset)))
       (advice-state
        (lambda ()
          (mapcar
           (lambda (pair)
             (list (car pair) (cdr pair)
                   (and (advice-member-p (cdr pair) (car pair)) t)))
           targets))))
  (let ((loaded (funcall advice-state)))
    (unload-feature 'evil-anzu t)
    (let ((unloaded (funcall advice-state))
          (feature-after-unload (featurep 'evil-anzu)))
      (require 'evil-anzu)
      (list
       :package
       (list :name (package-desc-name descriptor)
             :version (package-version-join (package-desc-version descriptor))
             :requirements (package-desc-reqs descriptor))
       :loaded loaded
       :unloaded unloaded
       :feature-after-unload feature-after-unload
       :reloaded (funcall advice-state)
       :feature-after-reload (featurep 'evil-anzu)
       :unload-function (functionp 'evil-anzu-unload-function)))))
"####;
    let expected = expect![[
        r#"OK (:package (:name evil-anzu :version "20250316.1617" :requirements ((evil (1 0 0)) (anzu (0 46)))) :loaded ((evil-search evil-anzu-start-search t) (evil-ex-find-next evil-anzu-search-next t) (evil-flash-hook evil-anzu-prevent-flicker t) (evil-ex-delete-hl evil-anzu-reset t)) :unloaded ((evil-search evil-anzu-start-search nil) (evil-ex-find-next evil-anzu-search-next nil) (evil-flash-hook evil-anzu-prevent-flicker nil) (evil-ex-delete-hl evil-anzu-reset nil)) :feature-after-unload nil :reloaded ((evil-search evil-anzu-start-search t) (evil-ex-find-next evil-anzu-search-next t) (evil-flash-hook evil-anzu-prevent-flicker t) (evil-ex-delete-hl evil-anzu-reset t)) :feature-after-reload t :unload-function t)"#
    ]];
    ParityBatchCase::value(
        "package_load_unload_and_reload_owns_exactly_the_four_evil_advices",
        elisp_form,
        expected,
    )
}

#[test]
fn evil_anzu_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(EVIL_ANZU_MELPA_PIN, "evil-anzu.el")
            .expect("prepare revision-pinned Evil Anzu source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "evil-anzu-package-batch",
        "Evil Anzu",
        &[
            repeated_forward_and_backward_evil_searches_track_the_visible_match(),
            ex_regexp_search_counts_matches_and_reports_wrapping_through_public_commands(),
            nohighlight_clears_the_anzu_indicator_after_a_real_evil_search(),
            failed_and_disabled_searches_preserve_the_expected_user_state(),
            package_load_unload_and_reload_owns_exactly_the_four_evil_advices(),
        ],
    );
}
