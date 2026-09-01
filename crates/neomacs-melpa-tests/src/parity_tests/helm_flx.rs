use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FLX_MELPA_PIN, HELM_FLX_MELPA_PIN, HELM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'helm)
(require 'helm-files)
(require 'helm-locate)
(require 'flx)
(require 'helm-flx)
(helm-flx-mode -1)

(defun neomacs-helm-flx-test-face-runs (string)
  "Return exact face runs from STRING, including their buffer offsets."
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((face (get-text-property position 'face string))
             (next (next-single-property-change
                    position 'face string (length string))))
        (push (list (substring-no-properties string position next)
                    face position next)
              runs)
        (setq position next)))
    (nreverse runs)))

(defun neomacs-helm-flx-test-snapshot (candidate)
  "Return CANDIDATE's plain display, real value, and exact face runs."
  (let ((display (if (consp candidate) (car candidate) candidate)))
    (list :display (substring-no-properties display)
          :real (and (consp candidate) (cdr candidate))
          :faces (neomacs-helm-flx-test-face-runs display))))
"###;

fn package_registration_and_global_mode_lifecycle_restore_every_helm_seam() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((descriptor (cadr (assq 'helm-flx package-alist)))
       (original-sort helm-fuzzy-sort-fn)
       (original-highlight helm-fuzzy-matching-highlight-fn)
       (original-locate helm-locate-fuzzy-sort-fn)
       enabled disabled)
  (unwind-protect
      (let ((helm-flx-for-helm-find-files t)
            (helm-flx-for-helm-locate t))
        (helm-flx-mode 1)
        (setq enabled
              (list :mode helm-flx-mode
                    :sort (eq helm-fuzzy-sort-fn
                              #'helm-flx-fuzzy-matching-sort)
                    :highlight
                    (eq helm-fuzzy-matching-highlight-fn
                        #'helm-flx-fuzzy-highlight-match)
                    :files-sort-advice
                    (and (advice-member-p #'helm-flx-helm-ff-sort-candidates
                                          'helm-ff-sort-candidates)
                         t)
                    :files-filter-advice
                    (and (advice-member-p
                          #'helm-flx-helm-ff-filter-candidate-one-by-one
                          'helm-ff-filter-candidate-one-by-one)
                         t)
                    :locate
                    (eq helm-locate-fuzzy-sort-fn
                        #'helm-flx-helm-locate-fuzzy-sort-fn)
                    :cache (and (hash-table-p helm-flx-cache) t)))
        (helm-flx-mode -1)
        (setq disabled
              (list :mode helm-flx-mode
                    :sort-restored (eq helm-fuzzy-sort-fn original-sort)
                    :highlight-restored
                    (eq helm-fuzzy-matching-highlight-fn original-highlight)
                    :files-sort-advice
                    (and (advice-member-p #'helm-flx-helm-ff-sort-candidates
                                          'helm-ff-sort-candidates)
                         t)
                    :files-filter-advice
                    (and (advice-member-p
                          #'helm-flx-helm-ff-filter-candidate-one-by-one
                          'helm-ff-filter-candidate-one-by-one)
                         t)
                    :locate-restored
                    (eq helm-locate-fuzzy-sort-fn original-locate))))
    (helm-flx-mode -1))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'helm-flx) t))
   :enabled enabled
   :disabled disabled))
"###;
    let expected = expect![[
        r#"OK (:package (:name helm-flx :version "20221020.1739" :requirements ((emacs (24 4)) (helm (1 7 9)) (flx (0 5))) :feature t) :enabled (:mode t :sort t :highlight t :files-sort-advice t :files-filter-advice t :locate t :cache t) :disabled (:mode nil :sort-restored t :highlight-restored t :files-sort-advice nil :files-filter-advice nil :locate-restored t))"#
    ]];
    ParityBatchCase::value(
        "package_registration_and_global_mode_lifecycle_restore_every_helm_seam",
        elisp_form,
        expected,
    )
}

fn command_palette_ranks_strings_symbols_and_multiword_queries_through_helm() -> ParityBatchCase {
    let elisp_form = r###"
(let ((candidates '(project-find-file
                    project-find-regexp
                    "project-forget-project"
                    "find-file"
                    "find-function"
                    "magit-file-dispatch"
                    "publish-final-release")))
  (list
   :project-file
   (let ((helm-pattern "pff"))
     (helm-flx-fuzzy-matching-sort (copy-sequence candidates) nil))
   :release
   (let ((helm-pattern "pflr"))
     (helm-flx-fuzzy-matching-sort (copy-sequence candidates) nil))
   :multiword
   (let ((helm-pattern "project file"))
     (helm-flx-fuzzy-matching-sort (copy-sequence candidates) nil))
   :empty
   (let ((helm-pattern ""))
     (helm-flx-fuzzy-matching-sort (copy-sequence candidates) nil))))
"###;
    let expected = expect![[
        r#"OK (:project-file ("project-find-file" "find-file" "find-function" "project-find-regexp" "magit-file-dispatch" "publish-final-release" "project-forget-project") :release ("publish-final-release" "find-file" "find-function" "project-find-file" "project-find-regexp" "magit-file-dispatch" "project-forget-project") :multiword ("find-file" "find-function" "project-find-file" "project-find-regexp" "magit-file-dispatch" "publish-final-release" "project-forget-project") :empty ("project-find-file" "project-find-regexp" "project-forget-project" "find-file" "find-function" "magit-file-dispatch" "publish-final-release"))"#
    ]];
    ParityBatchCase::value(
        "command_palette_ranks_strings_symbols_and_multiword_queries_through_helm",
        elisp_form,
        expected,
    )
}

fn annotated_deployment_candidates_can_rank_display_labels_or_real_targets() -> ParityBatchCase {
    let elisp_form = r###"
(let ((candidates
       '(("Deploy production" . "ops/release/deploy-prod.sh")
         ("Open deployment log" . "var/log/releases/current.log")
         ("Rollback release" . "ops/release/rollback-prod.sh")
         ("Publish documentation" . "scripts/publish-docs.sh")
         ("Restart API" . "infra/kubernetes/restart-api.yaml"))))
  (list
   :display
   (let ((helm-pattern "dpl"))
     (helm-flx-fuzzy-matching-sort (copy-tree candidates) nil))
   :real-path
   (let ((helm-pattern "rpd"))
     (helm-flx-fuzzy-matching-sort (copy-tree candidates) nil t))
   :display-path-mismatch
   (let ((helm-pattern "kube"))
     (list
      :display (helm-flx-fuzzy-matching-sort (copy-tree candidates) nil)
      :real (helm-flx-fuzzy-matching-sort (copy-tree candidates) nil t)))))
"###;
    let expected = expect![[
        r#"OK (:display (("Open deployment log" . "var/log/releases/current.log") ("Deploy production" . "ops/release/deploy-prod.sh") ("Restart API" . "infra/kubernetes/restart-api.yaml") ("Rollback release" . "ops/release/rollback-prod.sh") ("Publish documentation" . "scripts/publish-docs.sh")) :real-path (("Rollback release" . "ops/release/rollback-prod.sh") ("Publish documentation" . "scripts/publish-docs.sh") ("Deploy production" . "ops/release/deploy-prod.sh") ("Open deployment log" . "var/log/releases/current.log") ("Restart API" . "infra/kubernetes/restart-api.yaml")) :display-path-mismatch (:display (("Restart API" . "infra/kubernetes/restart-api.yaml") ("Rollback release" . "ops/release/rollback-prod.sh") ("Deploy production" . "ops/release/deploy-prod.sh") ("Open deployment log" . "var/log/releases/current.log") ("Publish documentation" . "scripts/publish-docs.sh")) :real (("Restart API" . "infra/kubernetes/restart-api.yaml") ("Publish documentation" . "scripts/publish-docs.sh") ("Deploy production" . "ops/release/deploy-prod.sh") ("Open deployment log" . "var/log/releases/current.log") ("Rollback release" . "ops/release/rollback-prod.sh"))))"#
    ]];
    ParityBatchCase::value(
        "annotated_deployment_candidates_can_rank_display_labels_or_real_targets",
        elisp_form,
        expected,
    )
}

fn candidate_limit_presorts_a_large_workspace_while_unlimited_search_finds_the_best_match()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((candidates '("a.el"
                    "bb.el"
                    "ccc.el"
                    "dddd.el"
                    "eeeee.el"
                    "archive/very-long-deploy-production-service.el")))
  (list
   :limited
   (let ((helm-flx-limit 3)
         (helm-pattern "dps"))
     (helm-flx-fuzzy-matching-sort (copy-sequence candidates) nil))
   :unlimited
   (let ((helm-flx-limit nil)
         (helm-pattern "dps"))
     (helm-flx-fuzzy-matching-sort (copy-sequence candidates) nil))
   :boundary
   (let ((helm-flx-limit 6)
         (helm-pattern "dps"))
     (helm-flx-fuzzy-matching-sort (copy-sequence candidates) nil))))
"###;
    let expected = expect![[
        r#"OK (:limited ("a.el" "bb.el" "ccc.el") :unlimited ("archive/very-long-deploy-production-service.el" "a.el" "bb.el" "ccc.el" "dddd.el" "eeeee.el") :boundary ("archive/very-long-deploy-production-service.el" "a.el" "bb.el" "ccc.el" "dddd.el" "eeeee.el"))"#
    ]];
    ParityBatchCase::value(
        "candidate_limit_presorts_a_large_workspace_while_unlimited_search_finds_the_best_match",
        elisp_form,
        expected,
    )
}

fn fuzzy_highlighting_preserves_real_values_and_existing_non_face_metadata() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((display (propertize "Deploy production to eu-west"
                            'help-echo "Run the production deployment"
                            'category 'release-command))
       (candidate (cons display '(:command deploy :region eu-west)))
       (highlighted
        (helm-flx-fuzzy-highlight-match candidate "dpew"))
       (symbol-result
        (helm-flx-fuzzy-highlight-match 'project-find-file "pff"))
       (fallback
        (helm-flx-fuzzy-highlight-match
         (cons "Deploy production to eu-west"
               '(:command deploy :region eu-west))
         "deploy west")))
  (list
   :flx (neomacs-helm-flx-test-snapshot highlighted)
   :metadata
   (list (get-text-property 0 'help-echo (car highlighted))
         (get-text-property 0 'category (car highlighted)))
   :symbol (neomacs-helm-flx-test-snapshot symbol-result)
   :helm-fallback (neomacs-helm-flx-test-snapshot fallback)))
"###;
    let expected = expect![[
        r#"OK (:flx (:display "Deploy production to eu-west" :real (:command deploy :region eu-west) :faces (("D" helm-match 0 1) ("eploy " nil 1 7) ("p" helm-match 7 8) ("roduction to " nil 8 21) ("e" helm-match 21 22) ("u-" nil 22 24) ("w" helm-match 24 25) ("est" nil 25 28))) :metadata ("Run the production deployment" release-command) :symbol (:display "project-find-file" :real nil :faces (("p" helm-match 0 1) ("roject-" nil 1 8) ("f" helm-match 8 9) ("ind-" nil 9 13) ("f" helm-match 13 14) ("ile" nil 14 17))) :helm-fallback (:display "Deploy production to eu-west" :real (:command deploy :region eu-west) :faces (("Deploy" helm-match 0 6) (" production to eu-" nil 6 24) ("west" helm-match 24 28))))"#
    ]];
    ParityBatchCase::value(
        "fuzzy_highlighting_preserves_real_values_and_existing_non_face_metadata",
        elisp_form,
        expected,
    )
}

fn find_files_sort_prioritizes_new_paths_and_delegates_multiword_search() -> ParityBatchCase {
    let elisp_form = r###"
(let ((candidates
       '(("Deploy production" . "/workspace/ops/deploy-production.sh")
         ("Deployment status" . "/workspace/docs/production-status.md")
         ("Restart service" . "/workspace/infra/restart-service.yaml")
         ("Create requested path" . "[?] /workspace/dps")))
      delegated)
  (list
   :single
   (let ((helm-input "/workspace/dps")
         (helm-pattern "dps"))
     (helm-flx-helm-ff-sort-candidates
      (lambda (&rest _) (error "single-token search delegated"))
      (copy-tree candidates)
      'release-files))
   :directory
   (let ((helm-input "/workspace/")
         (helm-pattern "workspace"))
     (helm-flx-helm-ff-sort-candidates
      (lambda (&rest _) (error "directory search delegated"))
      (copy-tree candidates)
      'release-files))
   :multiword
   (let ((helm-input "/workspace/deploy production")
         (helm-pattern "deploy production"))
     (helm-flx-helm-ff-sort-candidates
      (lambda (received source)
        (setq delegated (list :source source :count (length received)))
        '(:helm-default-result))
      (copy-tree candidates)
      'release-files))
   :delegated delegated))
"###;
    let expected = expect![[
        r#"OK (:single (("Create requested path" . "[?] /workspace/dps") ("Deployment status" . "/workspace/docs/production-status.md") ("Deploy production" . "/workspace/ops/deploy-production.sh") ("Restart service" . "/workspace/infra/restart-service.yaml")) :directory (("Deploy production" . "/workspace/ops/deploy-production.sh") ("Deployment status" . "/workspace/docs/production-status.md") ("Restart service" . "/workspace/infra/restart-service.yaml") ("Create requested path" . "[?] /workspace/dps")) :multiword (:helm-default-result) :delegated (:source release-files :count 4))"#
    ]];
    ParityBatchCase::value(
        "find_files_sort_prioritizes_new_paths_and_delegates_multiword_search",
        elisp_form,
        expected,
    )
}

fn find_files_filter_highlights_only_the_basename_and_preserves_directory_properties()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((path "/workspace/src/deploy-production.el")
       (make-candidate
        (lambda ()
          (cons (propertize path
                            'face 'helm-ff-directory
                            'help-echo "Repository file")
                path)))
       (highlighted
        (let ((helm-input "/workspace/src/dpe"))
          (helm-flx-helm-ff-filter-candidate-one-by-one
           (lambda (&rest _) (funcall make-candidate)))))
       (directory-input
        (let ((helm-input "/workspace/src/"))
          (helm-flx-helm-ff-filter-candidate-one-by-one
           (lambda (&rest _) (funcall make-candidate)))))
       (new-candidate
        (let ((helm-input "/workspace/src/dpe"))
          (helm-flx-helm-ff-filter-candidate-one-by-one
           (lambda (&rest _)
             (cons " [?] /workspace/src/dpe" "[?] /workspace/src/dpe"))))))
  (list
   :highlighted (neomacs-helm-flx-test-snapshot highlighted)
   :directory-help (get-text-property 0 'help-echo (car highlighted))
   :basename-help
   (get-text-property (length "/workspace/src/")
                      'help-echo (car highlighted))
   :directory-input (neomacs-helm-flx-test-snapshot directory-input)
   :new-candidate (neomacs-helm-flx-test-snapshot new-candidate)))
"###;
    let expected = expect![[
        r#"OK (:highlighted (:display "/workspace/src/deploy-production.el" :real "/workspace/src/deploy-production.el" :faces (("/workspace/src/" helm-ff-directory 0 15) ("d" helm-match 15 16) ("eploy-" helm-ff-directory 16 22) ("p" helm-match 22 23) ("roduction." helm-ff-directory 23 33) ("e" helm-match 33 34) ("l" helm-ff-directory 34 35))) :directory-help "Repository file" :basename-help "Repository file" :directory-input (:display "/workspace/src/deploy-production.el" :real "/workspace/src/deploy-production.el" :faces (("/workspace/src/deploy-production.el" helm-ff-directory 0 35))) :new-candidate (:display " [?] /workspace/src/dpe" :real "[?] /workspace/src/dpe" :faces ((" [?] /workspace/src/dpe" nil 0 23))))"#
    ]];
    ParityBatchCase::value(
        "find_files_filter_highlights_only_the_basename_and_preserves_directory_properties",
        elisp_form,
        expected,
    )
}

fn enabled_helm_pipeline_and_locate_adapter_share_flx_ranking_without_leaking_state()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((candidates '("release-dashboard"
                    "release-deploy-log"
                    "renderer-debug"
                    "remote-development"
                    "README.md"))
      helm-result locate-result highlighted)
  (unwind-protect
      (let ((helm-flx-for-helm-find-files nil)
            (helm-flx-for-helm-locate t))
        (helm-flx-mode 1)
        (let ((helm-pattern "reldl"))
          (setq helm-result
                (funcall helm-fuzzy-sort-fn
                         (copy-sequence candidates) nil))
          (setq highlighted
                (funcall helm-fuzzy-matching-highlight-fn
                         "release-deploy-log" "reldl")))
        (let ((helm-input "reldl"))
          (setq locate-result
                (funcall helm-locate-fuzzy-sort-fn
                         (copy-sequence candidates)))))
    (helm-flx-mode -1))
  (list :helm helm-result
        :locate locate-result
        :same-order (equal helm-result locate-result)
        :highlight (neomacs-helm-flx-test-snapshot highlighted)
        :disabled (not helm-flx-mode)))
"###;
    let expected = expect![[
        r#"OK (:helm ("release-deploy-log" "README.md" "renderer-debug" "release-dashboard" "remote-development") :locate ("release-deploy-log" "README.md" "renderer-debug" "release-dashboard" "remote-development") :same-order t :highlight (:display "release-deploy-log" :real nil :faces (("rel" helm-match 0 3) ("ease-" nil 3 8) ("d" helm-match 8 9) ("eploy-" nil 9 15) ("l" helm-match 15 16) ("og" nil 16 18))) :disabled t)"#
    ]];
    ParityBatchCase::value(
        "enabled_helm_pipeline_and_locate_adapter_share_flx_ranking_without_leaking_state",
        elisp_form,
        expected,
    )
}

#[test]
fn helm_flx_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(HELM_FLX_MELPA_PIN, "helm-flx.el")
            .expect("prepare revision-pinned Helm-Flx source below ./tmp")
            .with_melpa_dependency(FLX_MELPA_PIN)
            .expect("prepare revision-pinned Flx dependency below ./tmp")
            .with_melpa_dependency(HELM_MELPA_PIN)
            .expect("prepare revision-pinned Helm dependency below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "helm-flx-package-batch",
        "Helm-Flx",
        &[
            package_registration_and_global_mode_lifecycle_restore_every_helm_seam(),
            command_palette_ranks_strings_symbols_and_multiword_queries_through_helm(),
            annotated_deployment_candidates_can_rank_display_labels_or_real_targets(),
            candidate_limit_presorts_a_large_workspace_while_unlimited_search_finds_the_best_match(
            ),
            fuzzy_highlighting_preserves_real_values_and_existing_non_face_metadata(),
            find_files_sort_prioritizes_new_paths_and_delegates_multiword_search(),
            find_files_filter_highlights_only_the_basename_and_preserves_directory_properties(),
            enabled_helm_pipeline_and_locate_adapter_share_flx_ranking_without_leaking_state(),
        ],
    );
}
