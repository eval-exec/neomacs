use expect_test::expect;

use super::ParityBatchCase;

fn authenticated_enterprise_repository_lookup_decodes_nested_metadata() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gh-test-with-sandbox "enterprise-repository"
    '((:status 200
       :headers (("Content-Type" . "application/json; charset=utf-8")
                 ("ETag" . "repo-v7"))
       :body
       "{\"id\":4242,\"name\":\"neomacs\",\"full_name\":\"octo-bot/neomacs\",\"description\":\"Editor λ runtime\",\"private\":false,\"fork\":false,\"owner\":{\"id\":17,\"login\":\"octo.bot\",\"html_url\":\"https://ghe.example/octo.bot\"},\"language\":\"Rust\",\"stargazers_count\":91,\"open_issues\":3}"))
  (let* ((gh-profile-current-profile "enterprise")
         (auth (make-instance 'gh-oauth-authenticator
                              :username "octo.bot"
                              :token "tok_test_123"))
         (api (make-instance 'gh-repos-api :auth auth :cache nil))
         (response (gh-repos-repo-get api "neomacs"))
         (repo (oref response :data))
         (owner (oref repo :owner)))
    (list
     :request (car (nreverse neomacs-gh-test-requests))
     :response
     (list :status (oref response :http-status)
           :class (eieio-object-class-name repo)
           :id (oref repo :id)
           :name (oref repo :name)
           :full-name (oref repo :full-name)
           :description (oref repo :description)
           :private (oref repo :private)
           :fork (oref repo :fork)
           :language (oref repo :language)
           :stars (oref repo :stargazers-count)
           :open-issues (oref repo :open-issues)
           :owner
           (list :class (eieio-object-class-name owner)
                 :id (oref owner :id)
                 :login (oref owner :login)
                 :html-url (oref owner :html-url)))
     :profile (oref api :profile)
     :base (oref api :base)
     :fixtures-consumed (null neomacs-gh-test-responses))))
"####;
    let expected = expect![[
        r#"OK (:request (:url "https://ghe.example/api/v3/repos/octo-bot/neomacs" :method "GET" :headers (("Authorization" . "token tok_test_123") ("Content-Type" . "application/json")) :data "{}") :response (:status 200 :class gh-repos-repo :id 4242 :name "neomacs" :full-name "octo-bot/neomacs" :description "Editor λ runtime" :private nil :fork nil :language "Rust" :stars 91 :open-issues 3 :owner (:class gh-user :id 17 :login "octo.bot" :html-url "https://ghe.example/octo.bot")) :profile "enterprise" :base "https://ghe.example/api/v3" :fixtures-consumed t)"#
    ]];
    ParityBatchCase::value(
        "authenticated_enterprise_repository_lookup_decodes_nested_metadata",
        elisp_form,
        expected,
    )
}

fn paginated_issue_triage_follows_link_headers_and_preserves_nested_objects() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gh-test-with-sandbox "paginated-issues"
    '((:status 200
       :headers
       (("Content-Type" . "application/json")
        ("Link" . "<https://api.github.test/repos/acme/neomacs/issues?page=2>; rel=\"next\", <https://api.github.test/repos/acme/neomacs/issues?page=2>; rel=\"last\""))
       :body
       "[{\"id\":1001,\"number\":71,\"state\":\"open\",\"title\":\"Unicode λ input\",\"body\":\"Reproduce on Wayland\",\"user\":{\"id\":4,\"login\":\"zoe\"},\"labels\":[{\"id\":9,\"name\":\"runtime\",\"color\":\"b60205\"}],\"assignees\":[{\"id\":8,\"login\":\"maintainer\"}],\"comments\":2},{\"id\":1002,\"number\":72,\"state\":\"open\",\"title\":\"Retain cursor phase\",\"body\":\"24 Hz after resume\",\"user\":{\"id\":5,\"login\":\"sam\"},\"labels\":[],\"assignees\":[],\"comments\":0}]")
      (:status 200
       :headers (("Content-Type" . "application/json"))
       :body
       "[{\"id\":1003,\"number\":73,\"state\":\"closed\",\"title\":\"Ship release\",\"body\":\"Published\",\"user\":{\"id\":6,\"login\":\"lee\"},\"labels\":[{\"id\":10,\"name\":\"done\",\"color\":\"0e8a16\"}],\"assignees\":[{\"id\":8,\"login\":\"maintainer\"}],\"comments\":5}]"))
  (let* ((auth (make-instance 'gh-password-authenticator
                              :username "release-bot"
                              :password "päss:42"))
         (api (make-instance 'gh-issues-api :auth auth :cache nil))
         (response (gh-issues-issue-list api "acme" "neomacs"))
         (issues (oref response :data)))
    (list
     :requests (nreverse neomacs-gh-test-requests)
     :status (oref response :http-status)
     :issues
     (mapcar
      (lambda (issue)
        (list
         :class (eieio-object-class-name issue)
         :id (oref issue :id)
         :number (oref issue :number)
         :state (oref issue :state)
         :title (oref issue :title)
         :body (oref issue :body)
         :author (oref (oref issue :user) :login)
         :labels
         (mapcar
          (lambda (label)
            (list (eieio-object-class-name label)
                  (oref label :name)
                  (oref label :color)))
          (oref issue :labels))
         :assignees
         (mapcar (lambda (user) (oref user :login))
                 (oref issue :assignees))
         :comments (oref issue :comments)))
      issues)
     :count (length issues)
     :fixtures-consumed (null neomacs-gh-test-responses))))
"####;
    let expected = expect![[
        r#"OK (:requests ((:url "https://api.github.test/repos/acme/neomacs/issues" :method "GET" :headers (("Authorization" . "Basic cmVsZWFzZS1ib3Q6cMOkc3M6NDI=") ("Content-Type" . "application/json")) :data "{}") (:url "https://api.github.test/repos/acme/neomacs/issues?page=2" :method "GET" :headers (("Authorization" . "Basic cmVsZWFzZS1ib3Q6cMOkc3M6NDI=") ("Content-Type" . "application/json")) :data "{}")) :status 200 :issues ((:class gh-issues-issue :id 1001 :number 71 :state "open" :title "Unicode λ input" :body "Reproduce on Wayland" :author "zoe" :labels ((gh-issues-label "runtime" "b60205")) :assignees ("maintainer") :comments 2) (:class gh-issues-issue :id 1002 :number 72 :state "open" :title "Retain cursor phase" :body "24 Hz after resume" :author "sam" :labels nil :assignees nil :comments 0) (:class gh-issues-issue :id 1003 :number 73 :state "closed" :title "Ship release" :body "Published" :author "lee" :labels ((gh-issues-label "done" "0e8a16")) :assignees ("maintainer") :comments 5)) :count 3 :fixtures-consumed t)"#
    ]];
    ParityBatchCase::value(
        "paginated_issue_triage_follows_link_headers_and_preserves_nested_objects",
        elisp_form,
        expected,
    )
}

fn creating_a_multifile_gist_sends_exact_json_and_returns_the_created_gist() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gh-test-with-sandbox "create-gist"
    '((:status 201
       :headers (("Content-Type" . "application/json")
                 ("Location" . "https://api.github.test/gists/gist-77"))
       :body
       "{\"id\":\"gist-77\",\"url\":\"https://api.github.test/gists/gist-77\",\"html_url\":\"https://gist.github.test/gist-77\",\"description\":\"Release helper λ\",\"public\":false,\"owner\":{\"id\":17,\"login\":\"octobot\"},\"files\":{\"deploy.el\":{\"size\":37,\"raw_url\":\"https://raw.github.test/deploy.el\",\"content\":\"(message \\\"deploy λ\\\")\\n\"},\"notes.md\":{\"size\":20,\"raw_url\":\"https://raw.github.test/notes.md\",\"content\":\"# Release\\n\\nReady ✓\\n\"}},\"comments\":0,\"created_at\":\"2026-08-09T12:00:00Z\",\"updated_at\":\"2026-08-09T12:00:00Z\",\"history\":[],\"forks\":[]}"))
  (let* ((auth (make-instance 'gh-oauth-authenticator
                              :username "octobot"
                              :token "gist_token"))
         (api (make-instance 'gh-gist-api :auth auth :cache nil))
         (stub
          (make-instance
           'gh-gist-gist-stub
           :description "Release helper λ"
           :public nil
           :files
           (list
            (make-instance 'gh-gist-gist-file
                           :filename "deploy.el"
                           :content "(message \"deploy λ\")\n")
            (make-instance 'gh-gist-gist-file
                           :filename "notes.md"
                           :content "# Release\n\nReady ✓\n"))))
         (response (gh-gist-new api stub))
         (gist (oref response :data)))
    (list
     :request (car (nreverse neomacs-gh-test-requests))
     :response
     (list
      :status (oref response :http-status)
      :class (eieio-object-class-name gist)
      :id (oref gist :id)
      :description (oref gist :description)
      :public (oref gist :public)
      :owner (oref (oref gist :user) :login)
      :html-url (oref gist :html-url)
      :files
      (mapcar
       (lambda (file)
         (list :class (eieio-object-class-name file)
               :filename (oref file :filename)
               :size (oref file :size)
               :url (oref file :url)
               :content (oref file :content)))
       (oref gist :files))
      :history (oref gist :history)
      :forks (oref gist :forks))
     :stub-still-complete (gh-gist-gist-has-files stub)
     :fixtures-consumed (null neomacs-gh-test-responses))))
"####;
    let expected = expect![[
        r##"OK (:request (:url "https://api.github.test/gists" :method "POST" :headers (("Authorization" . "token gist_token") ("Content-Type" . "application/json")) :data "{\"description\":\"Release helper λ\",\"public\":null,\"files\":{\"deploy.el\":{\"filename\":\"deploy.el\",\"content\":\"(message \\\"deploy λ\\\")\\n\"},\"notes.md\":{\"filename\":\"notes.md\",\"content\":\"# Release\\n\\nReady ✓\\n\"}}}") :response (:status 201 :class gh-gist-gist :id "gist-77" :description "Release helper λ" :public nil :owner "octobot" :html-url "https://gist.github.test/gist-77" :files ((:class gh-gist-gist-file :filename nil :size 37 :url "https://raw.github.test/deploy.el" :content "(message \"deploy λ\")\n") (:class gh-gist-gist-file :filename nil :size 20 :url "https://raw.github.test/notes.md" :content "# Release\n\nReady ✓\n")) :history nil :forks nil) :stub-still-complete t :fixtures-consumed t)"##
    ]];
    ParityBatchCase::value(
        "creating_a_multifile_gist_sends_exact_json_and_returns_the_created_gist",
        elisp_form,
        expected,
    )
}

fn repository_cache_reuses_reads_then_invalidates_after_an_update() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gh-test-with-sandbox "repository-cache"
    '((:status 200
       :headers (("Content-Type" . "application/json")
                 ("ETag" . "repo-v1"))
       :body
       "{\"id\":700,\"name\":\"neomacs\",\"full_name\":\"acme/neomacs\",\"description\":\"Before migration\",\"private\":false,\"owner\":{\"id\":3,\"login\":\"acme\"},\"has_issues\":true,\"has_wiki\":true,\"has_downloads\":false}")
      (:status 200
       :headers (("Content-Type" . "application/json"))
       :body
       "{\"id\":700,\"name\":\"neomacs\",\"full_name\":\"acme/neomacs\",\"description\":\"After migration λ\",\"private\":false,\"owner\":{\"id\":3,\"login\":\"acme\"},\"has_issues\":true,\"has_wiki\":true,\"has_downloads\":null}")
      (:status 200
       :headers (("Content-Type" . "application/json")
                 ("ETag" . "repo-v2"))
       :body
       "{\"id\":700,\"name\":\"neomacs\",\"full_name\":\"acme/neomacs\",\"description\":\"After migration λ\",\"private\":false,\"owner\":{\"id\":3,\"login\":\"acme\"},\"has_issues\":true,\"has_wiki\":true,\"has_downloads\":null}"))
  (let* ((auth (make-instance 'gh-oauth-authenticator
                              :username "cache-bot"
                              :token "cache_token"))
         (api (make-instance 'gh-repos-api :auth auth :cache t))
         (first-response (gh-repos-repo-get api "neomacs" "acme"))
         (first (oref first-response :data))
         (cached-response (gh-repos-repo-get api "neomacs" "acme"))
         (cached (oref cached-response :data))
         (requests-after-cache-hit (length neomacs-gh-test-requests))
         (update-stub
          (make-instance 'gh-repos-repo-stub
                         :name "neomacs"
                         :description "After migration λ"
                         :private nil))
         (update-response
          (gh-repos-repo-update api update-stub "acme"
                                :issues t :wiki t :downloads nil))
         (fresh-response (gh-repos-repo-get api "neomacs" "acme"))
         (fresh (oref fresh-response :data)))
    (list
     :requests (nreverse neomacs-gh-test-requests)
     :requests-after-cache-hit requests-after-cache-hit
     :first
     (list :status (oref first-response :http-status)
           :description (copy-sequence (oref first :description)))
     :cached
     (list :status (oref cached-response :http-status)
           :description (copy-sequence (oref cached :description)))
     :update
     (list :status (oref update-response :http-status)
           :description
           (copy-sequence (oref (oref update-response :data) :description)))
     :fresh
     (list :status (oref fresh-response :http-status)
           :description (copy-sequence (oref fresh :description))
           :downloads (oref fresh :has-downloads))
     :fixtures-consumed (null neomacs-gh-test-responses))))
"####;
    let expected = expect![[
        r#"OK (:requests ((:url "https://api.github.test/repos/acme/neomacs" :method "GET" :headers (("Authorization" . "token cache_token") ("Content-Type" . "application/json")) :data "{}") (:url "https://api.github.test/repos/acme/neomacs" :method "PATCH" :headers (("Authorization" . "token cache_token") ("Content-Type" . "application/json")) :data "{\"name\":\"neomacs\",\"description\":\"After migration λ\",\"public\":true,\"has_issues\":true,\"has_wiki\":true,\"has_downloads\":null}") (:url "https://api.github.test/repos/acme/neomacs" :method "GET" :headers (("Authorization" . "token cache_token") ("Content-Type" . "application/json")) :data "{}")) :requests-after-cache-hit 1 :first (:status 200 :description "Before migration") :cached (:status nil :description "Before migration") :update (:status 200 :description "After migration λ") :fresh (:status 200 :description "After migration λ" :downloads nil) :fixtures-consumed t)"#
    ]];
    ParityBatchCase::value(
        "repository_cache_reuses_reads_then_invalidates_after_an_update",
        elisp_form,
        expected,
    )
}

fn repository_star_status_can_be_checked_removed_and_restored() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gh-test-with-sandbox "repository-star-lifecycle"
    '((:status 204 :headers (("Content-Type" . "application/json")) :body "")
      (:status 204 :headers (("Content-Type" . "application/json")) :body "")
      (:status 404 :headers (("Content-Type" . "application/json"))
       :body "{\"message\":\"Not Found\"}")
      (:status 204 :headers (("Content-Type" . "application/json")) :body ""))
  (let* ((auth (make-instance 'gh-oauth-authenticator
                              :username "reader"
                              :token "star_token"))
         (api (make-instance 'gh-repos-api :auth auth :cache nil))
         (owner (make-instance 'gh-user :login "acme"))
         (repo (make-instance 'gh-repos-repo
                              :id 700 :name "neomacs" :owner owner))
         (initially-starred (gh-repos-starred-p api repo))
         (unstar-response (gh-repos-unstar api repo))
         (starred-after-removal (gh-repos-starred-p api repo))
         (star-response (gh-repos-star api repo)))
    (list
     :initially-starred initially-starred
     :unstar
     (list :status (oref unstar-response :http-status)
           :data (oref unstar-response :data))
     :starred-after-removal starred-after-removal
     :restore
     (list :status (oref star-response :http-status)
           :data (oref star-response :data))
     :requests (nreverse neomacs-gh-test-requests)
     :fixtures-consumed (null neomacs-gh-test-responses))))
"####;
    let expected = expect![[
        r#"OK (:initially-starred t :unstar (:status 204 :data empty) :starred-after-removal nil :restore (:status 204 :data empty) :requests ((:url "https://api.github.test/user/starred/acme/neomacs" :method "GET" :headers (("Authorization" . "token star_token") ("Content-Type" . "application/json")) :data "{}") (:url "https://api.github.test/user/starred/acme/neomacs" :method "DELETE" :headers (("Authorization" . "token star_token") ("Content-Type" . "application/json")) :data "{}") (:url "https://api.github.test/user/starred/acme/neomacs" :method "GET" :headers (("Authorization" . "token star_token") ("Content-Type" . "application/json")) :data "{}") (:url "https://api.github.test/user/starred/acme/neomacs" :method "PUT" :headers (("Authorization" . "token star_token") ("Content-Type" . "application/json")) :data "{}")) :fixtures-consumed t)"#
    ]];
    ParityBatchCase::value(
        "repository_star_status_can_be_checked_removed_and_restored",
        elisp_form,
        expected,
    )
}

fn malformed_search_payload_is_rejected_instead_of_becoming_empty_results() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-gh-test-with-sandbox "malformed-search"
    '((:status 200
       :expect-method "GET"
       :expect-url
       "https://api.github.test/search/repositories?q=neomacs editor&sort=stars&order=desc"
       :headers (("Content-Type" . "application/json"))
       :body "{\"total_count\":2,\"incomplete_results\":false}"))
  (let* ((auth (make-instance 'gh-oauth-authenticator
                              :username "searcher"
                              :token "search_token"))
         (api (make-instance 'gh-search-api :auth auth :cache nil)))
    (gh-search-repos api "neomacs editor" 1
                     '(sort . "stars") '(order . "desc"))))
"####;
    let expected = expect![[r#"ERR (error "Search query did not return items")"#]];
    ParityBatchCase::signal(
        "malformed_search_payload_is_rejected_instead_of_becoming_empty_results",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        authenticated_enterprise_repository_lookup_decodes_nested_metadata(),
        paginated_issue_triage_follows_link_headers_and_preserves_nested_objects(),
        creating_a_multifile_gist_sends_exact_json_and_returns_the_created_gist(),
        repository_cache_reuses_reads_then_invalidates_after_an_update(),
        repository_star_status_can_be_checked_removed_and_restored(),
        malformed_search_payload_is_rejected_instead_of_becoming_empty_results(),
    ]
}
