use expect_test::expect;

use super::ParityBatchCase;

fn records_updates_and_renders_a_release_inventory_with_audit_history() -> ParityBatchCase {
    ParityBatchCase::value(
        "records_updates_and_renders_a_release_inventory_with_audit_history",
        r##"(let ((annalist--tomes nil)
      (annalist--local-tomes nil)
      (annalist--tomes-settings nil)
      (annalist--tomes-views (make-hash-table :test #'equal)))
  (annalist-define-tome
   'deployments
   (list
    :primary-key '(environment service)
    :table-start-index 0
    :preprocess
    (lambda (record _settings)
      (let ((copy (copy-sequence record)))
        (setf (nth 0 copy) (downcase (nth 0 copy))
              (nth 1 copy) (downcase (nth 1 copy)))
        copy))
    :record-update
    (lambda (old-record new-record settings)
      (let* ((copy (copy-sequence new-record))
             (metadata-index (plist-get settings :metadata-index))
             (old-metadata
              (and old-record (nth metadata-index old-record)))
             (history
              (copy-sequence (plist-get old-metadata :status-history))))
        (when old-record
          (setq history (append history (list (nth 3 old-record)))))
        (setf
         (nth metadata-index copy)
         (list
          :status-history history
          :revision (1+ (or (plist-get old-metadata :revision) 0))))
        copy))
    'environment 'service 'version 'status 'owner))
  (annalist-define-view
   'deployments
   'default
   (list
    :sort
    (lambda (left right)
      (string<
       (format "%s/%s" (nth 0 left) (nth 1 left))
       (format "%s/%s" (nth 0 right) (nth 1 right))))
    '(environment :title "Environment")
    '(service :title "Service")
    '(version :title "Version")
    '(status :title "Status")
    '(owner :title "Owner")))
  (dolist
      (record
       '(("PRODUCTION" "API" "4.0.0" "deploying" "platform")
         ("staging" "worker" "4.1.0-rc1" "validating" "runtime")
         ("production" "frontend" "9.2.0" "healthy" "web")))
    (annalist-record 'release-team 'deployments record))
  (annalist-record
   'release-team 'deployments
   '("Production" "Api" "4.0.0" "healthy" "platform"))
  (annalist-record
   'release-team 'deployments
   '("production" "api" "4.0.1" "degraded" "sre"))
  (let ((description-buffer nil))
    (unwind-protect
        (progn
          (annalist-describe 'release-team 'deployments)
          (setq description-buffer
                (get-buffer "*release-team deployments*"))
          (let* ((records
                  (gethash
                   'release-team
                   (annalist--tome 'deployments)))
                 (api-record
                  (gethash '("production" "api") records)))
            (with-current-buffer description-buffer
              (list
               (hash-table-count records)
               api-record
               major-mode
               buffer-read-only
               (buffer-substring-no-properties (point-min) (point-max))))))
      (when (buffer-live-p description-buffer)
        (kill-buffer description-buffer)))))"##,
        expect![[
            r#"OK (3 ("production" "api" "4.0.1" "degraded" "sre" (:status-history ("deploying" "healthy") :revision 3)) org-mode t "| Environment | Service  |   Version | Status     | Owner   |\n|-------------+----------+-----------+------------+---------|\n| production  | api      |     4.0.1 | degraded   | sre     |\n| production  | frontend |     9.2.0 | healthy    | web     |\n| staging     | worker   | 4.1.0-rc1 | validating | runtime |\n")"#
        ]],
    )
}

fn builds_a_filtered_nested_incident_runbook_with_extracted_elisp_actions() -> ParityBatchCase {
    ParityBatchCase::value(
        "builds_a_filtered_nested_incident_runbook_with_extracted_elisp_actions",
        r##"(let ((annalist--tomes nil)
      (annalist--local-tomes nil)
      (annalist--tomes-settings nil)
      (annalist--tomes-views (make-hash-table :test #'equal)))
  (annalist-define-tome
   'incidents
   '(:primary-key (environment service)
     :table-start-index 2
     environment service status responder runbook))
  (annalist-define-view
   'incidents
   'default
   (list
    '(environment :title "Environment")
    '(service :title "Service")
    '(status :title "Status")
    '(responder :title "Responder")
    (list
     'runbook
     :title "Recovery action"
     :max-width 12
     :extractp #'listp
     :src-block-p #'listp)))
  (annalist-define-view
   'incidents
   'active-runbook
   (list
    :predicate
    (lambda (record)
      (not (string= (nth 2 record) "healthy")))
    (list
     'environment
     :predicate
     (lambda (environment)
       (member environment '("production" "staging")))
     :prioritize '("production")
     :sort #'string<)
    (list 'service :sort #'string<)
    'status
    'responder
    'runbook)
   :inherit 'default)
  (dolist
      (record
       '(("production" "api" "degraded" "alice"
          (progn
            (rollback 'api "4.0.0")
            (notify-on-call 'platform)))
         ("production" "frontend" "healthy" "bob"
          (message "No action"))
         ("staging" "worker" "validating" "carol"
          (progn
            (inspect-queue 'worker)
            (resume-deployment 'worker)))
         ("development" "search" "degraded" "dave"
          (restart-service 'search))))
    (annalist-record 'operations 'incidents record))
  (let ((description-buffer nil))
    (unwind-protect
        (progn
          (annalist-describe 'operations 'incidents 'active-runbook)
          (setq description-buffer (get-buffer "*operations incidents*"))
          (with-current-buffer description-buffer
            (list
             major-mode
             buffer-read-only
             (buffer-substring-no-properties (point-min) (point-max)))))
      (when (buffer-live-p description-buffer)
        (kill-buffer description-buffer)))))"##,
        expect![[
            r#"OK (org-mode t "* production\n** api\n| Status   | Responder | Recovery action |\n|----------+-----------+-----------------|\n| degraded | alice     | [fn:1]          |\n\n[fn:1]\n#+begin_src emacs-lisp\n(progn (rollback 'api 4.0.0) (notify-on-call 'platform))\n#+end_src\n\n** frontend\n| Status | Responder | Recovery action |\n|--------+-----------+-----------------|\n\n* staging\n** worker\n| Status     | Responder | Recovery action |\n|------------+-----------+-----------------|\n| validating | carol     | [fn:2]          |\n\n[fn:2]\n#+begin_src emacs-lisp\n(progn (inspect-queue 'worker) (resume-deployment 'worker))\n#+end_src\n")"#
        ]],
    )
}

fn audits_live_keybinding_changes_with_the_builtin_valid_view() -> ParityBatchCase {
    ParityBatchCase::value(
        "audits_live_keybinding_changes_with_the_builtin_valid_view",
        r##"(let ((annalist--tomes nil)
      (annalist--local-tomes nil))
  (defvar annalist-review-map (make-sparse-keymap))
  (setq annalist-review-map (make-sparse-keymap))
  (let ((deploy-key (kbd "C-c d"))
        (rollback-key (kbd "C-c r"))
        (rollback-command
         (lambda ()
           (interactive)
           (message "Rollback the current deployment"))))
    (define-key annalist-review-map deploy-key #'delete-region)
    (define-key annalist-review-map rollback-key #'replace-string)
    (annalist-record
     'team-config
     'keybindings
     (list
      'annalist-review-map nil deploy-key #'project-compile nil))
    (define-key annalist-review-map deploy-key #'project-compile)
    (annalist-record
     'team-config
     'keybindings
     (list
      'annalist-review-map nil deploy-key #'recompile nil))
    (define-key annalist-review-map deploy-key #'recompile)
    (annalist-record
     'team-config
     'keybindings
     (list
      'annalist-review-map nil rollback-key
      rollback-command
      nil))
    (define-key annalist-review-map rollback-key rollback-command)
    (let ((description-buffer nil))
      (unwind-protect
          (progn
            (annalist-describe 'team-config 'keybindings 'valid)
            (setq description-buffer
                  (get-buffer "*team-config keybindings*"))
            (with-current-buffer description-buffer
              (list
               (lookup-key annalist-review-map deploy-key)
               (commandp (lookup-key annalist-review-map rollback-key))
               major-mode
               buffer-read-only
               (buffer-substring-no-properties (point-min) (point-max)))))
        (when (buffer-live-p description-buffer)
          (kill-buffer description-buffer))))))"##,
        expect![[
            r#"OK (recompile t org-mode t "* ~annalist-review-map~\n| Key     | Definition                                           | Previous          |\n|---------+------------------------------------------------------+-------------------|\n| =C-c d= | ~recompile~                                          | ~project-compile~ |\n| =C-c r= | ~#[nil ((message Rollback the current deployment)) ~ | ~replace-string~  |\n")"#
        ]],
    )
}

fn keeps_project_local_records_out_of_unrelated_description_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "keeps_project_local_records_out_of_unrelated_description_buffers",
        r##"(let ((annalist--tomes nil)
      (annalist--local-tomes nil)
      (annalist--tomes-settings nil)
      (annalist--tomes-views (make-hash-table :test #'equal)))
  (annalist-define-tome
   'tasks
   '(:primary-key (project task)
     :table-start-index 0
     project task status owner))
  (annalist-define-view
   'tasks
   'default
   '((project :title "Project")
     (task :title "Task")
     (status :title "Status")
     (owner :title "Owner")))
  (annalist-record
   'dashboard 'tasks
   '("neomacs" "release" "ready" "platform"))
  (let ((project-buffer (generate-new-buffer " *annalist-neomacs-project*"))
        local-report
        global-report
        description-buffer)
    (unwind-protect
        (progn
          (with-current-buffer project-buffer
            (annalist-record
             'dashboard 'tasks
             '("neomacs" "fix display" "in progress" "alice")
             :local t)
            (annalist-describe 'dashboard 'tasks)
            (setq description-buffer (get-buffer "*dashboard tasks*"))
            (with-current-buffer description-buffer
              (setq local-report
                    (buffer-substring-no-properties (point-min) (point-max)))))
          (with-temp-buffer
            (annalist-describe 'dashboard 'tasks)
            (setq description-buffer (get-buffer "*dashboard tasks*"))
            (with-current-buffer description-buffer
              (setq global-report
                    (buffer-substring-no-properties (point-min) (point-max)))))
          (list local-report global-report))
      (when (buffer-live-p project-buffer)
        (kill-buffer project-buffer))
      (when (buffer-live-p description-buffer)
        (kill-buffer description-buffer)))))"##,
        expect![[
            r#"OK ("* Local\n| Project | Task        | Status      | Owner |\n|---------+-------------+-------------+-------|\n| neomacs | fix display | in progress | alice |\n\n* Global\n| Project | Task    | Status | Owner    |\n|---------+---------+--------+----------|\n| neomacs | release | ready  | platform |\n" "| Project | Task    | Status | Owner    |\n|---------+---------+--------+----------|\n| neomacs | release | ready  | platform |\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        records_updates_and_renders_a_release_inventory_with_audit_history(),
        builds_a_filtered_nested_incident_runbook_with_extracted_elisp_actions(),
        audits_live_keybinding_changes_with_the_builtin_valid_view(),
        keeps_project_local_records_out_of_unrelated_description_buffers(),
    ]
}
