use expect_test::expect;

use super::ParityBatchCase;

fn interactive_link_insertion_survives_buffer_switch_and_normalizes_real_html() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site
    '(("/runbooks/release"
       :body "<html><head><TITLE data-owner=\"release\">\n  Release &amp; Reliability [Runbook] &#945;\n</TITLE></head></html>"))
  (let ((destination (generate-new-buffer "*cliplink-release-notes*"))
        (observer (generate-new-buffer "*cliplink-observer*"))
        (url (concat base-url "/runbooks/release")))
    (unwind-protect
        (progn
          (with-current-buffer destination
            (org-mode)
            (insert "* Release notes\n- Primary runbook: ")
            (kill-new (propertize (concat "  " url "\n") 'source 'browser))
            (call-interactively #'org-cliplink))
          ;; The user can continue elsewhere while url.el finishes.  The
          ;; package must insert into the buffer that initiated the command.
          (switch-to-buffer observer)
          (insert "triaging another incident")
          (neomacs-org-cliplink-test-wait-for
           (lambda ()
             (with-current-buffer destination
               (string-match-p "\\[\\[" (buffer-string))))
           "the asynchronous Org link insertion")
          (with-current-buffer destination
            (goto-char (point-max))
            (list :destination
                  (neomacs-org-cliplink-test-normalize-origin
                   (buffer-string) base-url)
                  :point-at-end (= (point) (point-max))
                  :modified (buffer-modified-p)
                  :link (neomacs-org-cliplink-test-link-state base-url)
                  :observer (with-current-buffer observer (buffer-string))
                  :selected (buffer-name (window-buffer (selected-window)))
                  :clipboard
                  (let ((value (current-kill 0 t)))
                    (list
                     :text (neomacs-org-cliplink-test-normalize-origin
                            (substring-no-properties value) base-url)
                     :source (get-text-property 0 'source value)))
                  :requests neomacs-org-cliplink-test-requests)))
      (when (buffer-live-p destination) (kill-buffer destination))
      (when (buffer-live-p observer) (kill-buffer observer)))))
"####;
    let expect = expect![[
        r####"OK (:destination "* Release notes\n- Primary runbook: [[<ORIGIN>/runbooks/release][Release & Reliability {Runbook} α]]" :point-at-end t :modified t :link (:type link :raw-link "<ORIGIN>/runbooks/release" :contents "Release & Reliability {Runbook} α") :observer "triaging another incident" :selected "*cliplink-observer*" :clipboard (:text "  <ORIGIN>/runbooks/release\n" :source browser) :requests ((:request-line "GET /runbooks/release HTTP/1.1" :authorization nil :accept-encoding "gzip")))"####
    ]];
    ParityBatchCase::value(
        "interactive_link_insertion_survives_buffer_switch_and_normalizes_real_html",
        elisp_form,
        expect,
    )
}

fn capture_returns_a_truncated_link_without_editing_the_capture_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site
    '(("/retrospectives/q3"
       :body "<html><title>  Quarterly Deployment Retrospective &amp; Corrective Actions  </title></html>"))
  (let ((url (concat base-url "/retrospectives/q3"))
        (org-cliplink-max-length 32)
        (org-cliplink-ellipsis "…"))
    (with-temp-buffer
      (org-mode)
      (insert "* Capture inbox\n")
      (goto-char (point-max))
      (set-buffer-modified-p nil)
      (kill-new (concat "\t" url "  "))
      (let ((result (call-interactively #'org-cliplink-capture)))
        (list
         :returned
         (neomacs-org-cliplink-test-normalize-origin result base-url)
         :buffer (buffer-string)
         :point (point)
         :modified (buffer-modified-p)
         :clipboard
         (neomacs-org-cliplink-test-normalize-origin
          (current-kill 0 t) base-url)
         :requests neomacs-org-cliplink-test-requests)))))
"####;
    let expect = expect![[
        r####"OK (:returned "[[<ORIGIN>/retrospectives/q3][Quarterly Deployment Retrospe…]]" :buffer "* Capture inbox\n" :point 17 :modified nil :clipboard "\11<ORIGIN>/retrospectives/q3  " :requests ((:request-line "GET /retrospectives/q3 HTTP/1.1" :authorization nil :accept-encoding "gzip")))"####
    ]];
    ParityBatchCase::value(
        "capture_returns_a_truncated_link_without_editing_the_capture_buffer",
        elisp_form,
        expect,
    )
}

fn configured_tracker_title_replacement_produces_a_concise_operational_link() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site
    '(("/issues/482"
       :body "<html><title>Prevent duplicate deployments · Incident #482 · Operations · Tracker</title></html>"))
  (let* ((url (concat base-url "/issues/482"))
         (org-cliplink-title-replacements
          (list
           (list
            (concat (regexp-quote base-url) "/issues/[0-9]+")
            (list
             "\\(.*\\) · Incident #\\([0-9]+\\) · \\(.*\\) · Tracker"
             "\\3#\\2 \\1")))))
    (with-temp-buffer
      (org-mode)
      (insert "* Incident review\n")
      (kill-new url)
      (call-interactively #'org-cliplink)
      (neomacs-org-cliplink-test-wait-for
       (lambda () (string-match-p "Operations#482" (buffer-string)))
       "the configured tracker title transformation")
      (goto-char (point-max))
      (list
       :buffer
       (neomacs-org-cliplink-test-normalize-origin
        (buffer-string) base-url)
       :link (neomacs-org-cliplink-test-link-state base-url)
       :point-at-end (= (point) (point-max))
       :requests neomacs-org-cliplink-test-requests))))
"####;
    let expect = expect![[
        r####"OK (:buffer "* Incident review\n[[<ORIGIN>/issues/482][Operations#482 Prevent duplicate deployments]]" :link (:type link :raw-link "<ORIGIN>/issues/482" :contents "Operations#482 Prevent duplicate deployments") :point-at-end t :requests ((:request-line "GET /issues/482 HTTP/1.1" :authorization nil :accept-encoding "gzip")))"####
    ]];
    ParityBatchCase::value(
        "configured_tracker_title_replacement_produces_a_concise_operational_link",
        elisp_form,
        expect,
    )
}

fn page_without_a_title_falls_back_to_a_valid_bare_org_link() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site
    '(("/status/untitled"
       :body "<html><body><h1>Untitled service status</h1><p>Healthy</p></body></html>"))
  (let ((url (concat base-url "/status/untitled")))
    (with-temp-buffer
      (org-mode)
      (insert "* Operations backlog\n- Investigate status: ")
      (kill-new url)
      (call-interactively #'org-cliplink)
      (neomacs-org-cliplink-test-wait-for
       (lambda () (string-match-p "\\[\\[" (buffer-string)))
       "the bare Org link fallback")
      (goto-char (point-max))
      (list
       :buffer
       (neomacs-org-cliplink-test-normalize-origin
        (buffer-string) base-url)
       :link (neomacs-org-cliplink-test-link-state base-url)
       :point-at-end (= (point) (point-max))
       :requests neomacs-org-cliplink-test-requests))))
"####;
    let expect = expect![[
        r####"OK (:buffer "* Operations backlog\n- Investigate status: [[<ORIGIN>/status/untitled]]" :link (:type link :raw-link "<ORIGIN>/status/untitled" :contents nil) :point-at-end t :requests ((:request-line "GET /status/untitled HTTP/1.1" :authorization nil :accept-encoding "gzip")))"####
    ]];
    ParityBatchCase::value(
        "page_without_a_title_falls_back_to_a_valid_bare_org_link",
        elisp_form,
        expect,
    )
}

fn https_url_el_auth_uses_the_exact_entry_after_a_wildcard_entry() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-tls-site
    "200 OK"
    "<html><title>Private Deployment Console</title></html>"
  (let* ((url (concat base-url "/private/deployment"))
         (secrets-path
          (neomacs-org-cliplink-test-write-file
           "org-cliplink/basic-auth-secrets.el"
           (prin1-to-string
            (list
             :basic-auth
             (list
              ;; Current GNU Emacs treats an ordinary `*' read from this
              ;; file literally.  Pin the fallthrough to the exact entry
              ;; users need for a working authenticated request.
              (list :url-pattern (concat base-url "/private/*")
                    :username "wrong-user"
                    :password "wrong-password")
              (list :url-pattern url
                    :username "release-bot"
                    :password "correct-horse"))))))
         (org-cliplink-secrets-path secrets-path))
    (unwind-protect
        (with-temp-buffer
          (let ((processes-before (process-list))
                (transport-error nil))
            (org-mode)
            (insert "* Protected operations\n- Console: ")
            (kill-new url)
            (call-interactively #'org-cliplink)
            (let* ((url-process
                    (seq-find
                     (lambda (process) (not (memq process processes-before)))
                     (process-list)))
                   (package-sentinel
                    (and url-process (process-sentinel url-process))))
              (unless (and url-process package-sentinel)
                (error "url.el did not create its request process"))
              ;; Delegate to url.el's real sentinel and retain any editor
              ;; error as parity data so one broken request cannot abort the
              ;; remaining shared-batch workflows.
              (set-process-sentinel
               url-process
               (lambda (process event)
                 (condition-case error-data
                     (funcall package-sentinel process event)
                   (error
                    (setq transport-error
                          (neomacs-org-cliplink-test-normalize-origin
                           (error-message-string error-data) base-url))))))
              (neomacs-org-cliplink-test-wait-for
               (lambda ()
                 (or transport-error
                     (string-match-p
                      "Private Deployment" (buffer-string))))
               "the authenticated HTTPS Org link or transport error")
              (goto-char (point-max))
              (list
               :transport-error transport-error
               :buffer
               (neomacs-org-cliplink-test-normalize-origin
                (buffer-string) base-url)
               :link
               (when (string-match-p "\\[\\[" (buffer-string))
                 (neomacs-org-cliplink-test-link-state base-url))
               :request
               (list (neomacs-org-cliplink-test-recorded-request request-file))
               :secret-still-on-disk (file-exists-p secrets-path)))))
      (when (file-exists-p secrets-path)
        (delete-file secrets-path)))))
"####;
    let expect = expect![[
        r####"OK (:transport-error nil :buffer "* Protected operations\n- Console: [[<ORIGIN>/private/deployment][Private Deployment Console]]" :link (:type link :raw-link "<ORIGIN>/private/deployment" :contents "Private Deployment Console") :request ((:request-line "GET /private/deployment HTTP/1.1" :authorization "Basic cmVsZWFzZS1ib3Q6Y29ycmVjdC1ob3JzZQ==" :accept-encoding "gzip")) :secret-still-on-disk t)"####
    ]];
    ParityBatchCase::value(
        "https_url_el_auth_uses_the_exact_entry_after_a_wildcard_entry",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn duplicate_url_el_completion_inserts_one_link_and_runs_one_transformer() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site
    '(("/deployments/duplicate-callback"
       :body "<html><title>Duplicate Callback Deployment Guide</title></html>"))
  (let* ((url (concat base-url "/deployments/duplicate-callback"))
         (transformer-calls 0)
         (completion-deliveries 0)
         (callback-filter
          (lambda (arguments)
            (let ((callback (nth 1 arguments)))
              (cons
               (car arguments)
               (cons
                (lambda (&rest callback-arguments)
                  ;; Reproduce the duplicate completion that exact
                  ;; Org Cliplink guards against while keeping the real
                  ;; url.el request and response buffer.
                  (setq completion-deliveries
                        (1+ completion-deliveries))
                  (apply callback callback-arguments)
                  (setq completion-deliveries
                        (1+ completion-deliveries))
                  (apply callback callback-arguments))
                (cddr arguments)))))))
    (advice-add 'url-retrieve :filter-args callback-filter)
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          (insert "* Deployment handoff\n- Guide: ")
          (org-cliplink-insert-transformed-title
           url
           (lambda (link title)
             (setq transformer-calls (1+ transformer-calls))
             (org-cliplink-org-mode-link-transformer link title)))
          (neomacs-org-cliplink-test-wait-for
           (lambda () (> transformer-calls 0))
           "the duplicated url.el completion")
          (let ((links
                 (org-element-map
                     (org-element-parse-buffer) 'link
                   (lambda (link)
                     (list
                      :raw-link
                      (neomacs-org-cliplink-test-normalize-origin
                       (org-element-property :raw-link link) base-url)
                      :contents
                      (buffer-substring-no-properties
                       (org-element-property :contents-begin link)
                       (org-element-property :contents-end link)))))))
            (list
             :buffer
             (neomacs-org-cliplink-test-normalize-origin
             (buffer-string) base-url)
             :completion-deliveries completion-deliveries
             :transformer-calls transformer-calls
             :links links
             :request-count (length neomacs-org-cliplink-test-requests)
             :requests neomacs-org-cliplink-test-requests)))
      (advice-remove 'url-retrieve callback-filter))))
"####;
    let expect = expect![[
        r####"OK (:buffer "* Deployment handoff\n- Guide: [[<ORIGIN>/deployments/duplicate-callback][Duplicate Callback Deployment Guide]]" :completion-deliveries 2 :transformer-calls 1 :links ((:raw-link "<ORIGIN>/deployments/duplicate-callback" :contents "Duplicate Callback Deployment Guide")) :request-count 1 :requests ((:request-line "GET /deployments/duplicate-callback HTTP/1.1" :authorization nil :accept-encoding "gzip")))"####
    ]];
    ParityBatchCase::value(
        "duplicate_url_el_completion_inserts_one_link_and_runs_one_transformer",
        elisp_form,
        expect,
    )
}

fn gzip_encoded_page_is_decompressed_through_url_el_before_link_insertion() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site
    `(("/runbooks/recovery.gz"
       :headers ("Content-Encoding: gzip")
       :body
       ,(base64-decode-string
         "H4sIAAAAAAAAA7PJKMnNsbPJSE1MsbMpySzJSbVzzs8tKEotLk5NUQhKTc4vSy2qVAgqzUvKz89WOLfSRh+iykYfoicpP6XSrgjIrLTRB7OBEiAjAXA3FmFZAAAA")))
  (let ((url (concat base-url "/runbooks/recovery.gz")))
    (with-temp-buffer
      (org-mode)
      (insert "* Incident recovery\n- Procedure: ")
      (kill-new url)
      (call-interactively #'org-cliplink)
      (neomacs-org-cliplink-test-wait-for
       (lambda () (string-match-p "Compressed Recovery" (buffer-string)))
       "the gzip-compressed page title")
      (goto-char (point-max))
      (list
       :buffer
       (neomacs-org-cliplink-test-normalize-origin
        (buffer-string) base-url)
       :link (neomacs-org-cliplink-test-link-state base-url)
       :point-at-end (= (point) (point-max))
       :requests neomacs-org-cliplink-test-requests))))
"####;
    let expect = expect![[
        r####"OK (:buffer "* Incident recovery\n- Procedure: [[<ORIGIN>/runbooks/recovery.gz][Compressed Recovery Runbook Ω]]" :link (:type link :raw-link "<ORIGIN>/runbooks/recovery.gz" :contents "Compressed Recovery Runbook Ω") :point-at-end t :requests ((:request-line "GET /runbooks/recovery.gz HTTP/1.1" :authorization nil :accept-encoding "gzip")))"####
    ]];
    ParityBatchCase::value(
        "gzip_encoded_page_is_decompressed_through_url_el_before_link_insertion",
        elisp_form,
        expect,
    )
}

fn documented_custom_transformer_cleans_a_runbook_title_before_insertion() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site
    '(("/docs/deploy-release"
       :body "<html><title>Internal Docs — Deploy Release 42 Safely</title></html>"))
  (let ((url (concat base-url "/docs/deploy-release")))
    (with-temp-buffer
      (org-mode)
      (insert "* Release checklist\n- [ ] Read: ")
      (org-cliplink-insert-transformed-title
       url
       (lambda (link title)
         (org-cliplink-org-mode-link-transformer
          link
          (replace-regexp-in-string
           "\\`Internal Docs — " "" title))))
      (neomacs-org-cliplink-test-wait-for
       (lambda () (string-match-p "Deploy Release 42" (buffer-string)))
       "the documented custom title transformer")
      (goto-char (point-max))
      (list
       :buffer
       (neomacs-org-cliplink-test-normalize-origin
        (buffer-string) base-url)
       :link (neomacs-org-cliplink-test-link-state base-url)
       :point-at-end (= (point) (point-max))
       :requests neomacs-org-cliplink-test-requests))))
"####;
    let expect = expect![[
        r####"OK (:buffer "* Release checklist\n- [ ] Read: [[<ORIGIN>/docs/deploy-release][Deploy Release 42 Safely]]" :link (:type link :raw-link "<ORIGIN>/docs/deploy-release" :contents "Deploy Release 42 Safely") :point-at-end t :requests ((:request-line "GET /docs/deploy-release HTTP/1.1" :authorization nil :accept-encoding "gzip")))"####
    ]];
    ParityBatchCase::value(
        "documented_custom_transformer_cleans_a_runbook_title_before_insertion",
        elisp_form,
        expect,
    )
}

fn authenticated_curl_https_records_real_argv_and_masks_its_log() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-tls-site
    "200 OK"
    "<html><title>Release Artifact via cURL</title></html>"
  (neomacs-org-cliplink-test-with-curl-wrapper
    (let* ((url (concat base-url "/release/curl"))
           (secrets-path
            (neomacs-org-cliplink-test-write-file
             "org-cliplink/curl-auth-secrets.el"
             (prin1-to-string
              (list
               :basic-auth
               (list
                (list :url-pattern (concat base-url "/release/*")
                      :username "decoy-user" :password "decoy-password")
                (list :url-pattern url
                      :username "release-bot" :password "correct-horse"))))))
           (org-cliplink-secrets-path secrets-path)
           (org-cliplink-transport-implementation 'curl)
           (org-cliplink-curl-transport-arguments
            '("--insecure" "--fail" "--location" "--max-time" "5")))
      (unwind-protect
          (with-temp-buffer
            (org-mode)
            (insert "* Distribution checklist\n- Artifact: ")
            (kill-new url)
            (call-interactively #'org-cliplink)
            (neomacs-org-cliplink-test-wait-for
             (lambda () (string-match-p "Release Artifact" (buffer-string)))
             "the authenticated real curl HTTPS callback")
            (goto-char (point-max))
            (list
             :buffer
             (neomacs-org-cliplink-test-normalize-origin
              (buffer-string) base-url)
             :link (neomacs-org-cliplink-test-link-state base-url)
             :curl-log
             (neomacs-org-cliplink-test-normalize-origin
              (or (neomacs-org-cliplink-test-last-message-matching "^curl .*$") "")
              base-url)
             :argv
             (mapcar
              (lambda (argument)
                (neomacs-org-cliplink-test-normalize-origin argument base-url))
              (split-string
               (neomacs-org-cliplink-test-read-file argv-file) "\n" t))
             :request
             (neomacs-org-cliplink-test-recorded-request request-file)
             :curl-process-live
             (let ((process (get-process "curl")))
               (and process (process-live-p process)))
             :secret-still-on-disk (file-exists-p secrets-path)))
        (when (file-exists-p secrets-path)
          (delete-file secrets-path))))))
"####;
    let expect = expect![[
        r####"OK (:buffer "* Distribution checklist\n- Artifact: [[<ORIGIN>/release/curl][Release Artifact via cURL]]" :link (:type link :raw-link "<ORIGIN>/release/curl" :contents "Release Artifact via cURL") :curl-log "curl --insecure --fail --location --max-time 5 --include --silent --show-error -X GET --user ***:*** <ORIGIN>/release/curl" :argv ("--insecure" "--fail" "--location" "--max-time" "5" "--include" "--silent" "--show-error" "-X" "GET" "--user" "release-bot:correct-horse" "<ORIGIN>/release/curl") :request (:request-line "GET /release/curl HTTP/1.1" :authorization "Basic cmVsZWFzZS1ib3Q6Y29ycmVjdC1ob3JzZQ==" :accept-encoding nil) :curl-process-live nil :secret-still-on-disk t)"####
    ]];
    ParityBatchCase::value(
        "authenticated_curl_https_records_real_argv_and_masks_its_log",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn curl_http_failure_runs_the_real_nonzero_sentinel_without_editing() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site
    '(("/maintenance/curl"
       :status "503 Service Unavailable"
       :body "<html><title>Maintenance must not be inserted</title></html>"))
  (neomacs-org-cliplink-test-with-curl-wrapper
    (let ((url (concat base-url "/maintenance/curl"))
          (org-cliplink-transport-implementation 'curl)
          (org-cliplink-curl-transport-arguments
           '("--fail" "--location" "--max-time" "5"))
          (transport-error nil))
      (with-temp-buffer
        (org-mode)
        (insert "* Release blockers\n- Failed transport: ")
        (goto-char (point-max))
        (set-buffer-modified-p nil)
        (kill-new url)
        (call-interactively #'org-cliplink)
        (let* ((curl-process (get-process "curl"))
               (package-sentinel (process-sentinel curl-process)))
          ;; Keep the real package sentinel and observe its signal.  An
          ;; uncaught process-sentinel error terminates a noninteractive test
          ;; editor before it can emit a structured outcome.
          (set-process-sentinel
           curl-process
           (lambda (process event)
             (condition-case error-data
                 (funcall package-sentinel process event)
               (error
                (setq transport-error
                      (list (car error-data)
                            (cdr error-data)
                            (error-message-string error-data)))))))
          (while (process-live-p curl-process)
            (accept-process-output curl-process 0.01))
          (neomacs-org-cliplink-test-wait-for
           (lambda () transport-error)
           "the delegated curl sentinel error")
          (list
           :transport-error transport-error
           :buffer (buffer-string)
           :point (point)
           :modified (buffer-modified-p)
           :link-present (string-match-p "\\[\\[" (buffer-string))
           :exit-status (process-exit-status curl-process)
           :argv
           (mapcar
            (lambda (argument)
              (neomacs-org-cliplink-test-normalize-origin argument base-url))
            (split-string
             (neomacs-org-cliplink-test-read-file argv-file) "\n" t))
           :requests neomacs-org-cliplink-test-requests))))))
"####;
    let expect = expect![[
        r####"OK (:transport-error (error ("curl: deterministic request failure\n") "curl: deterministic request failure\n") :buffer "* Release blockers\n- Failed transport: " :point 40 :modified nil :link-present nil :exit-status 22 :argv ("--fail" "--location" "--max-time" "5" "--include" "--silent" "--show-error" "-X" "GET" "<ORIGIN>/maintenance/curl") :requests ((:request-line "GET /maintenance/curl HTTP/1.1" :authorization nil :accept-encoding nil)))"####
    ]];
    ParityBatchCase::value(
        "curl_http_failure_runs_the_real_nonzero_sentinel_without_editing",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn empty_clipboard_surfaces_the_gnu_error_without_editing_or_requesting() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site nil
  (with-temp-buffer
    (org-mode)
    (insert "* Pending links\n- ")
    (goto-char (point-max))
    (set-buffer-modified-p nil)
    (condition-case error-data
        (progn
          (call-interactively #'org-cliplink)
          (list :unexpected-success t))
      (error
       (list
        :signal (car error-data)
        :data (cdr error-data)
        :message (error-message-string error-data)
        :buffer (buffer-string)
        :point (point)
        :modified (buffer-modified-p)
        :requests neomacs-org-cliplink-test-requests)))))
"####;
    let expect = expect![[
        r####"OK (:signal error :data ("Kill ring is empty") :message "Kill ring is empty" :buffer "* Pending links\n- " :point 19 :modified nil :requests nil)"####
    ]];
    ParityBatchCase::value(
        "empty_clipboard_surfaces_the_gnu_error_without_editing_or_requesting",
        elisp_form,
        expect,
    )
}

fn http_service_failure_preserves_the_package_error_page_link_behavior() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-org-cliplink-test-with-site
    '(("/maintenance"
       :status "503 Service Unavailable"
       :body "<html><title>Deployment Console Maintenance</title><body>retry later</body></html>"))
  (let ((url (concat base-url "/maintenance")))
    (with-temp-buffer
      (org-mode)
      (insert "* Release blockers\n- Status page: ")
      (kill-new url)
      (call-interactively #'org-cliplink)
      (neomacs-org-cliplink-test-wait-for
       (lambda () (string-match-p "Console Maintenance" (buffer-string)))
       "Org Cliplink's HTTP failure callback")
      (goto-char (point-max))
      (list
       :buffer
       (neomacs-org-cliplink-test-normalize-origin
        (buffer-string) base-url)
       :link (neomacs-org-cliplink-test-link-state base-url)
       :point-at-end (= (point) (point-max))
       :requests neomacs-org-cliplink-test-requests))))
"####;
    let expect = expect![[
        r####"OK (:buffer "* Release blockers\n- Status page: [[<ORIGIN>/maintenance][Deployment Console Maintenance]]" :link (:type link :raw-link "<ORIGIN>/maintenance" :contents "Deployment Console Maintenance") :point-at-end t :requests ((:request-line "GET /maintenance HTTP/1.1" :authorization nil :accept-encoding "gzip")))"####
    ]];
    ParityBatchCase::value(
        "http_service_failure_preserves_the_package_error_page_link_behavior",
        elisp_form,
        expect,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        interactive_link_insertion_survives_buffer_switch_and_normalizes_real_html(),
        capture_returns_a_truncated_link_without_editing_the_capture_buffer(),
        configured_tracker_title_replacement_produces_a_concise_operational_link(),
        page_without_a_title_falls_back_to_a_valid_bare_org_link(),
        https_url_el_auth_uses_the_exact_entry_after_a_wildcard_entry(),
        duplicate_url_el_completion_inserts_one_link_and_runs_one_transformer(),
        gzip_encoded_page_is_decompressed_through_url_el_before_link_insertion(),
        documented_custom_transformer_cleans_a_runbook_title_before_insertion(),
        authenticated_curl_https_records_real_argv_and_masks_its_log(),
        curl_http_failure_runs_the_real_nonzero_sentinel_without_editing(),
        empty_clipboard_surfaces_the_gnu_error_without_editing_or_requesting(),
        http_service_failure_preserves_the_package_error_page_link_behavior(),
    ]
}
