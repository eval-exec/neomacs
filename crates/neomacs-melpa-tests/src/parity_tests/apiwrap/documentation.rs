use expect_test::expect;

use super::ParityBatchCase;

// These cases sit beside `practical.rs`, which already drives generated
// clients end to end.  They cover what that file does not: the documentation
// apiwrap generates, and two places where the generated result does not match
// what the package's own documentation promises.

/// one, which is what a reader of `C-h f` or `apropos-api-endpoint` sees.
fn the_documented_resource_and_the_internal_one_are_kept_apart() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_documented_resource_and_the_internal_one_are_kept_apart",
        r##"
(progn
  (apiwrap-new-backend
   "MyForge" "mf" '((repo . "A repository object"))
   :request #'apiwrap-test-request)
  (defapiget-mf "/repos/:owner/:repo/issues" "List issues for a repository."
    "issues/#list-issues-for-a-repository"
    (repo) "/repos/:repo.owner.login/:repo.name/issues")
  (let ((repo '((name . "Hallo Welt") (owner (login . "octocat")))))
    (setq apiwrap-test-calls nil)
    (mf-get-repos-owner-repo-issues repo)
    (list :requested-resource (plist-get (car (apiwrap-test-log)) :resource)
          :advertised-endpoint (alist-get 'endpoint
                                          (get 'mf-get-repos-owner-repo-issues 'apiwrap))
          :docstring (apiwrap-test-doc 'mf-get-repos-owner-repo-issues))))
"##,
        expect![[
            r#"OK (:requested-resource "/repos/octocat/Hallo%20Welt/issues" :advertised-endpoint "/repos/:owner/:repo/issues" :docstring "List issues for a repository.\n\nDATA is a data structure to be sent with this request.  If it’s\nnot required, it can simply be omitted.\n\nPARAMS is a plist of parameters appended to the method call.\n\n--------------------\n\nThis generated function wraps the MyForge API endpoint\n\n    GET /repos/:owner/:repo/issues\n\nwhich is documented at\n\n    URL ‘issues/#list-issues-for-a-repository’")"#
        ]],
    )
}

fn the_generated_docstring_never_documents_the_object_parameter() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_generated_docstring_never_documents_the_object_parameter",
        r##"
(progn
  (apiwrap-new-backend
   "KeyProbe" "kp"
   '((repo . "Documentation keyed by the symbol")
     ((repo) . "Documentation keyed by the list"))
   :request #'apiwrap-test-request)
  (defapiget-kp "/a/:name" "Symbol key." "link/a" (repo) "/a/:repo.name")
  (list :docstring (apiwrap-test-doc 'kp-get-a-name)
        :mentions-symbol-keyed-doc
        (and (string-match-p "keyed by the symbol" (apiwrap-test-doc 'kp-get-a-name)) t)
        :mentions-list-keyed-doc
        (and (string-match-p "keyed by the list" (apiwrap-test-doc 'kp-get-a-name)) t)))
"##,
        expect![[
            r#"OK (:docstring "Symbol key.\n\nDATA is a data structure to be sent with this request.  If it’s\nnot required, it can simply be omitted.\n\nPARAMS is a plist of parameters appended to the method call.\n\n--------------------\n\nThis generated function wraps the KeyProbe API endpoint\n\n    GET /a/:name\n\nwhich is documented at\n\n    URL ‘link/a’" :mentions-symbol-keyed-doc nil :mentions-list-keyed-doc nil)"#
        ]],
    )
}

fn configuring_a_condition_case_needs_bytecomp_before_the_hooks_work() -> ParityBatchCase {
    ParityBatchCase::value(
        "configuring_a_condition_case_needs_bytecomp_before_the_hooks_work",
        r##"
(list
 :without-bytecomp
 (progn
   (when (featurep 'bytecomp) (error "bytecomp was already loaded"))
   (apiwrap-test-outcome
    (apiwrap-new-backend
     "EarlyProbe" "ep" '((repo . "A repository"))
     :request #'apiwrap-test-request
     :condition-case '((wrong-type-argument (list :handled (car it)))))))
 :with-bytecomp
 (progn
   (require 'bytecomp)
   (apiwrap-new-backend
    "AroundProbe" "ap" '((repo . "A repository"))
    :request #'apiwrap-test-request
    :around 'apiwrap-test-around
    :condition-case '((wrong-type-argument (list :handled (car it) (cadr it)))))
   (defapiget-ap "/b/:name" "Plain." "link/b" (repo) "/b/:repo.name")
   (defapiget-ap "/c/:name" "Boom." "link/c" (repo) "/c/:repo.name"
     :request #'apiwrap-test-exploding-request)
   (let ((repo '((name . "Hallo Welt"))))
     (setq apiwrap-test-calls nil apiwrap-test-around-log nil)
     (list :wrapped (ap-get-b-name repo)
           :handled (ap-get-c-name repo)
           :around-ran apiwrap-test-around-log
           :calls (apiwrap-test-log)))))
"##,
        expect![[
            r#"OK (:without-bytecomp (:error void-function (byte-compile-warn)) :with-bytecomp (:wrapped (:wrapped (:status 200 :body ((echo . "/b/Hallo%20Welt")))) :handled (:handled wrong-type-argument stringp) :around-ran (:around-ran :around-ran) :calls ((:method get :resource "/b/Hallo%20Welt" :params nil :data nil) (:method get :resource "/c/Hallo%20Welt" :params nil :data nil))))"#
        ]],
    )
}

pub(super) fn documentation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_documented_resource_and_the_internal_one_are_kept_apart(),
        the_generated_docstring_never_documents_the_object_parameter(),
        configuring_a_condition_case_needs_bytecomp_before_the_hooks_work(),
    ]
}
