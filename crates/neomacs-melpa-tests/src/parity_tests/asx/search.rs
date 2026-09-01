use expect_test::expect;

use super::ParityBatchCase;

fn asx_google_extractor_reads_result_titles_and_question_urls_in_dom_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_google_extractor_reads_result_titles_and_question_urls_in_dom_order",
        r##"(asx--extract-links-google
         (asx-test-search-dom))"##,
        expect![[
            r#"OK (("First " . "https://stackoverflow.com/questions/101/first") ("Second result" . "https://emacs.stackexchange.com/questions/202/second"))"#
        ]],
    )
}

fn asx_duckduckgo_extractor_combines_nested_title_text_and_trims_display_urls() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_duckduckgo_extractor_combines_nested_title_text_and_trims_display_urls",
        r##"(asx--extract-links-duckduckgo
         (asx-test-search-dom))"##,
        expect![[
            r#"OK (("Duck  one" . "stackoverflow.com/questions/303/duck-one") ("Duck two" . "example.com/articles/404"))"#
        ]],
    )
}

fn asx_extract_links_dispatches_through_builtin_and_custom_engine_configuration() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asx_extract_links_dispatches_through_builtin_and_custom_engine_configuration",
        r##"(let ((dom
                (asx-test-search-dom))
               calls)
         (cl-letf
             (((symbol-function
                'asx-test-custom-extractor)
               (lambda (value)
                 (push
                  (car value)
                  calls)
                 '(("Custom"
                    .
                    "https://custom.invalid/questions/9")))))
           (list
            (let ((asx-search-engine 'google))
              (asx--extract-links dom))
            (let ((asx-search-engine
                   'duckduckgo))
              (asx--extract-links dom))
            (let ((asx-search-engine 'custom)
                  (asx-search-engine-alist
                   '((custom
                      :format "%s"
                      :extract-fn
                      #'asx-test-custom-extractor))))
              (asx--extract-links dom))
            calls)))"##,
        expect![[
            r#"OK ((("First " . "https://stackoverflow.com/questions/101/first") ("Second result" . "https://emacs.stackexchange.com/questions/202/second")) (("Duck  one" . "stackoverflow.com/questions/303/duck-one") ("Duck two" . "example.com/articles/404")) (("Custom" . "https://custom.invalid/questions/9")) (html))"#
        ]],
    )
}

fn asx_filter_posts_keeps_only_question_paths_without_reordering_or_rewriting_links()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_filter_posts_keeps_only_question_paths_without_reordering_or_rewriting_links",
        r##"(asx--filter-posts
         '(("Question"
            .
            "https://stackoverflow.com/questions/123/title")
           ("Bare question path"
            .
            "questions/9")
           ("Answer"
            .
            "https://stackoverflow.com/a/123")
           ("Question text only"
            .
            "https://example.com/search?q=questions")
           ("Leading zeros"
            .
            "https://serverfault.com/questions/0007/x")
           ("No digits"
            .
            "https://example.com/questions/abc")))"##,
        expect![[
            r#"OK (("Question" . "https://stackoverflow.com/questions/123/title") ("Bare question path" . "questions/9") ("Leading zeros" . "https://serverfault.com/questions/0007/x"))"#
        ]],
    )
}

fn asx_post_prefixes_mark_every_title_equal_to_current_post_and_preserve_urls() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_post_prefixes_mark_every_title_equal_to_current_post_and_preserve_urls",
        r##"(let ((asx--posts
                '(("First" . "url-1")
                  ("Duplicate" . "url-2")
                  ("Duplicate" . "url-3")
                  ("Last" . "url-4")))
               (asx--current-post-index 1))
         (list
          (asx--get-current-post)
          (mapcar
           (lambda (post)
             (list
              post
              (asx--get-prefix post)))
           asx--posts)
          (asx--get-posts-with-prefix
           asx--posts)))"##,
        expect![[
            r#"OK (#1=("Duplicate" . "url-2") ((("First" . "url-1") "   ") (#1# "=> ") (("Duplicate" . "url-3") "=> ") (("Last" . "url-4") "   ")) (("   First" . "url-1") ("=> Duplicate" . "url-2") ("=> Duplicate" . "url-3") ("   Last" . "url-4")))"#
        ]],
    )
}

fn asx_select_post_prompts_with_prefixed_candidates_and_stores_selected_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_select_post_prompts_with_prefixed_candidates_and_stores_selected_index",
        r##"(let ((asx--posts
                '(("First" . "url-1")
                  ("Second" . "url-2")
                  ("Third" . "url-3")))
               (asx--current-post-index 0)
               observed)
         (cl-letf
             (((symbol-function
                'completing-read)
               (lambda
                 (prompt collection &rest arguments)
                 (setq observed
                       (list
                        prompt
                        collection
                        arguments))
                 (car
                  (nth 2 collection)))))
           (list
            (asx--select-post asx--posts)
            asx--current-post-index
            (asx--get-current-post)
            observed)))"##,
        expect![[
            r#"OK (2 2 ("Third" . "url-3") ("Post: " (("=> First" . "url-1") ("   Second" . "url-2") ("   Third" . "url-3")) nil))"#
        ]],
    )
}

fn asx_handle_search_filters_results_selects_first_and_requests_post_without_prompt()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_handle_search_filters_results_selects_first_and_requests_post_without_prompt",
        r##"(let ((asx-prompt-post-p nil)
               (asx--current-post-index 9)
               requests)
         (cl-letf
             (((symbol-function
                'asx--extract-links)
               (lambda (_)
                 '(("Article" . "https://example.com/article")
                   ("First" . "https://stackoverflow.com/questions/1/first")
                   ("Second" . "https://emacs.stackexchange.com/questions/2/second"))))
              ((symbol-function
                'asx--request-post)
               (lambda (post)
                 (push post requests)
                 :queued)))
           (list
            (asx--handle-search
             '(fixture-dom))
            asx--posts
            asx--current-post-index
            (nreverse requests))))"##,
        expect![[
            r#"OK (:queued (#1=("First" . "https://stackoverflow.com/questions/1/first") ("Second" . "https://emacs.stackexchange.com/questions/2/second")) 0 (#1#))"#
        ]],
    )
}

fn asx_handle_search_prompt_path_uses_selector_before_requesting_chosen_post() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_handle_search_prompt_path_uses_selector_before_requesting_chosen_post",
        r##"(let ((asx-prompt-post-p t)
               events)
         (cl-letf
             (((symbol-function
                'asx--extract-links)
               (lambda (_)
                 '(("First" . "questions/1")
                   ("Second" . "questions/2"))))
              ((symbol-function
                'asx--select-post)
               (lambda (posts)
                 (push
                  (list :select posts)
                  events)
                 (setq
                  asx--current-post-index
                  1)))
              ((symbol-function
                'asx--request-post)
               (lambda (post)
                 (push
                  (list :request post)
                  events)
                 :queued)))
           (list
            (asx--handle-search
             '(fixture-dom))
            asx--posts
            asx--current-post-index
            (nreverse events))))"##,
        expect![[
            r#"OK (:queued #1=(("First" . "questions/1") #2=("Second" . "questions/2")) 1 ((:select #1#) (:request #2#)))"#
        ]],
    )
}

fn asx_handle_search_signals_when_extraction_contains_no_question_posts() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_handle_search_signals_when_extraction_contains_no_question_posts",
        r##"(let (requests)
         (cl-letf
             (((symbol-function
                'asx--extract-links)
               (lambda (_)
                 '(("Article" . "https://example.com/article"))))
              ((symbol-function
                'asx--request-post)
               (lambda (post)
                 (push post requests))))
           (list
            (condition-case error
                (asx--handle-search
                 '(fixture-dom))
              (error
               (list
                (car error)
                (cdr error))))
            asx--posts
            requests)))"##,
        expect![[r#"OK ((user-error ("No posts found")) nil nil)"#]],
    )
}

fn asx_remove_and_next_deletes_all_matching_urls_then_advances_in_remaining_ring() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asx_remove_and_next_deletes_all_matching_urls_then_advances_in_remaining_ring",
        r##"(let ((asx--posts
                '(("First" . "url-1")
                  ("Bad A" . "bad-url")
                  ("Bad B" . "bad-url")
                  ("Last" . "url-4")))
               (asx--current-post-index 1)
               calls)
         (cl-letf
             (((symbol-function 'asx-n-post)
               (lambda (steps)
                 (push
                  (list
                   steps
                   asx--current-post-index
                   asx--posts)
                  calls)
                 :advanced)))
           (list
            (asx--remove-and-next
             "bad-url")
            asx--posts
            asx--current-post-index
            (nreverse calls))))"##,
        expect![[r#"OK (:advanced #1=(("First" . "url-1") ("Last" . "url-4")) 1 ((1 1 #1#)))"#]],
    )
}

fn asx_remove_and_next_signals_after_removing_the_last_available_post() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_remove_and_next_signals_after_removing_the_last_available_post",
        r##"(let ((asx--posts
                '(("Only" . "bad-url")))
               calls)
         (cl-letf
             (((symbol-function 'asx-n-post)
               (lambda (steps)
                 (push steps calls))))
           (list
            (condition-case error
                (asx--remove-and-next
                 "bad-url")
              (error
               (list
                (car error)
                (cdr error))))
            asx--posts
            calls)))"##,
        expect![[r#"OK ((user-error ("No posts found")) nil nil)"#]],
    )
}

fn asx_navigation_commands_forward_exact_offsets_and_first_post_delta() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_navigation_commands_forward_exact_offsets_and_first_post_delta",
        r##"(let ((asx--current-post-index 4)
               calls)
         (cl-letf
             (((symbol-function 'asx-n-post)
               (lambda (steps)
                 (push steps calls)
                 steps)))
           (list
            (asx-next-post)
            (asx-previous-post)
            (asx-reload-post)
            (asx-first-post)
            (nreverse calls))))"##,
        expect!["OK (1 -1 0 -4 (1 -1 0 -4))"],
    )
}

fn asx_n_post_wraps_forward_backward_and_large_offsets_then_requests_current_post()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_n_post_wraps_forward_backward_and_large_offsets_then_requests_current_post",
        r##"(let ((asx--posts
                '(("Zero" . "url-0")
                  ("One" . "url-1")
                  ("Two" . "url-2")))
               (asx--current-post-index 0)
               events)
         (cl-letf
             (((symbol-function
                'asx--request-post)
               (lambda (post)
                 (push
                  (list
                   asx--current-post-index
                   post)
                  events)
                 :requested)))
           (list
            (asx-n-post 1)
            (asx-n-post 1)
            (asx-n-post 1)
            (asx-n-post -1)
            (asx-n-post 8)
            asx--current-post-index
            (nreverse events))))"##,
        expect![[
            r#"OK (:requested :requested :requested :requested :requested 1 ((1 #2=("One" . "url-1")) (2 #1=("Two" . "url-2")) (0 ("Zero" . "url-0")) (2 #1#) (1 #2#)))"#
        ]],
    )
}

fn asx_jump_selects_from_current_posts_then_requests_the_selected_post() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_jump_selects_from_current_posts_then_requests_the_selected_post",
        r##"(let ((asx--posts
                '(("First" . "url-1")
                  ("Second" . "url-2")))
               (asx--current-post-index 0)
               events)
         (cl-letf
             (((symbol-function
                'asx--select-post)
               (lambda (posts)
                 (push
                  (list :select posts)
                  events)
                 (setq
                  asx--current-post-index
                  1)))
              ((symbol-function
                'asx--request-post)
               (lambda (post)
                 (push
                  (list :request post)
                  events)
                 :queued)))
           (list
            (asx-jump)
            asx--current-post-index
            (nreverse events))))"##,
        expect![[
            r#"OK (:queued 1 ((:select (("First" . "url-1") #1=("Second" . "url-2"))) (:request #1#)))"#
        ]],
    )
}

pub(super) fn search_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asx_google_extractor_reads_result_titles_and_question_urls_in_dom_order(),
        asx_duckduckgo_extractor_combines_nested_title_text_and_trims_display_urls(),
        asx_extract_links_dispatches_through_builtin_and_custom_engine_configuration(),
        asx_filter_posts_keeps_only_question_paths_without_reordering_or_rewriting_links(),
        asx_post_prefixes_mark_every_title_equal_to_current_post_and_preserve_urls(),
        asx_select_post_prompts_with_prefixed_candidates_and_stores_selected_index(),
        asx_handle_search_filters_results_selects_first_and_requests_post_without_prompt(),
        asx_handle_search_prompt_path_uses_selector_before_requesting_chosen_post(),
        asx_handle_search_signals_when_extraction_contains_no_question_posts(),
        asx_remove_and_next_deletes_all_matching_urls_then_advances_in_remaining_ring(),
        asx_remove_and_next_signals_after_removing_the_last_available_post(),
        asx_navigation_commands_forward_exact_offsets_and_first_post_delta(),
        asx_n_post_wraps_forward_backward_and_large_offsets_then_requests_current_post(),
        asx_jump_selects_from_current_posts_then_requests_the_selected_post(),
    ]
}
