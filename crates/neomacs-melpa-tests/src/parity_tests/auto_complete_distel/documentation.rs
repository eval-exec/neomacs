use expect_test::expect;

use super::ParityBatchCase;

fn distel_completion_html_normalization_handles_paragraphs_breaks_entities_and_tags()
-> ParityBatchCase {
    ParityBatchCase::value(
        "distel_completion_html_normalization_handles_paragraphs_breaks_entities_and_tags",
        r##"(mapcar
          #'distel-completion-html-to-string
          '("<p>map(Fun, List) -&gt; List</p>"
            "<div><b>Types</b><br>Fun = fun()</div>"
            "   <p>alpha</p>\n\n<p>beta &lt; gamma</p>   "
            "<code>lists:map/2</code>"
            ""))"##,
        expect![[
            r#"OK ("\nmap(Fun, List) -> List" "Types\nFun = fun()" "\nalpha\nbeta < gamma\n" "lists:map/2" "")"#
        ]],
    )
}

fn distel_completion_document_prefers_nonempty_local_distel_docs_but_still_collects_metadata()
-> ParityBatchCase {
    ParityBatchCase::value(
        "distel_completion_document_prefers_nonempty_local_distel_docs_but_still_collects_metadata",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'distel-completion-local-docs)
                (lambda (module function)
                  (push
                   (list :local module function)
                   calls)
                  "lists:map/2\nMap Fun over List."))
               ((symbol-function
                 'distel-completion-get-docs-from-internet-p)
                (lambda (module function)
                  (push
                   (list :internet module function)
                   calls)
                  "unexpected internet"))
               ((symbol-function
                 'distel-completion-get-metadoc)
                (lambda (module function)
                  (push
                   (list :metadata module function)
                   calls)
                  '((fun list))))
               ((symbol-function
                 'erl-format-arglists)
                (lambda (arglists)
                  (format "<%S>" arglists))))
            (list
             (distel-completion-get-doc-string
              "lists:map")
             (nreverse calls))))"##,
        expect![[
            r#"OK ("lists:map/2\nMap Fun over List." ((:local "lists" "map") (:metadata "lists" "map")))"#
        ]],
    )
}

fn distel_completion_document_uses_internet_docs_when_local_distel_docs_are_empty()
-> ParityBatchCase {
    ParityBatchCase::value(
        "distel_completion_document_uses_internet_docs_when_local_distel_docs_are_empty",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'distel-completion-local-docs)
                (lambda (module function)
                  (push
                   (list :local module function)
                   calls)
                  ""))
               ((symbol-function
                 'distel-completion-get-docs-from-internet-p)
                (lambda (module function)
                  (push
                   (list :internet module function)
                   calls)
                  "Erlang online documentation"))
               ((symbol-function
                 'distel-completion-get-metadoc)
                (lambda (module function)
                  (push
                   (list :metadata module function)
                   calls)
                  '((fun list))))
               ((symbol-function
                 'erl-format-arglists)
                (lambda (_arglists)
                  "(Fun, List)")))
            (list
             (distel-completion-get-doc-string
              "lists:map")
             (nreverse calls))))"##,
        expect![[
            r#"OK ("Erlang online documentation" ((:local "lists" "map") (:internet "lists" "map") (:metadata "lists" "map")))"#
        ]],
    )
}

fn distel_completion_document_falls_back_to_formatted_metadata_then_explicit_no_help()
-> ParityBatchCase {
    ParityBatchCase::value(
        "distel_completion_document_falls_back_to_formatted_metadata_then_explicit_no_help",
        r##"(cl-letf
          (((symbol-function
             'distel-completion-local-docs)
            (lambda (_module function)
              (if
                  (equal function "missing")
                  nil
                "")))
           ((symbol-function
             'distel-completion-get-docs-from-internet-p)
            (lambda (_module _function)
              ""))
           ((symbol-function
             'distel-completion-get-metadoc)
            (lambda (_module function)
              (and
               (equal function "map")
               '((fun list)))))
           ((symbol-function
             'erl-format-arglists)
            (lambda (arglists)
              (and arglists
                   "(Fun, List)"))))
          (mapcar
           #'distel-completion-get-doc-string
           '("lists:map"
             "lists:missing")))"##,
        expect![[r#"OK ("lists:map(Fun, List)" "Couldn't find any help for lists:missing.")"#]],
    )
}

fn distel_completion_document_without_function_obeys_internet_option_and_exact_fallback_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "distel_completion_document_without_function_obeys_internet_option_and_exact_fallback_text",
        r##"(mapcar
          (lambda (internet)
            (let ((distel-completion-get-doc-from-internet
                   internet)
                  calls)
              (cl-letf
                  (((symbol-function
                     'distel-completion-get-docs-from-internet-p)
                    (lambda (module function)
                      (push
                       (list module function)
                       calls)
                      (and internet
                           "module documentation")))
                   ((symbol-function
                     'erl-format-arglists)
                    (lambda (_arglists)
                      :unexpected)))
                (list
                 internet
                 (distel-completion-get-doc-string
                  "lists")
                 (nreverse calls)))))
          '(nil t))"##,
        expect![[
            r#"OK ((nil "Couldn't find any help for lists." nil) (t "module documentation" (("lists" nil))))"#
        ]],
    )
}

fn distel_completion_internet_parser_requests_module_page_and_extracts_function_body()
-> ParityBatchCase {
    ParityBatchCase::value(
        "distel_completion_internet_parser_requests_module_page_and_extracts_function_body",
        r##"(let ((buffer
                               (generate-new-buffer
                                " *distel-http-response*"))
                              requested)
          (unwind-protect
              (cl-letf
                  (((symbol-function
                     'url-retrieve-synchronously)
                    (lambda (url)
                      (setq requested url)
                      (with-current-buffer buffer
                        (erase-buffer)
                        (insert
                         "HTTP/1.1 200 OK\n\n"
                         "<p><a name=\"map-2\">map(Fun, List)</a>"
                         "<div class=\"REFBODY\">"
                         "<p>Maps &lt;Fun&gt; over List.<br>"
                         "Returns a new list.</p></div>")
                        (current-buffer)))))
                (list
                 (distel-completion-get-docs-from-internet-p
                  "lists"
                  "map")
                 requested))
            (kill-buffer buffer)))"##,
        expect![[
            r#"OK ("map(Fun, List)\nMaps <Fun> over List.\nReturns a new list." "http://www.erlang.org/doc/man/lists.html")"#
        ]],
    )
}

pub(super) fn documentation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        distel_completion_html_normalization_handles_paragraphs_breaks_entities_and_tags(),
        distel_completion_document_prefers_nonempty_local_distel_docs_but_still_collects_metadata(),
        distel_completion_document_uses_internet_docs_when_local_distel_docs_are_empty(),
        distel_completion_document_falls_back_to_formatted_metadata_then_explicit_no_help(),
        distel_completion_document_without_function_obeys_internet_option_and_exact_fallback_text(),
        distel_completion_internet_parser_requests_module_page_and_extracts_function_body(),
    ]
}
