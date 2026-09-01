use std::time::Duration;

use expect_test::expect;

use crate::{
    COMPAT_GNU_ELPA_PIN, COND_LET_MELPA_PIN, CachedMelpaOracle, GHUB_MELPA_PIN, LLAMA_MELPA_PIN,
    TREEPY_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'ghub)

(defun neomacs-ghub-test-capture (function)
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-ghub-test-response-buffer (code headers body status)
  (let ((buffer (generate-new-buffer " *ghub fixture response*")))
    (with-current-buffer buffer
      (insert (format "HTTP/1.1 %s Fixture\n" code))
      (dolist (header headers)
        (insert (car header) ": " (cdr header) "\n"))
      (insert "\n")
      (setq-local url-http-end-of-headers (1- (point)))
      (setq-local url-http-response-status code)
      (setq-local url-callback-arguments (list status))
      (insert body))
    buffer))

(defun neomacs-ghub-test-request-summary (payload req)
  (let ((headers (ghub--req-headers req)))
    (list :method (decode-coding-string (ghub--req-method req) 'utf-8)
          :url (url-recreate-url (ghub--req-url req))
          :headers (if (functionp headers) (funcall headers) headers)
          :payload (and payload (decode-coding-string payload 'utf-8))
          :unpaginate (ghub--req-unpaginate req)
          :noerror (ghub--req-noerror req)
          :extra (ghub-req-extra req))))
"####;

fn rest_method_wrappers_route_params_headers_and_utf8_payloads_into_real_request_objects()
-> ParityBatchCase {
    let elisp_form = r####"
(let (requests)
  (cl-letf (((symbol-function 'ghub--retrieve)
             (lambda (payload req)
               (let ((summary (neomacs-ghub-test-request-summary payload req)))
                 (push summary requests)
                 summary))))
    (let ((results
           (list
      (ghub-head "/repos/acme/widgets" '((ref . "release/Ω"))
                 :headers '(("X-Trace" . "head"))
                 :auth 'none :host "api.github.test")
      (ghub-get "repos/acme/widgets/issues"
                '((state . "open") (page . 2) (draft . nil))
                :auth 'none :host "api.github.test" :extra 'get-context)
      (ghub-post "/repos/acme/widgets/issues"
                 '((title . "Release Ω") (draft . nil))
                 :auth 'none :host "api.github.test")
      (ghub-put "/repos/acme/widgets/labels/ready"
                '((color . "00ff00"))
                :auth 'none :host "api.github.test")
      (ghub-patch "/repos/acme/widgets/issues/42"
                  '((state . "closed"))
                  :auth 'none :host "api.github.test")
      (ghub-delete "/repos/acme/widgets/issues/42/labels/stale"
                   '((reason . "released"))
                   :auth 'none :host "api.github.test"))))
      (list :return-methods (mapcar (lambda (result)
                                      (plist-get result :method))
                                    results)
            :requests (nreverse requests)))))
"####;
    let expected = expect![[
        r#"OK (:return-methods ("HEAD" "GET" "POST" "PUT" "PATCH" "DELETE") :requests ((:method "HEAD" :url "https://api.github.test/repos/acme/widgets?ref=release%2F%CE%A9" :headers (("Content-Type" . "application/json") ("X-Trace" . "head")) :payload nil :unpaginate nil :noerror nil :extra nil) (:method "GET" :url "https://api.github.test/repos/acme/widgets/issues?state=open&page=2&draft=false" :headers (("Content-Type" . "application/json")) :payload nil :unpaginate nil :noerror nil :extra get-context) (:method "POST" :url "https://api.github.test/repos/acme/widgets/issues" :headers (("Content-Type" . "application/json")) :payload "{\"title\":\"Release Ω\",\"draft\":false}" :unpaginate nil :noerror nil :extra nil) (:method "PUT" :url "https://api.github.test/repos/acme/widgets/labels/ready" :headers (("Content-Type" . "application/json")) :payload "{\"color\":\"00ff00\"}" :unpaginate nil :noerror nil :extra nil) (:method "PATCH" :url "https://api.github.test/repos/acme/widgets/issues/42" :headers (("Content-Type" . "application/json")) :payload "{\"state\":\"closed\"}" :unpaginate nil :noerror nil :extra nil) (:method "DELETE" :url "https://api.github.test/repos/acme/widgets/issues/42/labels/stale" :headers (("Content-Type" . "application/json")) :payload "{\"reason\":\"released\"}" :unpaginate nil :noerror nil :extra nil)))"#
    ]];
    ParityBatchCase::value(
        "rest_method_wrappers_route_params_headers_and_utf8_payloads_into_real_request_objects",
        elisp_form,
        expected,
    )
}

fn synchronous_rest_workflow_unpaginates_json_and_replaces_response_headers_with_the_last_page()
-> ParityBatchCase {
    let elisp_form = r####"
(let ((responses
       (list
        (list 200
              '(("Content-Type" . "application/json")
                ("Link" . "<https://api.github.test/repos/acme/widgets/issues?page=2>; rel=\"next\"")
                ("X-RateLimit-Remaining" . "4999"))
              "[{\"number\":41,\"title\":\"Prepare Ω\"}]" nil)
        (list 200
              '(("Content-Type" . "application/json")
                ("X-RateLimit-Remaining" . "4998"))
              "[{\"number\":42,\"title\":\"Ship release\"}]" nil)))
      requests
      (ghub-response-headers nil))
  (cl-letf (((symbol-function 'url-retrieve-synchronously)
             (lambda (url silent &rest _)
               (push (list :url (url-recreate-url url)
                           :silent silent
                           :method url-request-method
                           :payload url-request-data
                           :headers (copy-tree url-request-extra-headers))
                     requests)
               (apply #'neomacs-ghub-test-response-buffer (pop responses)))))
    (let ((value
           (ghub-get "/repos/acme/widgets/issues"
                     '((state . "open") (per_page . 1))
                     :unpaginate t :silent t
                     :auth 'none :host "api.github.test")))
      (list :value value
            :requests (nreverse requests)
            :remaining-responses (length responses)
            :last-headers ghub-response-headers))))
"####;
    let expected = expect![[
        r#"OK (:value (((number . 41) (title . "Prepare Ω")) ((number . 42) (title . "Ship release"))) :requests ((:url "https://api.github.test/repos/acme/widgets/issues?state=open&per_page=1" :silent t :method "GET" :payload nil :headers (("Content-Type" . "application/json"))) (:url "https://api.github.test/repos/acme/widgets/issues?page=2" :silent t :method "GET" :payload nil :headers (("Content-Type" . "application/json")))) :remaining-responses 0 :last-headers (("Content-Type" . "application/json") ("X-RateLimit-Remaining" . "4998")))"#
    ]];
    ParityBatchCase::value(
        "synchronous_rest_workflow_unpaginates_json_and_replaces_response_headers_with_the_last_page",
        elisp_form,
        expected,
    )
}

fn http_failures_distinguish_signaling_nil_and_payload_return_policies() -> ParityBatchCase {
    let elisp_form = r####"
(let ((body "{\"message\":\"Validation Failed\",\"errors\":[{\"field\":\"title\",\"code\":\"missing\"}]}"))
  (cl-labels
      ((request (noerror)
         (cl-letf (((symbol-function 'url-retrieve-synchronously)
                    (lambda (&rest _)
                      (neomacs-ghub-test-response-buffer
                       422
                       '(("Content-Type" . "application/json")
                         ("X-GitHub-Request-Id" . "fixture-422"))
                       body
                       '(:error (error http 422))))))
           (ghub-post "/repos/acme/widgets/issues" '((body . "missing title"))
                      :noerror noerror :auth 'none
                      :host "api.github.test"))))
    (list :signal (neomacs-ghub-test-capture (lambda () (request nil)))
          :nil-policy (request t)
          :return-policy (request 'return))))
"####;
    let expected = expect![[
        r#"OK (:signal (:error ghub-http-error :data (422 "Unprocessable Entity (Added by DAV)" "https://api.github.test/repos/acme/widgets/issues" ((message . "Validation Failed") (errors ((field . "title") (code . "missing"))))) :message "HTTP Error: 422, \"Unprocessable Entity (Added by DAV)\", \"https://api.github.test/repos/acme/widgets/issues\", ((message . \"Validation Failed\") (errors ((field . \"title\") (code . \"missing\"))))") :nil-policy nil :return-policy ((message . "Validation Failed") (errors ((field . "title") (code . "missing")))))"#
    ]];
    ParityBatchCase::value(
        "http_failures_distinguish_signaling_nil_and_payload_return_policies",
        elisp_form,
        expected,
    )
}

fn authentication_uses_domain_fallback_forge_specific_headers_and_cache_reset() -> ParityBatchCase {
    let elisp_form = r####"
(let (searches forgotten git-lookups cache-events)
  (cl-letf (((symbol-function 'auth-source-search)
             (lambda (&rest spec)
               (push spec searches)
               (pcase (plist-get spec :host)
                 ("github.test"
                  (list (list :secret (lambda () "domain-token-Ω"))))
                 ("bitbucket.test"
                  (list (list :secret (lambda () "bitbucket-app-pass")))))))
            ((symbol-function 'auth-source-forget)
             (lambda (&rest spec) (push spec forgotten)))
            ((symbol-function 'auth-source-forget+)
             (lambda () (push :auth-source cache-events)))
            ((symbol-function 'ghub--git-get)
             (lambda (variable)
               (push variable git-lookups)
               (cdr (assoc variable
                           '(("github.enterprise.example.user" . "enterprise-user")
                             ("github.user" . "default-user")))))))
    (let ((url-http-real-basic-auth-storage 'stale))
      (list
       :github (ghub--auth "api.github.test/v3" 'forge "alice" 'github)
       :gitlab (ghub--auth "gitlab.test/api/v4" "gl-token" "bob" 'gitlab)
       :bitbucket (ghub--auth "api.bitbucket.test/2.0" "app-pass" "carol" 'bitbucket)
       :custom-headers
       (funcall (ghub--headers '(("X-Package" . "parity"))
                                "api.github.test/v3" 'forge "alice" 'github))
       :enterprise-user (ghub--username "enterprise.example" 'github)
       :default-user (ghub--username "api.github.com" 'github)
       :missing-user
       (neomacs-ghub-test-capture
        (lambda () (ghub--username "unconfigured.example" 'github)))
       :host-domains
       (mapcar #'ghub--host-domain
               '("api.github.com" "git.example.co.uk/api/v3" "localhost"))
       :identity (ghub--ident "alice" 'forge)
       :cache-reset (progn (ghub-clear-caches)
                           (list url-http-real-basic-auth-storage cache-events))
       :searches (nreverse searches)
       :forgotten (nreverse forgotten)
       :git-lookups (nreverse git-lookups)))))
"####;
    let expected = expect![[
        r#"OK (:github ("Authorization" . "token domain-token-Ω") :gitlab ("Private-Token" . "gl-token") :bitbucket ("Authorization" . "Basic Y2Fyb2w6Yml0YnVja2V0LWFwcC1wYXNz") :custom-headers (("Authorization" . "token domain-token-Ω") ("Content-Type" . "application/json") ("X-Package" . "parity")) :enterprise-user "enterprise-user" :default-user "default-user" :missing-user (:error error :data ("Cannot determine username; Git variable ‘github.unconfigured.example.user’ is unset") :message "Cannot determine username; Git variable ‘github.unconfigured.example.user’ is unset") :host-domains ("github.com" "example.co.uk" "localhost") :identity "alice^forge" :cache-reset (nil (:auth-source)) :searches ((:host "api.github.test/v3" :user "alice^forge") (:host "github.test" :user "alice^forge") (:host "api.bitbucket.test/2.0" :user "carol^app-pass") (:host "bitbucket.test" :user "carol^app-pass") (:host "api.github.test/v3" :user "alice^forge") (:host "github.test" :user "alice^forge")) :forgotten (((:host "api.github.test/v3" :user "alice^forge")) ((:host "api.bitbucket.test/2.0" :user "carol^app-pass")) ((:host "api.github.test/v3" :user "alice^forge"))) :git-lookups ("github.enterprise.example.user" "github.api.github.com.user" "github.user" "github.unconfigured.example.user"))"#
    ]];
    ParityBatchCase::value(
        "authentication_uses_domain_fallback_forge_specific_headers_and_cache_reset",
        elisp_form,
        expected,
    )
}

fn graphql_query_builds_typed_variables_edges_and_enterprise_endpoint_payload() -> ParityBatchCase {
    let elisp_form = r####"
(let ((query
       '(query
         (repository [(owner $owner String!) (name $name String!)]
          nameWithOwner
          (issues [(:edges t)
                   (states [OPEN])
                   (orderBy ((field UPDATED_AT) (direction DESC)))]
                  number title updatedAt))))
      (variables '((owner . "acme") (name . "widgets")))
      captured)
  (let ((ghub-graphql-items-per-request 25))
    (cl-letf (((symbol-function 'ghub--retrieve)
               (lambda (payload req)
                 (setq captured
                       (list :request
                             (neomacs-ghub-test-request-summary payload req)
                             :query (ghub--graphql-req-query-str req)
                             :decoded
                             (json-parse-string
                              (decode-coding-string payload 'utf-8)
                              :object-type 'alist :array-type 'list
                              :null-object nil :false-object nil))))))
      (ghub-query query variables
        :synchronous t :auth 'none :host "ghe.example/api/v3"))
    (list :captured captured
          :prepared (gsexp-encode (ghub--graphql-prepare-query query))
          :narrowed
          (gsexp-encode
           (ghub--graphql-narrow-query
            (ghub--graphql-prepare-query query)
            '(repository issues))))))
"####;
    let expected = expect![[
        r#"OK (:captured (:request (:method "POST" :url "https://ghe.example/api/graphql" :headers (("Content-Type" . "application/json")) :payload "{\"query\":\"query ($owner: String!,$name: String!) {\\n  repository (owner: $owner,name: $name) {\\n    nameWithOwner\\n    issues (first: 25,states: (OPEN),orderBy: {field: UPDATED_AT, direction: DESC}) {\\n      pageInfo {\\n\\tendCursor\\n\\thasNextPage\\n      }\\n      edges {\\n\\tnode {\\n\\t  number\\n\\t  title\\n\\t  updatedAt\\n\\t}\\n      }\\n    }\\n  }\\n}\",\"variables\":{\"owner\":\"acme\",\"name\":\"widgets\"}}" :unpaginate nil :noerror nil :extra nil) :query "query ($owner: String!,$name: String!) {\n  repository (owner: $owner,name: $name) {\n    nameWithOwner\n    issues (first: 25,states: (OPEN),orderBy: {field: UPDATED_AT, direction: DESC}) {\n      pageInfo {\n\11endCursor\n\11hasNextPage\n      }\n      edges {\n\11node {\n\11  number\n\11  title\n\11  updatedAt\n\11}\n      }\n    }\n  }\n}" :decoded ((query . "query ($owner: String!,$name: String!) {\n  repository (owner: $owner,name: $name) {\n    nameWithOwner\n    issues (first: 25,states: (OPEN),orderBy: {field: UPDATED_AT, direction: DESC}) {\n      pageInfo {\n\11endCursor\n\11hasNextPage\n      }\n      edges {\n\11node {\n\11  number\n\11  title\n\11  updatedAt\n\11}\n      }\n    }\n  }\n}") (variables (owner . "acme") (name . "widgets")))) :prepared "query ($owner: String!,$name: String!) {\n  repository (owner: $owner,name: $name) {\n    nameWithOwner\n    issues (first: 25,states: (OPEN),orderBy: {field: UPDATED_AT, direction: DESC}) {\n      pageInfo {\n\11endCursor\n\11hasNextPage\n      }\n      edges {\n\11node {\n\11  number\n\11  title\n\11  updatedAt\n\11}\n      }\n    }\n  }\n}" :narrowed "query ($owner: String!,$name: String!) {\n  repository (owner: $owner,name: $name) {\n    issues (first: 25,states: (OPEN),orderBy: {field: UPDATED_AT, direction: DESC}) {\n      pageInfo {\n\11endCursor\n\11hasNextPage\n      }\n      edges {\n\11node {\n\11  number\n\11  title\n\11  updatedAt\n\11}\n      }\n    }\n  }\n}")"#
    ]];
    ParityBatchCase::value(
        "graphql_query_builds_typed_variables_edges_and_enterprise_endpoint_payload",
        elisp_form,
        expected,
    )
}

fn graphql_cursor_workflow_merges_two_issue_pages_and_returns_the_narrowed_repository()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((req
        (ghub--make-graphql-req
         :url (url-generic-parse-url "https://api.github.test/graphql")
         :forge 'github :method "POST" :buffer (current-buffer)
         :query '(query) :narrow '(repository) :pages 1))
       retrievals)
  (cl-letf (((symbol-function 'ghub--graphql-retrieve)
             (lambda (page-req &optional lineage cursor)
               (cl-incf (ghub--graphql-req-pages page-req))
               (push (list lineage cursor) retrievals))))
    (ghub--graphql-walk-response
     req
     '(data . ((repository .
                ((nameWithOwner . "acme/widgets")
                 (issues .
                  ((pageInfo . ((hasNextPage . t)
                                (endCursor . "cursor-1")))
                   (edges . (((node . ((number . 41)
                                       (title . "Prepare Ω")))))))))))))
    (ghub--graphql-walk-response
     req
     '(data . ((repository .
                ((issues .
                  ((pageInfo . ((hasNextPage . nil)
                                (endCursor . "cursor-2")))
                   (edges . (((node . ((number . 42)
                                       (title . "Ship release")))))))))))))
    (list :value (ghub--req-value req)
          :retrievals (nreverse retrievals)
          :pages (ghub--graphql-req-pages req))))
"####;
    let expected = expect![[
        r#"OK (:value ((nameWithOwner . "acme/widgets") (issues ((number . 41) (title . "Prepare Ω")) ((number . 42) (title . "Ship release")))) :retrievals (((repository issues) "cursor-1")) :pages 2)"#
    ]];
    ParityBatchCase::value(
        "graphql_cursor_workflow_merges_two_issue_pages_and_returns_the_narrowed_repository",
        elisp_form,
        expected,
    )
}

fn repository_identity_normalizes_ids_across_supported_forges_and_reports_missing_projects()
-> ParityBatchCase {
    let elisp_form = r####"
(let (calls)
  (cl-letf (((symbol-function 'ghub-query)
             (lambda (&rest args)
               (push (cons :graphql args) calls)
               '((repository . ((id . "R_widget"))))))
            ((symbol-function 'ghub-get)
             (lambda (resource &rest args)
               (push (list :rest resource args) calls)
               (cond ((string-prefix-p "/projects/" resource) '((id . 314)))
                     ((string-prefix-p "/repos/" resource) '((id . 2718)))
                     ((string-prefix-p "/repositories/" resource)
                      '((uuid . "{bitbucket-widget}")))
                     (t nil)))))
    (list :github (ghub-repository-id "acme" "widgets" :auth 'none)
          :gitlab (ghub-repository-id "groups/platform" "widgets"
                                      :forge 'gitlab :auth 'none)
          :forgejo (ghub-repository-id "acme" "widgets"
                                       :forge 'forgejo :auth 'none)
          :bitbucket (ghub-repository-id "acme" "widgets"
                                         :forge 'bitbucket :auth 'none)
          :unknown
          (neomacs-ghub-test-capture
           (lambda ()
             (ghub-repository-id "acme" "widgets"
                                 :forge 'unknown :auth 'none)))
          :missing
          (cl-letf (((symbol-function 'ghub-query) (lambda (&rest _) nil)))
            (neomacs-ghub-test-capture
             (lambda ()
               (ghub-repository-id "gone" "repository"
                                   :host "api.github.test"
                                   :auth 'none))))
          :missing-noerror
          (cl-letf (((symbol-function 'ghub-query) (lambda (&rest _) nil)))
            (ghub-repository-id "gone" "repository"
                                :noerror t :auth 'none))
          :calls (nreverse calls))))
"####;
    let expected = expect![[
        r#"OK (:github "R_widget" :gitlab "314" :forgejo "2718" :bitbucket "bitbucket-widget" :unknown (:error error :data ("ghub-repository-id: Forge type ‘unknown’ is unknown") :message "ghub-repository-id: Forge type ‘unknown’ is unknown") :missing (:error error :data ("Repository \"gone/repository\" does not exist on \"api.github.test\".\nMaybe it was renamed and you have to update \"remote.<remote>.url\"?") :message "Repository \"gone/repository\" does not exist on \"api.github.test\".\nMaybe it was renamed and you have to update \"remote.<remote>.url\"?") :missing-noerror nil :calls ((:graphql (query (repository [(owner $owner String!) (name $name String!)] id)) ((owner . "acme") (name . "widgets")) :synchronous t :username nil :auth none :host nil) (:rest "/projects/groups%2Fplatform%2Fwidgets" (nil :forge gitlab :username nil :auth none :host nil)) (:rest "/repos/acme/widgets" (nil :forge forgejo :username nil :auth none :host nil)) (:rest "/repositories/acme/widgets" (nil :forge bitbucket :username nil :auth none :host nil))))"#
    ]];
    ParityBatchCase::value(
        "repository_identity_normalizes_ids_across_supported_forges_and_reports_missing_projects",
        elisp_form,
        expected,
    )
}

fn wait_workflow_uses_exponential_backoff_and_reports_a_deterministic_timeout() -> ParityBatchCase {
    let elisp_form = r####"
(cl-labels
    ((run (responses duration)
       (let (attempts sleeps)
         (cl-letf (((symbol-function 'ghub-request)
                    (lambda (method resource &rest args)
                      (push (list method resource args) attempts)
                      (pop responses)))
                   ((symbol-function 'sit-for)
                    (lambda (seconds &rest _)
                      (push seconds sleeps)
                      t)))
           (list :outcome
                 (neomacs-ghub-test-capture
                  (lambda ()
                    (ghub-wait "/repos/acme/widgets" duration
                               :auth 'none :host "api.github.test")))
                 :attempts (nreverse attempts)
                 :sleeps (nreverse sleeps))))))
  (list :eventual (run '(nil nil ((ready . t))) 6)
        :timeout (run '(nil nil nil nil) 4)))
"####;
    let expected = expect![[
        r#"OK (:eventual (:outcome (:ok nil) :attempts (("GET" "/repos/acme/widgets" (nil :noerror t :username nil :auth none :host "api.github.test" :forge nil)) ("GET" "/repos/acme/widgets" (nil :noerror t :username nil :auth none :host "api.github.test" :forge nil)) ("GET" "/repos/acme/widgets" (nil :noerror t :username nil :auth none :host "api.github.test" :forge nil))) :sleeps (2 2)) :timeout (:outcome (:error error :data ("Github is taking too long to create /repos/acme/widgets") :message "Github is taking too long to create /repos/acme/widgets") :attempts (("GET" "/repos/acme/widgets" (nil :noerror t :username nil :auth none :host "api.github.test" :forge nil)) ("GET" "/repos/acme/widgets" (nil :noerror t :username nil :auth none :host "api.github.test" :forge nil)) ("GET" "/repos/acme/widgets" (nil :noerror t :username nil :auth none :host "api.github.test" :forge nil))) :sleeps (2 2)))"#
    ]];
    ParityBatchCase::value(
        "wait_workflow_uses_exponential_backoff_and_reports_a_deterministic_timeout",
        elisp_form,
        expected,
    )
}

#[test]
fn ghub_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(GHUB_MELPA_PIN, "ghub.el")
            .expect("prepare revision-pinned Ghub source below ./tmp")
            .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
            .expect("prepare exact Compat dependency below ./tmp")
            .with_melpa_dependency(COND_LET_MELPA_PIN)
            .expect("prepare revision-pinned Cond-Let dependency below ./tmp")
            .with_melpa_dependency(LLAMA_MELPA_PIN)
            .expect("prepare revision-pinned Llama dependency below ./tmp")
            .with_melpa_dependency(TREEPY_MELPA_PIN)
            .expect("prepare revision-pinned Treepy dependency below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "ghub-package-batch",
        "Ghub",
        &[
            rest_method_wrappers_route_params_headers_and_utf8_payloads_into_real_request_objects(),
            synchronous_rest_workflow_unpaginates_json_and_replaces_response_headers_with_the_last_page(),
            http_failures_distinguish_signaling_nil_and_payload_return_policies(),
            authentication_uses_domain_fallback_forge_specific_headers_and_cache_reset(),
            graphql_query_builds_typed_variables_edges_and_enterprise_endpoint_payload(),
            graphql_cursor_workflow_merges_two_issue_pages_and_returns_the_narrowed_repository(),
            repository_identity_normalizes_ids_across_supported_forges_and_reports_missing_projects(),
            wait_workflow_uses_exponential_backoff_and_reports_a_deterministic_timeout(),
        ],
    );
}
