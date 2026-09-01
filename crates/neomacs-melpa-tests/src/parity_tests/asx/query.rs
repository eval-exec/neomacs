use expect_test::expect;

use super::ParityBatchCase;

fn asx_query_string_sites_handles_default_custom_single_and_empty_site_sets() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_query_string_sites_handles_default_custom_single_and_empty_site_sets",
        r##"(mapcar
         (lambda (sites)
           (let ((asx-sites sites))
             (list
              sites
              (asx--query-string-sites)
              (asx--query-string
               "mapcar examples"))))
         (list
          asx-sites
          '("emacs.stackexchange.com")
          '("stackoverflow.com"
            "unix.stackexchange.com")
          nil))"##,
        expect![[
            r#"OK ((("stackoverflow.com" "stackexchange.com" "superuser.com" "serverfault.com" "askubuntu.com") "site:stackoverflow.com OR site:stackexchange.com OR site:superuser.com OR site:serverfault.com OR site:askubuntu.com" "mapcar examples site:stackoverflow.com OR site:stackexchange.com OR site:superuser.com OR site:serverfault.com OR site:askubuntu.com") (("emacs.stackexchange.com") "site:emacs.stackexchange.com" "mapcar examples site:emacs.stackexchange.com") (("stackoverflow.com" "unix.stackexchange.com") "site:stackoverflow.com OR site:unix.stackexchange.com" "mapcar examples site:stackoverflow.com OR site:unix.stackexchange.com") (nil "" "mapcar examples "))"#
        ]],
    )
}

fn asx_search_engine_lookup_and_query_construction_support_builtin_and_custom_engines()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_search_engine_lookup_and_query_construction_support_builtin_and_custom_engines",
        r##"(let ((asx-sites
                '("emacs.stackexchange.com"))
               (asx-search-engine-alist
                (append
                 asx-search-engine-alist
                 '((fixture
                    :format
                    "https://search.invalid/?term=%s"
                    :extract-fn
                    identity)))))
         (mapcar
          (lambda (engine)
            (let ((asx-search-engine engine))
              (list
               engine
               (asx--get-search-engine)
               (asx--query-construct
                "C++ & Elisp"))))
          '(google
            duckduckgo
            fixture)))"##,
        expect![[
            r#"OK ((google (:format "https://www.google.com/search?q=%s" :extract-fn #'asx--extract-links-google) "https://www.google.com/search?q=C%2B%2B%20%26%20Elisp%20site%3Aemacs.stackexchange.com") (duckduckgo (:format "https://www.duckduckgo.com/?q=%s" :extract-fn #'asx--extract-links-duckduckgo) "https://www.duckduckgo.com/?q=C%2B%2B%20%26%20Elisp%20site%3Aemacs.stackexchange.com") (fixture (:format "https://search.invalid/?term=%s" :extract-fn identity) "https://search.invalid/?term=C%2B%2B%20%26%20Elisp%20site%3Aemacs.stackexchange.com"))"#
        ]],
    )
}

fn asx_query_construction_percent_encodes_unicode_punctuation_and_site_expression()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_query_construction_percent_encodes_unicode_punctuation_and_site_expression",
        r##"(let ((asx-sites
                '("stackoverflow.com"
                  "emacs.stackexchange.com"))
               (asx-search-engine
                'google))
         (mapcar
          (lambda (query)
            (list
             query
             (asx--query-string query)
             (asx--query-construct query)))
          '("mapcar & seq-filter"
            "naïve café"
            "C# / F#"
            "quotes \"and spaces\"")))"##,
        expect![[
            r#"OK (("mapcar & seq-filter" "mapcar & seq-filter site:stackoverflow.com OR site:emacs.stackexchange.com" "https://www.google.com/search?q=mapcar%20%26%20seq-filter%20site%3Astackoverflow.com%20OR%20site%3Aemacs.stackexchange.com") ("naïve café" "naïve café site:stackoverflow.com OR site:emacs.stackexchange.com" "https://www.google.com/search?q=na%C3%AFve%20caf%C3%A9%20site%3Astackoverflow.com%20OR%20site%3Aemacs.stackexchange.com") ("C# / F#" "C# / F# site:stackoverflow.com OR site:emacs.stackexchange.com" "https://www.google.com/search?q=C%23%20%2F%20F%23%20site%3Astackoverflow.com%20OR%20site%3Aemacs.stackexchange.com") ("quotes \"and spaces\"" "quotes \"and spaces\" site:stackoverflow.com OR site:emacs.stackexchange.com" "https://www.google.com/search?q=quotes%20%22and%20spaces%22%20site%3Astackoverflow.com%20OR%20site%3Aemacs.stackexchange.com"))"#
        ]],
    )
}

fn asx_primary_command_updates_history_constructs_url_and_dispatches_search_callback()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_primary_command_updates_history_constructs_url_and_dispatches_search_callback",
        r##"(let ((asx--query-history
                '("older"))
               (asx-sites
                '("emacs.stackexchange.com"))
               requests
               messages)
         (cl-letf
             (((symbol-function 'asx--request)
               (lambda
                 (url callback
                      &optional error-callback)
                 (push
                  (list
                   url
                   callback
                   error-callback)
                  requests)
                 :queued))
              ((symbol-function 'message)
               (lambda
                 (format-string &rest arguments)
                 (push
                  (apply
                   #'format
                   format-string
                   arguments)
                  messages))))
           (list
            (asx
             "How to map a list?")
            asx--query-history
            (nreverse messages)
            (nreverse requests))))"##,
        expect![[
            r#"OK (:queued ("How to map a list?" "older") ("Loading: How to map a list?") (("https://www.google.com/search?q=How%20to%20map%20a%20list%3F%20site%3Aemacs.stackexchange.com" asx--handle-search nil)))"#
        ]],
    )
}

fn asx_primary_command_rejects_empty_query_without_mutating_history_or_dispatching()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_primary_command_rejects_empty_query_without_mutating_history_or_dispatching",
        r##"(let ((asx--query-history
                '("kept"))
               requests)
         (cl-letf
             (((symbol-function 'asx--request)
               (lambda
                 (&rest arguments)
                 (push arguments requests))))
           (list
            (condition-case error
                (asx "")
              (error
               (list
                (car error)
                (cdr error))))
            asx--query-history
            requests)))"##,
        expect![[r#"OK ((user-error ("No query specified")) ("kept") nil)"#]],
    )
}

fn asx_symbol_or_region_prefers_active_region_then_uses_xref_identifier() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_symbol_or_region_prefers_active_region_then_uses_xref_identifier",
        r##"(list
         (with-temp-buffer
           (insert
            "alpha beta gamma")
           (goto-char 7)
           (set-mark 11)
           (setq
            mark-active t
            transient-mark-mode t)
           (asx--symbol-or-region))
         (with-temp-buffer
           (insert
            "alpha beta gamma")
           (goto-char 9)
           (cl-letf
               (((symbol-function
                  'xref-find-backend)
                 (lambda ()
                   'fixture-backend))
                ((symbol-function
                  'xref-backend-identifier-at-point)
                 (lambda (backend)
                   (list
                    backend
                    (thing-at-point
                     'symbol
                     t)))))
             (asx--symbol-or-region))))"##,
        expect![[r#"OK ("beta" "beta")"#]],
    )
}

fn asx_initial_input_only_reads_symbol_or_region_when_prefix_argument_is_active() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asx_initial_input_only_reads_symbol_or_region_when_prefix_argument_is_active",
        r##"(let (calls)
         (cl-letf
             (((symbol-function
                'asx--symbol-or-region)
               (lambda ()
                 (push 'called calls)
                 "fixture")))
           (list
            (let ((current-prefix-arg nil))
              (asx--initial-input))
            (let ((current-prefix-arg '(4)))
              (asx--initial-input))
            (let ((current-prefix-arg 0))
              (asx--initial-input))
            (nreverse calls))))"##,
        expect![[r#"OK (nil "fixture" "fixture" (called called))"#]],
    )
}

fn asx_read_query_selects_ivy_helm_or_plain_read_string_in_priority_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_read_query_selects_ivy_helm_or_plain_read_string_in_priority_order",
        r##"(let (events)
         (list
          (cl-letf
              (((symbol-function 'require)
                (lambda
                  (feature &rest _)
                  (eq feature 'ivy)))
               ((symbol-function 'counsel-google)
                (lambda ()))
               ((symbol-function 'asx--ivy-search)
                (lambda ()
                  (push 'ivy events)
                  "ivy query")))
            (asx--read-query))
          (cl-letf
              (((symbol-function 'require)
                (lambda
                  (feature &rest _)
                  (eq feature 'helm-net)))
               ((symbol-function
                 'helm-google-suggest)
                (lambda ()))
               ((symbol-function 'asx--helm-search)
                (lambda ()
                  (push 'helm events)
                  "helm query")))
            (asx--read-query))
          (cl-letf
              (((symbol-function 'require)
                (lambda
                  (&rest _)
                  nil))
               ((symbol-function 'read-string)
                (lambda
                  (prompt initial history)
                  (push
                   (list
                    'plain
                    prompt
                    initial
                    history)
                   events)
                  "plain query"))
               ((symbol-function 'asx--initial-input)
                (lambda ()
                  "seed")))
            (asx--read-query))
          (nreverse events)))"##,
        expect![[
            r#"OK ("ivy query" "helm query" "plain query" (ivy helm (plain "Query: " "seed" asx--query-history)))"#
        ]],
    )
}

fn asx_ivy_search_passes_dynamic_collection_history_initial_input_and_caller() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_ivy_search_passes_dynamic_collection_history_initial_input_and_caller",
        r##"(let ((asx--query-history
                '("old query")))
         (cl-letf
             (((symbol-function 'ivy-read)
               (lambda
                 (prompt collection
                         &rest properties)
                 (list
                  prompt
                  collection
                  properties)))
              ((symbol-function 'asx--initial-input)
               (lambda ()
                 "region seed")))
           (asx--ivy-search)))"##,
        expect![[
            r#"OK ("Query: " counsel-search-function (:dynamic-collection t :history asx--query-history :initial-input "region seed" :caller counsel-search))"#
        ]],
    )
}

fn asx_helm_search_builds_volatile_three_character_google_source_and_target_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_helm_search_builds_volatile_three_character_google_source_and_target_buffer",
        r##"(let ((asx--query-history
                '("old query"))
               (helm-google-suggest-default-function
                (lambda ()
                  '("candidate one"
                    "candidate two")))
               events)
         (cl-letf
             (((symbol-function
                'helm-build-sync-source)
               (lambda
                 (name &rest properties)
                 (let ((candidates
                        (plist-get
                         properties
                         :candidates)))
                   (list
                    name
                    (funcall candidates)
                    (plist-get properties :history)
                    (plist-get properties :volatile)
                    (plist-get properties :requires-pattern)))))
              ((symbol-function 'helm-other-buffer)
               (lambda (source buffer)
                 (push
                  (list source buffer)
                  events)
                 :shown)))
           (list
            (asx--helm-search)
            (nreverse events))))"##,
        expect![[
            r#"OK (:shown ((("Query" ("candidate one" "candidate two") asx--query-history t 3) "*Helm Google*")))"#
        ]],
    )
}

pub(super) fn query_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asx_query_string_sites_handles_default_custom_single_and_empty_site_sets(),
        asx_search_engine_lookup_and_query_construction_support_builtin_and_custom_engines(),
        asx_query_construction_percent_encodes_unicode_punctuation_and_site_expression(),
        asx_primary_command_updates_history_constructs_url_and_dispatches_search_callback(),
        asx_primary_command_rejects_empty_query_without_mutating_history_or_dispatching(),
        asx_symbol_or_region_prefers_active_region_then_uses_xref_identifier(),
        asx_initial_input_only_reads_symbol_or_region_when_prefix_argument_is_active(),
        asx_read_query_selects_ivy_helm_or_plain_read_string_in_priority_order(),
        asx_ivy_search_passes_dynamic_collection_history_initial_input_and_caller(),
        asx_helm_search_builds_volatile_three_character_google_source_and_target_buffer(),
    ]
}
