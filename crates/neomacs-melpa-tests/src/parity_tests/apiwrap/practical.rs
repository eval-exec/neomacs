use expect_test::expect;

use super::ParityBatchCase;

fn generated_repository_client_runs_a_complete_issue_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "generated_repository_client_runs_a_complete_issue_lifecycle",
        r##"(progn
  (defvar aw-lifecycle-requests nil)
  (defun aw-lifecycle-request (method resource params data)
    (push
     (list
      :method method
      :resource resource
      :params params
      :data data)
     aw-lifecycle-requests)
    (list
     :status
     (pcase method
       ('post 201)
       ('delete 204)
       (_ 200))
     :method method
     :resource resource))
  (apiwrap-new-backend
      "Forge" "aw-lifecycle"
      '((repo . "REPO is a repository returned by the Forge API.")
        (issue . "ISSUE is an issue returned by the Forge API."))
    :request #'aw-lifecycle-request)
  (defapiget-aw-lifecycle "/repos/:owner/:repo/issues"
    "List repository issues."
    "issues/list"
    (repo)
    "/repos/:repo.owner.login/:repo.name/issues")
  (defapipost-aw-lifecycle "/repos/:owner/:repo/issues"
    "Create a repository issue."
    "issues/create"
    (repo)
    "/repos/:repo.owner.login/:repo.name/issues")
  (defapihead-aw-lifecycle "/repos/:owner/:repo/issues/:number"
    "Check whether an issue exists."
    "issues/head"
    (repo issue)
    "/repos/:repo.owner.login/:repo.name/issues/:issue.number")
  (defapipatch-aw-lifecycle "/repos/:owner/:repo/issues/:number"
    "Update a repository issue."
    "issues/update"
    (repo issue)
    "/repos/:repo.owner.login/:repo.name/issues/:issue.number")
  (defapiput-aw-lifecycle "/repos/:owner/:repo/issues/:number/lock"
    "Lock a repository issue."
    "issues/lock"
    (repo issue)
    "/repos/:repo.owner.login/:repo.name/issues/:issue.number/lock")
  (defapidelete-aw-lifecycle "/repos/:owner/:repo/issues/:number/lock"
    "Unlock a repository issue."
    "issues/unlock"
    (repo issue)
    "/repos/:repo.owner.login/:repo.name/issues/:issue.number/lock")
  (let* ((repo
          '((owner (login . "GNU Project"))
            (name . "neomacs core")))
         (issue '((number . 163)))
         (responses
          (list
           :list
           (aw-lifecycle-get-repos-owner-repo-issues
            repo :state "open" :labels '("bug" "help wanted") :page 2)
           :create
           (aw-lifecycle-post-repos-owner-repo-issues
            repo
            '((title . "Parity failure")
              (body . "Reproduce on GNU and Neomacs")
              (labels . ["compatibility" "elisp"]))
            :notify "maintainers")
           :exists
           (aw-lifecycle-head-repos-owner-repo-issues-number
            repo issue :cache-control "no-cache")
           :update
           (aw-lifecycle-patch-repos-owner-repo-issues-number
            repo issue
            '((state . "closed")
              (milestone . 7)))
           :lock
           (aw-lifecycle-put-repos-owner-repo-issues-number-lock
            repo issue
            '((lock-reason . "resolved")))
           :unlock
           (aw-lifecycle-delete-repos-owner-repo-issues-number-lock
            repo issue))))
    (list
     :responses responses
     :requests (nreverse aw-lifecycle-requests))))"##,
        expect![[
            r#"OK (:responses (:list (:status 200 :method get :resource "/repos/GNU%20Project/neomacs%20core/issues") :create (:status 201 :method post :resource "/repos/GNU%20Project/neomacs%20core/issues") :exists (:status 200 :method head :resource "/repos/GNU%20Project/neomacs%20core/issues/163") :update (:status 200 :method patch :resource "/repos/GNU%20Project/neomacs%20core/issues/163") :lock (:status 200 :method put :resource "/repos/GNU%20Project/neomacs%20core/issues/163/lock") :unlock (:status 204 :method delete :resource "/repos/GNU%20Project/neomacs%20core/issues/163/lock")) :requests ((:method get :resource "/repos/GNU%20Project/neomacs%20core/issues" :params (:state "open" :labels ("bug" "help wanted") :page 2) :data nil) (:method post :resource "/repos/GNU%20Project/neomacs%20core/issues" :params (:notify "maintainers") :data ((title . "Parity failure") (body . "Reproduce on GNU and Neomacs") (labels . ["compatibility" "elisp"]))) (:method head :resource "/repos/GNU%20Project/neomacs%20core/issues/163" :params (:cache-control "no-cache") :data nil) (:method patch :resource "/repos/GNU%20Project/neomacs%20core/issues/163" :params nil :data ((state . "closed") (milestone . 7))) (:method put :resource "/repos/GNU%20Project/neomacs%20core/issues/163/lock" :params nil :data ((lock-reason . "resolved"))) (:method delete :resource "/repos/GNU%20Project/neomacs%20core/issues/163/lock" :params nil :data nil)))"#
        ]],
    )
}

fn backend_policies_transform_requests_and_allow_endpoint_specific_overrides() -> ParityBatchCase {
    ParityBatchCase::value(
        "backend_policies_transform_requests_and_allow_endpoint_specific_overrides",
        r##"(progn
  (defvar aw-policy-events nil)
  (defun aw-policy-default-params (params)
    (push
     (list :phase 'default-params :input params)
     aw-policy-events)
    (let (result)
      (while params
        (push
         (cons
          (intern
           (substring
            (symbol-name (car params))
            1))
          (cadr params))
         result)
        (setq params (cddr params)))
      (nreverse result)))
  (defun aw-policy-retry-params (params)
    (push
     (list :phase 'retry-params :input params)
     aw-policy-events)
    (list
     (cons 'retry-count (plist-get params :retry-count))
     (cons 'priority
           (upcase (plist-get params :priority)))))
  (defun aw-policy-data (data)
    (push
     (list :phase 'data :input data)
     aw-policy-events)
    (append data '((client . "parity-suite"))))
  (defmacro aw-policy-around (form)
    `(progn
       (push
        (list :phase 'policy-enter)
        aw-policy-events)
       (unwind-protect
           ,form
         (push
          (list :phase 'policy-leave)
          aw-policy-events))))
  (defun aw-policy-request (method resource params data)
    (push
     (list
      :phase 'transport
      :method method
      :resource resource
      :params params
      :data data)
     aw-policy-events)
    (list
     :accepted t
     :resource resource
     :params params
     :data data))
  (apiwrap-new-backend
      "Deployments" "aw-policy"
      '((project . "PROJECT is a project returned by the service.")
        (deployment . "DEPLOYMENT is an existing deployment."))
    :request #'aw-policy-request
    :pre-process-params #'aw-policy-default-params
    :pre-process-data #'aw-policy-data
    :around #'aw-policy-around)
  (defapipost-aw-policy "/projects/:owner/:project/deployments"
    "Create a deployment."
    "deployments/create"
    (project)
    "/projects/:project.owner/:project.slug/deployments")
  (defapipost-aw-policy "/projects/:owner/:project/deployments/:id/retry"
    "Retry a failed deployment."
    "deployments/retry"
    (project deployment)
    "/projects/:project.owner/:project.slug/deployments/:deployment.id/retry"
    :pre-process-params #'aw-policy-retry-params
    :pre-process-data nil)
  (let* ((project
          '((owner . "GNU Team")
            (slug . "neomacs runner")))
         (deployment '((id . 42)))
         (created
          (aw-policy-post-projects-owner-project-deployments
           project
           '((revision . "abc123")
             (environment . "staging"))
           :dry-run nil
           :labels '("compatibility" "nightly")))
         (retried
          (aw-policy-post-projects-owner-project-deployments-id-retry
           project
           deployment
           '((reason . "transient network failure"))
           :retry-count 3
           :priority "urgent")))
    (list
     :created created
     :retried retried
     :events (nreverse aw-policy-events))))"##,
        expect![[
            r#"OK (:created (:accepted t :resource "/projects/GNU%20Team/neomacs%20runner/deployments" :params #4=((dry-run) (labels . #1=("compatibility" "nightly"))) :data #5=(#2=(revision . "abc123") #3=(environment . "staging") (client . "parity-suite"))) :retried (:accepted t :resource "/projects/GNU%20Team/neomacs%20runner/deployments/42/retry" :params #6=((retry-count . 3) (priority . "URGENT")) :data #7=((reason . "transient network failure"))) :events ((:phase policy-enter) (:phase default-params :input (:dry-run nil :labels #1#)) (:phase data :input (#2# #3#)) (:phase transport :method post :resource "/projects/GNU%20Team/neomacs%20runner/deployments" :params #4# :data #5#) (:phase policy-leave) (:phase policy-enter) (:phase retry-params :input (:retry-count 3 :priority "urgent")) (:phase transport :method post :resource "/projects/GNU%20Team/neomacs%20runner/deployments/42/retry" :params #6# :data #7#) (:phase policy-leave)))"#
        ]],
    )
}

fn endpoint_error_policy_recovers_missing_resources_but_propagates_auth_failures() -> ParityBatchCase
{
    ParityBatchCase::value(
        "endpoint_error_policy_recovers_missing_resources_but_propagates_auth_failures",
        r##"(progn
  (define-error 'aw-resource-missing "API resource is missing")
  (define-error 'aw-auth-expired "API authentication expired")
  (defvar aw-errors-behavior 'present)
  (defvar aw-errors-requests nil)
  (defun aw-errors-request (method resource params data)
    (push
     (list
      :method method
      :resource resource
      :params params
      :data data)
     aw-errors-requests)
    (pcase aw-errors-behavior
      ('missing
       (signal
        'aw-resource-missing
        (list
         :status 404
         :resource resource
         :request-id "request-404")))
      ('unauthorized
       (signal
        'aw-auth-expired
        (list
         :status 401
         :scope "repository:read"
         :request-id "request-401")))
      (_
       (list
        :status 200
        :repository
        '((owner . "GNU Project")
          (name . "neomacs core"))
        :resource resource))))
  (apiwrap-new-backend
      "Forge" "aw-errors"
      '((repo . "REPO identifies the requested repository."))
    :request #'aw-errors-request)
  (defapiget-aw-errors "/repos/:owner/:repo"
    "Fetch one repository."
    "repositories/get"
    (repo)
    "/repos/:repo.owner/:repo.name"
    :condition-case
    ((aw-resource-missing
      (list
       :status 'not-found
       :details (cdr it)))))
  (let* ((repo
          '((owner . "GNU Project")
            (name . "neomacs core")))
         (aw-errors-behavior 'missing)
         (missing
          (aw-errors-get-repos-owner-repo
           repo
           :include '("owner" "permissions")))
         (aw-errors-behavior 'present)
         (present
          (aw-errors-get-repos-owner-repo
           repo
           :include '("owner" "permissions")))
         (aw-errors-behavior 'unauthorized)
         (unauthorized
          (condition-case error
              (aw-errors-get-repos-owner-repo
               repo
               :include '("owner" "permissions"))
            (error
             (list
              :condition (car error)
              :details (cdr error))))))
    (list
     :missing missing
     :present present
     :unauthorized unauthorized
     :requests (nreverse aw-errors-requests))))"##,
        expect![[
            r#"OK (:missing (:status not-found :details (:status 404 :resource "/repos/GNU%20Project/neomacs%20core" :request-id "request-404")) :present (:status 200 :repository ((owner . "GNU Project") (name . "neomacs core")) :resource "/repos/GNU%20Project/neomacs%20core") :unauthorized (:condition aw-auth-expired :details (:status 401 :scope "repository:read" :request-id "request-401")) :requests ((:method get :resource "/repos/GNU%20Project/neomacs%20core" :params (:include ("owner" "permissions")) :data nil) (:method get :resource "/repos/GNU%20Project/neomacs%20core" :params (:include ("owner" "permissions")) :data nil) (:method get :resource "/repos/GNU%20Project/neomacs%20core" :params (:include ("owner" "permissions")) :data nil)))"#
        ]],
    )
}

fn endpoint_catalog_selects_and_runs_a_wrapper_from_each_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "endpoint_catalog_selects_and_runs_a_wrapper_from_each_backend",
        r##"(progn
  (defvar aw-catalog-requests nil)
  (defun aw-catalog-request (method resource params data)
    (push
     (list
      :method method
      :resource resource
      :params params
     :data data)
     aw-catalog-requests)
    (list
     :source
     (if
         (string-prefix-p "/mirror/" resource)
         'foreign
       'catalog)
     :items
     '(((number . 17) (title . "Fix evaluator parity"))
       ((number . 42) (title . "Stabilize package tests")))
     :resource resource))
  (apiwrap-new-backend
      "Forge" "aw-catalog"
      '((repo . "REPO is the selected repository returned by Forge."))
    :request #'aw-catalog-request)
  (apiwrap-new-backend
      "Foreign Forge" "aw-foreign"
      '((repo . "REPO is a repository from another service."))
    :request #'aw-catalog-request)
  (defapiget-aw-catalog "/repos/:owner/:repo/issues/open"
    "List open issues for the selected repository."
    "issues/open"
    (repo)
    "/repos/:repo.owner/:repo.name/issues/open")
  (defapiget-aw-catalog "/repos/:owner/:repo/issues/closed"
    "List closed issues for the selected repository."
    "issues/closed"
    (repo)
    "/repos/:repo.owner/:repo.name/issues/closed")
  (defapipost-aw-catalog "/repos/:owner/:repo/issues"
    "Create an issue in the selected repository."
    "issues/create"
    (repo)
    "/repos/:repo.owner/:repo.name/issues")
  (defapiget-aw-catalog "/users/:user"
    "Fetch one user."
    "users/get")
  (defapiget-aw-foreign "/repos/:owner/:repo/issues/open"
    "List open issues on the foreign service."
    "foreign/issues/open"
    (repo)
    "/mirror/:repo.owner/:repo.name/issues/open")
  (let* ((catalog-selected
          (caar
           (apropos-api-endpoint
            "aw-catalog"
            "issues-open")))
         (foreign-selected
          (caar
           (apropos-api-endpoint
            "aw-foreign"
            "issues-open")))
         (repo
          '((owner . "GNU Project")
            (name . "neomacs core")))
         (responses
          (list
           :catalog
           (funcall
            catalog-selected repo
            :assignee "alice"
            :labels '("compatibility" "high priority"))
           :foreign
           (funcall
            foreign-selected repo
            :assignee "bob"
            :labels '("mirror" "triage")))))
    (list
     :selected
     (list catalog-selected foreign-selected)
     :responses responses
     :requests (nreverse aw-catalog-requests))))"##,
        expect![[
            r#"OK (:selected (aw-catalog-get-repos-owner-repo-issues-open aw-foreign-get-repos-owner-repo-issues-open) :responses (:catalog (:source catalog :items #1=(((number . 17) (title . "Fix evaluator parity")) ((number . 42) (title . "Stabilize package tests"))) :resource "/repos/GNU%20Project/neomacs%20core/issues/open") :foreign (:source foreign :items #1# :resource "/mirror/GNU%20Project/neomacs%20core/issues/open")) :requests ((:method get :resource "/repos/GNU%20Project/neomacs%20core/issues/open" :params (:assignee "alice" :labels ("compatibility" "high priority")) :data nil) (:method get :resource "/mirror/GNU%20Project/neomacs%20core/issues/open" :params (:assignee "bob" :labels ("mirror" "triage")) :data nil)))"#
        ]],
    )
}

pub(super) fn practical_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        generated_repository_client_runs_a_complete_issue_lifecycle(),
        backend_policies_transform_requests_and_allow_endpoint_specific_overrides(),
        endpoint_error_policy_recovers_missing_resources_but_propagates_auth_failures(),
        endpoint_catalog_selects_and_runs_a_wrapper_from_each_backend(),
    ]
}
