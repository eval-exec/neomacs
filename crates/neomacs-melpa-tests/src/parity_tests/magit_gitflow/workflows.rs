use expect_test::expect;

use super::ParityBatchCase;

fn status_hook_and_keyboard_popup_dispatch_a_feature_start() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-magit-gitflow-test-with-repository "status-popup"
  (neomacs-magit-gitflow-test-configure-repository)
  (let* ((magit-mode-hook
          (cons #'turn-on-magit-gitflow magit-mode-hook))
         (status-buffer (magit-status-setup-buffer repo)))
    (with-current-buffer status-buffer
      (let ((mode-state
             (list :major-mode major-mode
                   :enabled magit-gitflow-mode
                   :lighter (assq 'magit-gitflow-mode minor-mode-alist)
                   :binding (key-binding (kbd magit-gitflow-popup-key)))))
        (execute-kbd-macro (kbd "C-f"))
        (let ((popup-state
               (list :buffer (buffer-name)
                     :major-mode major-mode
                     :read-only buffer-read-only
                     :source (buffer-name magit-pre-popup-buffer)
                     :text (buffer-substring-no-properties
                            (point-min) (point-max))
                     :buttons
                     (mapcar
                      (lambda (button)
                        (list :label
                              (buffer-substring-no-properties
                               (button-start button) (button-end button))
                              :event (button-get button 'event)
                              :function (button-get button 'function)))
                      (sort
                       (cl-remove-if-not
                        (lambda (overlay) (overlay-get overlay 'button))
                        (overlays-in (point-min) (point-max)))
                       (lambda (left right)
                         (< (overlay-start left) (overlay-start right)))))
                     :init-binding
                     (key-binding (kbd "i"))
                     :feature-binding
                     (key-binding (kbd "f")))))
          (execute-kbd-macro
           (kbd "f s - F s billing/quote-λ RET"))
          (list :mode mode-state
                :popup popup-state
                :returned-buffer (buffer-name)
                :mode-still-enabled magit-gitflow-mode
                :popup-live (and (get-buffer "*magit-gitflow-popup*") t)
                :git-flow-calls
                (neomacs-magit-gitflow-test-trace trace)))))))
"####;
    let expected = expect![[
        r#"OK (:mode (:major-mode magit-status-mode :enabled t :lighter (magit-gitflow-mode magit-gitflow-mode-lighter) :binding magit-gitflow-popup) :popup (:buffer "*magit-gitflow-popup*" :major-mode magit-popup-mode :read-only t :source "magit: repo" :text "Actions\n i Init       f Feature    b Bugfix     r Release    h Hotfix     s Support\n\n" :buttons ((:label " i Init" :event 105 :function magit-invoke-popup-action) (:label " f Feature" :event 102 :function magit-invoke-popup-action) (:label " b Bugfix" :event 98 :function magit-invoke-popup-action) (:label " r Release" :event 114 :function magit-invoke-popup-action) (:label " h Hotfix" :event 104 :function magit-invoke-popup-action) (:label " s Support" :event 115 :function magit-invoke-popup-action)) :init-binding magit-invoke-popup-action :feature-binding magit-invoke-popup-action) :returned-buffer "magit: repo" :mode-still-enabled t :popup-live nil :git-flow-calls (("feature" "start" "--fetch" "billing/quote-λ")))"#
    ]];
    ParityBatchCase::value(
        "status_hook_and_keyboard_popup_dispatch_a_feature_start",
        elisp_form,
        expected,
    )
}

fn initialization_dispatches_defaults_without_fabricated_external_effects() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-magit-gitflow-test-with-repository "init-defaults"
  (let* ((magit-current-popup-args nil)
         (init-result (call-interactively #'magit-gitflow-init)))
    (list
     :return init-result
     :current-branch (magit-get-current-branch)
     :branches
     (neomacs-magit-gitflow-test-git-lines
      "for-each-ref" "--sort=refname" "--format=%(refname:short)"
      "refs/heads")
     :head-file
     (with-temp-buffer
       (insert-file-contents (expand-file-name "service.txt" repo))
       (buffer-string))
     :gitflow-config
     (neomacs-magit-gitflow-test-git-lines
      "config" "--get-regexp" "^gitflow\\.")
     :status (neomacs-magit-gitflow-test-git-lines "status" "--short")
     :git-flow-calls (neomacs-magit-gitflow-test-trace trace))))
"####;
    let expected = expect![[
        r#"OK (:return 0 :current-branch "master" :branches ("master") :head-file "version=1\nchannel=stable\nowner=Zoë\n" :gitflow-config nil :status nil :git-flow-calls (("init" "-d")))"#
    ]];
    ParityBatchCase::value(
        "initialization_dispatches_defaults_without_fabricated_external_effects",
        elisp_form,
        expected,
    )
}

fn interactive_prefix_customization_persists_before_feature_dispatch() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-magit-gitflow-test-with-repository "custom-feature-prefix"
  (let ((magit-current-popup-args nil))
    (neomacs-magit-gitflow-test-configure-repository)
    (let ((executing-kbd-macro t)
          (unread-command-events
           (listify-key-sequence (kbd "C-a C-k topic/λ- RET"))))
      (call-interactively #'magit-gitflow-init-feature))
    (magit-gitflow-feature-start "checkout-v2")
    (list
     :configured-prefix (magit-get "gitflow.prefix.feature")
     :current-branch (magit-get-current-branch)
     :branch-heads
     (neomacs-magit-gitflow-test-git-lines
      "for-each-ref" "--sort=refname" "--format=%(refname:short)"
      "refs/heads")
     :status (neomacs-magit-gitflow-test-git-lines "status" "--short")
     :git-flow-calls (neomacs-magit-gitflow-test-trace trace))))
"####;
    let expected = expect![[
        r#"OK (:configured-prefix "topic/λ-" :current-branch "master" :branch-heads ("develop" "master") :status nil :git-flow-calls (("feature" "start" "checkout-v2")))"#
    ]];
    ParityBatchCase::value(
        "interactive_prefix_customization_persists_before_feature_dispatch",
        elisp_form,
        expected,
    )
}

fn finishing_a_feature_selects_the_current_topic_and_dispatches_it() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-magit-gitflow-test-with-repository "finish-feature"
  (let ((magit-current-popup-args nil))
    (neomacs-magit-gitflow-test-configure-repository)
    (neomacs-magit-gitflow-test-checkout-topic
     "feature" "invoice/retry" "develop")
    (neomacs-magit-gitflow-test-write
     (expand-file-name "src/retry policy λ.txt" repo)
     "attempts=3\nbackoff=exponential\n")
    (neomacs-magit-gitflow-test-run-git "add" "src/retry policy λ.txt")
    (neomacs-magit-gitflow-test-run-git
     "commit" "--quiet" "-m" "add invoice retry policy")
    (let* ((executing-kbd-macro t)
           (unread-command-events (listify-key-sequence (kbd "RET")))
           (finish-result
            (call-interactively #'magit-gitflow-feature-finish)))
      (list
       :return finish-result
       :current-branch (magit-get-current-branch)
       :branches
       (neomacs-magit-gitflow-test-git-lines
        "for-each-ref" "--sort=refname" "--format=%(refname:short)"
        "refs/heads")
       :topic-history
       (neomacs-magit-gitflow-test-git-lines
        "log" "--format=%s" "feature/invoice/retry")
       :status (neomacs-magit-gitflow-test-git-lines "status" "--short")
       :git-flow-calls (neomacs-magit-gitflow-test-trace trace)))))
"####;
    let expected = expect![[
        r#"OK (:return 0 :current-branch "feature/invoice/retry" :branches ("develop" "feature/invoice/retry" "master") :topic-history ("add invoice retry policy" "baseline") :status nil :git-flow-calls (("feature" "finish" "invoice/retry")))"#
    ]];
    ParityBatchCase::value(
        "finishing_a_feature_selects_the_current_topic_and_dispatches_it",
        elisp_form,
        expected,
    )
}

fn rebasing_a_feature_forwards_popup_options_and_topic_asynchronously() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-magit-gitflow-test-with-repository "rebase-feature"
  (let ((magit-current-popup-args nil))
    (neomacs-magit-gitflow-test-configure-repository)
    (neomacs-magit-gitflow-test-checkout-topic
     "feature" "shipping-label" "develop")
    (neomacs-magit-gitflow-test-write
     (expand-file-name "feature.txt" repo)
     "carrier=acme\nformat=unicode-λ\n")
    (neomacs-magit-gitflow-test-run-git "add" "feature.txt")
    (neomacs-magit-gitflow-test-run-git
     "commit" "--quiet" "-m" "add shipping label feature")
    (neomacs-magit-gitflow-test-run-git "switch" "--quiet" "develop")
    (neomacs-magit-gitflow-test-write
     (expand-file-name "deploy.txt" repo)
     "region=us-east\nwindow=09:30\n")
    (neomacs-magit-gitflow-test-run-git "add" "deploy.txt")
    (neomacs-magit-gitflow-test-run-git
     "commit" "--quiet" "-m" "advance deployment baseline")
    (neomacs-magit-gitflow-test-run-git
     "switch" "--quiet" "feature/shipping-label")
    (let* ((magit-current-popup-args '("--preserve-merges"))
           (process (call-interactively #'magit-gitflow-feature-rebase))
           (exit-status
            (neomacs-magit-gitflow-test-await-process process)))
      (list
       :exit-status exit-status
       :current-branch (magit-get-current-branch)
       :history
       (neomacs-magit-gitflow-test-git-lines
        "log" "--first-parent" "--format=%s"
        "feature/shipping-label")
       :develop-history
       (neomacs-magit-gitflow-test-git-lines
        "log" "--first-parent" "--format=%s" "develop")
       :feature-file
       (with-temp-buffer
         (insert-file-contents (expand-file-name "feature.txt" repo))
         (buffer-string))
       :develop-file (magit-git-output "show" "develop:deploy.txt")
       :status (neomacs-magit-gitflow-test-git-lines "status" "--short")
       :git-flow-calls (neomacs-magit-gitflow-test-trace trace)))))
"####;
    let expected = expect![[
        r#"OK (:exit-status 0 :current-branch "feature/shipping-label" :history ("add shipping label feature" "baseline") :develop-history ("advance deployment baseline" "baseline") :feature-file "carrier=acme\nformat=unicode-λ\n" :develop-file "region=us-east\nwindow=09:30\n" :status nil :git-flow-calls (("feature" "rebase" "--preserve-merges" "shipping-label")))"#
    ]];
    ParityBatchCase::value(
        "rebasing_a_feature_forwards_popup_options_and_topic_asynchronously",
        elisp_form,
        expected,
    )
}

fn finishing_a_release_forwards_tag_and_branch_retention_options() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-magit-gitflow-test-with-repository "finish-release"
  (neomacs-magit-gitflow-test-configure-repository)
  (neomacs-magit-gitflow-test-checkout-topic
   "release" "2.4.0-rc1" "develop")
  (neomacs-magit-gitflow-test-write
   (expand-file-name "release.json" repo)
   "{\"version\":\"2.4.0-rc1\",\"channel\":\"canary\"}\n")
  (neomacs-magit-gitflow-test-run-git "add" "release.json")
  (neomacs-magit-gitflow-test-run-git
   "commit" "--quiet" "-m" "prepare release candidate")
  (let* ((magit-current-popup-args '("--notag" "--keep"))
         (executing-kbd-macro t)
         (unread-command-events (listify-key-sequence (kbd "RET")))
         (process (call-interactively #'magit-gitflow-release-finish))
         (exit-status
          (neomacs-magit-gitflow-test-await-process process)))
    (list
     :exit-status exit-status
     :current-branch (magit-get-current-branch)
     :release-history
     (neomacs-magit-gitflow-test-git-lines
      "log" "--format=%s" "release/2.4.0-rc1")
     :release-file
     (with-temp-buffer
       (insert-file-contents (expand-file-name "release.json" repo))
       (buffer-string))
     :status (neomacs-magit-gitflow-test-git-lines "status" "--short")
     :git-flow-calls (neomacs-magit-gitflow-test-trace trace))))
"####;
    let expected = expect![[
        r#"OK (:exit-status 0 :current-branch "release/2.4.0-rc1" :release-history ("prepare release candidate" "baseline") :release-file "{\"version\":\"2.4.0-rc1\",\"channel\":\"canary\"}\n" :status nil :git-flow-calls (("release" "finish" "--notag" "--keep" "2.4.0-rc1")))"#
    ]];
    ParityBatchCase::value(
        "finishing_a_release_forwards_tag_and_branch_retention_options",
        elisp_form,
        expected,
    )
}

fn starting_support_reads_a_name_and_real_local_base_branch() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-magit-gitflow-test-with-repository "start-support"
  (neomacs-magit-gitflow-test-configure-repository)
  (let* ((executing-kbd-macro t)
         (unread-command-events
          (listify-key-sequence (kbd "1.x RET master RET")))
         (start-result
          (call-interactively #'magit-gitflow-support-start)))
    (list
     :return start-result
     :current-branch (magit-get-current-branch)
     :available-bases
     (neomacs-magit-gitflow-test-git-lines
      "for-each-ref" "--sort=refname" "--format=%(refname:short)"
      "refs/heads")
     :status (neomacs-magit-gitflow-test-git-lines "status" "--short")
     :git-flow-calls (neomacs-magit-gitflow-test-trace trace))))
"####;
    let expected = expect![[
        r#"OK (:return 0 :current-branch "master" :available-bases ("develop" "master") :status nil :git-flow-calls (("support" "start" "1.x" "master")))"#
    ]];
    ParityBatchCase::value(
        "starting_support_reads_a_name_and_real_local_base_branch",
        elisp_form,
        expected,
    )
}

fn feature_diff_surfaces_the_modern_magit_signature_incompatibility() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-magit-gitflow-test-with-repository "feature-diff"
  (let ((magit-current-popup-args nil))
    (neomacs-magit-gitflow-test-configure-repository)
    (neomacs-magit-gitflow-test-checkout-topic
     "feature" "delivery-window" "develop")
    (neomacs-magit-gitflow-test-write
     (expand-file-name "service.txt" repo)
     "version=2\nchannel=canary\nowner=Zoë\n")
    (neomacs-magit-gitflow-test-write
     (expand-file-name "docs/release notes λ.md" repo)
     "# Delivery window\n\nShip Tuesday at 09:30 UTC.\n")
    (neomacs-magit-gitflow-test-run-git
     "add" "service.txt" "docs/release notes λ.md")
    (neomacs-magit-gitflow-test-run-git
     "commit" "--quiet" "-m" "prepare delivery window")
    (let ((branch (magit-get-current-branch)))
      (neomacs-magit-gitflow-test-run-git
       "config" (format "gitflow.branch.%s.base" branch) "develop")
      (call-interactively #'magit-gitflow-feature-diff))))
"####;
    let expected = expect![[
        r#"ERR (wrong-number-of-arguments #[nil ((transient-setup 'magit-diff)) (magit-buffer-log-files magit-buffer-log-args magit-blame-mode gravatar-size magit-log-heading-re magit-status-use-buffer-arguments t) nil nil nil] 2)"#
    ]];
    ParityBatchCase::signal(
        "feature_diff_surfaces_the_modern_magit_signature_incompatibility",
        elisp_form,
        expected,
    )
}

fn feature_command_in_an_uninitialized_repository_explains_the_remedy() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-magit-gitflow-test-with-repository "not-initialized"
  (call-interactively #'magit-gitflow-feature-diff))
"####;
    let expected = expect![[
        r#"ERR (user-error "Not a gitflow-enabled repo, please run ’git flow init’ first")"#
    ]];
    ParityBatchCase::signal(
        "feature_command_in_an_uninitialized_repository_explains_the_remedy",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        status_hook_and_keyboard_popup_dispatch_a_feature_start(),
        initialization_dispatches_defaults_without_fabricated_external_effects(),
        interactive_prefix_customization_persists_before_feature_dispatch(),
        finishing_a_feature_selects_the_current_topic_and_dispatches_it(),
        rebasing_a_feature_forwards_popup_options_and_topic_asynchronously(),
        finishing_a_release_forwards_tag_and_branch_retention_options(),
        starting_support_reads_a_name_and_real_local_base_branch(),
        feature_diff_surfaces_the_modern_magit_signature_incompatibility(),
        feature_command_in_an_uninitialized_repository_explains_the_remedy(),
    ]
}
