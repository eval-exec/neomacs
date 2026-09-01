use expect_test::expect;

use super::ParityBatchCase;

fn headings_policy_and_action_map_are_wired() -> ParityBatchCase {
    ParityBatchCase::value(
        "headings_policy_and_action_map_are_wired",
        r####"
(list :min-depth helm-org-headings-min-depth
      :max-depth helm-org-headings-max-depth
      :actions (mapcar #'car helm-org-headings-actions)
      :map-indirect (lookup-key helm-org-headings-map (kbd "C-c i"))
      :map-refile (lookup-key helm-org-headings-map (kbd "C-c w"))
      :map-link (lookup-key helm-org-headings-map (kbd "C-c l")))
"####,
        expect![[
            r#"OK (:min-depth 1 :max-depth 8 :actions ("Go to heading" "Open in indirect buffer `C-c i'" "Refile heading(s) (marked-to-selected|current-to-selected) `C-c w`" "Insert link to this heading `C-c l`") :map-indirect helm-org-run-open-heading-in-indirect-buffer :map-refile helm-org-run-refile-heading-to :map-link helm-org-run-insert-link-to-heading-at-marker)"#
        ]],
    )
}

fn candidates_collect_headings_with_markers_and_respect_depth() -> ParityBatchCase {
    ParityBatchCase::value(
        "candidates_collect_headings_with_markers_and_respect_depth",
        r####"
(neomacs-helm-org-test-with-buffer
 (lambda (buffer)
   (let* ((helm-org-headings-fontify nil)
          (helm-org-format-outline-path nil)
          (helm-org-headings-min-depth 1)
          (helm-org-headings-max-depth 8)
          (all (helm-org--get-candidates-in-file buffer nil t nil t))
          (shallow
           (let ((helm-org-headings-max-depth 1))
             (helm-org--get-candidates-in-file buffer nil t nil t)))
          (deep-only
           (let ((helm-org-headings-min-depth 3)
                 (helm-org-headings-max-depth 3))
             (helm-org--get-candidates-in-file buffer nil t nil t)))
          (displays (mapcar #'substring-no-properties all))
          (markers (mapcar (lambda (c) (get-text-property 0 'helm-realvalue c))
                           all))
          (reals (mapcar (lambda (c) (get-text-property 0 'helm-real-display c))
                         all)))
     (list :count (length all)
           :displays displays
           :reals reals
           :markers-live
           (cl-every (lambda (m)
                       (and (markerp m)
                            (eq (marker-buffer m) buffer)
                            (integerp (marker-position m))))
                     markers)
           :marker-positions (mapcar #'marker-position markers)
           :shallow (mapcar #'substring-no-properties shallow)
           :deep-only (mapcar #'substring-no-properties deep-only)))))
"####,
        expect![[
            r#"OK (:count 4 :displays ("* Alpha" "** Beta" "*** Gamma" "* Delta") :reals ("Alpha" "Beta" "Gamma" "Delta") :markers-live t :marker-positions (8 27 47 66) :shallow ("* Alpha" "* Delta") :deep-only ("*** Gamma"))"#
        ]],
    )
}

fn preselect_and_goto_marker_move_point_to_headings() -> ParityBatchCase {
    ParityBatchCase::value(
        "preselect_and_goto_marker_move_point_to_headings",
        r####"
(neomacs-helm-org-test-with-buffer
 (lambda (buffer)
   (goto-char (point-min))
   (search-forward "** Beta")
   (forward-line 0)
   (let ((preselect (helm-org-in-buffer-preselect))
         (candidates
          (let ((helm-org-headings-fontify nil)
                (helm-org-format-outline-path nil))
            (helm-org--get-candidates-in-file buffer nil t nil t))))
     (let* ((gamma
             (cl-find-if
              (lambda (c)
                (equal (get-text-property 0 'helm-real-display c) "Gamma"))
              candidates))
            (marker (get-text-property 0 'helm-realvalue gamma)))
       (helm-org-goto-marker marker)
       (list :preselect preselect
             :at-gamma (org-get-heading t t t t)
             :point (point)
             :marker-pos (marker-position marker))))))
"####,
        expect![[r#"OK (:preselect "\\*\\* Beta" :at-gamma "Gamma" :point 38 :marker-pos 47)"#]],
    )
}

fn in_buffer_command_builds_helm_source_with_heading_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "in_buffer_command_builds_helm_source_with_heading_candidates",
        r####"
(neomacs-helm-org-test-with-buffer
 (lambda (_buffer)
   (goto-char (point-min))
   (search-forward "* Delta")
   (forward-line 0)
   (let ((helm-org-headings-fontify nil)
         (helm-org-format-outline-path nil)
         (helm-org-truncate-lines t))
     (let ((captured
            (neomacs-helm-org-test-capture-helm
             (lambda () (helm-org-in-buffer-headings)))))
       (list :buffer (plist-get captured :buffer)
             :preselect (plist-get captured :preselect)
             :truncate (plist-get captured :truncate)
             :source-count (plist-get captured :source-count)
             :source-name (plist-get captured :source-name)
             :candidates
             (mapcar #'substring-no-properties
                     (plist-get captured :candidates)))))))
"####,
        expect![[
            r#"OK (:buffer "*helm org inbuffer*" :preselect "\\* Delta" :truncate t :source-count 1 :source-name "Org headings ( *neomacs-helm-org-test*)" :candidates ("* Alpha" "** Beta" "*** Gamma" "* Delta"))"#
        ]],
    )
}

fn indent_helper_preserves_heading_text_without_fontify() -> ParityBatchCase {
    ParityBatchCase::value(
        "indent_helper_preserves_heading_text_without_fontify",
        r####"
(let ((helm-org-headings-fontify nil)
      (org-hide-leading-stars nil)
      (inputs '("* Alpha" "** Beta" "*** Gamma"))
      (helm-buffer (get-buffer-create "*helm*")))
  (unwind-protect
      (with-current-buffer helm-buffer
        (list :plain (mapcar #'helm-org-indent-headings-1 inputs)
              :batch (helm-org-indent-headings inputs nil)))
    (when (buffer-live-p helm-buffer)
      (let ((kill-buffer-hook nil)
            (kill-buffer-query-functions nil))
        (kill-buffer helm-buffer)))))
"####,
        expect![[
            r#"OK (:plain ("* Alpha" "** Beta" "*** Gamma") :batch ("* Alpha" "** Beta" "*** Gamma"))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        headings_policy_and_action_map_are_wired(),
        candidates_collect_headings_with_markers_and_respect_depth(),
        preselect_and_goto_marker_move_point_to_headings(),
        in_buffer_command_builds_helm_source_with_heading_candidates(),
        indent_helper_preserves_heading_text_without_fontify(),
    ]
}
