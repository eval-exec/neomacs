use expect_test::expect;

use super::ParityBatchCase;

fn fetching_a_quote_sends_a_real_request_and_echoes_the_line_it_scrapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "fetching_a_quote_sends_a_real_request_and_echoes_the_line_it_scrapes",
        r##"
(progn
  (apc-test-listen)
  (let ((result
         (apc-test-fetch
          (lambda (_request)
            (apc-test-response
             (concat "<!DOCTYPE html>\n"
                     "<html><head><title>The Angry Police Captain</title>"
                     "</head>\n<body>\n"
                     "<div id=\"quote\">"
                     "<a href=\"http://theangrypolicecaptain.com\">"
                     "You have 24 hours, Sanchez.</a></div>\n"
                     "</body></html>\n"))))))
    (list :result result
          ;; The URL is hard-coded in the package, so what reaches the wire
          ;; is the whole of what the user gets to control.
          :request (apc-test-request)
          :autoloaded (and (commandp 'angry-police-captain) t))))
"##,
        expect![[
            r#"OK (:result (:echoed ("You have 24 hours, Sanchez") :requests-served 1 :leftover-buffers nil) :request ("GET http://theangrypolicecaptain.com HTTP/1.1\15\nMIME-Version: 1.0\15\nConnection: close\15\nHost: theangrypolicecaptain.com\15\nAccept-encoding: gzip\15\nAccept: */*\15\nUser-Agent: URL/Emacs Emacs/<VERSION> (TTY; x86_64-pc-linux-gnu)\15\n\15\n") :autoloaded t)"#
        ]],
    )
}

fn the_scraper_takes_the_first_marked_link_and_drops_its_last_character() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_scraper_takes_the_first_marked_link_and_drops_its_last_character",
        r##"
(progn
  (apc-test-listen)
  (list
   ;; `re-search-forward' for "</a>" leaves point after the ">", and the
   ;; package then walks back five characters where "</a>" is four.  Every
   ;; quote loses its final character.
   :trailing-period
   (cons "You have 24 hours, Sanchez."
         (apc-test-quote
          (concat "<a href=\"http://theangrypolicecaptain.com\">"
                  "You have 24 hours, Sanchez.</a>\n")))
   :trailing-exclamation
   (cons "Give me the badge!"
         (apc-test-quote
          (concat "<a href=\"http://theangrypolicecaptain.com\">"
                  "Give me the badge!</a>\n")))
   ;; With nothing between the tags the walk-back crosses the opening tag
   ;; and returns part of the href attribute.
   :empty-link
   (apc-test-quote "<a href=\"http://theangrypolicecaptain.com\"></a>\n")
   ;; The first marked link wins; a second one is never reached.
   :two-links
   (apc-test-quote
    (concat "<a href=\"http://theangrypolicecaptain.com\">"
            "First, and the only one.</a>\n"
            "<a href=\"http://theangrypolicecaptain.com\">"
            "Second, never seen.</a>\n"))
   ;; No entity decoding and no coding-system decoding: the response buffer
   ;; is raw, so accented text arrives as its undecoded bytes.
   :entities-and-accents
   (apc-test-quote
    (concat "<a href=\"http://theangrypolicecaptain.com\">"
            "Sánchez &mdash; hand it over!</a>\n"))))
"##,
        expect![[
            r#"OK (:trailing-period ("You have 24 hours, Sanchez." . "You have 24 hours, Sanchez") :trailing-exclamation ("Give me the badge!" . "Give me the badge") :empty-link ">" :two-links "First, and the only one" :entities-and-accents "S\303\241nchez &mdash; hand it over")"#
        ]],
    )
    .fresh_process()
}

fn the_command_can_only_finish_when_it_is_invoked_the_way_the_menu_invokes_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_command_can_only_finish_when_it_is_invoked_the_way_the_menu_invokes_it",
        r##"
(list
 ;; The callback ends by calling `kill-this-buffer', which since Emacs 30
 ;; refuses to run unless `last-command-event' is the menu item's own
 ;; symbol.  `M-x angry-police-captain' therefore never reaches the
 ;; `message' on the following line, and no quote is ever displayed.
 :invoked-from-m-x
 (condition-case failure
     (let ((last-command-event nil)) (kill-this-buffer))
   (error (list (car failure) (cadr failure))))
 :invoked-from-a-key
 (condition-case failure
     (let ((last-command-event ?x)) (kill-this-buffer))
   (error (list (car failure) (cadr failure))))
 :invoked-from-the-menu
 (with-current-buffer (get-buffer-create "*apc-doomed*")
   (let ((last-command-event 'kill-buffer))
     (list :killed (progn (kill-this-buffer)
                          (not (buffer-live-p (get-buffer "*apc-doomed*"))))))))
"##,
        expect![[
            r#"OK (:invoked-from-m-x (error "This command must be called from a menu or a tool bar") :invoked-from-a-key (error "This command must be called from a menu or a tool bar") :invoked-from-the-menu (:killed t))"#
        ]],
    )
}

fn the_response_status_is_ignored_and_a_redirect_leaves_its_first_buffer_behind() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_response_status_is_ignored_and_a_redirect_leaves_its_first_buffer_behind",
        r##"
(progn
  (apc-test-listen)
  (list
   ;; Nothing looks at the status line, so an error page that happens to
   ;; carry a marked link is scraped and echoed like any other.
   :not-found
   (apc-test-fetch
    (lambda (_request)
      (apc-test-response
       (concat "<a href=\"http://theangrypolicecaptain.com\">"
               "Gone, but still quotable.</a>\n")
       "404 Not Found")))
   ;; url.el follows the redirect and the quote comes from the second
   ;; response, but `kill-this-buffer' only ever kills the buffer the
   ;; callback runs in, so the first response buffer is left behind.
   :redirected
   (apc-test-fetch
    (lambda (request)
      (if (string-match-p "/moved" request)
          (apc-test-response
           (concat "<a href=\"http://theangrypolicecaptain.com\">"
                   "Found me after all.</a>\n"))
        (apc-test-response
         "" "301 Moved Permanently"
         "Location: http://theangrypolicecaptain.com/moved\r\n"))))
   :requests (apc-test-request)))
"##,
        expect![[
            r#"OK (:not-found (:echoed ("Gone, but still quotable") :requests-served 1 :leftover-buffers nil) :redirected (:echoed ("Found me after all") :requests-served 2 :leftover-buffers (" *http theangrypolicecaptain.com:80*")) :requests ("GET http://theangrypolicecaptain.com HTTP/1.1\15\nMIME-Version: 1.0\15\nConnection: close\15\nHost: theangrypolicecaptain.com\15\nAccept-encoding: gzip\15\nAccept: */*\15\nUser-Agent: URL/Emacs Emacs/<VERSION> (TTY; x86_64-pc-linux-gnu)\15\n\15\n" "GET http://theangrypolicecaptain.com/moved HTTP/1.1\15\nMIME-Version: 1.0\15\nConnection: close\15\nHost: theangrypolicecaptain.com\15\nAccept-encoding: gzip\15\nAccept: */*\15\nUser-Agent: URL/Emacs Emacs/<VERSION> (TTY; x86_64-pc-linux-gnu)\15\n\15\n"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        fetching_a_quote_sends_a_real_request_and_echoes_the_line_it_scrapes(),
        the_scraper_takes_the_first_marked_link_and_drops_its_last_character(),
        the_command_can_only_finish_when_it_is_invoked_the_way_the_menu_invokes_it(),
        the_response_status_is_ignored_and_a_redirect_leaves_its_first_buffer_behind(),
    ]
}
