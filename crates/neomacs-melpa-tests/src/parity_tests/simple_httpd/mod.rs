use std::time::Duration;

use expect_test::expect;

use crate::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, SIMPLE_HTTPD_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SIMPLE_HTTPD_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SIMPLE_HTTPD_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'simple-httpd)

(setq httpd-log-buffer nil
      httpd-server-name "simple-httpd parity")

(defun simple-httpd-test-header (response name)
  (cadr (assoc name (plist-get response :headers))))

(defun simple-httpd-test-parse-wire (wire)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert wire)
    (let* ((headers (httpd-parse))
           (date-header (assoc "Date" headers))
           (date (cadr date-header)))
      (list
       :headers (delq date-header headers)
       :date-valid
       (and date
            (equal date (httpd-date-string (date-to-time date))))
       :body (buffer-substring-no-properties (point) (point-max))))))

(defun simple-httpd-test-capture (request responder)
  (let ((properties (list (cons :request-active request)))
        chunks
        deleted)
    (cl-letf (((symbol-function 'process-get)
               (lambda (_process property)
                 (alist-get property properties)))
              ((symbol-function 'process-put)
               (lambda (_process property value)
                 (setf (alist-get property properties) value)))
              ((symbol-function 'process-send-string)
               (lambda (_process string)
                 (push string chunks)))
              ((symbol-function 'process-send-region)
               (lambda (_process start end)
                 (push (buffer-substring-no-properties start end) chunks)))
              ((symbol-function 'process-contact)
               (lambda (_process &optional _key _no-block)
                 '("127.0.0.1" 4242)))
              ((symbol-function 'delete-process)
               (lambda (_process) (setq deleted t))))
      (funcall responder 'simple-httpd-test-client))
    (append
     (simple-httpd-test-parse-wire
      (apply #'concat (nreverse chunks)))
     (list
      :active-after (alist-get :request-active properties)
      :closed deleted))))
"####;

fn simple_httpd_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SIMPLE_HTTPD_MELPA_PIN, "simple-httpd.el")
        .expect("prepare pinned simple-httpd source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare pinned Compat dependency below ./tmp")
        .with_prelude(SIMPLE_HTTPD_TEST_PRELUDE)
        .with_timeout(SIMPLE_HTTPD_TEST_TIMEOUT)
}

fn post_request_parsing_preserves_the_body_and_decodes_application_inputs() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert
   "POST /jobs/release%2042?owner=Ana+Ng&tag=release%2F2026#summary HTTP/1.1\r\n"
   "Host: localhost:8080\r\n"
   "Content-Type: application/x-www-form-urlencoded; charset=utf-8\r\n"
   "Content-Length: 42\r\n"
   "X-Request-ID: deploy-417\r\n"
   "Connection: keep-alive\r\n\r\n"
   "state=ready&note=two+words&token=a%2Bb")
  (let* ((request (httpd-parse))
         (content (buffer-substring-no-properties (point) (point-max)))
         (uri (httpd-parse-uri (cadar request))))
    (list
     :request request
     :body content
     :body-bytes (string-bytes content)
     :uri uri
     :form (httpd-parse-args content)
     :point (point)
     :buffer-bytes (buffer-size))))
"####;
    let expect = expect![[
        r####"OK (:request (("POST" "/jobs/release%2042?owner=Ana+Ng&tag=release%2F2026#summary" "HTTP/1.1") ("Host" "localhost:8080") ("Content-Type" "application/x-www-form-urlencoded; charset=utf-8") ("Content-Length" "42") ("X-Request-Id" "deploy-417") ("Connection" "keep-alive")) :body "state=ready&note=two+words&token=a%2Bb" :body-bytes 38 :uri ("/jobs/release 42" (("owner" "Ana Ng") ("tag" "release/2026")) "summary") :form (("state" "ready") ("note" "two words") ("token" "a+b")) :point 233 :buffer-bytes 270)"####
    ]];
    ParityBatchCase::value(
        "post_request_parsing_preserves_the_body_and_decodes_application_inputs",
        elisp_form,
        expect,
    )
}

fn servlet_routing_binds_a_real_deployment_request_and_emits_the_wire_response() -> ParityBatchCase
{
    let elisp_form = r####"
(progn
  (httpd-servlet* deployments/:release text/plain
      ((dry-run "false" dry-run-supplied-p) actor)
    (insert
     (format
      "release=%s; actor=%s; dry-run=%s; supplied=%S; path=%s; trace=%s"
      release actor dry-run dry-run-supplied-p httpd-path
      (cadr (assoc "X-Trace" httpd-request)))))
  (let* ((path "/deployments/release 42")
         (query '(("actor" "Ana Ng")))
         (request
          '(("GET" "/deployments/release%2042?actor=Ana+Ng" "HTTP/1.1")
            ("Host" "localhost")
            ("X-Trace" "trace-417")))
         (servlet (httpd-get-servlet path))
         (response
          (simple-httpd-test-capture
           request
           (lambda (client)
             (funcall servlet client path query request)))))
    (list :servlet servlet :response response)))
"####;
    let expect = expect![[
        r####"OK (:servlet httpd/deployments :response (:headers (("HTTP/1.1" "200" "OK") ("Content-Type" "text/plain; charset=utf-8") ("Content-Length" "108") ("Connection" "keep-alive") ("Server" "simple-httpd parity")) :date-valid t :body "release=release 42; actor=Ana Ng; dry-run=false; supplied=nil; path=/deployments/release 42; trace=trace-417" :active-after nil :closed nil))"####
    ]];
    ParityBatchCase::value(
        "servlet_routing_binds_a_real_deployment_request_and_emits_the_wire_response",
        elisp_form,
        expect,
    )
}

fn static_asset_serving_round_trips_bytes_and_honors_current_conditional_gets() -> ParityBatchCase {
    let elisp_form = r####"
(let ((root (make-temp-file "simple-httpd-static-" t)))
  (unwind-protect
      (let* ((asset-directory (expand-file-name "assets" root))
             (asset (expand-file-name "release notes.txt" asset-directory))
             (contents "Release 42: ready\nOwner: Ana Ng\n"))
        (make-directory asset-directory)
        (let ((coding-system-for-write 'no-conversion))
          (write-region contents nil asset nil 'silent))
        ;; Relatime filesystems may legitimately advance access time on the
        ;; first read. Prime it before simple-httpd computes the response ETag
        ;; so the client cache workflow observes stable file metadata.
        (set-file-times asset (time-subtract nil (seconds-to-time 3600)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally asset))
        (let* ((mapped (httpd-gen-path "/assets/release notes.txt" root))
               (etag (httpd-etag mapped))
               (mtime
                (httpd-date-string
                 (file-attribute-modification-time (file-attributes mapped))))
               (request
                '(("GET" "/assets/release%20notes.txt" "HTTP/1.1")
                  ("Host" "localhost")))
               (first
                (simple-httpd-test-capture
                 request
                 (lambda (client)
                   (httpd-serve-root
                    client root "/assets/release notes.txt" request))))
               ;; Recompute the current validator after the file response.
               ;; The upstream ETag hashes access time, whose update policy is
               ;; filesystem-dependent, so a stale pre-read value is not a
               ;; deterministic cross-host oracle.
               (current-etag (httpd-etag mapped))
               (cached-request
                `(("GET" "/assets/release%20notes.txt" "HTTP/1.1")
                  ("Host" "localhost")
                  ("If-None-Match" ,current-etag)))
               (cached
                (simple-httpd-test-capture
                 cached-request
                 (lambda (client)
                   (httpd-serve-root
                    client root "/assets/release notes.txt" cached-request)))))
          (list
           :mapped (file-relative-name mapped root)
           :mime (httpd-get-mime (file-name-extension mapped))
           :first
           (list
            :status (car (plist-get first :headers))
            :content-type (simple-httpd-test-header first "Content-Type")
            :content-length (simple-httpd-test-header first "Content-Length")
            :etag-matches (equal (simple-httpd-test-header first "Etag") etag)
            :last-modified-matches
            (equal (simple-httpd-test-header first "Last-Modified") mtime)
            :body (plist-get first :body))
           :cached
           (list
            :status (car (plist-get cached :headers))
            :content-type (simple-httpd-test-header cached "Content-Type")
            :content-length (simple-httpd-test-header cached "Content-Length")
            :body (plist-get cached :body)))))
    (delete-directory root t)))
"####;
    let expect = expect![[
        r####"OK (:mapped "assets/release notes.txt" :mime "text/plain" :first (:status ("HTTP/1.1" "200" "OK") :content-type "text/plain; charset=utf-8" :content-length "32" :etag-matches t :last-modified-matches t :body "Release 42: ready\nOwner: Ana Ng\n") :cached (:status ("HTTP/1.1" "304" "Not Modified") :content-type "text/plain; charset=utf-8" :content-length "0" :body ""))"####
    ]];
    ParityBatchCase::value(
        "static_asset_serving_round_trips_bytes_and_honors_current_conditional_gets",
        elisp_form,
        expect,
    )
}

fn directory_workflow_escapes_entries_redirects_canonical_urls_and_contains_traversal()
-> ParityBatchCase {
    let elisp_form = r####"
(let ((root (make-temp-file "simple-httpd-directory-" t)))
  (unwind-protect
      (let* ((reports (expand-file-name "reports" root))
             (archive (expand-file-name "archive" reports)))
        (make-directory archive t)
        (dolist (file-content
                 '(("reports/alpha & beta.txt" . "alpha report\n")
                   ("reports/release plan.md" . "# Release plan\n")
                   ("reports/.private" . "not listed\n")
                   ("secret.txt" . "inside configured root\n")))
          (write-region
           (cdr file-content) nil
           (expand-file-name (car file-content) root) nil 'silent))
        (let* ((listing-request
                '(("GET" "/reports/" "HTTP/1.1") ("Host" "localhost")))
               (listing
                (simple-httpd-test-capture
                 listing-request
                 (lambda (client)
                   (httpd-serve-root client root "/reports/" listing-request))))
               (redirect-request
                '(("GET" "/reports" "HTTP/1.1") ("Host" "localhost")))
               (redirect
                (simple-httpd-test-capture
                 redirect-request
                 (lambda (client)
                   (httpd-serve-root client root "/reports" redirect-request))))
               (traversal-request
                '(("GET" "/../../secret.txt" "HTTP/1.1")
                  ("Host" "localhost")))
               (traversal
                (simple-httpd-test-capture
                 traversal-request
                 (lambda (client)
                   (httpd-serve-root
                    client root "/../../secret.txt" traversal-request))))
               (blocked
                (let ((httpd-listings nil))
                  (simple-httpd-test-capture
                   listing-request
                   (lambda (client)
                     (httpd-serve-root
                      client root "/reports/" listing-request))))))
          (list
           :cleaned (httpd-clean-path "/../../secret.txt")
           :mapped
           (file-relative-name
            (httpd-gen-path "/../../secret.txt" root) root)
           :listing
           (list
            :status (car (plist-get listing :headers))
            :content-type (simple-httpd-test-header listing "Content-Type")
            :length (simple-httpd-test-header listing "Content-Length")
            :body (plist-get listing :body))
           :redirect
           (list
            :status (car (plist-get redirect :headers))
            :location (simple-httpd-test-header redirect "Location")
            :length (simple-httpd-test-header redirect "Content-Length")
            :body (plist-get redirect :body))
           :traversal
           (list
            :status (car (plist-get traversal :headers))
            :body (plist-get traversal :body))
           :listings-disabled
           (list
            :status (car (plist-get blocked :headers))
            :body (plist-get blocked :body)))))
    (delete-directory root t)))
"####;
    let expect = expect![[
        r####"OK (:cleaned "./secret.txt" :mapped "secret.txt" :listing (:status ("HTTP/1.1" "200" "OK") :content-type "text/html; charset=utf-8" :length "333" :body "<!DOCTYPE html>\n<html>\n<head><title>Directory listing for /reports/</title></head>\n<body>\n<h2>Directory listing for /reports/</h2>\n<hr/>\n<ul><li><a href=\"alpha%20%26%20beta.txt\">alpha &amp; beta.txt</a></li>\n<li><a href=\"archive/\">archive/</a></li>\n<li><a href=\"release%20plan.md\">release plan.md</a></li>\n</ul>\n<hr/>\n</body>\n</html>") :redirect (:status ("HTTP/1.1" "301" "Moved Permanently") :location "/reports/" :length "0" :body "") :traversal (:status ("HTTP/1.1" "200" "OK") :body "inside configured root\n") :listings-disabled (:status ("HTTP/1.1" "403" "Forbidden") :body "<!DOCTYPE html>\n<html><head><title>403 Forbidden</title></head><body>\n<h1>403 Forbidden</h1>\n<p>An error occurred.</p>\n<pre></pre>\n</body></html>\n"))"####
    ]];
    ParityBatchCase::value(
        "directory_workflow_escapes_entries_redirects_canonical_urls_and_contains_traversal",
        elisp_form,
        expect,
    )
}

fn response_protocol_honors_head_legacy_close_and_custom_headers() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((head-request
        '(("HEAD" "/status" "HTTP/1.1") ("Host" "localhost")))
       (head
        (simple-httpd-test-capture
         head-request
         (lambda (client)
           (httpd-with-buffer client "text/plain"
             (insert "release status: ready")))))
       (legacy-request
        '(("GET" "/jobs/417" "HTTP/1.0") ("Host" "localhost")))
       (legacy
        (simple-httpd-test-capture
         legacy-request
         (lambda (client)
           (httpd-with-buffer client "application/json"
             (insert "{\"job\":417,\"state\":\"accepted\"}")
             (httpd-send-header
              t "application/json" 202
              :Location "/jobs/417"
              :X-Queue "deployments"))))))
  (list
   :head
   (list
    :status (car (plist-get head :headers))
    :type (simple-httpd-test-header head "Content-Type")
    :length (simple-httpd-test-header head "Content-Length")
    :connection (simple-httpd-test-header head "Connection")
    :body (plist-get head :body)
    :closed (plist-get head :closed))
   :legacy
   (list
    :status (car (plist-get legacy :headers))
    :type (simple-httpd-test-header legacy "Content-Type")
    :length (simple-httpd-test-header legacy "Content-Length")
    :connection (simple-httpd-test-header legacy "Connection")
    :location (simple-httpd-test-header legacy "Location")
    :queue (simple-httpd-test-header legacy "X-Queue")
    :body (plist-get legacy :body)
    :closed (plist-get legacy :closed))))
"####;
    let expect = expect![[
        r####"OK (:head (:status ("HTTP/1.1" "200" "OK") :type "text/plain; charset=utf-8" :length "21" :connection "keep-alive" :body "" :closed nil) :legacy (:status ("HTTP/1.0" "202" "Accepted") :type "application/json" :length "30" :connection "close" :location "/jobs/417" :queue "deployments" :body "{\"job\":417,\"state\":\"accepted\"}" :closed t))"####
    ]];
    ParityBatchCase::value(
        "response_protocol_honors_head_legacy_close_and_custom_headers",
        elisp_form,
        expect,
    )
}

fn request_filters_feed_servlet_context_and_errors_become_escaped_http_responses() -> ParityBatchCase
{
    let elisp_form = r####"
(progn
  (httpd-servlet* filtered/jobs/:job text/plain (actor)
    (insert
     (format
      "job=%s;actor=%s;policy=%s;trace=%s"
      job actor
      (cadr (assoc "X-Policy" httpd-request))
      (cadr (assoc "X-Trace" httpd-request)))))
  (defun httpd/failing (_client _path _query _request)
    (error "database <offline> for REL-417"))
  (let* ((httpd-filter-functions
          (list
           (lambda (request)
             (append request '(("X-Policy" "verified"))))
           (lambda (request)
             (append
              request
              (list
               (list
                "X-Trace"
                (concat (cadr (assoc "X-Policy" request)) "-trace")))))))
         (filtered-request
          '(("GET" "/filtered/jobs/417?actor=Ana+Ng" "HTTP/1.1")
            ("Host" "localhost")))
         (filtered
          (simple-httpd-test-capture
           filtered-request
           (lambda (client)
             (httpd--handle-request client filtered-request))))
         (failing-request
          '(("GET" "/failing" "HTTP/1.1") ("Host" "localhost")))
         (failed
          (let ((httpd-filter-functions nil))
            (simple-httpd-test-capture
             failing-request
             (lambda (client)
               (httpd--handle-request client failing-request))))))
    (list
     :filtered
     (list
      :status (car (plist-get filtered :headers))
      :body (plist-get filtered :body)
      :active-after (plist-get filtered :active-after))
     :failed
     (list
      :status (car (plist-get failed :headers))
      :type (simple-httpd-test-header failed "Content-Type")
      :body (plist-get failed :body)
      :active-after (plist-get failed :active-after)))))
"####;
    let expect = expect![[
        r####"OK (:filtered (:status ("HTTP/1.1" "200" "OK") :body "job=417;actor=Ana Ng;policy=verified;trace=verified-trace" :active-after nil) :failed (:status ("HTTP/1.1" "500" "Internal Server Error") :type "text/html; charset=utf-8" :body "<!DOCTYPE html>\n<html><head><title>500 Internal Server Error</title></head><body>\n<h1>500 Internal Server Error</h1>\n<p>An error occurred.</p>\n<pre>error: (error database &lt;offline&gt; for REL-417)\n</pre>\n</body></html>\n" :active-after nil))"####
    ]];
    ParityBatchCase::value(
        "request_filters_feed_servlet_context_and_errors_become_escaped_http_responses",
        elisp_form,
        expect,
    )
}

fn live_loopback_server_accepts_a_fragmented_post_and_completes_its_lifecycle() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (httpd-servlet* live/jobs/:job text/plain (owner state dry-run)
    (insert
     (format
      "job=%s;owner=%s;state=%s;dry-run=%s;method=%s"
      job owner state dry-run (caar httpd-request))))
  (let ((response-buffer (generate-new-buffer " *simple-httpd-live-response*"))
        client
        events)
    (unwind-protect
        (let ((httpd-host "127.0.0.1")
              (httpd-port t)
              (httpd-ip-family 'ipv4)
              (httpd-start-hook (list (lambda () (push :started events))))
              (httpd-stop-hook (list (lambda () (push :stopped events)))))
          (httpd-start)
          (let* ((running-after-start (httpd-running-p))
                 (port (process-contact httpd--server :service))
                 (body "owner=Ana+Ng&state=ready")
                 (headers
                  (format
                   (concat
                    "POST /live/jobs/417?dry-run=true HTTP/1.1\r\n"
                    "Host: 127.0.0.1:%s\r\n"
                    "Content-Type: application/x-www-form-urlencoded\r\n"
                    "Content-Length: %d\r\n"
                    "Connection: close\r\n\r\n")
                   port (string-bytes body))))
            (with-current-buffer response-buffer
              (set-buffer-multibyte nil))
            (setq client
                  (make-network-process
                   :name "simple-httpd-parity-client"
                   :buffer response-buffer
                   :host "127.0.0.1"
                   :service port
                   :family 'ipv4
                   :coding 'binary
                   :nowait nil
                   :sentinel #'ignore
                   :noquery t))
            (process-send-string client (concat headers (substring body 0 10)))
            (accept-process-output nil 0.01)
            (process-send-string client (substring body 10))
            (let ((deadline (+ (float-time) 5.0)))
              (while
                  (and
                   (< (float-time) deadline)
                   (with-current-buffer response-buffer
                     (save-excursion
                       (goto-char (point-min))
                       (not (search-forward
                             "job=417;owner=Ana Ng;state=ready;dry-run=true;method=POST"
                             nil t)))))
                (accept-process-output nil 0.02)))
            (let ((response
                   (with-current-buffer response-buffer
                     (simple-httpd-test-parse-wire
                      (buffer-substring-no-properties (point-min) (point-max))))))
              (httpd-stop)
              (list
               :port-valid (and (integerp port) (> port 0))
               :running-after-start (and running-after-start t)
               :events (nreverse events)
               :response response
               :server-after-stop httpd--server
               :clients-after-stop httpd--clients))))
      (when (and client (process-live-p client))
        (delete-process client))
      (httpd-stop)
      (when (buffer-live-p response-buffer)
        (kill-buffer response-buffer)))))
"####;
    let expect = expect![[
        r####"OK (:port-valid t :running-after-start t :events (:started :stopped) :response (:headers (("HTTP/1.1" "200" "OK") ("Content-Type" "text/plain; charset=utf-8") ("Content-Length" "57") ("Connection" "close") ("Server" "simple-httpd parity")) :date-valid t :body "job=417;owner=Ana Ng;state=ready;dry-run=true;method=POST") :server-after-stop nil :clients-after-stop nil)"####
    ]];
    ParityBatchCase::value(
        "live_loopback_server_accepts_a_fragmented_post_and_completes_its_lifecycle",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn live_keep_alive_connection_delivers_pipelined_responses_in_request_order() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (httpd-servlet pipeline/first text/plain ()
    (insert "first=release-417\n"))
  (httpd-servlet* pipeline/second text/plain (state)
    (insert (format "second=%s\n" state)))
  (let ((response-buffer
         (generate-new-buffer " *simple-httpd-pipeline-response*"))
        client)
    (unwind-protect
        (let ((httpd-host "127.0.0.1")
              (httpd-port t)
              (httpd-ip-family 'ipv4))
          (httpd-start)
          (let ((port (process-contact httpd--server :service)))
            (with-current-buffer response-buffer
              (set-buffer-multibyte nil))
            (setq client
                  (make-network-process
                   :name "simple-httpd-pipeline-client"
                   :buffer response-buffer
                   :host "127.0.0.1"
                   :service port
                   :family 'ipv4
                   :coding 'binary
                   :nowait nil
                   :sentinel #'ignore
                   :noquery t))
            (process-send-string
             client
             (concat
              "GET /pipeline/first HTTP/1.1\r\n"
              (format "Host: 127.0.0.1:%s\r\n\r\n" port)
              "GET /pipeline/second?state=ready HTTP/1.1\r\n"
              (format "Host: 127.0.0.1:%s\r\n" port)
              "Connection: close\r\n\r\n"))
            (let ((deadline (+ (float-time) 5.0)))
              (while
                  (and
                   (< (float-time) deadline)
                   (with-current-buffer response-buffer
                     (save-excursion
                       (goto-char (point-min))
                       (not (search-forward "second=ready\n" nil t)))))
                (accept-process-output nil 0.02)))
            (let ((wire
                   (with-current-buffer response-buffer
                     (buffer-substring-no-properties (point-min) (point-max)))))
              (httpd-stop)
              (list
               :response-count
               (let ((start 0) (count 0))
                 (while (string-match "HTTP/1.1 200 OK" wire start)
                   (setq count (1+ count)
                         start (match-end 0)))
                 count)
               :wire
               (replace-regexp-in-string
                "Date: [^\r\n]*\r\n" "Date: <DATE>\r\n" wire t t)
               :server-after-stop httpd--server
               :clients-after-stop httpd--clients))))
      (when (and client (process-live-p client))
        (delete-process client))
      (httpd-stop)
      (when (buffer-live-p response-buffer)
        (kill-buffer response-buffer)))))
"####;
    let expect = expect![[
        r####"OK (:response-count 2 :wire "HTTP/1.1 200 OK\15\nDate: <DATE>\15\nContent-Type: text/plain; charset=utf-8\15\nContent-Length: 18\15\nConnection: keep-alive\15\nServer: simple-httpd parity\15\n\15\nfirst=release-417\nHTTP/1.1 200 OK\15\nDate: <DATE>\15\nContent-Type: text/plain; charset=utf-8\15\nContent-Length: 13\15\nConnection: close\15\nServer: simple-httpd parity\15\n\15\nsecond=ready\n" :server-after-stop nil :clients-after-stop nil)"####
    ]];
    ParityBatchCase::value(
        "live_keep_alive_connection_delivers_pipelined_responses_in_request_order",
        elisp_form,
        expect,
    )
    .fresh_process()
}

#[test]
fn simple_httpd_package_batch() {
    let cases = vec![
        post_request_parsing_preserves_the_body_and_decodes_application_inputs(),
        servlet_routing_binds_a_real_deployment_request_and_emits_the_wire_response(),
        static_asset_serving_round_trips_bytes_and_honors_current_conditional_gets(),
        directory_workflow_escapes_entries_redirects_canonical_urls_and_contains_traversal(),
        response_protocol_honors_head_legacy_close_and_custom_headers(),
        request_filters_feed_servlet_context_and_errors_become_escaped_http_responses(),
        live_loopback_server_accepts_a_fragmented_post_and_completes_its_lifecycle(),
        live_keep_alive_connection_delivers_pipelined_responses_in_request_order(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed simple-httpd parity test");
    assert_oracle_batch_cases(
        simple_httpd_oracle(),
        test_name,
        "simple_httpd_parity",
        &cases,
    );
}
