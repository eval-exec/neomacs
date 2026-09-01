use expect_test::expect;

use super::ParityBatchCase;

fn zero_x_zero_upload_file_expands_path_and_forwards_exact_size() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_upload_file_expands_path_and_forwards_exact_size",
        r##"(let* ((root
                        (getenv
                         "NEOMACS_TEST_SANDBOX_ROOT"))
                       (default-directory
                        (file-name-as-directory root))
                       (file "payload.txt")
                       (server
                        '(:host "example.test"))
                       events)
               (with-temp-file file
                 (insert "payload"))
               (unwind-protect
                   (cl-letf (((symbol-function '0x0--send)
                              (lambda
                                  (actual-server
                                   actual-file
                                   &optional bounds)
                                (push
                                 (list
                                  'send
                                  actual-server
                                  (file-name-nondirectory
                                   actual-file)
                                  bounds)
                                 events)
                                'response))
                             ((symbol-function
                               '0x0--handle-resp)
                              (lambda
                                  (actual-server
                                   size
                                   response)
                                (push
                                 (list
                                  'handle
                                  actual-server
                                  size
                                  response)
                                 events)
                                'uploaded)))
                     (list
                      (0x0-upload-file server file)
                      (nreverse events)))
                 (delete-file file)))"##,
        expect![[
            r#"OK (uploaded ((send #1=(:host "example.test") "payload.txt" nil) (handle #1# 7 response)))"#
        ]],
    )
}

fn zero_x_zero_upload_text_forwards_full_buffer_and_active_region_bounds() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_upload_text_forwards_full_buffer_and_active_region_bounds",
        r##"(let ((server
                      '(:host "example.test"))
                     events)
               (cl-letf (((symbol-function '0x0--send)
                          (lambda
                              (_server file bounds)
                            (push
                             (list
                              'send
                              file
                              bounds)
                             events)
                            'response))
                         ((symbol-function
                           '0x0--handle-resp)
                          (lambda
                              (_server size response)
                            (push
                             (list
                              'handle
                              size
                              response)
                             events)
                            'uploaded)))
                 (with-temp-buffer
                   (insert "abcdef")
                   (let ((full
                          (cl-letf
                              (((symbol-function
                                 'use-region-p)
                                (lambda () nil)))
                            (0x0-upload-text
                             server)))
                         region)
                     (setq region
                           (cl-letf
                               (((symbol-function
                                  'use-region-p)
                                 (lambda () t))
                                ((symbol-function
                                  'region-beginning)
                                 (lambda () 2))
                                ((symbol-function
                                  'region-end)
                                 (lambda () 5)))
                             (0x0-upload-text
                              server)))
                     (list
                      full
                      region
                      (nreverse events))))))"##,
        expect![[
            r#"OK (uploaded uploaded ((send "upload.txt" (:start 1 :end 7)) (handle 6 response) (send "upload.txt" (:start 2 :end 5)) (handle 3 response)))"#
        ]],
    )
}

fn zero_x_zero_upload_kill_ring_copies_content_into_an_isolated_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_upload_kill_ring_copies_content_into_an_isolated_buffer",
        r##"(let ((server
                      '(:host "example.test"))
                     (kill-ring '("copied λ text"))
                     (kill-ring-yank-pointer nil)
                     events)
               (setq kill-ring-yank-pointer kill-ring)
               (cl-letf (((symbol-function '0x0--send)
                          (lambda
                              (_server file bounds)
                            (push
                             (list
                              'send
                              file
                              bounds
                              (buffer-string))
                             events)
                            'response))
                         ((symbol-function
                           '0x0--handle-resp)
                          (lambda
                              (_server size response)
                            (push
                             (list
                              'handle
                              size
                              response)
                             events)
                            'uploaded)))
                 (list
                  (0x0-upload-kill-ring server)
                  (nreverse events))))"##,
        expect![[
            r#"OK (uploaded ((send " *temp*" (:start 1 :end 14) "copied λ text") (handle 13 response)))"#
        ]],
    )
}

fn zero_x_zero_shorten_uri_builds_curl_request_and_skips_timeout_estimate() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_shorten_uri_builds_curl_request_and_skips_timeout_estimate",
        r##"(let ((server
                      '(:scheme "https"
                        :host "example.test"))
                     (0x0-use-curl t)
                     events)
               (cl-letf (((symbol-function '0x0--curl)
                          (lambda (args &optional bounds)
                            (push
                             (list 'curl args bounds)
                             events)
                            'response))
                         ((symbol-function
                           '0x0--handle-resp)
                          (lambda
                              (actual-server
                               size
                               response)
                            (push
                             (list
                              'handle
                              actual-server
                              size
                              response)
                             events)
                            'shortened)))
                 (list
                  (0x0-shorten-uri
                   server
                   "https://long.example/path?q=1")
                  (nreverse events))))"##,
        expect![[
            r#"OK (shortened ((curl ("-s" "-S" "-F" "shorten=https://long.example/path?q=1" "https://example.test") nil) (handle (:scheme "https" :host "example.test") nil response)))"#
        ]],
    )
}

fn zero_x_zero_shorten_uri_rejects_url_fallback() -> ParityBatchCase {
    ParityBatchCase::signal(
        "zero_x_zero_shorten_uri_rejects_url_fallback",
        r##"(let ((0x0-use-curl nil))
               (0x0-shorten-uri
                '(:scheme "https"
                  :host "example.test")
                "https://long.example"))"##,
        expect![[r#"ERR (error "Unsupported currenlty without using curl")"#]],
    )
}

fn zero_x_zero_popup_upload_forwards_content_then_kills_the_popup() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_popup_upload_forwards_content_then_kills_the_popup",
        r##"(let ((server
                      '(:host "example.test"))
                     (popup
                      (generate-new-buffer
                       "*0x0 popup probe*"))
                     events
                     result)
               (with-current-buffer popup
                 (insert "popup body")
                 (cl-letf (((symbol-function '0x0--send)
                            (lambda
                                (_server file bounds)
                              (push
                               (list
                                'send
                                file
                                bounds
                                (buffer-string))
                               events)
                              'response))
                           ((symbol-function
                             '0x0--handle-resp)
                            (lambda
                                (_server size response)
                              (push
                               (list
                                'handle
                                size
                                response)
                               events)
                              'uploaded)))
                   (setq result
                         (0x0-popup-upload server))))
               (list
                result
                (nreverse events)
                (buffer-live-p popup)))"##,
        expect![[
            r#"OK (t ((send "popup-upload.txt" (:start 1 :end 11) "popup body") (handle 10 response)) nil)"#
        ]],
    )
}

fn zero_x_zero_popup_creates_local_upload_binding_and_displays_instructions() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_popup_creates_local_upload_binding_and_displays_instructions",
        r##"(let (shown message-text popup)
               (cl-letf (((symbol-function 'pop-to-buffer)
                          (lambda (buffer &rest _)
                            (setq shown buffer)
                            buffer))
                         ((symbol-function 'message)
                          (lambda (format-string &rest args)
                            (setq message-text
                                  (apply
                                   #'format
                                   format-string
                                   args))
                            message-text)))
                 (unwind-protect
                     (progn
                       (0x0-popup
                        '(:host "example.test"))
                       (setq popup shown)
                       (list
                        (buffer-name popup)
                        (with-current-buffer popup
                          (let ((binding
                                 (local-key-binding
                                  (kbd "C-c C-c"))))
                            (list
                             (functionp binding)
                             (commandp binding))))
                        message-text))
                   (when (buffer-live-p popup)
                     (kill-buffer popup)))))"##,
        expect![[r#"OK ("*upload*" (t t) "Press C-c C-c to upload.")"#]],
    )
}

fn zero_x_zero_dwim_dispatches_region_kill_dired_file_guess_and_fallback() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_zero_dwim_dispatches_region_kill_dired_file_guess_and_fallback",
        r##"(let ((server
                      '(:host "example.test"))
                     events
                     region-active
                     derived-dired
                     guessed
                     confirmation
                     last-command)
               (cl-letf (((symbol-function 'region-active-p)
                          (lambda () region-active))
                         ((symbol-function 'derived-mode-p)
                          (lambda (&rest modes)
                            (and
                             derived-dired
                             (memq
                              'dired-mode modes))))
                         ((symbol-function
                           'dired-file-name-at-point)
                          (lambda () "/dired/file"))
                         ((symbol-function
                           'ffap-guess-file-name-at-point)
                          (lambda () guessed))
                         ((symbol-function 'yes-or-no-p)
                          (lambda (prompt)
                            (push
                             (list 'prompt prompt)
                             events)
                            confirmation))
                         ((symbol-function
                           '0x0-upload-text)
                          (lambda (_)
                            (push 'text events)
                            'text))
                         ((symbol-function
                           '0x0-upload-kill-ring)
                          (lambda (_)
                            (push 'kill-ring events)
                            'kill-ring))
                         ((symbol-function
                           '0x0-upload-file)
                          (lambda (_ file)
                            (push
                             (list 'file file)
                             events)
                            'file)))
                 (let ((region-result
                        (progn
                          (setq region-active t
                                last-command nil)
                          (0x0-dwim server)))
                       kill-result
                       dired-result
                       accepted-file-result
                       rejected-file-result
                       fallback-result)
                   (setq region-active nil
                         last-command
                         'kill-ring-save
                         kill-result
                         (0x0-dwim server)
                         last-command nil
                         derived-dired t
                         dired-result
                         (0x0-dwim server)
                         derived-dired nil
                         guessed "/guessed/file"
                         confirmation t
                         accepted-file-result
                         (0x0-dwim server)
                         confirmation nil
                         rejected-file-result
                         (0x0-dwim server)
                         guessed nil
                         fallback-result
                         (0x0-dwim server))
                   (list
                    region-result
                    kill-result
                    dired-result
                    accepted-file-result
                    rejected-file-result
                    fallback-result
                    (nreverse events)))))"##,
        expect![[
            r#"OK (text kill-ring file file nil text (text kill-ring (file "/dired/file") (prompt "Is publicly sharing this file, /guessed/file, what you intended?") (file "/guessed/file") (prompt "Is publicly sharing this file, /guessed/file, what you intended?") text))"#
        ]],
    )
}

pub(super) fn commands_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        zero_x_zero_upload_file_expands_path_and_forwards_exact_size(),
        zero_x_zero_upload_text_forwards_full_buffer_and_active_region_bounds(),
        zero_x_zero_upload_kill_ring_copies_content_into_an_isolated_buffer(),
        zero_x_zero_shorten_uri_builds_curl_request_and_skips_timeout_estimate(),
        zero_x_zero_shorten_uri_rejects_url_fallback(),
        zero_x_zero_popup_upload_forwards_content_then_kills_the_popup(),
        zero_x_zero_popup_creates_local_upload_binding_and_displays_instructions(),
        zero_x_zero_dwim_dispatches_region_kill_dired_file_guess_and_fallback(),
    ]
}
