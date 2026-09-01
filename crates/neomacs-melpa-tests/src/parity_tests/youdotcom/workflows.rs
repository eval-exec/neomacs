use expect_test::expect;

use super::ParityBatchCase;

fn starts_a_session_and_runs_search_then_rag_with_exact_rendering() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((youdotcom-buffer-name "*Youdotcom parity session*")
       (youdotcom-search-api-key "search-key-λ")
       (youdotcom-rag-api-key "rag-key-β")
       (youdotcom-number-of-results 3)
       (youdotcom-session-started nil)
       (neomacs-melpa-youdotcom--response-buffers nil)
       (inputs (list "/search resilient systems λ"
                     "/rag summarize release risks"))
       prompts requests state face-runs)
   (unwind-protect
       (save-window-excursion
         (cl-letf (((symbol-function 'read-string)
                    (lambda (prompt &rest _)
                      (push prompt prompts)
                      (pop inputs)))
                   ((symbol-function 'url-retrieve)
                    (lambda (url callback callback-arguments &rest _)
                      (push
                       (list :url url
                             :method url-request-method
                             :headers (copy-tree url-request-extra-headers)
                             :data url-request-data
                             :callback callback
                             :callback-arguments callback-arguments)
                       requests)
                      (let* ((search-p (string-match-p "/search?" url))
                             (json
                              (if search-p
                                  "{\"hits\":[{\"title\":\"Resilient Systems λ\",\"description\":\"Patterns for dependable services.\",\"snippets\":[\"Retry with bounded backoff.\",\"Keep Unicode: λ.\"],\"url\":\"https://example.test/resilience?q=λ\"},{\"title\":\"Failure Budgets\",\"description\":\"Balance reliability and delivery.\",\"snippets\":[\"Measure burn rate.\",\"Review duplicate alerts.\"],\"url\":\"https://example.test/budgets\"}]}"
                                "{\"answer\":\"Prioritize rollback safety, migration checks, and alert ownership — then rehearse recovery.\"}"))
                             (buffer
                              (neomacs-melpa-youdotcom--response-buffer json)))
                        (with-current-buffer buffer
                          (apply callback nil callback-arguments))
                        buffer))))
           (youdotcom-enter)
           (with-current-buffer youdotcom-buffer-name
             (let ((position (point-min)))
               (while (< position (point-max))
                 (let ((next
                        (or (next-single-property-change
                             position 'face nil (point-max))
                            (point-max))))
                   (push
                    (list
                     (buffer-substring-no-properties position next)
                     (copy-tree (get-text-property position 'face)))
                    face-runs)
                   (setq position next))))
             (setq state
                   (list
                    :text (buffer-substring-no-properties (point-min) (point-max))
                    :point (point)
                    :mode major-mode
                    :session youdotcom-session-started
                    :current-buffer (buffer-name)
                    :faces (nreverse face-runs))))
           (list :prompts (nreverse prompts)
                 :requests (nreverse requests)
                 :state state)))
     (when (get-buffer youdotcom-buffer-name)
       (kill-buffer youdotcom-buffer-name))
     (neomacs-melpa-youdotcom--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:prompts ("> " "> ") :requests ((:url "https://api.ydc-index.io/search?query=resilient%20systems%20%CE%BB&num_web_results=3" :method "GET" :headers (("X-API-Key" . "search-key-λ") ("Content-Type" . "application/json")) :data nil :callback youdotcom-handle-response :callback-arguments ("resilient systems λ" "search")) (:url "https://api.ydc-index.io/rag?query=summarize%20release%20risks" :method "GET" :headers (("X-API-Key" . "rag-key-β") ("Content-Type" . "application/json")) :data nil :callback youdotcom-handle-response :callback-arguments ("summarize release risks" "rag"))) :state (:text "user: resilient systems λ\nassistant: \n\n# Title: Resilient Systems λ\n\n## Description : Patterns for dependable services.\n\nRetry with bounded backoff.\nKeep Unicode: λ.\n\nhttps://example.test/resilience?q=λ\n\n\n# Title: Failure Budgets\n\n## Description : Balance reliability and delivery.\n\nMeasure burn rate.\nReview duplicate alerts.\n\nhttps://example.test/budgets\n\n\nuser: summarize release risks\nassistant: Prioritize rollback safety, migration checks, and alert ownership — then rehearse recovery.\n\n" :point 1 :mode youdotcom-mode :session t :current-buffer "*Youdotcom parity session*" :faces (("user: resilient systems λ\n" (:foreground "red")) ("assistant: \n\n# Title: Resilient Systems λ\n\n## Description : Patterns for dependable services.\n\nRetry with bounded backoff.\nKeep Unicode: λ.\n\nhttps://example.test/resilience?q=λ\n\n\n# Title: Failure Budgets\n\n## Description : Balance reliability and delivery.\n\nMeasure burn rate.\nReview duplicate alerts.\n\nhttps://example.test/budgets\n\n\n" nil) ("user: summarize release risks\n" (:foreground "red")) ("assistant: Prioritize rollback safety, migration checks, and alert ownership — then rehearse recovery.\n\n" nil))))"####
    ]];
    ParityBatchCase::value(
        "starts_a_session_and_runs_search_then_rag_with_exact_rendering",
        elisp_form,
        expect,
    )
}

fn handles_help_invalid_empty_clear_and_quit_commands() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((youdotcom-buffer-name "*Youdotcom command parity*")
       (youdotcom-session-started t)
       (commands
        '("/help" "/unknown payload" "/search" "/rag" "/clear" "/quit"))
       next-input prompts messages states network-calls)
   (unwind-protect
       (let ((buffer (get-buffer-create youdotcom-buffer-name)))
         (with-current-buffer buffer
           (insert "stale search output\n"))
         (cl-letf (((symbol-function 'read-string)
                    (lambda (prompt &rest _)
                      (push prompt prompts)
                      next-input))
                   ((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let ((rendered (apply #'format format-string arguments)))
                        (push rendered messages)
                        rendered)))
                   ((symbol-function 'url-retrieve)
                    (lambda (&rest _)
                      (setq network-calls (1+ (or network-calls 0))))))
           (dolist (command commands)
             (setq next-input command)
             (with-current-buffer buffer
               (youdotcom-start))
             (push
              (list command
                    (buffer-live-p buffer)
                    (and (buffer-live-p buffer)
                         (with-current-buffer buffer (buffer-string)))
                    youdotcom-session-started)
              states)))
         (list :prompts (nreverse prompts)
               :messages (nreverse messages)
               :states (nreverse states)
               :network-calls (or network-calls 0)))
     (when (get-buffer youdotcom-buffer-name)
       (kill-buffer youdotcom-buffer-name)))))
"####;
    let expect = expect![[
        r####"OK (:prompts ("> " "> " "> " "> " "> " "> ") :messages ("Commands: /quit, /clear, /help, /search, /rag" "Invalid command. type /help for available commands." "Please provide a query" "Please provide a query") :states (("/help" t "stale search output\n" t) ("/unknown payload" t "stale search output\n" t) ("/search" t "stale search output\n" t) ("/rag" t "stale search output\n" t) ("/clear" t "" t) ("/quit" nil nil nil)) :network-calls 0)"####
    ]];
    ParityBatchCase::value(
        "handles_help_invalid_empty_clear_and_quit_commands",
        elisp_form,
        expect,
    )
}

fn reports_invalid_configuration_and_malformed_service_data() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((youdotcom-buffer-name "*Youdotcom failure parity*")
       (neomacs-melpa-youdotcom--response-buffers nil)
       (youdotcom-session-started nil)
       missing-key zero-results malformed-json malformed-buffer request)
   (unwind-protect
       (save-window-excursion
         (setq missing-key
               (condition-case error-data
                   (let ((youdotcom-search-api-key "")
                         (youdotcom-rag-api-key ""))
                     (cl-letf (((symbol-function 'read-string)
                                (lambda (&rest _)
                                  "/search production outage"))
                               ((symbol-function 'url-retrieve)
                                (lambda (&rest _) 'unexpected-network-call)))
                       (youdotcom-enter)))
                 (error
                  (list error-data
                        :buffer-live (and (get-buffer youdotcom-buffer-name) t)
                        :mode (and (get-buffer youdotcom-buffer-name)
                                   (buffer-local-value
                                    'major-mode
                                    (get-buffer youdotcom-buffer-name)))
                        :session youdotcom-session-started))))
         (when (get-buffer youdotcom-buffer-name)
           (kill-buffer youdotcom-buffer-name))
         (setq youdotcom-session-started nil)
         (setq zero-results
               (condition-case error-data
                   (let ((youdotcom-search-api-key "valid-search-key")
                         (youdotcom-number-of-results 0))
                     (cl-letf (((symbol-function 'read-string)
                                (lambda (&rest _) "/search capacity planning"))
                               ((symbol-function 'url-retrieve)
                                (lambda (&rest _) 'unexpected-network-call)))
                       (youdotcom-enter)))
                 (error
                  (list error-data
                        :buffer-live (and (get-buffer youdotcom-buffer-name) t)
                        :mode (and (get-buffer youdotcom-buffer-name)
                                   (buffer-local-value
                                    'major-mode
                                    (get-buffer youdotcom-buffer-name)))
                        :session youdotcom-session-started))))
         (when (get-buffer youdotcom-buffer-name)
           (kill-buffer youdotcom-buffer-name))
         (setq youdotcom-session-started nil)
         (setq malformed-json
               (condition-case error-data
                   (let ((youdotcom-search-api-key "valid-search-key")
                         (youdotcom-number-of-results 2))
                     (cl-letf (((symbol-function 'read-string)
                                (lambda (&rest _) "/search malformed response"))
                               ((symbol-function 'url-retrieve)
                                (lambda (url callback callback-arguments &rest _)
                                  (setq request
                                        (list url url-request-extra-headers
                                              callback-arguments))
                                  (setq malformed-buffer
                                        (neomacs-melpa-youdotcom--response-buffer
                                         "{not valid json"))
                                  (with-current-buffer malformed-buffer
                                    (apply callback nil callback-arguments))
                                  malformed-buffer)))
                       (youdotcom-enter)))
                 (error
                  (list error-data
                        :buffer-live (and (get-buffer youdotcom-buffer-name) t)
                        :mode (and (get-buffer youdotcom-buffer-name)
                                   (buffer-local-value
                                    'major-mode
                                    (get-buffer youdotcom-buffer-name)))
                        :session youdotcom-session-started))))
         (list
          :missing-key missing-key
          :zero-results zero-results
          :malformed-json malformed-json
          :request request
          :response-buffer-live (buffer-live-p malformed-buffer)
          :result
          (with-current-buffer youdotcom-buffer-name
            (list (buffer-string) major-mode youdotcom-session-started))))
     (when (get-buffer youdotcom-buffer-name)
       (kill-buffer youdotcom-buffer-name))
     (neomacs-melpa-youdotcom--cleanup-response-buffers))))
"####;
    let expect = expect![[
        r####"OK (:missing-key ((error "Invalid arguments or global variables") :buffer-live t :mode youdotcom-mode :session t) :zero-results ((error "Invalid arguments or global variables") :buffer-live t :mode youdotcom-mode :session t) :malformed-json ((json-end-of-file) :buffer-live t :mode youdotcom-mode :session t) :request ("https://api.ydc-index.io/search?query=malformed%20response&num_web_results=2" (("X-API-Key" . "valid-search-key") ("Content-Type" . "application/json")) ("malformed response" "search")) :response-buffer-live t :result ("" youdotcom-mode t))"####
    ]];
    ParityBatchCase::value(
        "reports_invalid_configuration_and_malformed_service_data",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        starts_a_session_and_runs_search_then_rag_with_exact_rendering(),
        handles_help_invalid_empty_clear_and_quit_commands(),
        reports_invalid_configuration_and_malformed_service_data(),
    ]
}
