use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, REQUEST_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const REQUEST_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const REQUEST_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'json)
(require 'request)

(defun request-test-response-summary (response)
  (list
   :status-code (request-response-status-code response)
   :symbol-status (request-response-symbol-status response)
   :done (request-response-done-p response)
   :url (request-response-url response)
   :data (copy-tree (request-response-data response))
   :error (copy-tree (request-response-error-thrown response))
   :backend (request-response--backend response)
   :raw-header (request-response--raw-header response)
   :buffer-live
   (buffer-live-p (request-response--buffer response))))

(defun request-test-json-plist ()
  (let ((json-object-type 'plist)
        (json-array-type 'list)
        (json-key-type 'keyword)
        (json-false :false)
        (json-null :null))
    (json-read)))

(defun request-test-normalize-root (value root)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name root))
   "[PROJECT]"
   value t t))
"##;

fn request_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(REQUEST_MELPA_PIN, "request.el")
        .expect("prepare pinned request source below ./tmp")
        .with_prelude(REQUEST_TEST_PRELUDE)
        .with_timeout(REQUEST_TEST_TIMEOUT)
}

fn request_builder_encodes_params_form_data_and_preserves_explicit_content_type() -> ParityBatchCase
{
    let elisp_form = r##"
(let (calls)
  (cl-letf
      (((symbol-function 'request--choose-backend)
        (lambda (method)
          (lambda (url &rest settings)
            (let ((response (plist-get settings :response)))
              (push
               (list
                :method method
                :url url
                :data (copy-tree (plist-get settings :data))
                :headers (copy-tree (plist-get settings :headers))
                :encoding (plist-get settings :encoding)
                :type (plist-get settings :type)
                :response-same
                (eq response
                    (plist-get
                     (request-response-settings response)
                     :response)))
               calls)
              (setf (request-response-status-code response) 200
                    (request-response-symbol-status response) 'success
                    (request-response-done-p response) t))))))
    (let ((request-backend 'curl))
      (request
       "https://api.example.test/search?existing=1"
       :params '((query . "space & λ") (page . 2))
       :data '((scope . "release notes") (limit . 10))
       :type "POST"
       :sync t)
      (request
       "https://api.example.test/events"
       :data '((event . "publish") (ok . t))
       :headers '(("Content-Type" . "application/json")
                  ("X-Trace" . "trace-41"))
       :type "PUT"
       :sync t)))
  (nreverse calls))
"##;
    let expect = expect![[
        r####"OK ((:method request-sync :url "https://api.example.test/search?existing=1&query=space%20%26%20%CE%BB&page=2" :data "scope=release%20notes&limit=10" :headers nil :encoding utf-8 :type "POST" :response-same t) (:method request-sync :url "https://api.example.test/events" :data ((event . "publish") (ok . t)) :headers (("Content-Type" . "application/json") ("X-Trace" . "trace-41")) :encoding utf-8 :type "PUT" :response-same t))"####
    ]];
    ParityBatchCase::value(
        "request_builder_encodes_params_form_data_and_preserves_explicit_content_type",
        elisp_form,
        expect,
    )
}

fn response_headers_support_case_insensitive_duplicates_and_structured_alist_extraction()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((response
       (make-request-response
        :status-code 206
        :-raw-header
        (concat
         "HTTP/1.1 206 Partial Content\n"
         "Content-Type: application/json; charset=utf-8\n"
         "X-Trace: alpha\n"
         "x-trace: beta\n"
         "Cache-Control: no-cache\n"
         "X-Long: first\n\tcontinued\n"))))
  (list
   :content-type
   (request-response-header response "content-type")
   :trace (request-response-header response "X-TRACE")
   :long (request-response-header response "x-long")
   :missing (request-response-header response "missing")
   :headers (request-response-headers response)))
"##;
    let expect = expect![[
        r####"OK (:content-type "application/json; charset=utf-8" :trace "alpha, beta" :long "first\n\11continued" :missing nil :headers ((content-type . "application/json; charset=utf-8") (x-trace . "alpha") (x-trace . "beta") (cache-control . "no-cache") (x-long . "first continued")))"####
    ]];
    ParityBatchCase::value(
        "response_headers_support_case_insensitive_duplicates_and_structured_alist_extraction",
        elisp_form,
        expect,
    )
}

fn successful_json_callback_runs_success_status_and_complete_in_order_with_same_response()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((buffer (generate-new-buffer " *request-success*"))
       (response
        (make-request-response
         :status-code 201
         :url "https://api.example.test/releases/41"
         :-backend 'curl
         :-buffer buffer))
       events)
  (with-current-buffer buffer
    (insert
     (concat
      "HTTP/1.1 201 Created\r\n"
      "Content-Type: application/json\r\n"
      "X-Release: 41\r\n\r\n"
      "{\"artifact\":\"neomacs\",\"version\":41,\"steps\":[\"build\",\"publish\"]}")))
  (request--callback
   buffer
   :response response
   :encoding 'utf-8
   :parser #'request-test-json-plist
   :success
   (cl-function
    (lambda (&key data symbol-status response &allow-other-keys)
      (push
       (list 'success symbol-status
             (plist-get data :artifact)
             (request-response-status-code response))
       events)))
   :status-code
   `((201 . ,(cl-function
              (lambda (&key data response &allow-other-keys)
                (push
                 (list 'status-201
                       (plist-get data :version)
                       (request-response-header response "x-release"))
                 events)))))
   :complete
   (cl-function
    (lambda (&key symbol-status response &allow-other-keys)
      (push
       (list 'complete symbol-status
             (request-response-done-p response))
       events))))
  (list
   :response (request-test-response-summary response)
   :headers (request-response-headers response)
   :events (nreverse events)))
"##;
    let expect = expect![[
        r####"OK (:response (:status-code 201 :symbol-status success :done t :url "https://api.example.test/releases/41" :data (:artifact "neomacs" :version 41 :steps ("build" "publish")) :error nil :backend curl :raw-header "HTTP/1.1 201 Created\nContent-Type: application/json\nX-Release: 41\n" :buffer-live nil) :headers ((content-type . "application/json") (x-release . "41")) :events ((success success "neomacs" 201) (status-201 41 "41") (complete success nil)))"####
    ]];
    ParityBatchCase::value(
        "successful_json_callback_runs_success_status_and_complete_in_order_with_same_response",
        elisp_form,
        expect,
    )
}

fn parser_and_http_failures_preserve_body_data_and_route_error_status_complete_callbacks()
-> ParityBatchCase {
    let elisp_form = r##"
(let (results)
  (dolist
      (spec
       '((parser-error 200 nil "not-json")
         (http-error 422 (error http 422)
                     "{\"message\":\"invalid release\"}")))
    (pcase-let* ((`(,name ,code ,initial-error ,body) spec)
                 (buffer (generate-new-buffer " *request-error*"))
                 (response
                  (make-request-response
                   :status-code code
                   :url "https://api.example.test/releases"
                   :error-thrown initial-error
                   :-backend 'curl
                   :-buffer buffer))
                 (events nil))
      (with-current-buffer buffer
        (insert
         (format
          "HTTP/1.1 %d Failure\r\nContent-Type: application/json\r\n\r\n%s"
          code body)))
      (request--callback
       buffer
       :response response
       :encoding 'utf-8
       :parser
       (if (eq name 'parser-error)
           (lambda () (error "invalid response schema"))
         #'request-test-json-plist)
       :error
       (cl-function
        (lambda (&key data symbol-status error-thrown &allow-other-keys)
          (push
           (list 'error symbol-status
                 (copy-tree error-thrown)
                 (copy-tree data))
           events)))
       :status-code
       `((,code . ,(cl-function
                    (lambda (&key symbol-status &allow-other-keys)
                      (push (list 'status code symbol-status) events)))))
       :complete
       (cl-function
        (lambda (&key symbol-status &allow-other-keys)
          (push (list 'complete symbol-status) events))))
      (push
       (list name
             (request-test-response-summary response)
             (nreverse events))
       results)))
  (nreverse results))
"##;
    let expect = expect![[
        r####"OK ((parser-error (:status-code 200 :symbol-status parse-error :done t :url "https://api.example.test/releases" :data nil :error (error "invalid response schema") :backend curl :raw-header "HTTP/1.1 200 Failure\nContent-Type: application/json\n" :buffer-live nil) ((error parse-error (error "invalid response schema") nil) (status 200 parse-error) (complete parse-error))) (http-error (:status-code 422 :symbol-status error :done t :url "https://api.example.test/releases" :data (:message "invalid release") :error (error http 422) :backend curl :raw-header "HTTP/1.1 422 Failure\nContent-Type: application/json\n" :buffer-live nil) ((error error (error http 422) (:message "invalid release")) (status 422 error) (complete error))))"####
    ]];
    ParityBatchCase::value(
        "parser_and_http_failures_preserve_body_data_and_route_error_status_complete_callbacks",
        elisp_form,
        expect,
    )
}

fn curl_preprocessor_consumes_continue_redirect_headers_and_absolutifies_history() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (insert
   (concat
    "HTTP/1.1 100 Continue\r\n\r\n"
    "HTTP/1.1 302 Found\r\n"
    "Location: /v2/items\r\n"
    "X-Hop: edge-a\r\n\r\n"
    "HTTP/1.1 200 OK\r\n"
    "Content-Type: application/json\r\n"
    "X-Final: origin\r\n\r\n"
    "{\"items\":[1,2]}"
    "\n(:num-redirects 1 :url-effective \"https://api.example.test/v2/items\")"))
  (let* ((start-url "https://api.example.test/v1/items")
         (info (request--curl-preprocess start-url))
         (history (plist-get info :history)))
    (request--curl-absolutify-location-history start-url history)
    (list
     :final-buffer (buffer-string)
     :info
     (list
      :redirects (plist-get info :num-redirects)
      :effective (plist-get info :url-effective)
      :version (plist-get info :version)
      :code (plist-get info :code))
     :history
     (mapcar
      (lambda (response)
        (list
         :url (request-response-url response)
         :location (request-response-header response "location")
         :hop (request-response-header response "x-hop")
         :headers (request-response-headers response)))
      history))))
"##;
    let expect = expect![[
        r####"OK (:final-buffer "HTTP/1.1 200 OK\15\nContent-Type: application/json\15\nX-Final: origin\15\n\15\n{\"items\":[1,2]}" :info (:redirects 1 :effective "https://api.example.test/v2/items" :version "1.1" :code 200) :history ((:url "https://api.example.test/v1/items" :location "/v2/items" :hop "edge-a" :headers ((location . "/v2/items") (x-hop . "edge-a")))))"####
    ]];
    ParityBatchCase::value(
        "curl_preprocessor_consumes_continue_redirect_headers_and_absolutifies_history",
        elisp_form,
        expect,
    )
}

fn curl_command_builder_combines_auth_headers_body_files_compression_and_safe_logging()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((request-curl-options '("--noproxy" "*" "--connect-timeout" "2"))
      (request--curl-cookie-jar "/cache/request-cookies.txt"))
  (cl-letf
      (((symbol-function 'request--curl-capabilities)
        (lambda () '(:version "8.0" :compression t)))
       ((symbol-function 'auth-source-search)
        (lambda (&rest _)
          '((:user "release-bot"
             :secret (lambda () "token-41"))))))
    (let* ((args
            (request--curl-command-args
             "https://api.example.test/releases"
             :type "POST"
             :auth "basic"
             :unix-socket "/run/api.sock"
             :headers
             '(("Content-Type" . "application/json")
               ("X-Trace" . "trace 41"))
             :data "{\"version\":41}"
             :files
             '(("manifest" . "/workspace/Cargo.toml")
               ("notes" .
                ("notes.txt"
                 :file "/workspace/notes.txt"
                 :use-contents t
                 :mime-type "text/plain")))))
           (config (apply #'request--curl-stdin-config args))
           (log-command
            (request--curl-occlude-secret
             (mapconcat #'identity args " "))))
      (list
       :args args
       :config config
       :safe-log log-command
       :secret-hidden
       (and (string-match-p "--user elided" log-command)
            (not (string-match-p "token-41" log-command)))))))
"##;
    let expect = expect![[
        r####"OK (:args ("--silent" "--location" "--cookie" "/cache/request-cookies.txt" "--cookie-jar" "/cache/request-cookies.txt" "--basic" "--user" "release-bot:token-41" "--include" "--write-out" "\\n(:num-redirects %{num_redirects} :url-effective \"%{url_effective}\")" "--noproxy" "*" "--connect-timeout" "2" "--compressed" "--unix-socket" "/run/api.sock" "--request" "POST" "--header" "Content-Type: application/json" "--header" "X-Trace: trace 41" "--url" "https://api.example.test/releases" "--data-binary" "@-" "--form" "manifest=@/workspace/Cargo.toml;filename=Cargo.toml" "--form" "notes=</workspace/notes.txt;filename=notes.txt;type=text/plain") :config "--silent\n--location\n--cookie /cache/request-cookies.txt\n--cookie-jar /cache/request-cookies.txt\n--basic\n--user release-bot:token-41\n--include\n--write-out \"\\n(:num-redirects %{num_redirects} :url-effective \\\"%{url_effective}\\\")\"\n--noproxy *\n--connect-timeout 2\n--compressed\n--unix-socket /run/api.sock\n--request POST\n--header \"Content-Type: application/json\"\n--header \"X-Trace: trace 41\"\n--url https://api.example.test/releases\n--data-binary @-\n--form manifest=@/workspace/Cargo.toml;filename=Cargo.toml\n--form notes=</workspace/notes.txt;filename=notes.txt;type=text/plain\n" :safe-log "--silent --location --cookie /cache/request-cookies.txt --cookie-jar /cache/request-cookies.txt --basic --user elided --include --write-out \\n(:num-redirects %{num_redirects} :url-effective \"%{url_effective}\") --noproxy * --connect-timeout 2 --compressed --unix-socket /run/api.sock --request POST --header Content-Type: application/json --header X-Trace: trace 41 --url https://api.example.test/releases --data-binary @- --form manifest=@/workspace/Cargo.toml;filename=Cargo.toml --form notes=</workspace/notes.txt;filename=notes.txt;type=text/plain" :secret-hidden t)"####
    ]];
    ParityBatchCase::value(
        "curl_command_builder_combines_auth_headers_body_files_compression_and_safe_logging",
        elisp_form,
        expect,
    )
}

fn netscape_cookie_jar_parses_http_only_secure_and_path_scoped_session_state() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   (concat
    "# Netscape HTTP Cookie File\n"
    "#HttpOnly_api.example.test\tFALSE\t/\tFALSE\t0\tsession\tquoted-token\n"
    "api.example.test\tFALSE\t/\tFALSE\t0\ttheme\tdark\n"
    "api.example.test\tFALSE\t/admin\tTRUE\t0\tadmin\tgranted\n"
    "other.example.test\tFALSE\t/\tFALSE\t0\tignored\tvalue\n"))
  (let ((cookies (request--netscape-cookie-parse)))
    (list
     :parsed cookies
     :root-http
     (request--netscape-filter-cookies
      cookies "api.example.test" "/" nil)
     :admin-http
     (request--netscape-filter-cookies
      cookies "api.example.test" "/admin" nil)
     :admin-https
     (request--netscape-filter-cookies
      cookies "api.example.test" "/admin" t)
     :cookie-string
     (mapconcat
      (lambda (pair) (concat (car pair) "=" (cdr pair)))
      (request--netscape-filter-cookies
       cookies "api.example.test" "/" nil)
      "; "))))
"##;
    let expect = expect![[
        r####"OK (:parsed (("api.example.test" nil "/" nil t 0 "session" "quoted-token") ("api.example.test" nil "/" nil nil 0 "theme" "dark") ("api.example.test" nil "/admin" t nil 0 "admin" "granted") ("other.example.test" nil "/" nil nil 0 "ignored" "value")) :root-http (("session" . "quoted-token") ("theme" . "dark")) :admin-http nil :admin-https (("admin" . "granted")) :cookie-string "session=quoted-token; theme=dark")"####
    ]];
    ParityBatchCase::value(
        "netscape_cookie_jar_parses_http_only_secure_and_path_scoped_session_state",
        elisp_form,
        expect,
    )
}

fn timeout_and_abort_workflows_set_terminal_state_invoke_callbacks_and_stop_process()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((timeout-buffer (generate-new-buffer " *request-timeout*"))
       (timeout-response
        (make-request-response
         :url "https://api.example.test/slow"
         :-backend 'curl
         :-buffer timeout-buffer))
       timeout-events
       abort-buffer
       abort-process
       abort-response
       terminated)
  (with-current-buffer timeout-buffer
    (insert
     "HTTP/1.1 206 Partial Content\r\nContent-Type: text/plain\r\n\r\npartial-body"))
  (setf
   (request-response-settings timeout-response)
   (list
    :response timeout-response
    :encoding 'utf-8
    :parser #'buffer-string
    :error
    (cl-function
     (lambda (&key data symbol-status error-thrown &allow-other-keys)
       (push
        (list 'error symbol-status (copy-tree error-thrown) data)
        timeout-events)))
    :complete
    (cl-function
     (lambda (&key symbol-status &allow-other-keys)
       (push (list 'complete symbol-status) timeout-events)))))
  (request-response--timeout-callback timeout-response)
  (setq abort-buffer (generate-new-buffer " *request-abort*")
        abort-process
        (start-process
         "request-test-sleeper" abort-buffer
         "sh" "-c" "sleep 30")
        abort-response
        (make-request-response
         :url "https://api.example.test/stream"
         :-backend 'curl
         :-buffer abort-buffer))
  (unwind-protect
      (cl-letf
          (((symbol-function 'request--choose-backend)
            (lambda (method)
              (when (eq method 'terminate-process)
                (lambda (process)
                  (setq terminated
                        (list (process-name process)
                              (and (process-live-p process) t)))
                  (delete-process process))))))
        (request-abort abort-response)
        (accept-process-output abort-process 0.1)
        (list
         :timeout
         (list
          :response
          (request-test-response-summary timeout-response)
          :events (nreverse timeout-events))
         :abort
         (list
          :response
          (request-test-response-summary abort-response)
          :terminated terminated
          :process-live (process-live-p abort-process))))
    (when (process-live-p abort-process)
      (delete-process abort-process))
    (when (buffer-live-p abort-buffer)
      (kill-buffer abort-buffer))))
"##;
    let expect = expect![[
        r####"OK (:timeout (:response (:status-code 206 :symbol-status timeout :done t :url "https://api.example.test/slow" :data "partial-body" :error (error "Timeout") :backend curl :raw-header "HTTP/1.1 206 Partial Content\nContent-Type: text/plain\n" :buffer-live nil) :events ((error timeout (error "Timeout") "partial-body") (complete timeout))) :abort (:response (:status-code nil :symbol-status abort :done t :url "https://api.example.test/stream" :data nil :error nil :backend curl :raw-header nil :buffer-live t) :terminated ("request-test-sleeper" t) :process-live nil))"####
    ]];
    ParityBatchCase::value(
        "timeout_and_abort_workflows_set_terminal_state_invoke_callbacks_and_stop_process",
        elisp_form,
        expect,
    )
}

fn synchronous_curl_file_request_parses_real_utf8_json_and_runs_public_callbacks() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((root (make-temp-file "request-file-" t))
       (file (expand-file-name "release.json" root))
       (request-backend 'curl)
       (request-storage-directory root)
       (request--curl-cookie-jar
        (expand-file-name "cookies.txt" root))
       events
       result)
  (unwind-protect
      (progn
        (with-temp-file file
          (set-buffer-file-coding-system 'utf-8-unix)
          (insert
           "{\"artifact\":\"neomacs\",\"label\":\"发布 λ\",\"version\":41}\n"))
        (let ((response
               (request
                (concat "file://" file)
                :sync t
                :timeout 10
                :parser #'request-test-json-plist
                :success
                (cl-function
                 (lambda (&key data symbol-status &allow-other-keys)
                   (push
                    (list 'success symbol-status
                          (plist-get data :label))
                    events)))
                :complete
                (cl-function
                 (lambda (&key symbol-status &allow-other-keys)
                   (push (list 'complete symbol-status) events))))))
          (setq result
                (list
                 :response
                 (let ((summary
                        (request-test-response-summary response)))
                   (plist-put
                    summary :url
                    (request-test-normalize-root
                     (plist-get summary :url) root)))
                 :events (nreverse events)
                 :cookie-jar-created
                 (file-exists-p request--curl-cookie-jar)))))
    (delete-directory root t))
  result)
"##;
    let expect = expect![[
        r####"OK (:response (:status-code nil :symbol-status success :done t :url "file://[PROJECT]/release.json" :data (:artifact "neomacs" :label "发布 λ" :version 41) :error nil :backend curl :raw-header nil :buffer-live nil) :events ((success success "发布 λ") (complete success)) :cookie-jar-created nil)"####
    ]];
    ParityBatchCase::value(
        "synchronous_curl_file_request_parses_real_utf8_json_and_runs_public_callbacks",
        elisp_form,
        expect,
    )
}

#[test]
fn request_package_batch() {
    let cases = vec![
        request_builder_encodes_params_form_data_and_preserves_explicit_content_type(),
        response_headers_support_case_insensitive_duplicates_and_structured_alist_extraction(),
        successful_json_callback_runs_success_status_and_complete_in_order_with_same_response(),
        parser_and_http_failures_preserve_body_data_and_route_error_status_complete_callbacks(),
        curl_preprocessor_consumes_continue_redirect_headers_and_absolutifies_history(),
        curl_command_builder_combines_auth_headers_body_files_compression_and_safe_logging(),
        netscape_cookie_jar_parses_http_only_secure_and_path_scoped_session_state(),
        timeout_and_abort_workflows_set_terminal_state_invoke_callbacks_and_stop_process(),
        synchronous_curl_file_request_parses_real_utf8_json_and_runs_public_callbacks(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed request parity test");
    assert_oracle_batch_cases(request_oracle(), test_name, "request_parity", &cases);
}
