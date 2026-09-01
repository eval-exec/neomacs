use expect_test::expect;

use super::ParityBatchCase;

fn first_require_delivers_the_complete_emergency_migration_warning() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-emacsql-sqlite-test-with-fresh-load
  (let* ((require-result (require 'emacsql-sqlite))
         (warning-state (neomacs-emacsql-sqlite-test-warning-state)))
    (list :require-result require-result
          :feature-present (featurep 'emacsql-sqlite)
          :legacy-connection-class
          (and (find-class 'emacsql-sqlite-connection nil) t)
          :warning warning-state
          :message-exactly-matches-warning
          (equal (neomacs-emacsql-sqlite-test-message-state)
                 (plist-get warning-state :text)))))
"####;
    let expected = expect![[
        r#"OK (:require-result emacsql-sqlite :feature-present t :legacy-connection-class nil :warning (:text "Emergency (emacsql): Uninstall all `emacsql-*' packages.\n\nAll EmacSQL back-ends are now distributed as part of the `emacsql'\npackage itself, and you must uninstall all `emacsql-*' packages.\nThese packages now do nothing but display this warning, but if they\nare located earlier on the `load-path' than `emacsql' is, then they\nprevent the respective libraries from `emacsql' from being loaded,\nrendering EmacSQL unusable.\n\n" :major-mode special-mode :read-only t :undo-disabled t) :message-exactly-matches-warning t)"#
    ]];
    ParityBatchCase::value(
        "first_require_delivers_the_complete_emergency_migration_warning",
        elisp_form,
        expected,
    )
}

fn repeated_require_does_not_spam_the_user_with_duplicate_warnings() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-emacsql-sqlite-test-with-fresh-load
  (let* ((first-result (require 'emacsql-sqlite))
         (warning-buffer (get-buffer "*Warnings*"))
         (warning-text (with-current-buffer warning-buffer (buffer-string)))
         (second-result (require 'emacsql-sqlite)))
    (list :returns (list first-result second-result)
          :same-warning-buffer (eq warning-buffer (get-buffer "*Warnings*"))
          :warning-count
          (with-current-buffer warning-buffer
            (save-excursion
              (goto-char (point-min))
              (how-many "^Emergency (emacsql):")))
          :warning-text-unchanged
          (equal warning-text
                 (with-current-buffer warning-buffer (buffer-string)))
          :message-exactly-matches-warning
          (equal (neomacs-emacsql-sqlite-test-message-state) warning-text)
          :feature-present (featurep 'emacsql-sqlite))))
"####;
    let expected = expect![
        "OK (:returns (emacsql-sqlite emacsql-sqlite) :same-warning-buffer t :warning-count 1 :warning-text-unchanged t :message-exactly-matches-warning t :feature-present t)"
    ];
    ParityBatchCase::value(
        "repeated_require_does_not_spam_the_user_with_duplicate_warnings",
        elisp_form,
        expected,
    )
}

fn user_warning_suppression_hides_the_notice_without_blocking_feature_loading() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-emacsql-sqlite-test-with-fresh-load
  (let ((warning-suppress-log-types
         (cons '(emacsql) warning-suppress-log-types)))
    (list :require-result (require 'emacsql-sqlite)
          :feature-present (featurep 'emacsql-sqlite)
          :legacy-connection-class
          (and (find-class 'emacsql-sqlite-connection nil) t)
          :warning (neomacs-emacsql-sqlite-test-warning-state)
          :messages (neomacs-emacsql-sqlite-test-message-state))))
"####;
    let expected = expect![
        "OK (:require-result emacsql-sqlite :feature-present t :legacy-connection-class nil :warning no-warning-buffer :messages no-message-buffer)"
    ];
    ParityBatchCase::value(
        "user_warning_suppression_hides_the_notice_without_blocking_feature_loading",
        elisp_form,
        expected,
    )
}

fn unload_then_require_again_replays_the_warning_and_restores_the_feature() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-emacsql-sqlite-test-with-fresh-load
  (require 'emacsql-sqlite)
  (let* ((warning-buffer (get-buffer "*Warnings*"))
         (first-warning (with-current-buffer warning-buffer (buffer-string)))
         (unload-result (unload-feature 'emacsql-sqlite t))
         (feature-after-unload (featurep 'emacsql-sqlite))
         (reload-result (require 'emacsql-sqlite))
         (combined-warning (with-current-buffer warning-buffer (buffer-string))))
    (list :unload-result unload-result
          :feature-after-unload feature-after-unload
          :reload-result reload-result
          :feature-after-reload (featurep 'emacsql-sqlite)
          :warning-count
          (with-current-buffer warning-buffer
            (save-excursion
              (goto-char (point-min))
              (how-many "^Emergency (emacsql):")))
          :second-warning-appended-exactly
          (equal combined-warning (concat first-warning first-warning))
          :messages-exactly-match-combined-warning
          (equal (neomacs-emacsql-sqlite-test-message-state)
                 combined-warning))))
"####;
    let expected = expect![
        "OK (:unload-result nil :feature-after-unload nil :reload-result emacsql-sqlite :feature-after-reload t :warning-count 2 :second-warning-appended-exactly t :messages-exactly-match-combined-warning t)"
    ];
    ParityBatchCase::value(
        "unload_then_require_again_replays_the_warning_and_restores_the_feature",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        first_require_delivers_the_complete_emergency_migration_warning(),
        repeated_require_does_not_spam_the_user_with_duplicate_warnings(),
        user_warning_suppression_hides_the_notice_without_blocking_feature_loading(),
        unload_then_require_again_replays_the_warning_and_restores_the_feature(),
    ]
}
