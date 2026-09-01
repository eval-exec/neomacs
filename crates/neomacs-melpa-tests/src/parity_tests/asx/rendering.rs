use expect_test::expect;

use super::ParityBatchCase;

fn asx_get_buffer_reuses_the_configured_live_buffer_and_honors_name_changes() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_get_buffer_reuses_the_configured_live_buffer_and_honors_name_changes",
        r##"(let ((asx-buffer-name "*asx-parity-buffer-a*")
               first second third)
         (unwind-protect
             (progn
               (setq first
                     (asx--get-buffer)
                     second
                     (asx--get-buffer))
               (let ((asx-buffer-name
                      "*asx-parity-buffer-b*"))
                 (setq third
                       (asx--get-buffer)))
               (list
                (eq first second)
                (eq first third)
                (buffer-name first)
                (buffer-name third)
                (buffer-live-p first)
                (buffer-live-p third)))
           (mapc
            (lambda (buffer)
              (when
                  (buffer-live-p buffer)
                (kill-buffer buffer)))
            (list first third))))"##,
        expect![[r#"OK (t nil "*asx-parity-buffer-a*" "*asx-parity-buffer-b*" t t)"#]],
    )
}

fn asx_prepare_buffer_switches_when_needed_clears_old_content_and_makes_it_writable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_prepare_buffer_switches_when_needed_clears_old_content_and_makes_it_writable",
        r##"(let ((asx-buffer-name "*asx-parity-prepare*")
               target
               switches)
         (unwind-protect
             (progn
               (setq target
                     (get-buffer-create
                      asx-buffer-name))
               (with-current-buffer target
                 (insert "stale content")
                 (read-only-mode 1))
               (with-temp-buffer
                 (cl-letf
                     (((symbol-function
                        'switch-to-buffer-other-window)
                       (lambda (buffer)
                         (push
                          (buffer-name buffer)
                          switches)
                         (set-buffer buffer))))
                   (asx--prepare-buffer)
                   (list
                    (current-buffer)
                    (buffer-substring-no-properties
                     (point-min)
                     (point-max))
                    buffer-read-only
                    (point)
                    (nreverse switches)))))
           (when
               (buffer-live-p target)
             (kill-buffer target))))"##,
        expect![[
            r##"OK ((:buffer nil) "#+STARTUP: overview indent\n" nil 28 ("*asx-parity-prepare*"))"##
        ]],
    )
}

fn asx_finalize_buffer_trims_trailing_space_enables_read_only_org_display_and_rewinds()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_finalize_buffer_trims_trailing_space_enables_read_only_org_display_and_rewinds",
        r##"(with-temp-buffer
         (insert
          "#+TITLE: Example   \n\nBody with spaces   \n\n")
         (goto-char
          (point-max))
         (asx--finalize-buffer)
         (asx-test-rendered-buffer-summary))"##,
        expect![[r##"OK ("#+TITLE: Example\n\nBody with spaces\n" org-mode t t 1 t)"##]],
    )
}

fn asx_insert_tags_preserves_order_duplicates_and_the_empty_tags_label_contract() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asx_insert_tags_preserves_order_duplicates_and_the_empty_tags_label_contract",
        r##"(list
         (with-temp-buffer
           (asx--insert-tags
            '("emacs"
              "common-lisp"
              "emacs"))
           (buffer-string))
         (with-temp-buffer
           (asx--insert-tags nil)
           (buffer-string)))"##,
        expect![[r#"OK ("\nTags: emacs common-lisp emacs " "\nTags: ")"#]],
    )
}

fn asx_insert_question_renders_metadata_body_links_code_and_tags_together() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_insert_question_renders_metadata_body_links_code_and_tags_together",
        r##"(let ((question
                (list
                 :url
                 "https://stackoverflow.com/questions/101/first"
                 :title
                 "How to map data?"
                 :score
                 "12"
                 :body
                 '((p nil
                      "Use "
                      (a
                       ((href . "https://www.gnu.org/software/emacs/"))
                       "Emacs")
                      " carefully.")
                   (pre
                    ((class . "lang-emacs-lisp"))
                    "(mapcar #'1+ '(1 2))"))
                 :tags
                 '("emacs" "elisp"))))
         (with-temp-buffer
           (asx--insert-question question)
           (buffer-substring-no-properties
            (point-min)
            (point-max))))"##,
        expect![[
            r##"OK "#+TITLE: How to map data?\nhttps://stackoverflow.com/questions/101/first\n* Question (12)\n\nUse [[https://www.gnu.org/software/emacs/][Emacs]] carefully.\n\nTags: emacs elisp ""##
        ]],
    )
}

fn asx_insert_answers_adds_visibility_only_to_the_first_answer_and_keeps_scores_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_insert_answers_adds_visibility_only_to_the_first_answer_and_keeps_scores_order",
        r##"(with-temp-buffer
         (asx--insert-answers
          '((:score "10"
             :body
             ((p nil "First answer")
              (ul nil
                  (li nil "step one")
                  (li nil "step two"))))
            (:score "3"
             :body
             ((p nil
                 "Second "
                 (strong nil "answer"))))
            (:score "-2"
             :body
             ((p nil
                 "Third answer with "
                 (code nil "inline-code"))))))
         (list
          (buffer-substring-no-properties
           (point-min)
           (point-max))
          (how-many
           "^:VISIBILITY: all$"
           (point-min)
           (point-max))
          (how-many
           "^\\* Answer"
           (point-min)
           (point-max))))"##,
        expect![[
            r#"OK ("\n* Answer (10)\n:PROPERTIES:\n:VISIBILITY: all\n:END:\n\nFirst answer\n\n* Answer (3)\n\nSecond answer\n\n* Answer (-2)\n\nThird answer with inline-code\n" 1 3)"#
        ]],
    )
}

fn asx_insert_node_renders_a_realistic_nested_dom_as_plain_org_friendly_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_insert_node_renders_a_realistic_nested_dom_as_plain_org_friendly_text",
        r##"(with-temp-buffer
         (asx--insert-node
          '((p nil
               "Read "
               (a
                ((href . "https://example.com/docs"))
                "the docs")
               " before trying this.")
            (ol nil
                (li nil "Install the package")
                (li nil
                    "Evaluate "
                    (code nil "(asx \"mapcar\")")))
            (blockquote nil
                        (p nil "A practical warning."))
            (pre
             ((class . "lang-emacs-lisp"))
             "(message \"done\")")))
         (buffer-substring-no-properties
          (point-min)
          (point-max)))"##,
        expect![[r#"OK "Read [[https://example.com/docs][the docs]] before trying this.\n""#]],
    )
}

fn asx_insert_post_runs_the_full_buffer_lifecycle_and_limits_answer_count() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_insert_post_runs_the_full_buffer_lifecycle_and_limits_answer_count",
        r##"(let ((asx-buffer-name "*asx-parity-full-post*")
               (asx-number-of-answers 1)
               target)
         (unwind-protect
             (progn
               (setq target
                     (get-buffer-create
                      asx-buffer-name))
               (cl-letf
                   (((symbol-function
                      'switch-to-buffer-other-window)
                     (lambda (buffer)
                       (set-buffer buffer))))
                 (with-temp-buffer
                   (let ((post
                          '(:url
                            "https://stackoverflow.com/questions/101/first"
                            :title
                            "A complete practical post"
                            :score
                            "12"
                            :body
                            ((p nil
                                "Question body with "
                                (strong nil "emphasis")
                                "."))
                            :tags
                            ("emacs" "elisp")
                            :answers
                            ((:score
                              "7"
                              :body
                              ((p nil "First answer.")))
                             (:score
                              "4"
                              :body
                              ((p nil "Second answer.")))))))
                     (asx--insert-post post)
                     (with-current-buffer target
                       (list
                        (asx-test-rendered-buffer-summary)
                        (how-many
                         "^\\* Answer"
                         (point-min)
                         (point-max))
                        (how-many
                         "Second answer"
                         (point-min)
                         (point-max))))))))
           (when
               (buffer-live-p target)
             (kill-buffer target))))"##,
        expect![[
            r##"OK (("#+STARTUP: overview indent\n#+TITLE: A complete practical post\nhttps://stackoverflow.com/questions/101/first\n* Question (12)\n\nQuestion body with emphasis.\n\nTags: emacs elisp\n* Answer (7)\n:PROPERTIES:\n:VISIBILITY: all\n:END:\n\nFirst answer.\n" org-mode t t 1 t) 1 0)"##
        ]],
    )
}

fn asx_full_normalized_code_block_workflow_reports_the_renderer_error_and_partial_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_full_normalized_code_block_workflow_reports_the_renderer_error_and_partial_buffer",
        r##"(let ((asx-buffer-name "*asx-parity-code-block-post*")
               target)
         (unwind-protect
             (progn
               (setq target
                     (get-buffer-create
                      asx-buffer-name))
               (cl-letf
                   (((symbol-function
                      'switch-to-buffer-other-window)
                     (lambda (buffer)
                       (set-buffer buffer))))
                 (with-temp-buffer
                   (let ((asx--posts
                          '(("First"
                             .
                             "https://stackoverflow.com/questions/101/first")))
                         (asx--current-post-index 0))
                     (let ((outcome
                            (condition-case error
                                (progn
                                  (asx--insert-post
                                   (asx--normalize-post
                                    (asx-test-post-dom)))
                                  :inserted)
                              (error
                               (list
                                :error
                                (car error)
                                (cdr error))))))
                       (with-current-buffer target
                         (list
                          outcome
                          (buffer-substring-no-properties
                           (point-min)
                           (point-max))
                          major-mode
                          visual-line-mode
                          buffer-read-only
                          (point))))))))
           (when
               (buffer-live-p target)
             (kill-buffer target))))"##,
        expect![[
            r##"OK ((:error wrong-type-argument (symbolp "(+ 1 2)")) "#+STARTUP: overview indent\n#+TITLE: How to  ?\nhttps://stackoverflow.com/questions/101/first\n* Question (12)\n\nQuestion body.\n\n#+BEGIN_EXAMPLE emacs\n" fundamental-mode nil nil 148)"##
        ]],
    )
}

fn asx_insert_post_dom_retries_unanswered_posts_or_inserts_them_by_configuration() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asx_insert_post_dom_retries_unanswered_posts_or_inserts_them_by_configuration",
        r##"(let (events)
         (cl-letf
             (((symbol-function
                'asx--normalize-post)
               (lambda (dom)
                 (list
                  :url
                  (cadr dom)
                  :title
                  (car dom)
                  :answers
                  (caddr dom))))
              ((symbol-function
                'asx--remove-and-next)
               (lambda (url)
                 (push
                  (list :retry url)
                  events)
                 :retried))
              ((symbol-function
                'asx--insert-post)
               (lambda (post)
                 (push
                  (list :insert post)
                  events)
                 :inserted)))
           (list
            (let ((asx-skip-unanswered t))
              (asx--insert-post-dom
               '("No answer"
                 "https://example.invalid/questions/1"
                 nil)))
            (let ((asx-skip-unanswered nil))
              (asx--insert-post-dom
               '("No answer"
                 "https://example.invalid/questions/1"
                 nil)))
            (let ((asx-skip-unanswered t))
              (asx--insert-post-dom
               '("Answered"
                 "https://example.invalid/questions/2"
                 ((:score "1")))))
            (nreverse events))))"##,
        expect![[
            r#"OK (:retried :inserted :inserted ((:retry "https://example.invalid/questions/1") (:insert (:url "https://example.invalid/questions/1" :title "No answer" :answers nil)) (:insert (:url "https://example.invalid/questions/2" :title "Answered" :answers ((:score "1"))))))"#
        ]],
    )
}

pub(super) fn rendering_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asx_get_buffer_reuses_the_configured_live_buffer_and_honors_name_changes(),
        asx_prepare_buffer_switches_when_needed_clears_old_content_and_makes_it_writable(),
        asx_finalize_buffer_trims_trailing_space_enables_read_only_org_display_and_rewinds(),
        asx_insert_tags_preserves_order_duplicates_and_the_empty_tags_label_contract(),
        asx_insert_question_renders_metadata_body_links_code_and_tags_together(),
        asx_insert_answers_adds_visibility_only_to_the_first_answer_and_keeps_scores_order(),
        asx_insert_node_renders_a_realistic_nested_dom_as_plain_org_friendly_text(),
        asx_insert_post_runs_the_full_buffer_lifecycle_and_limits_answer_count(),
        asx_full_normalized_code_block_workflow_reports_the_renderer_error_and_partial_buffer(),
        asx_insert_post_dom_retries_unanswered_posts_or_inserts_them_by_configuration(),
    ]
}
