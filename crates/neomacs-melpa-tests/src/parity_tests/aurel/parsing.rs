use expect_test::expect;

use super::ParityBatchCase;

fn aurel_aur_json_interprets_error_empty_info_and_search_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_aur_json_interprets_error_empty_info_and_search_shapes",
        r##"(let ((responses
                '((("type" . "error")
                   ("error" . "bad request")
                   ("resultcount" . 0))
                  (("type" . "search")
                   ("resultcount" . 0)
                   ("results"))
                  (("type" . "info")
                   ("resultcount" . 1)
                   ("results"
                    ("Name" . "emacs-git")
                    ("ID" . 41)))
                  (("type" . "search")
                   ("resultcount" . 2)
                   ("results"
                    (("Name" . "one"))
                    (("Name" . "two")))))))
         (cl-letf
             (((symbol-function
                'aurel-receive-parse-info)
               (lambda (_url)
                 (pop responses))))
           (list
            (aurel-test-error-data
             (lambda ()
               (aurel-get-aur-packages-info
                "fixture:error")))
            (aurel-get-aur-packages-info
             "fixture:empty")
            (aurel-get-aur-packages-info
             "fixture:info")
            (aurel-get-aur-packages-info
             "fixture:search"))))"##,
        expect![[
            r#"OK ((:error error ("ERROR from AUR server: bad request")) nil ((("Name" . "emacs-git") ("ID" . 41))) ((("Name" . "one")) (("Name" . "two"))))"#
        ]],
    )
}

fn aurel_parameter_maps_round_trip_known_and_unknown_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_parameter_maps_round_trip_known_and_unknown_names",
        r##"(list
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (aurel-get-aur-param-name
              symbol)))
          '(name
            pkg-url
            depends-make
            missing))
         (mapcar
          (lambda (name)
            (list
             name
             (aurel-get-aur-param-symbol
              name)))
          '("Name"
            "URLPath"
            "MakeDepends"
            "Missing"))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (aurel-get-pacman-param-name
              symbol)))
          '(installed-name
            depends-opt
            installed-size
            missing))
         (mapcar
          (lambda (name)
            (list
             name
             (aurel-get-pacman-param-symbol
              name)))
          '("Name"
            "Optional Deps"
            "Installed Size"
            "Missing")))"##,
        expect![[
            r#"OK (((name "Name") (pkg-url "URLPath") (depends-make "MakeDepends") (missing nil)) (("Name" name) ("URLPath" pkg-url) ("MakeDepends" depends-make) ("Missing" nil)) ((installed-name "Name") (depends-opt "Optional Deps") (installed-size "Installed Size") (missing nil)) (("Name" installed-name) ("Optional Deps" depends-opt) ("Installed Size" installed-size) ("Missing" nil)))"#
        ]],
    )
}

fn aurel_filter_intern_keeps_known_fields_and_reports_unknown_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_filter_intern_keeps_known_fields_and_reports_unknown_once",
        r##"(let (messages)
         (cl-letf
             (((symbol-function 'message)
               (lambda (format-string &rest arguments)
                 (let ((text
                        (apply
                         #'format
                         format-string
                         arguments)))
                   (push text messages)
                   text))))
           (list
            (aurel-aur-filter-intern
             '(("Name" . "demo")
               ("NumVotes" . 17)
               ("UnknownField" . "drop")
               ("Description" . nil)))
            (aurel-pacman-filter-intern
             '(("Name" . "demo")
               ("Version" . "1.2")
               ("UnknownField" . "drop")))
            (nreverse messages))))"##,
        expect![[
            r#"OK (((name . "demo") (votes . 17) (description)) ((installed-name . "demo") (installed-version . "1.2")) ("Warning: unknown parameter `UnknownField'. It will be omitted."))"#
        ]],
    )
}

fn aurel_pacman_name_parser_returns_matching_names_in_scan_reverse_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_pacman_name_parser_returns_matching_names_in_scan_reverse_order",
        r##"(with-temp-buffer
         (insert
          "alpha 1.0-1\n"
          "c++-headers 2.0\n"
          "_private 3\n"
          "invalid/nope 4\n"
          "z-last 5 extra\n"
          "trailing-without-version\n")
         (list
          (aurel-pacman-query-names-buffer-parse
           (current-buffer))
          (point)))"##,
        expect![[r#"OK (("z-last" "_private" "c++-headers" "alpha") 62)"#]],
    )
}

fn aurel_pacman_info_parser_handles_multiple_packages_and_continuations() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_pacman_info_parser_handles_multiple_packages_and_continuations",
        r##"(with-temp-buffer
         (insert
          "Name            : alpha\n"
          "Version         : 1.0-1\n"
          "Depends On      : one  two\n"
          "                  three\n"
          "Optional Deps   : None\n"
          "\n"
          "Name            : beta\n"
          "Version         : 2.0\n"
          "Description     : ignored field\n"
          "\n")
         (aurel-pacman-query-buffer-parse
          (current-buffer)))"##,
        expect![[
            r#"OK ((("Name" . "alpha") ("Version" . "1.0-1") ("Depends On" . "one  two\n                  three") ("Optional Deps" . "None")) (("Name" . "beta") ("Version" . "2.0") ("Description" . "ignored field")))"#
        ]],
    )
}

fn aurel_call_pacman_erases_buffer_sets_locale_and_forwards_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_call_pacman_erases_buffer_sets_locale_and_forwards_arguments",
        r##"(let ((aurel-pacman-locale
                "C.UTF-8")
               captured)
         (with-temp-buffer
           (insert "stale")
           (cl-letf
               (((symbol-function
                  'call-process)
                 (lambda
                     (program infile destination display
                              &rest arguments)
                   (setq captured
                         (list
                          program
                          infile
                          destination
                          display
                          arguments
                          (car process-environment)
                          (buffer-string)))
                   23)))
             (list
              (aurel-call-pacman
               (current-buffer)
               "--query"
               "--info"
               "alpha"
               "beta")
              captured
              (buffer-string)))))"##,
        expect![[
            r#"OK (23 ("/fixture/bin/pacman" nil t nil ("--query" "--info" "alpha" "beta") "LC_ALL=C.UTF-8" "") "")"#
        ]],
    )
}

fn aurel_call_pacman_missing_program_fails_before_touching_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_call_pacman_missing_program_fails_before_touching_buffer",
        r##"(let ((aurel-pacman-program
                nil))
         (with-temp-buffer
           (insert "preserved")
           (list
            (aurel-test-error-data
             (lambda ()
               (aurel-call-pacman
                (current-buffer)
                "--query")))
            (buffer-string))))"##,
        expect![[
            r#"OK ((:error error ("Couldn’t find pacman.\nSet aurel-pacman-program to a proper value")) "preserved")"#
        ]],
    )
}

fn aurel_response_status_accepts_2xx_3xx_and_handles_bad_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_response_status_accepts_2xx_3xx_and_handles_bad_values",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (setq-local
              url-http-response-status
              (car case))
             (list
              (car case)
              (aurel-test-error-data
               (lambda ()
                 (aurel-check-response-status
                  (current-buffer)
                  (cdr case)))))))
         '((200)
           (399)
           (400)
           (500 . t)
           (nil)
           ("200" . t)))"##,
        expect![[
            r#"OK ((200 (:ok t)) (399 (:ok t)) (400 (:error error ("Error during request: 400"))) (500 (:ok nil)) (nil (:error error ("Error during request: nil"))) ("200" (:ok nil)))"#
        ]],
    )
}

fn aurel_receive_parse_info_reads_json_with_string_keys_lists_and_alists() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_receive_parse_info_reads_json_with_string_keys_lists_and_alists",
        r##"(cl-letf
         (((symbol-function
            'url-insert-file-contents)
           (lambda (url)
             (insert
              "{\"type\":\"search\","
              "\"resultcount\":2,"
              "\"results\":["
              "{\"Name\":\"alpha\","
              "\"Keywords\":[\"one\",\"two\"],"
              "\"OutOfDate\":null},"
              "{\"Name\":\"beta\","
              "\"NumVotes\":7}]}")
             (list url
                   (point-max)))))
         (aurel-receive-parse-info
          "fixture:packages.json"))"##,
        expect![[
            r#"OK (("type" . "search") ("resultcount" . 2) ("results" (("Name" . "alpha") ("Keywords" "one" "two") ("OutOfDate")) (("Name" . "beta") ("NumVotes" . 7))))"#
        ]],
    )
}

fn aurel_html_action_detection_distinguishes_available_completed_and_unknown() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_html_action_detection_distinguishes_available_completed_and_unknown",
        r##"(mapcar
         (lambda (html)
           (with-temp-buffer
             (insert html)
             (list
              (aurel-aur-package-voted
               (current-buffer))
              (aurel-aur-package-subscribed
               (current-buffer))
              (mapcar
               #'aurel-get-aur-user-action-name
               '(vote
                 unvote
                 subscribe
                 unsubscribe
                 missing)))))
         '("<form name=\"do_Vote\">Vote</form><form name=\"do_Notify\">Notify</form>"
           "<form name=\"do_UnVote\">Unvote</form><form name=\"do_UnNotify\">Unnotify</form>"
           "<html>No user controls</html>"))"##,
        expect![[
            r#"OK ((nil nil ("do_Vote" "do_UnVote" "do_Notify" "do_UnNotify" nil)) (t t ("do_Vote" "do_UnVote" "do_Notify" "do_UnNotify" nil)) ("Unknown" "Unknown" ("do_Vote" "do_UnVote" "do_Notify" "do_UnNotify" nil)))"#
        ]],
    )
}

fn aurel_installed_package_wrappers_use_shared_buffer_and_exact_pacman_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_installed_package_wrappers_use_shared_buffer_and_exact_pacman_modes",
        r##"(let (calls)
         (cl-letf
             (((symbol-function
                'aurel-call-pacman)
               (lambda (buffer &rest arguments)
                 (push
                  (list
                   (buffer-name buffer)
                   arguments)
                  calls)
                 (with-current-buffer buffer
                   (erase-buffer)
                   (if
                       (equal arguments
                              '("--query"
                                "--foreign"))
                       (insert
                        "alpha 1\n"
                        "beta 2\n")
                     (insert
                      "Name : alpha\n"
                      "Version : 1\n"
                      "\n"
                      "Name : beta\n"
                      "Version : 2\n"
                      "\n")))
                 0)))
           (list
            (aurel-get-foreign-packages)
            (aurel-get-installed-packages-info
             "alpha"
             "beta")
            (nreverse calls))))"##,
        expect![[
            r#"OK (("beta" "alpha") ((("Name" . "alpha") ("Version" . "1")) (("Name" . "beta") ("Version" . "2"))) ((" *aurel-pacman*" ("--query" "--foreign")) (" *aurel-pacman*" ("--query" "--info" "alpha" "beta"))))"#
        ]],
    )
}

pub(super) fn parsing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurel_aur_json_interprets_error_empty_info_and_search_shapes(),
        aurel_parameter_maps_round_trip_known_and_unknown_names(),
        aurel_filter_intern_keeps_known_fields_and_reports_unknown_once(),
        aurel_pacman_name_parser_returns_matching_names_in_scan_reverse_order(),
        aurel_pacman_info_parser_handles_multiple_packages_and_continuations(),
        aurel_call_pacman_erases_buffer_sets_locale_and_forwards_arguments(),
        aurel_call_pacman_missing_program_fails_before_touching_buffer(),
        aurel_response_status_accepts_2xx_3xx_and_handles_bad_values(),
        aurel_receive_parse_info_reads_json_with_string_keys_lists_and_alists(),
        aurel_html_action_detection_distinguishes_available_completed_and_unknown(),
        aurel_installed_package_wrappers_use_shared_buffer_and_exact_pacman_modes(),
    ]
}
