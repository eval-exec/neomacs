use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LSP_UI_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const LSP_UI_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const LSP_UI_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'seq)
(require 'lsp-ui)
(require 'lsp-ui-flycheck)

(defun neomacs-lsp-ui-test-position (line character)
  "Build an LSP position at LINE and CHARACTER."
  (lsp-make-position :line line :character character))

(defun neomacs-lsp-ui-test-range (start-line start-character end-line end-character)
  "Build an LSP range from its four wire coordinates."
  (lsp-make-range
   :start (neomacs-lsp-ui-test-position start-line start-character)
   :end (neomacs-lsp-ui-test-position end-line end-character)))

(defun neomacs-lsp-ui-test-location (file line start end)
  "Build an LSP location in FILE on LINE between START and END."
  (lsp-make-location
   :uri (lsp--path-to-uri file)
   :range (neomacs-lsp-ui-test-range line start line end)))

(defun neomacs-lsp-ui-test-kill-files-below (root)
  "Kill every file-visiting buffer whose file is below ROOT."
  (dolist (buffer (buffer-list))
    (when-let* ((file (buffer-file-name buffer))
                ((string-prefix-p root (file-truename file))))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))))

(defun neomacs-lsp-ui-test-property-runs (property)
  "Return stable runs carrying PROPERTY in the current buffer."
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let* ((value (get-text-property position property))
             (next (next-single-property-change
                    position property nil (point-max))))
        (when value
          (push (list (- position (point-min))
                      (- next (point-min))
                      value)
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-lsp-ui-test-reset ()
  "Remove shared buffers, overlays, and timers created by parity cases."
  (dolist (name '("*lsp-diagnostics*" "*lsp-ui-imenu*"))
    (when-let ((buffer (get-buffer name)))
      (kill-buffer buffer)))
  (dolist (buffer (buffer-list))
    (when (string-prefix-p lsp-ui-doc--buffer-prefix (buffer-name buffer))
      (kill-buffer buffer)))
  (when (boundp 'lsp-ui-doc--timer-mouse-idle)
    (lsp-ui-util-safe-kill-timer lsp-ui-doc--timer-mouse-idle)
    (setq lsp-ui-doc--timer-mouse-idle nil)))

(defun neomacs-lsp-ui-test-with-reset (function)
  "Run FUNCTION without leaking LSP UI state into the next case."
  (neomacs-lsp-ui-test-reset)
  (unwind-protect
      (funcall function)
    (neomacs-lsp-ui-test-reset)))
"###;

fn reference_navigation_sorts_server_locations_and_visits_adjacent_uses() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-lsp-ui-test-with-reset
 (lambda ()
   (let* ((root (file-name-as-directory
                 (file-truename (make-temp-file "lsp-ui-references-" t))))
          (api (expand-file-name "api.el" root))
          (worker (expand-file-name "worker.el" root)))
     (unwind-protect
         (progn
           (with-temp-file api
             (insert "(defun deploy-api ())\n"
                     "(message \"between\")\n"
                     "(deploy-api)\n"))
           (with-temp-file worker
             (insert "(message \"start\")\n"
                     "(deploy-api)\n"
                     "(message \"done\")\n"))
           (let ((locations
                  (vector
                   (neomacs-lsp-ui-test-location worker 1 1 11)
                   (neomacs-lsp-ui-test-location api 2 1 11)
                   (neomacs-lsp-ui-test-location api 0 7 17))))
             (cl-letf (((symbol-function 'lsp-request)
                        (lambda (&rest _) locations))
                       ((symbol-function 'lsp--make-reference-params)
                        (lambda (&rest _) 'request-params))
                       ((symbol-function 'lsp-workspace-root)
                        (lambda (&rest _) root)))
               (switch-to-buffer (find-file-noselect api))
               (goto-char (point-min))
               (forward-line 1)
               (let* ((triples (lsp-ui--reference-triples t))
                      (next-index (lsp-ui-find-next-reference t))
                      (next-target
                       (list (file-relative-name buffer-file-name root)
                             (line-number-at-pos)
                             (current-column)))
                      previous-index previous-target)
                 (switch-to-buffer (find-file-noselect worker))
                 (goto-char (point-max))
                 (setq previous-index (lsp-ui-find-prev-reference t)
                       previous-target
                       (list (file-relative-name buffer-file-name root)
                             (line-number-at-pos)
                             (current-column)))
                 (list
                  :sorted
                  (mapcar
                   (lambda (triple)
                     (cons (file-relative-name (car triple) root)
                           (cdr triple)))
                   triples)
                  :next (list next-index next-target)
                  :previous (list previous-index previous-target))))))
       (neomacs-lsp-ui-test-kill-files-below root)
       (delete-directory root t)))))
"###;
    let expected = expect![[
        r#"OK (:sorted (("api.el" 0 7) ("api.el" 2 1) ("worker.el" 1 1)) :next ((1 . 3) ("api.el" 3 1)) :previous ((2 . 3) ("worker.el" 2 1)))"#
    ]];
    ParityBatchCase::value(
        "reference_navigation_sorts_server_locations_and_visits_adjacent_uses",
        elisp_form,
        expected,
    )
}

fn hover_content_preserves_plain_markdown_and_optional_language_signatures() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-lsp-ui-test-with-reset
 (lambda ()
   (let* ((signature
           (lsp-make-marked-string
            :language "emacs-lisp"
            :value "(deploy-release release environment)"))
          (documentation
           "Deploy **release** to `environment`.\n\nReturns the rollout id.")
          (contents (vector signature documentation))
          without-signature with-signature plain-text)
     (let ((lsp-ui-doc-include-signature nil))
       (setq without-signature (lsp-ui-doc--extract contents)))
     (let ((lsp-ui-doc-include-signature t))
       (setq with-signature (lsp-ui-doc--extract contents)))
     (setq plain-text
           (lsp-ui-doc--extract
            (lsp-make-markup-content
             :kind lsp/markup-kind-plain-text
             :value "release v2\nready")))
     (list
      :without-signature (substring-no-properties without-signature)
      :without-signature-faces
      (let (runs (position 0))
        (while (< position (length without-signature))
          (let ((face (get-text-property position 'face without-signature))
                (next (next-single-property-change
                       position 'face without-signature
                       (length without-signature))))
            (when face (push (list position next face) runs))
            (setq position next)))
        (nreverse runs))
      :with-signature (substring-no-properties with-signature)
      :plain plain-text))))
"###;
    let expected = expect![[
        r#"OK (:without-signature "" :without-signature-faces nil :with-signature "(deploy-release release environment)\n\nDeploy release to environment.\n\nReturns the rollout id." :plain "release v2\nready")"#
    ]];
    ParityBatchCase::value(
        "hover_content_preserves_plain_markdown_and_optional_language_signatures",
        elisp_form,
        expected,
    )
}

fn rendered_hover_compacts_blank_lines_and_replaces_markdown_rules() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-lsp-ui-test-with-reset
 (lambda ()
   (with-temp-buffer
     (insert "Deployment status\n\n---\nHealthy\n\n---\nOwner: release\n")
     (markdown-mode)
     (lsp-ui-doc--make-smaller-empty-lines)
     (lsp-ui-doc--handle-hr-lines)
     (list
      :text (buffer-substring-no-properties (point-min) (point-max))
      :compact (neomacs-lsp-ui-test-property-runs 'lsp-ui-doc-no-space)
      :rules (neomacs-lsp-ui-test-property-runs 'lsp-ui-doc--replace-hr)
      :display (neomacs-lsp-ui-test-property-runs 'display)
      :faces (neomacs-lsp-ui-test-property-runs 'face)))))
"###;
    let expected = expect![[
        r#"OK (:text "\nDeployment status\n  \n   \nHealthy\n \n   \nOwner: release\n\n\n" :compact nil :rules ((22 23 t) (36 37 t)) :display ((22 23 #1=(space :height (1))) (23 24 #2=(space :height (1))) (36 37 #1#) (37 38 #2#)) :faces ((0 1 (:height 0.3)) (19 20 (:height 0.1)) (20 21 #3=(:height 0.2)) (21 22 #4=(:height 0.4)) (22 23 #5=(:background "dark grey")) (24 26 #6=(:height 0.2)) (34 35 #3#) (35 36 #4#) (36 37 #5#) (38 40 #6#) (55 57 (:height 0.3))))"#
    ]];
    ParityBatchCase::value(
        "rendered_hover_compacts_blank_lines_and_replaces_markdown_rules",
        elisp_form,
        expected,
    )
}

fn inline_hover_merges_visible_source_with_a_fixed_width_document_panel() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-lsp-ui-test-with-reset
 (lambda ()
   (with-temp-buffer
     (insert "deploy\tapi [hidden]\nworker ready\n")
     (add-text-properties 12 20 '(invisible t))
     (let ((tab-width 4)
           (lsp-ui-doc-text-scale-level 0))
       (lsp-ui-doc--with-buffer
         (erase-buffer)
         (insert "Release v2\nHealthy"))
       (cl-letf (((symbol-function 'lsp-ui-doc--inline-window-width)
                  (lambda () 38)))
         (let ((merged (lsp-ui-doc--inline-merge (buffer-string))))
           (list :text (substring-no-properties merged)
                 :panel-width lsp-ui-doc--inline-width
                 :hidden-source-retained
                 (and (string-match-p "hidden" merged) t))))))))
"###;
    let expected = expect![[
        r#"OK (:text "deploy    api              Release v2 \nworker ready               Healthy    \n" :panel-width 10 :hidden-source-retained nil)"#
    ]];
    ParityBatchCase::value(
        "inline_hover_merges_visible_source_with_a_fixed_width_document_panel",
        elisp_form,
        expected,
    )
}

fn sideline_hover_places_overlays_and_tracks_the_symbol_at_point() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-lsp-ui-test-with-reset
 (lambda ()
   (with-temp-buffer
     (insert "deploy release\nshort\nroom for annotation\nfinal\n")
     (goto-char 3)
     (let* ((bounds (cons (line-beginning-position) (line-end-position)))
            (hover
             (lsp-make-hover
              :contents
              (lsp-make-markup-content
               :kind lsp/markup-kind-plain-text
               :value "release v2 is healthy")))
            (lsp-ui-sideline-show-symbol t)
            (lsp-ui-sideline-ignore-duplicate t)
            (lsp-ui-sideline--occupied-lines nil)
            (lsp-ui-sideline--cached-infos nil)
            (lsp-ui-sideline--ovs nil))
       (cl-letf (((symbol-function 'lsp-ui-sideline--window-width)
                  (lambda () 72)))
         (lsp-ui-sideline--push-info
          72 "deploy" bounds hover
          (line-beginning-position) (line-end-position)))
       (let* ((overlay (car lsp-ui-sideline--ovs))
              (initial
               (list :position (overlay-start overlay)
                     :occupied lsp-ui-sideline--occupied-lines
                     :symbol (overlay-get overlay 'symbol)
                     :info (overlay-get overlay 'info)
                     :current (overlay-get overlay 'current)
                     :display
                     (substring-no-properties
                      (overlay-get overlay 'after-string)))))
         (goto-char (point-max))
         (lsp-ui-sideline--highlight-current (point))
         (let ((away (overlay-get overlay 'current)))
           (goto-char (car bounds))
           (lsp-ui-sideline--highlight-current (point))
           (list :initial initial
                 :away away
                 :returned (overlay-get overlay 'current)
                 :overlay-count
                 (length (overlays-in (point-min) (point-max))))))))))
"###;
    let expected = expect![[
        r#"OK (:initial (:position 21 :occupied (21) :symbol "deploy" :info #("release v2 is healthy" 0 7 (face #1=(lsp-ui-sideline-symbol-info default)) 8 10 (face #1#) 11 13 (face #1#) 14 21 (face #1#)) :current t :display " release v2 is healthy  deploy ") :away nil :returned t :overlay-count 1)"#
    ]];
    ParityBatchCase::value(
        "sideline_hover_places_overlays_and_tracks_the_symbol_at_point",
        elisp_form,
        expected,
    )
}

fn workspace_diagnostics_render_actionable_lines_with_navigation_metadata() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-lsp-ui-test-with-reset
 (lambda ()
   (let* ((root (file-name-as-directory
                 (file-truename (make-temp-file "lsp-ui-diagnostics-" t))))
          (file (expand-file-name "service.el" root))
          (diagnostics (make-hash-table :test 'equal)))
     (unwind-protect
         (progn
           (puthash
            file
            (list
             (lsp-make-diagnostic
              :range (neomacs-lsp-ui-test-range 2 4 2 11)
              :message "Undefined rollout id\nsecondary detail"
              :severity? 1
              :source? "deploy-ls")
             (lsp-make-diagnostic
              :range (neomacs-lsp-ui-test-range 6 0 6 7)
              :message "Stale deployment target"
              :severity? 2))
            diagnostics)
           (with-temp-buffer
             (cl-letf (((symbol-function 'lsp-diagnostics)
                        (lambda () diagnostics))
                       ((symbol-function 'lsp-ui--workspace-path)
                        (lambda (path) (file-relative-name path root))))
               (lsp-ui-flycheck-list--update (selected-window) 'workspace))
             (let* ((first (point-min))
                    (second (save-excursion (goto-char first)
                                            (forward-line 1)
                                            (point)))
                    (first-diag (get-text-property first 'diag))
                    (second-diag (get-text-property second 'diag))
                    (heading
                     (mapcar
                      (lambda (overlay)
                        (substring-no-properties
                         (or (overlay-get overlay 'after-string) "")))
                      (overlays-in (point-min) (point-max)))))
               (list
                :text (buffer-substring-no-properties
                       (point-min) (point-max))
                :heading heading
                :mode major-mode
                :first
                (list (lsp:diagnostic-message first-diag)
                      (lsp:diagnostic-severity? first-diag)
                      (file-relative-name
                       (get-text-property first 'file) root)
                      (get-text-property first 'face))
                :second
                (list (lsp:diagnostic-message second-diag)
                      (lsp:diagnostic-severity? second-diag)
                      (get-text-property second 'face))))))
       (delete-directory root t)))))
"###;
    let expected = expect![[
        r#"OK (:text "3: deploy-ls: Undefined rollout id\n7: Stale deployment target\n" :heading ("\nservice.el\n") :mode lsp-ui-flycheck-list-mode :first ("Undefined rollout id\nsecondary detail" 1 "service.el" error) :second ("Stale deployment target" 2 warning))"#
    ]];
    ParityBatchCase::value(
        "workspace_diagnostics_render_actionable_lines_with_navigation_metadata",
        elisp_form,
        expected,
    )
}

fn peek_builds_contextual_xrefs_from_real_source_files() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-lsp-ui-test-with-reset
 (lambda ()
   (let* ((root (file-name-as-directory
                 (file-truename (make-temp-file "lsp-ui-peek-" t))))
          (api (expand-file-name "api.el" root))
          (worker (expand-file-name "worker.el" root)))
     (unwind-protect
         (progn
           (with-temp-file api
             (insert ";; API\n(defun deploy-api ()\n  (message \"ready\"))\n"))
           (with-temp-file worker
             (insert ";; Worker\n(deploy-api)\n(message \"done\")\n"))
           (let* ((api-location
                   (neomacs-lsp-ui-test-location api 1 7 17))
                  (worker-location
                   (neomacs-lsp-ui-test-location worker 1 1 11))
                  (lsp-ui-peek-peek-height 6)
                  (lsp-ui-peek-fontify nil)
                  (groups
                   (cl-letf (((symbol-function 'lsp-request)
                              (lambda (&rest _)
                                (vector worker-location api-location))))
                     (lsp-ui-peek--get-references
                      "textDocument/references" 'request-params)))
                  (items
                   (mapcar
                    (lambda (group)
                      (cons (plist-get group :file)
                            (lsp-ui-peek--get-xrefs-in-file
                             (cons (plist-get group :file)
                                   (plist-get group :xrefs)))))
                    groups)))
             (mapcar
              (lambda (group)
                (list
                 :file (file-relative-name (car group) root)
                 :count (length (cdr group))
                 :items
                 (mapcar
                  (lambda (item)
                    (let ((summary (plist-get item :summary))
                          (chunk (plist-get item :chunk)))
                      (list
                       :summary (substring-no-properties summary)
                       :chunk (substring-no-properties chunk)
                       :line (plist-get item :line)
                       :column (plist-get item :column)
                       :highlight
                       (let ((position
                              (text-property-any
                               0 (length chunk) 'face
                               'lsp-ui-peek-highlight chunk)))
                         (and position
                              (substring-no-properties
                               chunk position
                               (next-single-property-change
                                position 'face chunk (length chunk))))))))
                  (cdr group))))
              items)))
       (neomacs-lsp-ui-test-kill-files-below root)
       (delete-directory root t)))))
"###;
    let expected = expect![[
        r#"OK ((:file "worker.el" :count 1 :items ((:summary "(deploy-api)" :chunk ";; Worker\n(deploy-api)\n(message \"done\")\n" :line 1 :column 1 :highlight "deploy-api"))) (:file "api.el" :count 1 :items ((:summary "(defun deploy-api ()" :chunk ";; API\n(defun deploy-api ()\n  (message \"ready\"))\n" :line 1 :column 7 :highlight "deploy-api"))))"#
    ]];
    ParityBatchCase::value(
        "peek_builds_contextual_xrefs_from_real_source_files",
        elisp_form,
        expected,
    )
}

fn imenu_refresh_builds_a_navigable_outline_from_real_elisp_definitions() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-lsp-ui-test-with-reset
 (lambda ()
   (with-temp-buffer
     (insert "(defvar deploy-state 'queued)\n\n"
             "(defun deploy-start (release)\n  (setq deploy-state release))\n\n"
             "(defun deploy-stop ()\n  (setq deploy-state 'stopped))\n")
     (emacs-lisp-mode)
     (setq-local imenu-auto-rescan t)
     (let ((lsp-ui-imenu-kind-position 'top)
           (lsp-ui-imenu-colors '("blue" "green")))
       (lsp-ui-imenu--refresh-content))
     (with-current-buffer lsp-ui-imenu-buffer-name
       (let ((position (point-min)) lines)
         (while (< position (point-max))
           (let* ((end (save-excursion
                         (goto-char position)
                         (line-end-position)))
                  (marker (get-text-property position 'marker)))
             (push
              (list
               :text (buffer-substring-no-properties position end)
               :index (get-text-property position 'index)
               :title (get-text-property position 'title)
               :depth (get-text-property position 'depth)
               :target (if (markerp marker)
                           (marker-position marker)
                         marker))
              lines)
             (setq position (min (point-max) (1+ end)))))
         (list
          :mode major-mode
          :lines (nreverse lines)
          :section-headings
          (mapcar
           (lambda (overlay)
             (substring-no-properties
              (or (overlay-get overlay 'after-string) "")))
           (sort (overlays-in (point-min) (point-max))
                 (lambda (left right)
                   (or (< (overlay-start left) (overlay-start right))
                       (and (= (overlay-start left) (overlay-start right))
                            (< (or (overlay-get left 'priority) 0)
                               (or (overlay-get right 'priority) 0)))))))))))))
"###;
    let expected = expect![[
        r#"OK (:mode lsp-ui-imenu-mode :lines ((:text "  ┃ deploy-state" :index 0 :title "Variables" :depth 1 :target 1) (:text "  ┃ deploy-start" :index 0 :title "" :depth 1 :target 32) (:text "  ┃ deploy-stop" :index 1 :title "" :depth 1 :target 94)) :section-headings ("\nVariables\n\n" "\n"))"#
    ]];
    ParityBatchCase::value(
        "imenu_refresh_builds_a_navigable_outline_from_real_elisp_definitions",
        elisp_form,
        expected,
    )
}

fn mode_lifecycle_installs_component_hooks_idempotently_and_removes_them() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-lsp-ui-test-with-reset
 (lambda ()
   (with-temp-buffer
     (let ((lsp-ui-peek-enable t)
           (lsp-ui-sideline-enable t)
           (lsp-ui-doc-enable t)
           (lsp-ui-imenu-enable nil)
           (lsp-ui-doc-show-with-mouse nil)
           (lsp-ui-doc-show-with-cursor t)
           (lsp-ui-sideline-show-diagnostics t))
       (unwind-protect
           (progn
             (lsp-ui-mode 1)
             (lsp-ui-mode 1)
             (let ((enabled
                    (list
                     :ui lsp-ui-mode
                     :sideline lsp-ui-sideline-mode
                     :doc lsp-ui-doc-mode
                     :post-command-sideline
                     (cl-count #'lsp-ui-sideline post-command-hook :test #'eq)
                     :post-command-doc
                     (cl-count #'lsp-ui-doc--make-request
                               post-command-hook :test #'eq)
                     :diagnostics
                     (and (memq #'lsp-ui-sideline--diagnostics-changed
                                flycheck-after-syntax-check-hook)
                          t)
                     :revert
                     (and (memq #'lsp-ui-sideline--delete-ov
                                before-revert-hook)
                          t))))
               (lsp-ui-mode -1)
               (list
                :enabled enabled
                :disabled
                (list
                 :ui lsp-ui-mode
                 :sideline lsp-ui-sideline-mode
                 :doc lsp-ui-doc-mode
                 :post-command-sideline
                 (memq #'lsp-ui-sideline post-command-hook)
                 :post-command-doc
                 (memq #'lsp-ui-doc--make-request post-command-hook)
                 :diagnostics
                 (memq #'lsp-ui-sideline--diagnostics-changed
                       flycheck-after-syntax-check-hook)
                 :revert
                 (memq #'lsp-ui-sideline--delete-ov before-revert-hook)))))
         (lsp-ui-mode -1))))))
"###;
    let expected = expect![[
        r#"OK (:enabled (:ui t :sideline t :doc t :post-command-sideline 1 :post-command-doc 1 :diagnostics t :revert t) :disabled (:ui nil :sideline nil :doc nil :post-command-sideline nil :post-command-doc nil :diagnostics nil :revert nil))"#
    ]];
    ParityBatchCase::value(
        "mode_lifecycle_installs_component_hooks_idempotently_and_removes_them",
        elisp_form,
        expected,
    )
    .fresh_process()
}

#[test]
fn lsp_ui_package_batch() {
    let cases = vec![
        reference_navigation_sorts_server_locations_and_visits_adjacent_uses(),
        hover_content_preserves_plain_markdown_and_optional_language_signatures(),
        rendered_hover_compacts_blank_lines_and_replaces_markdown_rules(),
        inline_hover_merges_visible_source_with_a_fixed_width_document_panel(),
        sideline_hover_places_overlays_and_tracks_the_symbol_at_point(),
        workspace_diagnostics_render_actionable_lines_with_navigation_metadata(),
        peek_builds_contextual_xrefs_from_real_source_files(),
        imenu_refresh_builds_a_navigable_outline_from_real_elisp_definitions(),
        mode_lifecycle_installs_component_hooks_idempotently_and_removes_them(),
    ];
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(LSP_UI_MELPA_PIN, "lsp-ui.el")
            .expect("prepare revision-pinned LSP UI source below ./tmp")
            .with_prelude(LSP_UI_TEST_PRELUDE)
            .with_timeout(LSP_UI_TEST_TIMEOUT),
        "lsp-ui-package-batch",
        "LSP UI",
        &cases,
    );
}
