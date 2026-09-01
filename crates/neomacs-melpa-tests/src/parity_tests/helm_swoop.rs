use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_SWOOP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HELM_SWOOP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HELM_SWOOP_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'helm-swoop)

(defun neomacs-helm-swoop-test-reset ()
  "Remove buffers and global state created by a Helm Swoop parity case."
  (dolist (name '("deploy-candidates" "deploy-edit-target"
                  "deploy-delete-target" "deploy-face-target"
                  "deploy-api" "deploy-worker" "deploy-directory"
                  "*deploy-internal*" "*helm-multi-swoop buffers list*"
                  "*Helm Swoop Edit*" "*Helm Multi Swoop Edit*"))
    (when-let ((buffer (get-buffer name)))
      (kill-buffer buffer)))
  (when (overlayp helm-swoop-line-overlay)
    (delete-overlay helm-swoop-line-overlay))
  (setq helm-swoop-line-overlay nil
        helm-swoop-invisible-targets nil
        helm-swoop-last-point nil
        helm-swoop-last-line-info nil
        helm-swoop-target-buffer nil
        helm-swoop-synchronizing-window nil
        helm-multi-swoop-last-selected-buffers nil
        helm-multi-swoop-query nil))

(defun neomacs-helm-swoop-test-with-reset (function)
  "Run FUNCTION without leaking Helm Swoop buffers or editor state."
  (neomacs-helm-swoop-test-reset)
  (unwind-protect
      (funcall function)
    (neomacs-helm-swoop-test-reset)))

(defun neomacs-helm-swoop-test-lines (string)
  "Return non-empty property-free lines from STRING."
  (seq-filter
   (lambda (line) (not (string-empty-p line)))
   (split-string (substring-no-properties string) "\n")))
"###;

fn candidate_indexing_preserves_real_lines_properties_and_narrowing() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-swoop-test-with-reset
 (lambda ()
   (let ((buffer (get-buffer-create "deploy-candidates")))
     (with-current-buffer buffer
       (insert "header\ndeploy api\n\ndeploy worker\nfooter\n")
       (goto-char (point-min))
       (search-forward "deploy api")
       (add-text-properties
        (match-beginning 0) (match-end 0)
        '(face font-lock-keyword-face))
       (let* ((helm-swoop-speed-or-color nil)
              (plain (helm-swoop--get-content buffer))
              (plain-match (string-match "deploy api" plain))
              (helm-swoop-speed-or-color t)
              (colored (helm-swoop--get-content buffer))
              (colored-match (string-match "deploy api" colored))
              narrowed)
         (save-restriction
           (goto-char (point-min))
           (forward-line 1)
           (let ((begin (point)))
             (forward-line 3)
             (narrow-to-region begin (point)))
           (setq narrowed (helm-swoop--get-content buffer t)))
         (list :plain-lines
               (neomacs-helm-swoop-test-lines plain)
               :plain-face (get-text-property plain-match 'face plain)
               :colored-lines
               (neomacs-helm-swoop-test-lines colored)
               :colored-face
               (get-text-property colored-match 'face colored)
               :narrowed-lines
               (neomacs-helm-swoop-test-lines narrowed)))))))
"###;
    let expected = expect![[
        r#"OK (:plain-lines ("1 header" "2 deploy api" "4 deploy worker" "5 footer") :plain-face nil :colored-lines ("1 header" "2 deploy api" "4 deploy worker" "5 footer") :colored-face font-lock-keyword-face :narrowed-lines ("1 deploy api" "2 " "3 deploy worker" "4 "))"#
    ]];
    ParityBatchCase::value(
        "candidate_indexing_preserves_real_lines_properties_and_narrowing",
        elisp_form,
        expected,
    )
}

fn multiline_candidate_chunks_keep_searchable_line_identity() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-swoop-test-with-reset
 (lambda ()
   (let ((buffer (get-buffer-create "deploy-candidates")))
     (with-current-buffer buffer
       (insert "alpha\nbeta deploy\ngamma\ndelta deploy\nepsilon\n")
       (let* ((numbered (helm-swoop--get-content buffer t))
              (pairs (helm-swoop--split-lines-by numbered "\n" 2))
              (triples (helm-swoop--split-lines-by numbered "\n" 3)))
         (list :pairs pairs
               :pair-search-texts
               (mapcar #'helm-swoop--match-part pairs)
               :triples triples
               :nearest
               (mapcar
                (lambda (target)
                  (helm-swoop--nearest-line target '(2 6 10 14)))
                '(1 4 8 12 20))))))))
"###;
    let expected = expect![[
        r#"OK (:pairs ("1 alpha" "2 beta deploy\n3 gamma" "4 delta deploy\n5 epsilon" "6 ") :pair-search-texts ("alpha" "beta deploy\ngamma" "delta deploy\nepsilon" "") :triples ("1 alpha\n2 beta deploy" "3 gamma\n4 delta deploy\n5 epsilon" "6 ") :nearest (2 6 10 14 14))"#
    ]];
    ParityBatchCase::value(
        "multiline_candidate_chunks_keep_searchable_line_identity",
        elisp_form,
        expected,
    )
}

fn interactive_launch_builds_a_searchable_source_from_the_current_symbol() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-swoop-test-with-reset
 (lambda ()
   (let ((buffer (get-buffer-create "deploy-candidates"))
         invocation)
     (switch-to-buffer buffer)
     (insert "queued\ndeploy_status healthy\ndeploy_status warning\n")
     (goto-char (point-min))
     (search-forward "deploy_status")
     (let ((helm-exit-status 0)
           (helm-pattern "deploy_status")
           (helm-swoop-speed-or-color nil)
           (transient-mark-mode nil))
       (cl-letf (((symbol-function 'helm)
                  (lambda (&rest arguments)
                    (setq invocation arguments)))
                 ((symbol-function 'recenter)
                  (lambda (&rest _) nil)))
         (helm-swoop)))
     (let* ((source (plist-get invocation :sources))
            (candidates (assoc-default 'candidates source)))
       (list :input (plist-get invocation :input)
             :prompt (plist-get invocation :prompt)
             :preselect (plist-get invocation :preselect)
             :candidate-limit
             (plist-get invocation :candidate-number-limit)
             :source-name (assoc-default 'name source)
             :candidates candidates
             :match-functions (assoc-default 'match source)
             :point (point)
             :overlay-live (overlayp helm-swoop-line-overlay))))))
"###;
    let expected = expect![[
        r#"OK (:input "deploy_status" :prompt "Swoop: " :preselect "^2 " :candidate-limit 19999 :source-name "deploy-candidates" :candidates ("1 queued" "2 deploy_status healthy" "3 deploy_status warning" "4 ") :match-functions (helm-mm-exact-match helm-mm-match helm-mm-3-migemo-match) :point 8 :overlay-live t)"#
    ]];
    ParityBatchCase::value(
        "interactive_launch_builds_a_searchable_source_from_the_current_symbol",
        elisp_form,
        expected,
    )
}

fn isearch_and_region_queries_transfer_literal_and_regexp_intent() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-swoop-test-with-reset
 (lambda ()
   (let (queries exits region-query symbol-query)
     (cl-letf (((symbol-function 'isearch-exit)
                (lambda () (setq exits (1+ (or exits 0)))))
               ((symbol-function 'helm-swoop)
                (lambda (&rest arguments)
                  (push (plist-get arguments :query) queries))))
       (let ((isearch-string "deploy.status")
             (isearch-regexp nil))
         (helm-swoop-from-isearch))
       (let ((isearch-string "deploy.*status")
             (isearch-regexp t))
         (helm-swoop-from-isearch)))
     (with-temp-buffer
       (insert "release deploy.status ready")
       (goto-char 9)
       (push-mark 22 t t)
       (setq region-query (helm-multi-swoop--get-query nil))
       (goto-char 12)
       (setq symbol-query
             (helm-swoop-pre-input-optimize
              (funcall helm-swoop-pre-input-function))))
     (list :isearch-queries (nreverse queries)
           :isearch-exits exits
           :region-query region-query
           :symbol-query symbol-query))))
"###;
    let expected = expect![[
        r#"OK (:isearch-queries ("deploy\\.status" "deploy.*status") :isearch-exits 2 :region-query "deploy\\.status" :symbol-query "deploy")"#
    ]];
    ParityBatchCase::value(
        "isearch_and_region_queries_transfer_literal_and_regexp_intent",
        elisp_form,
        expected,
    )
}

fn edit_completion_applies_selected_replacements_from_bottom_to_top() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-swoop-test-with-reset
 (lambda ()
   (let ((target (get-buffer-create "deploy-edit-target"))
         (edit (get-buffer-create helm-swoop-edit-buffer))
         message-text)
     (with-current-buffer target
       (insert "alpha\nbeta old\ngamma\ndelta old\n"))
     (with-current-buffer edit
       (insert "Helm Swoop\n2 beta revised\n4 delta revised\n"))
     (setq helm-swoop-edit-target-buffer target
           helm-swoop-synchronizing-window (selected-window))
     (with-current-buffer edit
       (cl-letf (((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (setq message-text
                          (apply #'format format-string arguments)))))
         (helm-swoop--edit-complete)))
     (list :content
           (with-current-buffer target (buffer-string))
           :modified
           (with-current-buffer target (buffer-modified-p))
           :edit-buffer-live (buffer-live-p edit)
           :message message-text))))
"###;
    let expected = expect![[
        r#"OK (:content "alpha\nbeta revised\ngamma\ndelta revised\n" :modified t :edit-buffer-live nil :message "Successfully helm-swoop-edit applied to original buffer")"#
    ]];
    ParityBatchCase::value(
        "edit_completion_applies_selected_replacements_from_bottom_to_top",
        elisp_form,
        expected,
    )
}

fn edit_deletion_removes_selected_middle_and_final_lines() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-swoop-test-with-reset
 (lambda ()
   (let ((target (get-buffer-create "deploy-delete-target"))
         (edit (get-buffer-create helm-swoop-edit-buffer))
         message-text)
     (with-current-buffer target
       (insert "alpha\nbeta remove\ngamma\ndelta remove\n"))
     (with-current-buffer edit
       (insert "Helm Swoop\n2 beta remove\n4 delta remove\n"))
     (setq helm-swoop-edit-target-buffer target
           helm-swoop-synchronizing-window (selected-window))
     (with-current-buffer edit
       (cl-letf (((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (setq message-text
                          (apply #'format format-string arguments)))))
         (helm-swoop--edit-delete-all-lines)))
     (list :content
           (with-current-buffer target (buffer-string))
           :line-count
           (with-current-buffer target (line-number-at-pos (point-max)))
           :edit-buffer-live (buffer-live-p edit)
           :message message-text))))
"###;
    let expected = expect![[
        r#"OK (:content "alpha\ngamma\n" :line-count 3 :edit-buffer-live nil :message "Successfully helm-swoop-edit applied to original buffer")"#
    ]];
    ParityBatchCase::value(
        "edit_deletion_removes_selected_middle_and_final_lines",
        elisp_form,
        expected,
    )
}

fn face_search_and_hidden_line_preview_restore_display_state() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-swoop-test-with-reset
 (lambda ()
   (let ((buffer (get-buffer-create "deploy-face-target")))
     (with-current-buffer buffer
       (insert "plain\ndeploy keyword\nother deploy keyword\nfooter\n")
       (goto-char (point-min))
       (while (search-forward "deploy" nil t)
         (add-text-properties
          (match-beginning 0) (match-end 0)
          '(face font-lock-keyword-face)))
       (let* ((candidates
               (mapcar #'substring-no-properties
                       (helm-swoop--cull-face-include-line
                        'font-lock-keyword-face)))
              (target-overlays
               (cl-count-if
                (lambda (overlay) (overlay-get overlay 'target-buffer))
                (overlays-in (point-min) (point-max))))
              (hidden-start
               (progn
                 (goto-char (point-min))
                 (forward-line 1)
                 (point)))
              (hidden-end (line-end-position))
              (hidden (make-overlay hidden-start hidden-end buffer))
              unveiled restored)
         (overlay-put hidden 'invisible 'deployment-details)
         (setq helm-swoop-line-overlay
               (make-overlay hidden-start hidden-end buffer))
         (helm-swoop--unveil-invisible-overlay)
         (setq unveiled
               (list (overlay-get hidden 'invisible)
                     (length helm-swoop-invisible-targets)))
         (helm-swoop--restore-unveiled-overlay)
         (setq restored
               (list (overlay-get hidden 'invisible)
                     helm-swoop-invisible-targets))
         (list :candidates candidates
               :target-overlays target-overlays
               :unveiled unveiled
               :restored restored))))))
"###;
    let expected = expect![[
        r#"OK (:candidates ("2 deploy keyword" "3 other deploy keyword") :target-overlays 2 :unveiled (nil 1) :restored (deployment-details nil))"#
    ]];
    ParityBatchCase::value(
        "face_search_and_hidden_line_preview_restore_display_state",
        elisp_form,
        expected,
    )
}

fn multi_buffer_selection_filters_internal_dired_and_ignored_buffers() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-swoop-test-with-reset
 (lambda ()
   (let ((api (get-buffer-create "deploy-api"))
         (worker (get-buffer-create "deploy-worker"))
         (directory (get-buffer-create "deploy-directory"))
         (internal (get-buffer-create "*deploy-internal*"))
         (selection (get-buffer-create helm-multi-swoop-buffer-list)))
     (with-current-buffer api (setq major-mode 'emacs-lisp-mode))
     (with-current-buffer worker (setq major-mode 'emacs-lisp-mode))
     (with-current-buffer directory (setq major-mode 'dired-mode))
     (with-current-buffer internal (setq major-mode 'emacs-lisp-mode))
     (with-current-buffer selection
       (insert " deploy-worker \n deploy-api \n")
       (let ((first (make-overlay 1 16))
             (second (make-overlay 17 29)))
         (overlay-put first 'face 'helm-visible-mark)
         (overlay-put second 'face 'helm-visible-mark)))
     (let* ((helm-multi-swoop-ignore-buffers-match
             "^\\*\\|worker")
            (available
             (seq-filter
              (lambda (name) (string-prefix-p "deploy-" name))
              (helm-multi-swoop--get-buffer-list)))
            (same-mode
             (sort
              (seq-filter
               (lambda (name) (string-prefix-p "deploy-" name))
               (get-buffers-matching-mode 'emacs-lisp-mode))
              #'string<))
            (marked (helm-multi-swoop--get-marked-buffers)))
       (list :available (sort available #'string<)
             :same-mode same-mode
             :marked marked)))))
"###;
    let expected = expect![[
        r#"OK (:available ("deploy-api") :same-mode ("deploy-api" "deploy-worker") :marked ("deploy-worker" "deploy-api"))"#
    ]];
    ParityBatchCase::value(
        "multi_buffer_selection_filters_internal_dired_and_ignored_buffers",
        elisp_form,
        expected,
    )
}

#[test]
fn helm_swoop_package_batch() {
    let cases = vec![
        candidate_indexing_preserves_real_lines_properties_and_narrowing(),
        multiline_candidate_chunks_keep_searchable_line_identity(),
        interactive_launch_builds_a_searchable_source_from_the_current_symbol(),
        isearch_and_region_queries_transfer_literal_and_regexp_intent(),
        edit_completion_applies_selected_replacements_from_bottom_to_top(),
        edit_deletion_removes_selected_middle_and_final_lines(),
        face_search_and_hidden_line_preview_restore_display_state(),
        multi_buffer_selection_filters_internal_dired_and_ignored_buffers(),
    ];
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(HELM_SWOOP_MELPA_PIN, "helm-swoop.el")
            .expect("prepare revision-pinned Helm Swoop source below ./tmp")
            .with_prelude(HELM_SWOOP_TEST_PRELUDE)
            .with_timeout(HELM_SWOOP_TEST_TIMEOUT),
        "helm-swoop-package-batch",
        "Helm Swoop",
        &cases,
    );
}
