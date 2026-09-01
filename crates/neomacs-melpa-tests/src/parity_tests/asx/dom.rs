use expect_test::expect;

use super::ParityBatchCase;

fn asx_normalizes_a_realistic_question_page_into_the_complete_post_model() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_normalizes_a_realistic_question_page_into_the_complete_post_model",
        r##"(let ((asx--posts
                '(("First result"
                   .
                   "https://stackoverflow.com/questions/101/first")
                  ("Second result"
                   .
                   "https://emacs.stackexchange.com/questions/202/second")))
               (asx--current-post-index 0))
         (asx-test-post-summary
          (asx--normalize-post
           (asx-test-post-dom))))"##,
        expect![[
            r#"OK (:url "https://stackoverflow.com/questions/101/first" :title "How to  ?" :body ((div ((class . "post-text")) (p nil "Question " (strong nil "body") ".") (pre ((class . "lang-emacs-lisp")) "(+ 1 2)"))) :score "12" :answers ((:body ((div ((class . "post-text")) (p nil "First answer."))) :score "7") (:body ((div ((class . "post-text")) (p nil "Second " (a ((href . "https://example.com")) "answer") "."))) :score "-1")) :tags ("emacs" "elisp"))"#
        ]],
    )
}

fn asx_normalization_uses_the_selected_post_url_and_preserves_empty_collections() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asx_normalization_uses_the_selected_post_url_and_preserves_empty_collections",
        r##"(let ((asx--posts
                '(("First" . "https://example.invalid/questions/1")
                  ("Selected" . "https://example.invalid/questions/2")))
               (asx--current-post-index 1)
               (dom
                '(html nil
                  (body nil
                   (a
                    ((class . "question-hyperlink"))
                    "Question without answers")
                   (div
                    ((id . "question"))
                    (div
                     ((class . "post-text"))
                     (p nil "Only a body."))
                    (span
                     ((class . "js-vote-count"))
                     "0"))
                   (div
                    ((class . "post-taglist")))))))
         (asx-test-post-summary
          (asx--normalize-post dom)))"##,
        expect![[
            r#"OK (:url "https://example.invalid/questions/2" :title "Question without answers" :body ((div ((class . "post-text")) (p nil "Only a body."))) :score "0" :answers nil :tags nil)"#
        ]],
    )
}

fn asx_extracts_all_tags_in_dom_order_with_nested_text_and_duplicates_intact() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_extracts_all_tags_in_dom_order_with_nested_text_and_duplicates_intact",
        r##"(asx--get-tags
         '(div nil
           (div
            ((class . "post-taglist"))
            (a
             ((class . "post-tag"))
             "emacs")
            (a
             ((class . "post-tag featured"))
             "not-an-exact-tag")
            (a
             ((class . "post-tag"))
             "common-"
             (strong nil "lisp"))
            (a
             ((class . "post-tag"))
             "emacs"))
           (div
            ((class . "post-taglist"))
            (a
             ((class . "post-tag"))
             "org-mode"))))"##,
        expect![[r#"OK ("emacs" "common-" "emacs")"#]],
    )
}

fn asx_extracts_answer_bodies_and_scores_from_each_answercell_parent() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_extracts_answer_bodies_and_scores_from_each_answercell_parent",
        r##"(let ((answers
                (asx--get-answers
                 (asx-test-post-dom))))
         (list
          (length answers)
          (mapcar
           (lambda (answer)
             (list
              :score
              (plist-get answer :score)
              :body-text
              (mapcar
               #'dom-texts
               (plist-get answer :body))
              :links
              (mapcar
               (lambda (node)
                 (list
                  (dom-attr node 'href)
                  (dom-texts node)))
               (dom-by-tag
                (plist-get answer :body)
                'a))))
           answers)))"##,
        expect![[
            r#"OK (2 ((:score "7" :body-text ("First answer.") :links nil) (:score "-1" :body-text ("Second  answer .") :links (("https://example.com" "answer")))))"#
        ]],
    )
}

fn asx_language_detection_handles_stackexchange_classes_and_non_language_classes() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asx_language_detection_handles_stackexchange_classes_and_non_language_classes",
        r##"(mapcar
         (lambda (class)
           (list
            class
            (condition-case error
                (asx--get-language-string class)
              (error
               (list
                (car error)
                (cdr error))))
            (asx--get-language-maybe
             (if class
                 (list
                  'pre
                  (list
                   (cons
                    'class
                    class))
                  "body")
               '(pre nil "body")))))
         '("lang-emacs-lisp"
           "prettyprint lang-python linenums"
           "language-rust"
           "lang-c++ extra"
           "lang-"
           ""
           nil))"##,
        expect![[
            r#"OK (("lang-emacs-lisp" "emacs" "emacs") ("prettyprint lang-python linenums" "python" "python") ("language-rust" nil nil) ("lang-c++ extra" "c" "c") ("lang-" nil nil) ("" nil nil) (nil (wrong-type-argument (stringp nil)) nil))"#
        ]],
    )
}

fn asx_maps_text_links_to_org_links_but_keeps_image_links_as_dom() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_maps_text_links_to_org_links_but_keeps_image_links_as_dom",
        r##"(list
         (asx--map-node
          '(a
            ((href . "https://example.com/a?x=1&y=2"))
            "Read "
            (strong nil "the answer")))
         (asx--map-node
          '(a
            ((href . "https://example.com/full"))
            (img
             ((src . "https://example.com/image.png")
              (alt . "diagram")))))
         (asx--map-node "literal text")
         (asx--map-node 17)
         (asx--map-node nil))"##,
        expect![[
            r#"OK ("[[https://example.com/a?x=1&y=2][Read  the answer]]" (a ((href . "https://example.com/full")) (img ((src . "https://example.com/image.png") (alt . "diagram")))) "literal text" 17 nil)"#
        ]],
    )
}

fn asx_maps_pre_blocks_to_org_example_blocks_with_detected_or_fallback_language() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asx_maps_pre_blocks_to_org_example_blocks_with_detected_or_fallback_language",
        r##"(mapcar
         #'asx--map-node
         '((pre
            ((class . "lang-rust"))
            "fn main() {\n    println!(\"hi\");\n}")
           (pre nil "(message \"plain\")")
           (pre
            ((class . "prettyprint"))
            "unclassified")
           (pre
            ((class . "lang-python extra"))
            (code nil "print('nested')"))))"##,
        expect![[
            r##"OK ((pre nil "#+BEGIN_EXAMPLE " "rust" "\n" ("fn main() {\n    println!(\"hi\");\n}") "\n" "#+END_EXAMPLE") (pre nil "#+BEGIN_EXAMPLE " "prog" "\n" ("(message \"plain\")") nil "#+END_EXAMPLE") (pre nil "#+BEGIN_EXAMPLE " "prog" "\n" ("unclassified") "\n" "#+END_EXAMPLE") (pre nil "#+BEGIN_EXAMPLE " "python" "\n" ((code nil "print('nested')")) "\n" "#+END_EXAMPLE"))"##
        ]],
    )
}

fn asx_recursively_maps_a_practical_mixed_post_body_without_losing_structure() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_recursively_maps_a_practical_mixed_post_body_without_losing_structure",
        r##"(asx--map-node
         '(div
           ((class . "post-text"))
           (p nil
              "See "
              (a
               ((href . "https://www.gnu.org/software/emacs/"))
               "GNU Emacs")
              " and compare:")
           (ul nil
               (li nil "first")
               (li nil
                   "second "
                   (code nil "(+ 1 2)")))
           (blockquote nil
                       (p nil "quoted advice"))
           (pre
            ((class . "lang-emacs-lisp"))
            "(mapcar #'1+ '(1 2 3))")))"##,
        expect![[
            r##"OK (div ((class . "post-text")) (p nil "See " "[[https://www.gnu.org/software/emacs/][GNU Emacs]]" " and compare:") (ul nil (li nil "first") (li nil "second " (code nil "(+ 1 2)"))) (blockquote nil (p nil "quoted advice")) (pre nil "#+BEGIN_EXAMPLE " "emacs" "\n" ("(mapcar #'1+ '(1 2 3))") "\n" "#+END_EXAMPLE"))"##
        ]],
    )
}

pub(super) fn dom_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asx_normalizes_a_realistic_question_page_into_the_complete_post_model(),
        asx_normalization_uses_the_selected_post_url_and_preserves_empty_collections(),
        asx_extracts_all_tags_in_dom_order_with_nested_text_and_duplicates_intact(),
        asx_extracts_answer_bodies_and_scores_from_each_answercell_parent(),
        asx_language_detection_handles_stackexchange_classes_and_non_language_classes(),
        asx_maps_text_links_to_org_links_but_keeps_image_links_as_dom(),
        asx_maps_pre_blocks_to_org_example_blocks_with_detected_or_fallback_language(),
        asx_recursively_maps_a_practical_mixed_post_body_without_losing_structure(),
    ]
}
