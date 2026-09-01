use expect_test::expect;

use super::ParityBatchCase;

fn anx_api_authenticated_campaign_session_uses_real_http_parsing_and_result_buffers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "anx_api_authenticated_campaign_session_uses_real_http_parsing_and_result_buffers",
        r##"(let ((*anx-sandbox-url* "https://sandbox.example/v1")
       (*anx-production-url* "https://api.example/v1")
       (*anx-current-url* "https://sandbox.example/v1")
       (anx-username nil)
       (anx-password nil)
       requests
       password-prompts
       response-buffers
       result-buffers
       payload-buffer
       setup
       selections)
  (unwind-protect
      (cl-letf
          (((symbol-function 'read-passwd)
            (lambda (prompt &rest _)
              (push prompt password-prompts)
              "correct horse"))
           ((symbol-function 'url-retrieve-synchronously)
            (lambda (url)
              (let* ((request
                      (list
                       url
                       url-request-method
                       url-request-extra-headers
                       url-request-data))
                     (json
                      (cond
                       ((string-suffix-p "/auth" url)
                        "{\"response\":{\"status\":\"OK\",\"token\":\"session-17\"}}")
                       ((string-suffix-p "/user?current" url)
                        "{\"response\":{\"status\":\"OK\"},\"user\":{\"id\":42,\"name\":\"Operator\"}}")
                       ((string-suffix-p
                         "/campaign/9?stats=true" url)
                        "{\"response\":{\"status\":\"OK\"},\"campaign\":{\"id\":9,\"name\":\"Launch\"},\"stats\":{\"impressions\":1200,\"clicks\":48}}")
                       (t
                        "{\"response\":{\"status\":\"OK\"},\"campaign\":{\"id\":9,\"name\":\"Launch\",\"active\":true}}")))
                     (buffer
                      (generate-new-buffer
                       " *anx-http-response*")))
                (push request requests)
                (push buffer response-buffers)
                (with-current-buffer buffer
                  (insert
                   "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n"
                   json)
                  (setq-local
                   url-http-end-of-headers
                   (save-excursion
                     (goto-char (point-min))
                     (search-forward "\r\n\r\n")
                     (point))))
                buffer))))
        (setq
         setup
         (list
          (anx-get-user-authentication-credentials
           "operator@example.test")
          anx-username
          anx-password
          (nreverse password-prompts)
          (anx-display-current-api-url)
          (progn
            (anx-toggle-current-api-url)
            (anx-display-current-api-url))
          (progn
            (anx-toggle-current-api-url)
            (anx-display-current-api-url))
          *anx-current-url*))
        (save-window-excursion
          (anx-authenticate)
          (push (current-buffer) result-buffers)
          (push
           (list
            'authenticate
            (buffer-name (current-buffer))
            (buffer-name (window-buffer (selected-window))))
           selections)
          (anx-get "campaign/9?stats=true")
          (push (current-buffer) result-buffers)
          (push
           (list
            'campaign-get
            (buffer-name (current-buffer))
            (buffer-name (window-buffer (selected-window))))
           selections)
          (anx-who-am-i)
          (push (current-buffer) result-buffers)
          (push
           (list
            'who-am-i
            (buffer-name (current-buffer))
            (buffer-name (window-buffer (selected-window))))
           selections)
          (setq payload-buffer
                (generate-new-buffer "campaign-update"))
          (switch-to-buffer payload-buffer)
          (insert
           "(:campaign (:id 9 :name \"Launch\" :active t)"
           " :audit (:ticket \"OPS-314\"))")
          (anx-send-buffer "PUT" "campaign/9")
          (push (current-buffer) result-buffers)
          (push
           (list
            'campaign-update
            (buffer-name (current-buffer))
            (buffer-name (window-buffer (selected-window))))
           selections))
        (list
         setup
         (nreverse requests)
         (nreverse selections)
         (mapcar
          (lambda (buffer)
            (with-current-buffer buffer
              (list
               (buffer-name)
               major-mode
               buffer-offer-save
               (buffer-modified-p)
               (point)
               (buffer-substring (point-min) (point-max))
               (read (buffer-string)))))
          (nreverse result-buffers))))
    (mapc
     (lambda (buffer)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (set-buffer-modified-p nil))
         (kill-buffer buffer)))
     (append
      response-buffers
      result-buffers
      (list payload-buffer)))))"##,
        expect![[
            r#"OK (("correct horse" "operator@example.test" "correct horse" ("password: ") "current api url is https://sandbox.example/v1" "current api url is https://api.example/v1" "current api url is https://sandbox.example/v1" "https://sandbox.example/v1") (("https://sandbox.example/v1/auth" "POST" #1=(("Content-Type" . "application/x-www-form-urlencoded")) "{\"auth\":{\"username\":\"operator@example.test\",\"password\":\"correct horse\"}}") ("https://sandbox.example/v1/campaign/9?stats=true" "GET" #1# "") ("https://sandbox.example/v1/user?current" "GET" #1# "") ("https://sandbox.example/v1/campaign/9" "PUT" #1# "{\"campaign\":{\"id\":9,\"name\":\"Launch\",\"active\":true},\"audit\":{\"ticket\":\"OPS-314\"}}")) ((authenticate "https://sandbox.example/v1/auth" "https://sandbox.example/v1/auth") (campaign-get "https://sandbox.example/v1/campaign/9?stats=true" "https://sandbox.example/v1/campaign/9?stats=true") (who-am-i "*anx-who-am-i*" "*anx-who-am-i*") (campaign-update "https://sandbox.example/v1/campaign/9[PUT]" "https://sandbox.example/v1/campaign/9[PUT]")) (("https://sandbox.example/v1/auth" emacs-lisp-mode t t 54 "\n((response (status . \"OK\") (token . \"session-17\")))\n" ((response (status . "OK") (token . "session-17")))) ("https://sandbox.example/v1/campaign/9?stats=true" emacs-lisp-mode t t 112 "\n((response (status . \"OK\")) (campaign (id . 9) (name . \"Launch\")) (stats (impressions . 1200) (clicks . 48)))\n" ((response (status . "OK")) (campaign (id . 9) (name . "Launch")) (stats (impressions . 1200) (clicks . 48)))) ("*anx-who-am-i*" emacs-lisp-mode t t 68 "\n((response (status . \"OK\")) (user (id . 42) (name . \"Operator\")))\n" ((response (status . "OK")) (user (id . 42) (name . "Operator")))) ("https://sandbox.example/v1/campaign/9[PUT]" emacs-lisp-mode t t 82 "\n((response (status . \"OK\")) (campaign (id . 9) (name . \"Launch\") (active . t)))\n" ((response (status . "OK")) (campaign (id . 9) (name . "Launch") (active . t))))))"#
        ]],
    )
}

fn anx_api_documented_lisp_json_edit_roundtrip_preserves_a_real_campaign_payload() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anx_api_documented_lisp_json_edit_roundtrip_preserves_a_real_campaign_payload",
        r##"(let (source json-buffer lisp-buffer)
  (unwind-protect
      (save-window-excursion
        (setq source (generate-new-buffer "campaign-draft"))
        (switch-to-buffer source)
        (insert
         "(:campaign (:id 9 :name \"Launch\" :active t)"
         " :segments [101 202] :budget 1250)")
        (anx-lisp-to-json)
        (setq json-buffer (get-buffer "campaign-draft.json"))
        (let ((json-selection
               (list
                (buffer-name (current-buffer))
                (buffer-name
                 (window-buffer (selected-window)))))
              (escaped-buffer-before-edit
               (buffer-substring (point-min) (point-max)))
              (escaped-before-edit (read (buffer-string))))
          (anx-unescape-json)
          (goto-char (point-min))
          (search-forward "\"Launch\"")
          (replace-match "\"Launch revised\"" t t)
          (goto-char (point-min))
          (search-forward "\"budget\":1250")
          (replace-match "\"budget\":1750" t t)
          (let ((json-after-edit
                 (buffer-substring
                  (point-min) (point-max))))
            (anx-escape-json)
            (let ((escaped-buffer-after-edit
                   (buffer-substring
                    (point-min) (point-max)))
                  (escaped-after-edit (read (buffer-string))))
              (anx-json-to-lisp)
              (setq lisp-buffer
                    (get-buffer "campaign-draft.json.el"))
              (list
               json-selection
               escaped-buffer-before-edit
               escaped-before-edit
               json-after-edit
               escaped-buffer-after-edit
               escaped-after-edit
               (list
                (buffer-name (current-buffer))
                (buffer-name
                 (window-buffer (selected-window))))
               (with-current-buffer lisp-buffer
                 (list
                  major-mode
                  buffer-offer-save
                  (buffer-modified-p)
                  (point)
                  (buffer-substring
                   (point-min) (point-max))
                  (read (buffer-string)))))))))
    (dolist (buffer (list source json-buffer lisp-buffer))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))))"##,
        expect![[
            r##"OK (("campaign-draft.json" "campaign-draft.json") "\n\"{\\\"campaign\\\":{\\\"id\\\":9,\\\"name\\\":\\\"Launch\\\",\\\"active\\\":true},\\\"segments\\\":[101,202],\\\"budget\\\":1250}\"\n" "{\"campaign\":{\"id\":9,\"name\":\"Launch\",\"active\":true},\"segments\":[101,202],\"budget\":1250}" "\n{\"campaign\":{\"id\":9,\"name\":\"Launch revised\",\"active\":true},\"segments\":[101,202],\"budget\":1750}\n" "\"\n{\\\"campaign\\\":{\\\"id\\\":9,\\\"name\\\":\\\"Launch revised\\\",\\\"active\\\":true},\\\"segments\\\":[101,202],\\\"budget\\\":1750}\n\"" "\n{\"campaign\":{\"id\":9,\"name\":\"Launch revised\",\"active\":true},\"segments\":[101,202],\"budget\":1750}\n" ("campaign-draft.json.el" "campaign-draft.json.el") (emacs-lisp-mode t t 102 "\n((campaign (id . 9) (name . \"Launch revised\") (active . t)) (segments . [101 202]) (budget . 1750))\n" ((campaign (id . 9) (name . "Launch revised") (active . t)) (segments . [101 202]) (budget . 1750))))"##
        ]],
    )
}

fn anx_api_raw_report_download_can_be_inspected_and_saved_to_the_configured_archive()
-> ParityBatchCase {
    ParityBatchCase::value(
        "anx_api_raw_report_download_can_be_inspected_and_saved_to_the_configured_archive",
        r##"(let* ((root
         (file-name-as-directory
          (expand-file-name
           "anx-api-report-workflow"
           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
        (archive (expand-file-name "archive/" root))
        (url "https://reports.example.test/download?id=77")
        response
        report-buffer
        requested)
  (unwind-protect
      (progn
        (make-directory archive t)
        (setq response (generate-new-buffer " *anx-report-response*"))
        (with-current-buffer response
          (insert
           "HTTP/1.1 200 OK\r\n"
           "Content-Type: text/csv\r\n\r\n"
           "campaign,impressions,clicks\n"
           "Launch,1200,48\n"))
        (save-window-excursion
          (cl-letf
              (((symbol-function 'url-retrieve-synchronously)
                (lambda (requested-url)
                  (setq requested
                        (list
                         requested-url
                         url-request-method
                         url-request-extra-headers))
                  response))
               ((symbol-function 'current-time-string)
                (lambda () "Mon Jan 02 03:04:05 2006")))
            (anx-raw-get url)
            (setq report-buffer (current-buffer))
            (let ((anx-save-directory archive)
                  (displayed (buffer-substring
                              (point-min) (point-max)))
                  (display-name (buffer-name))
                  (selection
                   (list
                    (buffer-name (current-buffer))
                    (buffer-name
                     (window-buffer (selected-window))))))
              (anx-save-buffer-contents)
              (let ((saved-file (buffer-file-name)))
                (list
                 requested
                 selection
                 display-name
                 (buffer-name)
                 major-mode
                 buffer-offer-save
                 (point)
                 displayed
                 (file-relative-name saved-file root)
                 (file-exists-p saved-file)
                 (with-temp-buffer
                   (insert-file-contents-literally saved-file)
                   (buffer-substring
                    (point-min) (point-max)))))))))
    (when (buffer-live-p report-buffer)
      (with-current-buffer report-buffer
        (set-buffer-modified-p nil))
      (kill-buffer report-buffer))
    (when (buffer-live-p response)
      (kill-buffer response))
    (when (file-exists-p root)
      (delete-directory root t))))"##,
        expect![[
            r#"OK (("https://reports.example.test/download?id=77" nil nil) ("https://reports.example.test/download?id=77" "https://reports.example.test/download?id=77") "https://reports.example.test/download?id=77" "https:__reports.example.test_download?id=77_Mon_Jan_02_03:04:05_2006" fundamental-mode t 102 "\n\"HTTP/1.1 200 OK\\15\\nContent-Type: text/csv\\15\\n\\15\\ncampaign,impressions,clicks\\nLaunch,1200,48\\n\"\n" "archive/https:__reports.example.test_download?id=77_Mon_Jan_02_03:04:05_2006" t "\n\"HTTP/1.1 200 OK\\15\\nContent-Type: text/csv\\15\\n\\15\\ncampaign,impressions,clicks\\nLaunch,1200,48\\n\"\n")"#
        ]],
    )
}

pub(super) fn practical_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        anx_api_authenticated_campaign_session_uses_real_http_parsing_and_result_buffers(),
        anx_api_documented_lisp_json_edit_roundtrip_preserves_a_real_campaign_payload(),
        anx_api_raw_report_download_can_be_inspected_and_saved_to_the_configured_archive(),
    ]
}
