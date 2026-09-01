use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HIGHLIGHT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'highlight)

(defun neomacs-highlight-test-overlays ()
  "Return stable records for overlays created by Highlight."
  (let (records)
    (dolist (overlay (overlays-in (point-min) (point-max)))
      (when (overlay-get overlay 'hlt-highlight)
        (push (list :range (list (overlay-start overlay) (overlay-end overlay))
                    :text (buffer-substring-no-properties
                           (overlay-start overlay) (overlay-end overlay))
                    :face (overlay-get overlay hlt-face-prop)
                    :mouse-face (overlay-get overlay 'mouse-face)
                    :highlight (overlay-get overlay 'hlt-highlight)
                    :priority (overlay-get overlay 'priority)
                    :invisible (overlay-get overlay 'invisible))
              records)))
    (sort records
          (lambda (left right)
            (string< (prin1-to-string left) (prin1-to-string right))))))

(defun neomacs-highlight-test-property-runs ()
  "Return stable Highlight-related text-property runs."
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let ((end (next-property-change position nil (point-max))))
        (when (or (get-text-property position 'hlt-highlight)
                  (get-text-property position 'face)
                  (get-text-property position 'font-lock-face)
                  (get-text-property position 'font-lock-ignore)
                  (get-text-property position 'mouse-face))
          (push (list :range (list position end)
                      :text (buffer-substring-no-properties position end)
                      :face (get-text-property position 'face)
                      :font-lock-face (get-text-property position 'font-lock-face)
                      :mouse-face (get-text-property position 'mouse-face)
                      :highlight (get-text-property position 'hlt-highlight)
                      :font-lock-ignore
                      (get-text-property position 'font-lock-ignore))
                runs))
        (setq position end)))
    (nreverse runs)))

"###;

fn package_contract_exposes_commands_prefix_keys_and_policy_defaults() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'highlight package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'highlight) t))
   :commands
   (mapcar #'commandp
           '(hlt-highlight-region hlt-unhighlight-region
             hlt-highlight-regexp-region hlt-unhighlight-regexp-region
             hlt-highlight-regexp-groups-region hlt-highlight-symbol
             hlt-highlight-line-dups-region hlt-copy-props hlt-yank-props
             hlt-hide-default-face hlt-show-default-face
             hlt-highlight-property-with-value hlt-next-highlight
             hlt-previous-highlight))
   :prefix (and (keymapp (lookup-key ctl-x-map "X")) t)
   :keys
   (mapcar (lambda (key) (lookup-key hlt-map (kbd key)))
           '("h r" "h s" "h x" "u r" "u s" "u x" "u f"
             "M-w" "C-y" "r" "-" "+" "t o" "h v" "h p" "u p"))
   :defaults
   (list hlt-use-overlays-flag hlt-face-prop hlt-overlays-priority
         hlt-auto-faces-flag hlt-default-copy/yank-props
         hlt-line-dups-ignore-regexp hlt-last-face hlt-last-regexp)))
"###;
    let expected = expect![[
        r#"OK (:package (:name highlight :version "20210318.2248" :requirements nil :feature t) :commands (t t t t t t t t t t t t t t) :prefix t :keys (hlt-highlight-region hlt-highlight-symbol hlt-highlight-regexp-region hlt-unhighlight-region hlt-unhighlight-symbol hlt-unhighlight-regexp-region hlt-unhighlight-region-for-face hlt-copy-props hlt-yank-props hlt-replace-highlight-face hlt-hide-default-face hlt-show-default-face hlt-toggle-use-overlays-flag hlt-highlight-property-with-value hlt-highlight-all-prop hlt-unhighlight-all-prop) :defaults (only font-lock-face 0 nil (face) "[ \11]*" highlight nil))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_commands_prefix_keys_and_policy_defaults",
        elisp_form,
        expected,
    )
}

fn overlay_incident_highlight_preserves_buffer_state_and_splits_on_partial_removal()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "INFO checkout started\n"
          "ERROR timeout contacting payments\n"
          "INFO checkout rolled back\n")
  (goto-char (point-min))
  (search-forward "timeout contacting payments")
  (let* ((incident-start (match-beginning 0))
         (incident-end (match-end 0))
         (contact-start (+ incident-start (length "timeout ")))
         (contact-end (+ contact-start (length "contacting")))
         (hlt-use-overlays-flag 'only)
         (hlt-overlays-priority 17))
    (set-buffer-modified-p nil)
    (setq buffer-read-only t)
    (hlt-highlight-region incident-start incident-end 'error)
    (let ((highlighted
           (list :overlays (neomacs-highlight-test-overlays)
                 :read-only buffer-read-only
                 :modified (buffer-modified-p)
                 :last-face hlt-last-face)))
      (hlt-unhighlight-region contact-start contact-end 'error)
      (list :highlighted highlighted
            :partially-removed (neomacs-highlight-test-overlays)
            :read-only buffer-read-only
            :modified (buffer-modified-p)
            :text (buffer-string)))))
"###;
    let expected = expect![[
        r#"OK (:highlighted (:overlays ((:range (29 56) :text "timeout contacting payments" :face error :mouse-face nil :highlight error :priority 17 :invisible nil)) :read-only t :modified nil :last-face error) :partially-removed ((:range (29 37) :text "timeout " :face error :mouse-face nil :highlight error :priority 17 :invisible nil) (:range (47 56) :text " payments" :face error :mouse-face nil :highlight error :priority 17 :invisible nil)) :read-only t :modified nil :text "INFO checkout started\nERROR timeout contacting payments\nINFO checkout rolled back\n")"#
    ]];
    ParityBatchCase::value(
        "overlay_incident_highlight_preserves_buffer_state_and_splits_on_partial_removal",
        elisp_form,
        expected,
    )
}

fn text_property_review_highlight_round_trips_without_overlays_or_dirtying_the_buffer()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "invoice 481 requires manual review before capture")
  (goto-char (point-min))
  (search-forward "manual review")
  (let ((start (match-beginning 0))
        (end (match-end 0))
        (hlt-use-overlays-flag nil)
        (hlt-face-prop 'face))
    (put-text-property start end 'audit-id 481)
    (set-buffer-modified-p nil)
    (hlt-highlight-region start end 'warning)
    (let ((highlighted
           (list :properties (neomacs-highlight-test-property-runs)
                 :overlays (neomacs-highlight-test-overlays)
                 :audit-id (get-text-property start 'audit-id)
                 :modified (buffer-modified-p))))
      (hlt-unhighlight-region start end 'warning)
      (list :highlighted highlighted
            :after (neomacs-highlight-test-property-runs)
            :audit-id (get-text-property start 'audit-id)
            :modified (buffer-modified-p)
            :text (buffer-string)))))
"###;
    let expected = expect![[
        r#"OK (:highlighted (:properties ((:range (22 35) :text "manual review" :face warning :font-lock-face nil :mouse-face nil :highlight warning :font-lock-ignore t)) :overlays nil :audit-id 481 :modified nil) :after nil :audit-id 481 :modified nil :text #("invoice 481 requires manual review before capture" 21 22 (audit-id 481) 22 23 (audit-id 481) 23 24 (audit-id 481) 24 25 (audit-id 481) 25 26 (audit-id 481) 26 27 (audit-id 481) 27 28 (audit-id 481) 28 29 (audit-id 481) 29 30 (audit-id 481) 30 31 (audit-id 481) 31 32 (audit-id 481) 32 33 (audit-id 481) 33 34 (audit-id 481)))"#
    ]];
    ParityBatchCase::value(
        "text_property_review_highlight_round_trips_without_overlays_or_dirtying_the_buffer",
        elisp_form,
        expected,
    )
}

fn regexp_capture_highlights_only_status_values_and_selectively_removes_failures() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (insert "order=417 status=FAILED retry=2\n"
          "order=418 status=PAID retry=0\n"
          "order=419 status=FAILED retry=1\n")
  (let ((hlt-use-overlays-flag 'only)
        (hlt-overlays-priority 5)
        (regexp "status=\\(FAILED\\|PAID\\)"))
    (hlt-highlight-regexp-region
     (point-min) (point-max) regexp 'error nil nil 1)
    (let ((highlighted (neomacs-highlight-test-overlays))
          (last-regexp hlt-last-regexp))
      (hlt-unhighlight-regexp-region
       (point-min) (point-max) "status=\\(FAILED\\)" 'error nil nil 1)
      (list :highlighted highlighted
            :last-regexp last-regexp
            :remaining (neomacs-highlight-test-overlays)
            :text (buffer-string)))))
"###;
    let expected = expect![[
        r#"OK (:highlighted ((:range (18 24) :text "FAILED" :face error :mouse-face nil :highlight error :priority 5 :invisible nil) (:range (50 54) :text "PAID" :face error :mouse-face nil :highlight error :priority 5 :invisible nil) (:range (80 86) :text "FAILED" :face error :mouse-face nil :highlight error :priority 5 :invisible nil)) :last-regexp "status=\\(FAILED\\|PAID\\)" :remaining ((:range (50 54) :text "PAID" :face error :mouse-face nil :highlight error :priority 5 :invisible nil)) :text "order=417 status=FAILED retry=2\norder=418 status=PAID retry=0\norder=419 status=FAILED retry=1\n")"#
    ]];
    ParityBatchCase::value(
        "regexp_capture_highlights_only_status_values_and_selectively_removes_failures",
        elisp_form,
        expected,
    )
}

fn nested_regexp_groups_layer_service_ticket_and_state_faces_by_priority() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "PAYMENTS-417:failed\nSHIPPING-882:queued\n")
  (let ((hlt-use-overlays-flag 'only)
        (hlt-overlays-priority 2)
        (hlt-last-face 'warning)
        (regexp "\\([A-Z]+\\)-\\([0-9]+\\):\\([a-z]+\\)"))
    (hlt-highlight-regexp-groups-region
     (point-min) (point-max) regexp nil nil)
    (let ((highlighted (neomacs-highlight-test-overlays)))
      (hlt-unhighlight-regexp-groups-region
       (point-min) (point-max) regexp nil nil)
      (list :highlighted highlighted
            :after (neomacs-highlight-test-overlays)
            :last-face hlt-last-face
            :last-regexp hlt-last-regexp))))
"###;
    let expected = expect![[
        r#"OK (:highlighted ((:range (1 20) :text "PAYMENTS-417:failed" :face warning :mouse-face nil :highlight warning :priority 2 :invisible nil) (:range (1 9) :text "PAYMENTS" :face hlt-regexp-level-1 :mouse-face nil :highlight hlt-regexp-level-1 :priority 3 :invisible nil) (:range (10 13) :text "417" :face hlt-regexp-level-2 :mouse-face nil :highlight hlt-regexp-level-2 :priority 4 :invisible nil) (:range (14 20) :text "failed" :face hlt-regexp-level-3 :mouse-face nil :highlight hlt-regexp-level-3 :priority 5 :invisible nil) (:range (21 29) :text "SHIPPING" :face hlt-regexp-level-1 :mouse-face nil :highlight hlt-regexp-level-1 :priority 3 :invisible nil) (:range (21 40) :text "SHIPPING-882:queued" :face warning :mouse-face nil :highlight warning :priority 2 :invisible nil) (:range (30 33) :text "882" :face hlt-regexp-level-2 :mouse-face nil :highlight hlt-regexp-level-2 :priority 4 :invisible nil) (:range (34 40) :text "queued" :face hlt-regexp-level-3 :mouse-face nil :highlight hlt-regexp-level-3 :priority 5 :invisible nil)) :after nil :last-face warning :last-regexp "\\([A-Z]+\\)-\\([0-9]+\\):\\([a-z]+\\)")"#
    ]];
    ParityBatchCase::value(
        "nested_regexp_groups_layer_service_ticket_and_state_faces_by_priority",
        elisp_form,
        expected,
    )
}

fn exact_symbol_highlights_navigate_replace_and_remove_without_prefix_collisions() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun checkout_total (items)\n"
          "  (let ((checkout_total_extra 4))\n"
          "    (+ checkout_total checkout_total_extra)))\n")
  (let ((hlt-use-overlays-flag 'only))
    (hlt-highlight-symbol 'checkout_total nil 'success)
    (let ((highlighted (neomacs-highlight-test-overlays)) first second previous)
      (goto-char (point-min))
      (setq first (hlt-next-highlight
                   (point-min) (point-max) 'success nil nil nil nil))
      (setq second (hlt-next-highlight
                    (point) (point-max) 'success nil nil nil nil))
      (goto-char (point-max))
      (setq previous (hlt-previous-highlight
                      (point-min) (point-max) 'success nil nil nil))
      (hlt-replace-highlight-face
       'success 'warning (point-min) (point-max))
      (let ((replaced (neomacs-highlight-test-overlays))
            (faces (hlt-highlight-faces-in-buffer (point-min) (point-max))))
        (hlt-unhighlight-symbol 'checkout_total nil 'warning)
        (list :highlighted highlighted
              :navigation (list first second previous)
              :navigation-text
              (mapcar (lambda (range)
                        (buffer-substring-no-properties (car range) (cdr range)))
                      (list first second previous))
              :replaced replaced
              :faces faces
              :after (neomacs-highlight-test-overlays))))))
"###;
    let expected = expect![[
        r#"OK (:highlighted ((:range (72 86) :text "checkout_total" :face success :mouse-face nil :highlight success :priority 0 :invisible nil) (:range (8 22) :text "checkout_total" :face success :mouse-face nil :highlight success :priority 0 :invisible nil)) :navigation ((8 . 22) (72 . 86) (72 . 86)) :navigation-text ("checkout_total" "checkout_total" "checkout_total") :replaced ((:range (72 86) :text "checkout_total" :face warning :mouse-face nil :highlight warning :priority 0 :invisible nil) (:range (8 22) :text "checkout_total" :face warning :mouse-face nil :highlight warning :priority 0 :invisible nil)) :faces (warning) :after nil)"#
    ]];
    ParityBatchCase::value(
        "exact_symbol_highlights_navigate_replace_and_remove_without_prefix_collisions",
        elisp_form,
        expected,
    )
}

fn property_copy_and_yank_support_default_face_only_and_explicit_full_metadata() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (insert "source-token => destination")
  (goto-char (point-min))
  (search-forward "source-token")
  (let ((source (match-beginning 0))
        (source-end (match-end 0))
        destination destination-end
        (hlt-face-prop 'face)
        (hlt-default-copy/yank-props '(face)))
    (put-text-property source source-end 'face 'warning)
    (put-text-property source source-end 'help-echo "Review invoice 481")
    (put-text-property source source-end 'audit-id 481)
    (search-forward "destination")
    (setq destination (match-beginning 0)
          destination-end (match-end 0))
    (set-buffer-modified-p nil)
    (hlt-copy-props source nil nil)
    (hlt-yank-props destination destination-end nil nil)
    (let ((default-copy
           (list :copied hlt-copied-props
                 :face (get-text-property destination 'face)
                 :help (get-text-property destination 'help-echo)
                 :audit-id (get-text-property destination 'audit-id)
                 :highlight (get-text-property destination 'hlt-highlight)
                 :ignore (get-text-property destination 'font-lock-ignore)
                 :modified (buffer-modified-p))))
      (hlt-copy-props source '(4) nil)
      (hlt-yank-props destination destination-end '(4) nil)
      (list :default default-copy
            :all
            (list :face (get-text-property destination 'face)
                  :help (get-text-property destination 'help-echo)
                  :audit-id (get-text-property destination 'audit-id)
                  :highlight (get-text-property destination 'hlt-highlight)
                  :ignore (get-text-property destination 'font-lock-ignore)
                  :modified (buffer-modified-p))
            :text (buffer-string)))))
"###;
    let expected = expect![[
        r#"OK (:default (:copied (face warning) :face warning :help nil :audit-id nil :highlight t :ignore t :modified nil) :all (:face warning :help "Review invoice 481" :audit-id 481 :highlight t :ignore t :modified nil) :text #("source-token => destination" 0 12 (audit-id 481 help-echo #1="Review invoice 481" face warning) 16 27 (font-lock-ignore t hlt-highlight t audit-id 481 help-echo #1# face warning)))"#
    ]];
    ParityBatchCase::value(
        "property_copy_and_yank_support_default_face_only_and_explicit_full_metadata",
        elisp_form,
        expected,
    )
}

fn duplicate_log_detection_ignores_incidental_whitespace_and_rotates_faces_by_group()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "timeout contacting gateway\n"
          "  timeout contacting gateway  \n"
          "\ttimeout contacting gateway\t\n"
          "paid order=17\n"
          " paid order=17 \n"
          "unique healthcheck\n")
  (let ((hlt-use-overlays-flag 'only)
        (hlt-auto-face-backgrounds '(warning success error))
        (hlt-line-dups-ignore-regexp "[ \t]*")
        (hlt-last-face 'highlight)
        (hlt-face-nb 0))
    (set-buffer-modified-p nil)
    (hlt-highlight-line-dups-region (point-min) (point-max) nil nil)
    (list :overlays (neomacs-highlight-test-overlays)
          :faces (hlt-highlight-faces-in-buffer (point-min) (point-max))
          :last-face hlt-last-face
          :face-index hlt-face-nb
          :modified (buffer-modified-p)
          :text (buffer-string))))
"###;
    let expected = expect![[
        r#"OK (:overlays ((:range (102 117) :text " paid order=17 " :face warning :mouse-face nil :highlight warning :priority 0 :invisible nil) (:range (28 58) :text "  timeout contacting gateway  " :face success :mouse-face nil :highlight success :priority 0 :invisible nil) (:range (59 87) :text "\11timeout contacting gateway\11" :face error :mouse-face nil :highlight error :priority 0 :invisible nil)) :faces (warning error success) :last-face warning :face-index 0 :modified nil :text "timeout contacting gateway\n  timeout contacting gateway  \n\11timeout contacting gateway\11\npaid order=17\n paid order=17 \nunique healthcheck\n")"#
    ]];
    ParityBatchCase::value(
        "duplicate_log_detection_ignores_incidental_whitespace_and_rotates_faces_by_group",
        elisp_form,
        expected,
    )
}

fn hidden_secret_highlight_retains_overlay_criterion_while_show_controls_visibility()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "deployment=blue secret=rotated-credential state=ready")
  (goto-char (point-min))
  (search-forward "rotated-credential")
  (let ((start (match-beginning 0))
        (end (match-end 0))
        (hlt-use-overlays-flag 'only)
        (buffer-invisibility-spec nil))
    (hlt-highlight-region start end 'warning)
    (hlt-hide-default-face (point-min) (point-max) 'warning)
    (let ((hidden
           (list :overlays (neomacs-highlight-test-overlays)
                 :spec buffer-invisibility-spec
                 :invisible (and (invisible-p start) t))))
      (hlt-show-default-face 'warning)
      (list :hidden hidden
            :shown
            (list :overlays (neomacs-highlight-test-overlays)
                  :spec buffer-invisibility-spec
                  :invisible (and (invisible-p start) t))
            :text (buffer-string)))))
"###;
    let expected = expect![[
        r#"OK (:hidden (:overlays ((:range (24 42) :text "rotated-credential" :face warning :mouse-face nil :highlight warning :priority 0 :invisible #1=(warning))) :spec (warning) :invisible t) :shown (:overlays ((:range (24 42) :text "rotated-credential" :face warning :mouse-face nil :highlight warning :priority 0 :invisible #1#)) :spec nil :invisible nil) :text "deployment=blue secret=rotated-credential state=ready")"#
    ]];
    ParityBatchCase::value(
        "hidden_secret_highlight_retains_overlay_criterion_while_show_controls_visibility",
        elisp_form,
        expected,
    )
}

fn semantic_property_filter_highlights_failed_and_ready_deployments_independently()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "api=ready payments=failed search=ready")
  (let ((hlt-use-overlays-flag 'only)
        api-start api-end payments-start payments-end search-start search-end)
    (goto-char (point-min))
    (search-forward "ready")
    (setq api-start (match-beginning 0) api-end (match-end 0))
    (search-forward "failed")
    (setq payments-start (match-beginning 0) payments-end (match-end 0))
    (search-forward "ready")
    (setq search-start (match-beginning 0) search-end (match-end 0))
    (put-text-property api-start api-end 'deployment-state 'ready)
    (put-text-property payments-start payments-end 'deployment-state 'failed)
    (put-text-property search-start search-end 'deployment-state 'ready)
    (set-buffer-modified-p nil)
    (hlt-highlight-property-with-value
     'deployment-state '(failed) (point-min) (point-max) 'error 'text nil nil)
    (hlt-highlight-property-with-value
     'deployment-state '(ready) (point-min) (point-max) 'success 'text nil nil)
    (let ((highlighted (neomacs-highlight-test-overlays)))
      (hlt-unhighlight-region-for-face
       'error (point-min) (point-max) nil nil)
      (list :highlighted highlighted
            :remaining (neomacs-highlight-test-overlays)
            :states (mapcar (lambda (position)
                              (get-text-property position 'deployment-state))
                            (list api-start payments-start search-start))
            :modified (buffer-modified-p)
            :text (buffer-string)))))
"###;
    let expected = expect![[
        r#"OK (:highlighted ((:range (20 26) :text "failed" :face error :mouse-face nil :highlight error :priority 0 :invisible nil) (:range (34 39) :text "ready" :face success :mouse-face nil :highlight success :priority 0 :invisible nil) (:range (5 10) :text "ready" :face success :mouse-face nil :highlight success :priority 0 :invisible nil)) :remaining ((:range (34 39) :text "ready" :face success :mouse-face nil :highlight success :priority 0 :invisible nil) (:range (5 10) :text "ready" :face success :mouse-face nil :highlight success :priority 0 :invisible nil)) :states (ready failed ready) :modified nil :text #("api=ready payments=failed search=ready" 4 9 (deployment-state ready) 19 25 (deployment-state failed) 33 38 (deployment-state ready)))"#
    ]];
    ParityBatchCase::value(
        "semantic_property_filter_highlights_failed_and_ready_deployments_independently",
        elisp_form,
        expected,
    )
}

#[test]
fn highlight_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(HIGHLIGHT_MELPA_PIN, "highlight.el")
            .expect("prepare revision-pinned Highlight below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "highlight-package-batch",
        "Highlight",
        &[
            package_contract_exposes_commands_prefix_keys_and_policy_defaults(),
            overlay_incident_highlight_preserves_buffer_state_and_splits_on_partial_removal(),
            text_property_review_highlight_round_trips_without_overlays_or_dirtying_the_buffer(),
            regexp_capture_highlights_only_status_values_and_selectively_removes_failures(),
            nested_regexp_groups_layer_service_ticket_and_state_faces_by_priority(),
            exact_symbol_highlights_navigate_replace_and_remove_without_prefix_collisions(),
            property_copy_and_yank_support_default_face_only_and_explicit_full_metadata(),
            duplicate_log_detection_ignores_incidental_whitespace_and_rotates_faces_by_group(),
            hidden_secret_highlight_retains_overlay_criterion_while_show_controls_visibility(),
            semantic_property_filter_highlights_failed_and_ready_deployments_independently(),
        ],
    );
}
