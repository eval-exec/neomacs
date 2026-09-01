use expect_test::expect;

use super::ParityBatchCase;

fn filtering_navigation_and_return_select_a_visible_candidate() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-vertico-test-with-mode
  (let ((candidates '("api-gateway" "api-worker" "app-console"
                      "billing-console" "cache-worker")))
    (minibuffer-with-setup-hook
        (lambda ()
          (neomacs-vertico-test-install-observer)
          (setq unread-command-events
                (append (string-to-list "ap")
                        (listify-key-sequence (kbd "C-n <f8> RET"))
                        unread-command-events)))
      (let ((result (completing-read "Deploy service: " candidates nil t)))
        (list :result result
              :observations (nreverse neomacs-vertico-test-observations)
              :minibuffer-active (and (active-minibuffer-window) t))))))
"####;
    let expected = expect![[
        r#"OK (:result "api-worker" :observations ((:prompt "Deploy service: " :input "ap" :point 2 :index 1 :total 3 :count "2/3    " :display " \napi-gateway\napi-worker\napp-console\n" :current ((14 25 "api-worker\n")) :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil)) :minibuffer-active nil)"#
    ]];
    ParityBatchCase::value(
        "filtering_navigation_and_return_select_a_visible_candidate",
        elisp_form,
        expected,
    )
}

fn tab_inserts_the_selected_candidate_before_exit() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-vertico-test-with-mode
  (let ((candidates '("api-gateway" "api-worker" "app-console"
                      "billing-console")))
    (minibuffer-with-setup-hook
        (lambda ()
          (neomacs-vertico-test-install-observer)
          (setq unread-command-events
                (append (string-to-list "ap")
                        (listify-key-sequence
                         (kbd "C-n C-n <f8> TAB <f8> RET"))
                        unread-command-events)))
      (let ((result (completing-read "Open service: " candidates nil t)))
        (list :result result
              :observations (nreverse neomacs-vertico-test-observations))))))
"####;
    let expected = expect![[
        r#"OK (:result "app-console" :observations ((:prompt "Open service: " :input "ap" :point 2 :index 2 :total 3 :count "3/3    " :display " \napi-gateway\napi-worker\napp-console\n" :current ((25 37 "app-console\n")) :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil) (:prompt "Open service: " :input "app-console" :point 11 :index 0 :total 1 :count "1/1    " :display " \napp-console\n" :current ((2 14 "app-console\n")) :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil)))"#
    ]];
    ParityBatchCase::value(
        "tab_inserts_the_selected_candidate_before_exit",
        elisp_form,
        expected,
    )
}

fn meta_return_submits_new_input_with_no_candidates() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-vertico-test-with-mode
  (let ((candidates '("production" "staging" "development")))
    (minibuffer-with-setup-hook
        (lambda ()
          (neomacs-vertico-test-install-observer)
          (setq unread-command-events
                (append (string-to-list "preview-42")
                        (listify-key-sequence (kbd "<f8> M-RET"))
                        unread-command-events)))
      (let ((result (completing-read "Create environment: " candidates nil nil)))
        (list :result result
              :observations (nreverse neomacs-vertico-test-observations))))))
"####;
    let expected = expect![[
        r#"OK (:result "preview-42" :observations ((:prompt "Create environment: " :input "preview-42" :point 10 :index -1 :total 0 :count "*/0    " :display " " :current nil :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil)))"#
    ]];
    ParityBatchCase::value(
        "meta_return_submits_new_input_with_no_candidates",
        elisp_form,
        expected,
    )
}

fn require_match_rejects_unknown_input_then_accepts_a_match() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-vertico-test-with-mode
  (let ((candidates '("alpha" "beta" "gamma")))
    (minibuffer-with-setup-hook
        (lambda ()
          (neomacs-vertico-test-install-observer)
          (setq unread-command-events
                (append (string-to-list "missing")
                        (listify-key-sequence (kbd "RET <f8> M-DEL"))
                        (string-to-list "beta")
                        (listify-key-sequence (kbd "<f8> RET"))
                        unread-command-events)))
      (let ((result (completing-read "Promote channel: " candidates nil t)))
        (list :result result
              :messages (nreverse neomacs-vertico-test-minibuffer-messages)
              :observations (nreverse neomacs-vertico-test-observations))))))
"####;
    let expected = expect![[
        r#"OK (:result "beta" :messages (" [Match required]") :observations ((:prompt "Promote channel: " :input "missing" :point 7 :index -1 :total 0 :count "!/0    " :display " " :current nil :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil) (:prompt "Promote channel: " :input "beta" :point 4 :index 0 :total 1 :count "1/1    " :display " \nbeta\n" :current ((2 7 "beta\n")) :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil)))"#
    ]];
    ParityBatchCase::value(
        "require_match_rejects_unknown_input_then_accepts_a_match",
        elisp_form,
        expected,
    )
}

fn annotations_and_group_navigation_change_the_visible_selection() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-vertico-test-with-mode
  (minibuffer-with-setup-hook
      (lambda ()
        (neomacs-vertico-test-install-observer)
        (setq unread-command-events
              (append (string-to-list "a")
                      (listify-key-sequence (kbd "<f8> M-} <f8> RET"))
                      unread-command-events)))
    (let ((result
           (completing-read
            "Inspect service: " #'neomacs-vertico-test-service-table nil t)))
      (list :result result
            :observations (nreverse neomacs-vertico-test-observations)))))
"####;
    let expected = expect![[
        r#"OK (:result "app-console" :observations ((:prompt "Inspect service: " :input "a" :point 1 :index 0 :total 3 :count "1/3    " :display " \n     API services  \napi-gateway  [production API]\napi-worker  [production API]\n     Applications  \n" :current ((22 52 "api-gateway  [production API]\n")) :semantic-faces ((completions-annotations (33 51 "  [production API]") (62 80 "  [production API]")) (vertico-group-title (6 20 " API services ") (85 99 " Applications ")) (vertico-group-separator (2 6 "    ") (20 21 " ") (81 85 "    ") (99 100 " "))) :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil) (:prompt "Inspect service: " :input "a" :point 1 :index 0 :total 3 :count "1/3    " :display " \n     Applications  \napp-console  [operator UI]\n     API services  \napi-gateway  [production API]\n" :current ((22 49 "app-console  [operator UI]\n")) :semantic-faces ((completions-annotations (33 48 "  [operator UI]") (80 98 "  [production API]")) (vertico-group-title (6 20 " Applications ") (53 67 " API services ")) (vertico-group-separator (2 6 "    ") (20 21 " ") (49 53 "    ") (67 68 " "))) :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil)))"#
    ]];
    ParityBatchCase::value(
        "annotations_and_group_navigation_change_the_visible_selection",
        elisp_form,
        expected,
    )
}

fn cycling_previous_from_the_first_candidate_wraps_to_the_last() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-vertico-test-with-mode
  (let ((candidates '("apple" "banana" "cherry")))
    (minibuffer-with-setup-hook
        (lambda ()
          (neomacs-vertico-test-install-observer)
          (setq unread-command-events
                (append (listify-key-sequence (kbd "C-p <f8> RET"))
                        unread-command-events)))
      (let ((result (completing-read "Pick fruit: " candidates nil t)))
        (list :result result
              :observations (nreverse neomacs-vertico-test-observations))))))
"####;
    let expected = expect![[
        r#"OK (:result "cherry" :observations ((:prompt "Pick fruit: " :input "" :point 0 :index 2 :total 3 :count "3/3    " :display " \napple\nbanana\ncherry\n" :current ((15 22 "cherry\n")) :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil)))"#
    ]];
    ParityBatchCase::value(
        "cycling_previous_from_the_first_candidate_wraps_to_the_last",
        elisp_form,
        expected,
    )
}

fn completing_read_multiple_inserts_two_selected_candidates() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-vertico-test-with-mode
  (let ((candidates '("apple" "apricot" "banana" "blueberry")))
    (minibuffer-with-setup-hook
        (lambda ()
          (neomacs-vertico-test-install-observer)
          (setq unread-command-events
                (append (string-to-list "ap")
                        (listify-key-sequence (kbd "TAB"))
                        (string-to-list ", ba")
                        (listify-key-sequence (kbd "TAB <f8> M-RET"))
                        unread-command-events)))
      (let ((result
             (completing-read-multiple "Release fruits: " candidates nil t)))
        (list :result result
              :observations (nreverse neomacs-vertico-test-observations))))))
"####;
    let expected = expect![[
        r#"OK (:result ("apple" "banana") :observations ((:prompt "[comma-separated list] Release fruits: " :input "apple, banana" :point 13 :index 0 :total 1 :count "1/1    " :display " \nbanana\n" :current ((2 9 "banana\n")) :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil)))"#
    ]];
    ParityBatchCase::value(
        "completing_read_multiple_inserts_two_selected_candidates",
        elisp_form,
        expected,
    )
}

fn file_completion_enters_a_directory_then_selects_a_real_file() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-vertico-test-with-mode
  (let* ((root
          (file-name-as-directory
           (expand-file-name
            "vertico-file-workspace"
            (or (getenv "NEOMACS_TEST_SANDBOX_ROOT")
                (error "NEOMACS_TEST_SANDBOX_ROOT is required")))))
         (configs (expand-file-name "configs/" root))
         (vertico-preselect 'directory)
         (completion-ignored-extensions '(".bak"))
         result)
    (make-directory configs t)
    (dolist (file '("production.toml" "preview.toml" "production.bak"))
      (with-temp-file (expand-file-name file configs)
        (insert "fixture for " file "\n")))
    (minibuffer-with-setup-hook
        (lambda ()
          (neomacs-vertico-test-install-observer)
          (setq unread-command-events
                (append (string-to-list "conf")
                        (listify-key-sequence (kbd "TAB <f8>"))
                        (string-to-list "prod")
                        (listify-key-sequence (kbd "<f8> RET"))
                        unread-command-events)))
      (setq result (read-file-name "Open config: " root nil t)))
    (list
     :result (file-relative-name result root)
     :regular-file (and (file-regular-p result) t)
     :observations
     (mapcar
      (lambda (observation)
        (let ((normalized (copy-sequence observation)))
          (setq normalized
                (plist-put normalized :input
                           (file-relative-name
                            (plist-get observation :input) root)))
          (plist-put normalized :point
                     (- (plist-get observation :point) (length root)))))
      (nreverse neomacs-vertico-test-observations)))))
"####;
    let expected = expect![[
        r#"OK (:result "configs/production.toml" :regular-file t :observations ((:prompt "Open config: " :input "configs/" :point 8 :index -1 :total 2 :count "!/2    " :display " \npreview.toml\nproduction.toml\n" :current nil :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil) (:prompt "Open config: " :input "configs/prod" :point 12 :index 0 :total 1 :count "1/1    " :display " \nproduction.toml\n" :current ((2 18 "production.toml\n")) :semantic-faces nil :return-command vertico-exit :tab-command vertico-insert :next-command vertico-next :message nil)))"#
    ]];
    ParityBatchCase::value(
        "file_completion_enters_a_directory_then_selects_a_real_file",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        filtering_navigation_and_return_select_a_visible_candidate(),
        tab_inserts_the_selected_candidate_before_exit(),
        meta_return_submits_new_input_with_no_candidates(),
        require_match_rejects_unknown_input_then_accepts_a_match(),
        annotations_and_group_navigation_change_the_visible_selection(),
        cycling_previous_from_the_first_candidate_wraps_to_the_last(),
        completing_read_multiple_inserts_two_selected_candidates(),
        file_completion_enters_a_directory_then_selects_a_real_file(),
    ]
}
