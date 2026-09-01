use expect_test::expect;

use super::ParityBatchCase;

fn package_registers_the_perspectives_scope_type() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_registers_the_perspectives_scope_type",
        r####"
(list :registered
      (alist-get 'Perspectives treemacs-scope-types)
      :valid-types
      (sort (mapcar (lambda (entry) (symbol-name (car entry)))
                    treemacs-scope-types)
            #'string<)
      :feature (featurep 'treemacs-persp))
"####,
        expect![[
            r#"OK (:registered treemacs-persp-scope :valid-types ("Frames" "Perspectives") :feature t)"#
        ]],
    )
}

fn scope_setup_installs_persp_hooks_and_reports_inactive_scope() -> ParityBatchCase {
    ParityBatchCase::value(
        "scope_setup_installs_persp_hooks_and_reports_inactive_scope",
        r####"
(neomacs-treemacs-persp-test-run
 "hooks"
 (lambda (_root)
   (list :scope-type (treemacs-current-scope-type)
         :scope (treemacs-scope->current-scope 'treemacs-persp-scope)
         :scope-name
         (treemacs-scope->current-scope-name 'treemacs-persp-scope 'none)
         :hooks (neomacs-treemacs-persp-test-hooks)
         :workspace (treemacs-workspace->name (treemacs-current-workspace))
         :workspaces (neomacs-treemacs-persp-test-workspace-names))))
"####,
        expect![[
            r#"OK (:scope-type treemacs-persp-scope :scope none :scope-name "No Perspective" :hooks (:activated t :renamed t :before-kill t) :workspace "No Perspective" :workspaces ("Default" "No Perspective"))"#
        ]],
    )
}

fn switching_perspectives_creates_and_selects_matching_workspaces() -> ParityBatchCase {
    ParityBatchCase::value(
        "switching_perspectives_creates_and_selects_matching_workspaces",
        r####"
(neomacs-treemacs-persp-test-run
 "switch"
 (lambda (root)
   (let (after-first after-second)
     (persp-switch "deployment")
     (setq after-first
           (list :persp (safe-persp-name (get-current-persp))
                 :scope-name
                 (treemacs-scope->current-scope-name
                  'treemacs-persp-scope
                  (treemacs-scope->current-scope 'treemacs-persp-scope))
                 :workspace
                 (treemacs-workspace->name (treemacs-current-workspace))
                 :projects
                 (mapcar #'treemacs-project->path
                         (treemacs-workspace->projects
                          (treemacs-current-workspace)))))
     (persp-switch "operations")
     (setq after-second
           (list :persp (safe-persp-name (get-current-persp))
                 :workspace
                 (treemacs-workspace->name (treemacs-current-workspace))
                 :projects
                 (mapcar (lambda (project)
                           (list (treemacs-project->name project)
                                 (treemacs-project->path project)))
                         (treemacs-workspace->projects
                          (treemacs-current-workspace)))))
     (list :first after-first
           :second after-second
           :workspaces (neomacs-treemacs-persp-test-workspace-names)
           :root-basename (file-name-nondirectory (directory-file-name root))))))
"####,
        expect![[
            r#"OK (:first (:persp "deployment" :scope-name "Perspective deployment" :workspace "Perspective deployment" :projects ("[ORACLE-SANDBOX]/treemacs-persp-switch/")) :second (:persp "operations" :workspace "Perspective operations" :projects (("treemacs-persp-switch" "[ORACLE-SANDBOX]/treemacs-persp-switch/"))) :workspaces ("Default" "No Perspective" "Perspective deployment" "Perspective operations") :root-basename "treemacs-persp-switch")"#
        ]],
    )
}

fn renaming_a_perspective_renames_its_treemacs_workspace() -> ParityBatchCase {
    ParityBatchCase::value(
        "renaming_a_perspective_renames_its_treemacs_workspace",
        r####"
(neomacs-treemacs-persp-test-run
 "rename"
 (lambda (_root)
   (persp-switch "canary")
   (let* ((before (treemacs-workspace->name (treemacs-current-workspace)))
          (persp (get-current-persp))
          (renamed (progn
                     (persp-rename "release" persp)
                     (treemacs-workspace->name (treemacs-current-workspace)))))
     (list :before before
           :after renamed
           :persp (safe-persp-name (get-current-persp))
           :workspaces (neomacs-treemacs-persp-test-workspace-names)))))
"####,
        expect![[
            r#"OK (:before "Perspective canary" :after "Perspective release" :persp "release" :workspaces ("Default" "No Perspective" "Perspective release"))"#
        ]],
    )
}

fn cleanup_removes_scope_hooks_when_leaving_perspectives() -> ParityBatchCase {
    ParityBatchCase::value(
        "cleanup_removes_scope_hooks_when_leaving_perspectives",
        r####"
(neomacs-treemacs-persp-test-run
 "cleanup"
 (lambda (_root)
   (let ((during (neomacs-treemacs-persp-test-hooks)))
     (treemacs-set-scope-type 'Frames)
     (list :during during
           :after (neomacs-treemacs-persp-test-hooks)
           :scope-type (treemacs-current-scope-type)))))
"####,
        expect![
            "OK (:during (:activated t :renamed t :before-kill t) :after (:activated nil :renamed nil :before-kill nil) :scope-type treemacs-frame-scope)"
        ],
    )
}

fn invalid_scope_type_reports_a_user_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "invalid_scope_type_reports_a_user_error",
        r####"
(condition-case err
    (list :value (treemacs-set-scope-type 'NotARealScope))
  (error (list :signal (car err)
               :message (error-message-string err))))
"####,
        expect![[
            r#"OK (:signal user-error :message "’NotARealScope’ is not a valid scope new-scope-type.  Valid types are: (Perspectives Frames)")"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        package_registers_the_perspectives_scope_type(),
        scope_setup_installs_persp_hooks_and_reports_inactive_scope(),
        switching_perspectives_creates_and_selects_matching_workspaces(),
        renaming_a_perspective_renames_its_treemacs_workspace(),
        cleanup_removes_scope_hooks_when_leaving_perspectives(),
        invalid_scope_type_reports_a_user_error(),
    ]
}
