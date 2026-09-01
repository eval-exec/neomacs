use expect_test::expect;

use super::ParityBatchCase;

fn zero_x_zero_host_uri_supports_plain_and_basic_auth_servers() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_host_uri_supports_plain_and_basic_auth_servers",
        r##"(let ((server
                      '(:scheme "https"
                        :host "upload.example")))
               (list
                (0x0--make-server-host-uri server)
                (0x0--make-server-host-uri
                 server
                 '(:user "alice" :pass "s3cret"))
                (0x0--make-server-host-uri
                 '(:scheme nil :host nil))))"##,
        expect![[
            r#"OK ("https://upload.example" "https://alice:s3cret@upload.example" "nil://nil")"#
        ]],
    )
}

fn zero_x_zero_curl_arguments_distinguish_file_and_buffer_uploads() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_curl_arguments_distinguish_file_and_buffer_uploads",
        r##"(let ((server
                      '(:scheme "https"
                        :host "example.test")))
               (list
                (0x0--make-0x0-curl-args
                 server "/path/a b.txt")
                (0x0--make-0x0-curl-args
                 server "upload.txt" t)))"##,
        expect![[
            r#"OK (("-s" "-S" "-F" "file=@/path/a b.txt" "https://example.test") ("-s" "-S" "-F" "file=@-;filename=upload.txt" "https://example.test"))"#
        ]],
    )
}

fn zero_x_zero_curl_dispatches_files_and_regions_to_exact_process_apis() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_curl_dispatches_files_and_regions_to_exact_process_apis",
        r##"(let (calls buffers)
               (cl-letf (((symbol-function 'call-process)
                          (lambda (&rest args)
                            (push
                             (cons 'call-process args)
                             calls)
                            0))
                         ((symbol-function
                           'call-process-region)
                          (lambda (&rest args)
                            (push
                             (cons
                              'call-process-region
                              args)
                             calls)
                            0)))
                 (unwind-protect
                     (with-temp-buffer
                       (insert "abcdef")
                       (let ((0x0-use-curl
                              "/custom/curl"))
                         (push
                          (0x0--curl
                           '("-s" "https://one"))
                          buffers)
                         (push
                          (0x0--curl
                           '("-s" "https://two")
                           '(:start 2 :end 5))
                          buffers))
                       (mapcar
                        (lambda (call)
                          (mapcar
                           (lambda (value)
                             (if (bufferp value)
                                 :buffer
                               value))
                           call))
                        (nreverse calls)))
                   (mapc
                    (lambda (buffer)
                      (when (buffer-live-p buffer)
                        (kill-buffer buffer)))
                    buffers))))"##,
        expect![[
            r#"OK ((call-process "/custom/curl" nil :buffer nil "-s" "https://one") (call-process-region 2 5 "/custom/curl" nil :buffer nil "-s" "https://two"))"#
        ]],
    )
}

fn zero_x_zero_url_properties_distinguish_file_and_bounded_sources() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_url_properties_distinguish_file_and_bounded_sources",
        r##"(let ((server
                      '(:scheme "http"
                        :host "local.test")))
               (list
                (0x0--make-url-props
                 server "/path/to/file.txt")
                (0x0--make-url-props
                 server "/path/to/file.txt" t)))"##,
        expect![[
            r#"OK ((:file-path "/path/to/file.txt" :query-str "name=\"file\"; filename=\"file.txt\"" :host-uri "http://local.test") (:file-path nil :query-str "name=\"file\"; filename=\"file.txt\"" :host-uri "http://local.test"))"#
        ]],
    )
}

fn zero_x_zero_url_builds_multipart_body_and_strips_response_headers() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_url_builds_multipart_body_and_strips_response_headers",
        r##"(let (request)
               (cl-letf (((symbol-function 'random)
                          (let ((values '(10 11 12)))
                            (lambda (&rest _)
                              (prog1
                                  (car values)
                                (setq values
                                      (cdr values))))))
                         ((symbol-function
                           'url-retrieve-synchronously)
                          (lambda (uri &rest _)
                            (setq request
                                  (list
                                   uri
                                   url-request-method
                                   url-request-extra-headers
                                   url-request-data))
                            (let ((buffer
                                   (generate-new-buffer
                                    " *0x0-url-response*")))
                              (with-current-buffer buffer
                                (insert
                                 "HTTP/1.1 200 OK\r\n"
                                 "Header: value\r\n"
                                 "\r\n"
                                 "https://local.test/id\n"))
                              buffer))))
                 (with-temp-buffer
                   (insert "012345")
                   (let ((response
                          (0x0--url
                           '(:file-path nil
                             :query-str
                             "name=\"file\"; filename=\"x.txt\""
                             :host-uri
                             "https://local.test")
                           '(:start 2 :end 5))))
                     (unwind-protect
                         (list
                          request
                          (with-current-buffer response
                            (list
                             (buffer-name)
                             (buffer-string))))
                       (when (buffer-live-p response)
                         (kill-buffer response)))))))"##,
        expect![[
            r#"OK (("https://local.test" "POST" (("Content-Type" . "multipart/form-data; boundary=A-B-C")) "--A-B-C\15\nContent-Disposition: form-data; name=\"file\"; filename=\"x.txt\"\15\nContent-type: text/plain\15\n\15\n123\15\n--A-B-C--") ("*0x0 response*" "\nhttps://local.test/id\n"))"#
        ]],
    )
}

fn zero_x_zero_send_selects_each_curl_policy_and_url_fallback() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_send_selects_each_curl_policy_and_url_fallback",
        r##"(let ((server
                      '(:scheme "https"
                        :host "example.test"
                        :curl-args-fun
                        0x0--make-0x0-curl-args))
                     events)
               (cl-letf (((symbol-function 'executable-find)
                          (lambda (program)
                            (push
                             (list
                              'executable-find
                              program)
                             events)
                            (and
                             (equal program "curl")
                             "/usr/bin/curl")))
                         ((symbol-function '0x0--curl)
                          (lambda (args &optional bounds)
                            (push
                             (list 'curl args bounds)
                             events)
                            'curl-result))
                         ((symbol-function '0x0--url)
                          (lambda (props &optional bounds)
                            (push
                             (list 'url props bounds)
                             events)
                            'url-result)))
                 (let ((bounds '(:start 2 :end 4)))
                   (list
                    (let ((0x0-use-curl t))
                      (0x0--send
                       server "one.txt" bounds))
                    (let ((0x0-use-curl
                           'if-installed))
                      (0x0--send
                       server "two.txt" bounds))
                    (let ((0x0-use-curl
                           "/opt/curl"))
                      (0x0--send
                       server "three.txt" nil))
                    (let ((0x0-use-curl nil))
                      (0x0--send
                       server "four.txt" bounds))
                    (nreverse events)))))"##,
        expect![[
            r#"OK (curl-result curl-result curl-result url-result ((curl ("-s" "-S" "-F" "file=@-;filename=one.txt" "https://example.test") #1=(:start 2 :end 4)) (executable-find "curl") (curl ("-s" "-S" "-F" "file=@-;filename=two.txt" "https://example.test") #1#) (curl ("-s" "-S" "-F" "file=@three.txt" "https://example.test") nil) (url (:file-path nil :query-str "name=\"file\"; filename=\"four.txt\"" :host-uri "https://example.test") #1#)))"#
        ]],
    )
}

fn zero_x_zero_handle_response_yanks_uri_reports_timeout_and_kills_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_handle_response_yanks_uri_reports_timeout_and_kills_buffer",
        r##"(let ((server
                      '(:scheme "https"
                        :host "example.test"
                        :min-age 30
                        :max-age 365
                        :max-size 100))
                     messages
                     (kill-ring nil)
                     (kill-ring-yank-pointer nil)
                     (response
                      (generate-new-buffer
                       "*0x0 parity response*")))
               (with-current-buffer response
                 (insert
                  "garbage\n"
                  "https://example.test/item.txt\n"
                  "tail\n"))
               (cl-letf (((symbol-function 'message)
                          (lambda (format-string &rest args)
                            (let ((text
                                   (apply
                                    #'format
                                    format-string
                                    args)))
                              (push text messages)
                              text))))
                 (let ((result
                        (0x0--handle-resp
                         server 50 response)))
                   (list
                    result
                    (current-kill 0)
                    (nreverse messages)
                    (buffer-live-p response)))))"##,
        expect![[
            r#"OK ("https://example.test/item.txt" "https://example.test/item.txt" ("yanked `https://example.test/item.txt' into kill ring. Should last ~71.875 days.") nil)"#
        ]],
    )
}

fn zero_x_zero_handle_response_failure_preserves_the_response_buffer() -> ParityBatchCase {
    ParityBatchCase::signal(
        "zero_x_zero_handle_response_failure_preserves_the_response_buffer",
        r##"(let ((server
                      '(:scheme "https"
                        :host "example.test"))
                     (response
                      (generate-new-buffer
                       "*0x0 bad response*")))
               (with-current-buffer response
                 (insert "not an upload response"))
               (unwind-protect
                   (0x0--handle-resp
                    server 1 response)
                 (when (buffer-live-p response)
                   (kill-buffer response))))"##,
        expect![[
            r#"ERR (error "Failed to upload/parse. see *0x0 bad response* for more details")"#
        ]],
    )
}

pub(super) fn transport_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        zero_x_zero_host_uri_supports_plain_and_basic_auth_servers(),
        zero_x_zero_curl_arguments_distinguish_file_and_buffer_uploads(),
        zero_x_zero_curl_dispatches_files_and_regions_to_exact_process_apis(),
        zero_x_zero_url_properties_distinguish_file_and_bounded_sources(),
        zero_x_zero_url_builds_multipart_body_and_strips_response_headers(),
        zero_x_zero_send_selects_each_curl_policy_and_url_fallback(),
        zero_x_zero_handle_response_yanks_uri_reports_timeout_and_kills_buffer(),
        zero_x_zero_handle_response_failure_preserves_the_response_buffer(),
    ]
}
