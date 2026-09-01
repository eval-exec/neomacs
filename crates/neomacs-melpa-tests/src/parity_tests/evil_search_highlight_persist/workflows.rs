use expect_test::expect;

use super::ParityBatchCase;

fn literal_search_persists_every_exact_match_until_public_clear_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "literal_search_persists_every_exact_match_until_public_clear_command",
        r####"
(with-temp-buffer
  (insert "alpha beta alpha alphabet alpha\n")
  (evil-search-highlight-persist 1)
  (neomacs-eshp-test-mark "alpha" nil)
  (let ((marked (neomacs-eshp-test-buffer-state)))
    (evil-search-highlight-persist-remove-all)
    (list :marked marked :cleared (neomacs-eshp-test-buffer-state))))
"####,
        expect![[
            r#"OK (:marked (:mode t :enabled t :overlays ((:range (1 6) :text "alpha" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (12 17) :text "alpha" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (18 23) :text "alpha" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (27 32) :text "alpha" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0)) :binding evil-search-highlight-persist-remove-all :text "alpha beta alpha alphabet alpha\n") :cleared (:mode t :enabled t :overlays nil :binding evil-search-highlight-persist-remove-all :text "alpha beta alpha alphabet alpha\n"))"#
        ]],
    )
}

fn regexp_search_honors_real_regexp_semantics_and_zero_width_progress() -> ParityBatchCase {
    ParityBatchCase::value(
        "regexp_search_honors_real_regexp_semantics_and_zero_width_progress",
        r####"
(with-temp-buffer
  (insert "cat cot cut c.t\nfoo\nbar\n")
  (evil-search-highlight-persist 1)
  (neomacs-eshp-test-mark "c.t" t)
  (let ((wildcard (neomacs-eshp-test-overlays)))
    (evil-search-highlight-persist-remove-all)
    (neomacs-eshp-test-mark "^" t)
    (list :wildcard wildcard
          :anchors (neomacs-eshp-test-overlays)
          :regex-flag evil-search-highlight-regex-flag)))
"####,
        expect![[
            r#"OK (:wildcard ((:range (1 4) :text "cat" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (5 8) :text "cot" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (9 12) :text "cut" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (13 16) :text "c.t" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0)) :anchors ((:range (1 1) :text "" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (17 17) :text "" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (21 21) :text "" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (25 25) :text "" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0)) :regex-flag t)"#
        ]],
    )
}

fn minimum_length_policy_ignores_short_searches_after_removing_old_highlights() -> ParityBatchCase {
    ParityBatchCase::value(
        "minimum_length_policy_ignores_short_searches_after_removing_old_highlights",
        r####"
(with-temp-buffer
  (insert "release re release ready re\n")
  (let ((evil-search-highlight-string-min-len 3))
    (evil-search-highlight-persist 1)
    (neomacs-eshp-test-mark "release" nil)
    (let ((long (neomacs-eshp-test-overlays)))
      (evil-search-highlight-persist-remove-all)
      (neomacs-eshp-test-mark "re" nil)
      (list :long long :short (neomacs-eshp-test-overlays)
            :last-regexp hlt-last-regexp))))
"####,
        expect![[
            r#"OK (:long ((:range (1 8) :text "release" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (12 19) :text "release" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0)) :short nil :last-regexp "release")"#
        ]],
    )
}

fn all_windows_policy_marks_unique_visible_buffers_and_clears_them_together() -> ParityBatchCase {
    ParityBatchCase::value(
        "all_windows_policy_marks_unique_visible_buffers_and_clears_them_together",
        r####"
(let ((left (generate-new-buffer " *eshp-left*"))
      (right (generate-new-buffer " *eshp-right*"))
      (evil-search-highlight-persist-all-windows t))
  (unwind-protect
      (save-window-excursion
        (delete-other-windows)
        (set-window-buffer (selected-window) left)
        (set-window-buffer (split-window-right) right)
        (with-current-buffer left
          (insert "deploy ready deploy\n")
          (text-mode)
          (evil-search-highlight-persist 1)
          (neomacs-eshp-test-mark "deploy" nil))
        (with-current-buffer right
          (insert "deploy blocked\n")
          (text-mode)
          (evil-search-highlight-persist 1))
        (let ((marked (list :left (with-current-buffer left (neomacs-eshp-test-overlays))
                            :right (with-current-buffer right (neomacs-eshp-test-overlays)))))
          (with-current-buffer left
            (evil-search-highlight-persist-remove-all))
          (list :marked marked
                :cleared (list :left (with-current-buffer left (neomacs-eshp-test-overlays))
                               :right (with-current-buffer right (neomacs-eshp-test-overlays))))))
    (kill-buffer left)
    (kill-buffer right)))
"####,
        expect![[
            r#"OK (:marked (:left ((:range (1 7) :text "deploy" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (14 20) :text "deploy" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0)) :right nil) :cleared (:left nil :right nil))"#
        ]],
    )
}

fn mode_disable_removes_highlights_and_global_mode_skips_fundamental_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_disable_removes_highlights_and_global_mode_skips_fundamental_buffers",
        r####"
(let ((fundamental (generate-new-buffer " *eshp-fundamental*"))
      (text (generate-new-buffer " *eshp-text*")))
  (unwind-protect
      (progn
        (with-current-buffer text (text-mode))
        (global-evil-search-highlight-persist 1)
        (with-current-buffer text
          (insert "token token\n")
          (neomacs-eshp-test-mark "token" nil))
        (let ((enabled
               (list :global global-evil-search-highlight-persist
                     :fundamental (with-current-buffer fundamental
                                    evil-search-highlight-persist)
                     :text (with-current-buffer text
                             (neomacs-eshp-test-buffer-state)))))
          (with-current-buffer text (evil-search-highlight-persist -1))
          (list :enabled enabled
                :disabled (with-current-buffer text
                            (neomacs-eshp-test-buffer-state)))))
    (global-evil-search-highlight-persist -1)
    (kill-buffer fundamental)
    (kill-buffer text)))
"####,
        expect![[
            r#"OK (:enabled (:global t :fundamental nil :text (:mode t :enabled t :overlays ((:range (1 6) :text "token" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (7 12) :text "token" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0)) :binding evil-search-highlight-persist-remove-all :text "token token\n")) :disabled (:mode nil :enabled nil :overlays nil :binding evil-search-highlight-persist-remove-all :text "token token\n"))"#
        ]],
    )
}

fn isearch_exit_advice_replaces_the_previous_persistent_search() -> ParityBatchCase {
    ParityBatchCase::value(
        "isearch_exit_advice_replaces_the_previous_persistent_search",
        r####"
(with-temp-buffer
  (insert "red blue red green blue\n")
  (evil-search-highlight-persist 1)
  (neomacs-eshp-test-mark "red" nil)
  (let ((red (neomacs-eshp-test-overlays)))
    ;; Invoke the advised public search-exit function with a minimal real
    ;; isearch state.  The package reads the completed search ring exactly as
    ;; it does after an interactive search.
    (let ((isearch-mode t)
          (isearch-string "blue")
          (isearch-message "blue")
          (isearch-success t)
          (isearch-regexp nil)
          (search-ring '("blue"))
          (regexp-search-ring nil))
      (cl-letf (((symbol-function 'isearch-done) (lambda (&rest _) nil))
                ((symbol-function 'isearch-clean-overlays) (lambda () nil)))
        (isearch-exit)))
    (list :before red :after (neomacs-eshp-test-overlays)
          :advice-active (ad-is-active 'isearch-exit))))
"####,
        expect![[
            r#"OK (:before ((:range (1 4) :text "red" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (10 13) :text "red" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0)) :after ((:range (5 9) :text "blue" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0) (:range (20 24) :text "blue" :face evil-search-highlight-persist-highlight-face :highlight evil-search-highlight-persist-highlight-face :priority 0)) :advice-active t)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        literal_search_persists_every_exact_match_until_public_clear_command(),
        regexp_search_honors_real_regexp_semantics_and_zero_width_progress(),
        minimum_length_policy_ignores_short_searches_after_removing_old_highlights(),
        all_windows_policy_marks_unique_visible_buffers_and_clears_them_together(),
        mode_disable_removes_highlights_and_global_mode_skips_fundamental_buffers(),
        isearch_exit_advice_replaces_the_previous_persistent_search(),
    ]
}
