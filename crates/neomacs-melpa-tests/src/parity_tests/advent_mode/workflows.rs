use expect_test::expect;

use super::ParityBatchCase;

/// The everyday loop: store the session cookie, sit in the day's directory,
/// and pull the puzzle input.  The year and day are never typed -- they are
/// inferred from `year2024/day03/' through the configured directory formats --
/// and the request is pinned in full: the URL the package builds, the GET
/// method, the absence of extra headers, and the `Cookie' header url.el
/// generates from the cookie `advent-login' actually stored.  The input lands
/// at the configured path with the service's bytes and is opened for the user.
/// Asking again does not hit the network: the file on disk is used.
fn logs_in_and_fetches_the_puzzle_input_for_the_day_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "logs_in_and_fetches_the_puzzle_input_for_the_day_at_point",
        r##"
        (progn
          (adv-test-install-transport)
          (adv-test-project "year2024/day03")
          (adv-test-serve
           (adv-test-reply "200 OK"
                           '("Content-Type: text/plain"
                             "Content-Length: 39")
                           "xmul(2,4)%&mul[3,7]!@^do_not_mul(5,5)+\n"))
          (list
           :login (list :cookie-before (advent--cookie-ok-p)
                        :message (advent-login adv-test-session)
                        :cookie-after (advent--cookie-ok-p))
           :context (progn
                      (adv-test-in-dir "year2024/day03")
                      (advent-mode 1)
                      (list :year-day (advent--context-year-day)
                            :lighter (adv-test-lighter)))
           :fetched (progn
                      (advent-fetch-input)
                      (list :requests (adv-test-requests)
                            :tree (adv-test-tree)
                            :input (adv-test-file-text "year2024/day03/input.txt")
                            :window-buffer
                            (buffer-name (window-buffer (selected-window)))
                            :messages (adv-test-messages "saved\\.\\'")))
           :second-fetch (progn
                           (advent-fetch-input)
                           (list :requests (length (adv-test-requests))
                                 :window-buffer
                                 (buffer-name (window-buffer (selected-window)))))))
    "##,
        expect![[
            r#"OK (:login (:cookie-before nil :message "AoC session cookie stored." :cookie-after t) :context (:year-day (2024 3) :lighter " AoC[Y2024/D3 ✓]") :fetched (:requests ((:url "https://adventofcode.com/2024/day/3/input" :method "GET" :extra-headers nil :data nil :cookie-header "Cookie: session=53616c7465645f5fdeadbeefcafef00d0123456789abcdef\15\n")) :tree ("year2024/" "year2024/day03/" "year2024/day03/input.txt") :input "xmul(2,4)%&mul[3,7]!@^do_not_mul(5,5)+\n" :window-buffer "input.txt" :messages ("[ORACLE-SANDBOX]/aoc/year2024/day03/input.txt saved.")) :second-fetch (:requests 1 :window-buffer "input.txt"))"#
        ]],
    )
}

fn submits_an_answer_and_shows_what_the_service_replied() -> ParityBatchCase {
    ParityBatchCase::value(
        "submits_an_answer_and_shows_what_the_service_replied",
        r##"
        (progn
          (adv-test-install-transport)
          (adv-test-project "year2024/day03")
          (advent-login adv-test-session)
          (adv-test-in-dir "year2024/day03")
          (adv-test-serve
           (adv-test-reply
            "200 OK" '("Content-Type: text/html")
            "<main><article><p>That's the right answer!  You are one gold star closer to finding the Chief Historian. [Continue to Part Two]</p></article></main>\n")
           (adv-test-reply
            "200 OK" '("Content-Type: text/html")
            "<main><article><p>That's not the right answer; your answer is too low.  Please wait one minute before trying again. [Return to Day 3]</p></article></main>\n")
           (adv-test-reply
            "200 OK" '("Content-Type: text/html")
            "<main><article><p>You gave an answer too recently; you have to wait after submitting an answer before trying again.  You have 44s left to wait. [Return to Day 3]</p></article></main>\n"))
          (cl-flet ((submit (answer level)
                      (let ((returned (advent-submit-answer answer level)))
                        (list :returned-equals-shown
                              (equal returned
                                     (with-current-buffer "*AoC Submit*"
                                       (buffer-substring-no-properties
                                        (point-min) (point-max))))
                              :shown (with-current-buffer "*AoC Submit*"
                                       (list :text (buffer-substring-no-properties
                                                    (point-min) (point-max))
                                             :point (point)))))))
            (list :correct (submit "161" "1")
                  :incorrect (submit "12" "2")
                  :too-soon (submit "999" "2")
                  :requests (adv-test-requests)
                  :messages (adv-test-messages "Submitted answer"))))
    "##,
        expect![[
            r#"OK (:correct (:returned-equals-shown t :shown (:text "<main><article><p>That's the right answer!  You are one gold star closer to finding the Chief Historian. [Continue to Part Two]</p></article></main>\n" :point 1)) :incorrect (:returned-equals-shown t :shown (:text "<main><article><p>That's not the right answer; your answer is too low.  Please wait one minute before trying again. [Return to Day 3]</p></article></main>\n" :point 1)) :too-soon (:returned-equals-shown t :shown (:text "<main><article><p>You gave an answer too recently; you have to wait after submitting an answer before trying again.  You have 44s left to wait. [Return to Day 3]</p></article></main>\n" :point 1)) :requests ((:url "https://adventofcode.com/2024/day/3/answer" :method "POST" :extra-headers #1=(("Content-Type" . "application/x-www-form-urlencoded")) :data "level=1&answer=161" :cookie-header "Cookie: session=53616c7465645f5fdeadbeefcafef00d0123456789abcdef\15\n") (:url "https://adventofcode.com/2024/day/3/answer" :method "POST" :extra-headers #1# :data "level=2&answer=12" :cookie-header "Cookie: session=53616c7465645f5fdeadbeefcafef00d0123456789abcdef\15\n") (:url "https://adventofcode.com/2024/day/3/answer" :method "POST" :extra-headers #1# :data "level=2&answer=999" :cookie-header "Cookie: session=53616c7465645f5fdeadbeefcafef00d0123456789abcdef\15\n")) :messages ("Submitted answer for 2024 day 3 (level 1)" "Submitted answer for 2024 day 3 (level 2) [2 times]"))"#
        ]],
    )
}

fn creates_a_day_directory_with_templates_and_downloads_the_input() -> ParityBatchCase {
    ParityBatchCase::value(
        "creates_a_day_directory_with_templates_and_downloads_the_input",
        r##"
        (progn
          (adv-test-install-transport)
          (adv-test-project)
          (advent-login adv-test-session)
          (adv-test-write (expand-file-name "template.py" adv-test-root)
                          "import sys\n\ndef part1(lines):\n    return 0\n")
          (adv-test-write (expand-file-name "notes.md" adv-test-root)
                          "# Notes\n")
          (setq advent-new-files '("template.py" "notes.md"))
          (adv-test-serve
           (adv-test-reply "200 OK" '("Content-Type: text/plain")
                           "1721\n979\n366\n299\n675\n1456\n"))
          (adv-test-in-dir "")
          (list
           :created (adv-test-answering
                     (list t nil t)
                     (advent-create-day 2024 5 adv-test-root))
           :tree (adv-test-tree)
           :copied (list :template (adv-test-file-text "year2024/day05/template.py")
                         :notes (adv-test-file-text "year2024/day05/notes.md"))
           :input (adv-test-file-text "year2024/day05/input.txt")
           :requests (adv-test-requests)
           :window-buffer (buffer-name (window-buffer (selected-window)))
           :messages (adv-test-messages "Created \\|saved\\.\\'")))
    "##,
        expect![[
            r##"OK (:created (:prompts ("Dir created.  Copy template files into it? " "Open the problem page in EWW? " "Download and open the input file? ") :result (:buffer "input.txt")) :tree ("notes.md" "template.py" "year2024/" "year2024/day05/" "year2024/day05/input.txt" "year2024/day05/notes.md" "year2024/day05/template.py") :copied (:template "import sys\n\ndef part1(lines):\n    return 0\n" :notes "# Notes\n") :input "1721\n979\n366\n299\n675\n1456\n" :requests ((:url "https://adventofcode.com/2024/day/5/input" :method "GET" :extra-headers nil :data nil :cookie-header "Cookie: session=53616c7465645f5fdeadbeefcafef00d0123456789abcdef\15\n")) :window-buffer "input.txt" :messages ("Created [ORACLE-SANDBOX]/aoc/year2024/day05" "[ORACLE-SANDBOX]/aoc/year2024/day05/input.txt saved."))"##
        ]],
    )
    .fresh_process()
}

fn directory_format_customizations_drive_both_paths_and_inference() -> ParityBatchCase {
    ParityBatchCase::value(
        "directory_format_customizations_drive_both_paths_and_inference",
        r##"
        (progn
          (adv-test-install-transport)
          (adv-test-project)
          (advent-login adv-test-session)
          (setq advent-year-dir-format "aoc-%d"
                advent-day-dir-format "puzzle-%03d"
                advent-input-file-name "puzzle-input.dat")
          (adv-test-serve
           (adv-test-reply "200 OK" '("Content-Type: text/plain") "3   4\n4   3\n"))
          (adv-test-in-dir "")
          (list
           :created (adv-test-answering
                     (list nil nil nil)
                     (advent-create-day 2023 7 adv-test-root))
           :tree-after-create (adv-test-tree)
           :inference (progn
                        (adv-test-in-dir "aoc-2023/puzzle-007")
                        (list :year-day (advent--context-year-day)
                              :lighter (progn (advent-mode 1) (adv-test-lighter))))
           :fetched (progn
                      (advent-fetch-input)
                      (list :requests (adv-test-requests)
                            :input (adv-test-file-text
                                    "aoc-2023/puzzle-007/puzzle-input.dat")
                            :tree (adv-test-tree)))
           :opened (progn
                     (advent-open-day 2023 7 adv-test-root)
                     (list :window-buffer
                           (buffer-name (window-buffer (selected-window)))
                           :major-mode
                           (with-current-buffer (window-buffer (selected-window))
                             major-mode)))
           :stock-names-no-longer-match
           (progn (adv-test-in-dir "")
                  (make-directory (expand-file-name "year2022/day01" adv-test-root) t)
                  (adv-test-in-dir "year2022/day01")
                  (advent--context-year-day))))
    "##,
        expect![[
            r#"OK (:created (:prompts ("Open the problem page in EWW? " "Download and open the input file? ") :result nil) :tree-after-create ("aoc-2023/" "aoc-2023/puzzle-007/") :inference (:year-day (2023 7) :lighter " AoC[Y2023/D7 ✓]") :fetched (:requests ((:url "https://adventofcode.com/2023/day/7/input" :method "GET" :extra-headers nil :data nil :cookie-header "Cookie: session=53616c7465645f5fdeadbeefcafef00d0123456789abcdef\15\n")) :input "3   4\n4   3\n" :tree ("aoc-2023/" "aoc-2023/puzzle-007/" "aoc-2023/puzzle-007/puzzle-input.dat")) :opened (:window-buffer "puzzle-007" :major-mode dired-mode) :stock-names-no-longer-match nil)"#
        ]],
    )
    .fresh_process()
}

fn refuses_to_reach_the_service_without_a_session_cookie() -> ParityBatchCase {
    ParityBatchCase::value(
        "refuses_to_reach_the_service_without_a_session_cookie",
        r##"
        (progn
          (adv-test-install-transport)
          (adv-test-project "year2024/day03")
          (adv-test-serve
           (adv-test-reply "200 OK" '("Content-Type: text/plain") "ok\n"))
          (adv-test-in-dir "year2024/day03")
          (list
           :cookie-before (advent--cookie-ok-p)
           :lighter (progn (advent-mode 1) (adv-test-lighter))
           :fetch-declined
           (adv-test-answering
            (list nil)
            (condition-case error (progn (advent-fetch-input) 'fetched)
              (error (list (car error) (error-message-string error)))))
           :submit-declined
           (adv-test-answering
            (list nil)
            (condition-case error (progn (advent-submit-answer "1" "1") 'submitted)
              (error (list (car error) (error-message-string error)))))
           :requests (adv-test-requests)
           :tree (adv-test-tree)
           :after-login (progn
                          (advent-login adv-test-session)
                          (advent-fetch-input)
                          (list :lighter (adv-test-lighter)
                                :requests (length (adv-test-requests))
                                :input (adv-test-file-text
                                        "year2024/day03/input.txt")))))
    "##,
        expect![[
            r#"OK (:cookie-before nil :lighter " AoC[Y2024/D3 ✗]" :fetch-declined (:prompts ("AoC session cookie missing.  Set it now? ") :result (user-error "No AoC session cookie set; run M-x advent-login")) :submit-declined (:prompts ("AoC session cookie missing.  Set it now? ") :result (user-error "No AoC session cookie set; run M-x advent-login")) :requests nil :tree ("year2024/" "year2024/day03/") :after-login (:lighter " AoC[Y2024/D3 ✓]" :requests 1 :input "ok\n"))"#
        ]],
    )
    .fresh_process()
}

fn reports_service_failures_and_writes_no_input_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "reports_service_failures_and_writes_no_input_file",
        r##"
        (progn
          (adv-test-install-transport)
          (adv-test-project "year2024/day03")
          (advent-login adv-test-session)
          (adv-test-in-dir "year2024/day03")
          (adv-test-serve
           (adv-test-reply "404 Not Found" '("Content-Type: text/html")
                           "<html><head><title>404 Not Found</title></head><body>Not Found</body></html>\n")
           (adv-test-reply "200 OK" '("Content-Type: text/plain") "")
           "HTTP/1.1 200 OK garbled-with-no-blank-line"
           nil
           (adv-test-reply "200 OK" '("Content-Type: text/plain") "7\n"))
          (cl-flet ((attempt ()
                      (list :error (condition-case error
                                       (progn (advent-fetch-input) 'fetched)
                                     (error (list (car error)
                                                  (error-message-string error))))
                            :tree (adv-test-tree))))
            (list :not-found (attempt)
                  :empty-body (attempt)
                  :malformed (attempt)
                  :no-connection (attempt)
                  :recovers (progn (advent-fetch-input)
                                   (list :input (adv-test-file-text
                                                 "year2024/day03/input.txt")
                                         :tree (adv-test-tree)))
                  :requests (mapcar (lambda (request) (plist-get request :url))
                                    (adv-test-requests)))))
    "##,
        expect![[
            r#"OK (:not-found (:error (error "HTTP 404: <html><head><title>404 Not Found</title></head><body>Not Found</body></html>") :tree ("year2024/" "year2024/day03/")) :empty-body (:error (error "Empty HTTP response body") :tree ("year2024/" "year2024/day03/")) :malformed (:error (error "Malformed HTTP response (no header/body separator)") :tree ("year2024/" "year2024/day03/")) :no-connection (:error (error "Failed to GET https://adventofcode.com/2024/day/3/input") :tree ("year2024/" "year2024/day03/")) :recovers (:input "7\n" :tree ("year2024/" "year2024/day03/" "year2024/day03/input.txt")) :requests ("https://adventofcode.com/2024/day/3/input" "https://adventofcode.com/2024/day/3/input" "https://adventofcode.com/2024/day/3/input" "https://adventofcode.com/2024/day/3/input" "https://adventofcode.com/2024/day/3/input"))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        logs_in_and_fetches_the_puzzle_input_for_the_day_at_point(),
        submits_an_answer_and_shows_what_the_service_replied(),
        creates_a_day_directory_with_templates_and_downloads_the_input(),
        directory_format_customizations_drive_both_paths_and_inference(),
        refuses_to_reach_the_service_without_a_session_cookie(),
        reports_service_failures_and_writes_no_input_file(),
    ]
}
