use expect_test::expect;

use super::ParityBatchCase;

fn academic_phrases_inserts_a_chosen_phrase_into_a_paper_draft_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "academic_phrases_inserts_a_chosen_phrase_into_a_paper_draft_at_point",
        r##"(with-temp-buffer
  (insert "\\section{Introduction}\n\nÜberblick: ")
  (academic-test-answer
   (list "Establishing why your topic X is important"
         "X is the [main/leading/primary/major] cause of ..."
         "leading")
   (lambda () (academic-phrases)))
  (insert "renewable energy adoption.\n")
  (list (buffer-string)
        (point)
        (buffer-size)
        (academic-test-prompts)))"##,
        expect![[
            r#"OK ("\\section{Introduction}\n\nÜberblick: X is the leading cause of ...renewable energy adoption.\n" 92 91 (("Choose a category: " 57) ("Choose a phrase: " 12) ("X is the [main/leading/primary/major] cause of ..." 4)))"#
        ]],
    )
}

fn academic_phrases_by_section_restricts_the_categories_to_that_paper_section() -> ParityBatchCase {
    ParityBatchCase::value(
        "academic_phrases_by_section_restricts_the_categories_to_that_paper_section",
        r##"(with-temp-buffer
  (academic-test-answer
   (list "Methods"
         "Describing benefits of your method, equipment etc."
         "Our method has many [interesting/attractive/beneficial/useful/practical/effective/valuable] applications."
         "practical")
   (lambda () (academic-phrases-by-section)))
  (list (buffer-string)
        (point)
        (academic-test-prompts)))"##,
        expect![[
            r#"OK ("Our method has many practical applications." 44 (("Choose a section: " 8) ("Choose a category: " 14) ("Choose a phrase: " 10) ("Our method has many [interesting/attractive/beneficial/useful/practical/effective/valuable] applications." 7)))"#
        ]],
    )
}

fn academic_phrases_fills_every_placeholder_of_a_multiple_choice_template() -> ParityBatchCase {
    ParityBatchCase::value(
        "academic_phrases_fills_every_placeholder_of_a_multiple_choice_template",
        r##"(with-temp-buffer
  (academic-test-answer
   (list "Acknowledgements"
         "We [thank/are grateful to/gratefully acknowledge] Dr. Y for her [help/valuable suggestions and discussions]."
         "gratefully acknowledge"
         "valuable suggestions and discussions")
   (lambda () (academic-phrases)))
  (list (buffer-string)
        (point)
        (string-match-p "{" (buffer-string))
        (string-match-p "\\[" (buffer-string))
        (academic-test-prompts)))"##,
        expect![[
            r#"OK ("We gratefully acknowledge Dr. Y for her valuable suggestions and discussions." 78 nil nil (("Choose a category: " 57) ("Choose a phrase: " 10) ("We [thank/are grateful to/gratefully acknowledge] Dr. Y for her [help/valuable suggestions and discussions]." 3) ("We [thank/are grateful to/gratefully acknowledge] Dr. Y for her [help/valuable suggestions and discussions]." 2)))"#
        ]],
    )
}

fn academic_phrases_offers_the_documented_sections_and_their_own_category_lists() -> ParityBatchCase
{
    ParityBatchCase::value(
        "academic_phrases_offers_the_documented_sections_and_their_own_category_lists",
        r##"(let (observed)
  (dolist (section '("Abstract" "Introduction" "Literature Review" "Methods"
                     "Results" "Discussion" "Conclusions" "Acknowledgements"))
    (with-temp-buffer
      (let* ((offered
              (academic-test-offered
               (list section
                     (lambda (candidates) (car candidates))
                     (lambda (candidates) (car candidates))
                     (lambda (candidates) (car candidates))
                     (lambda (candidates) (car candidates))
                     (lambda (candidates) (car candidates)))
               (lambda ()
                 (ignore-errors (academic-phrases-by-section)))))
             (sections (nth 0 offered))
             (categories (nth 1 offered)))
        (push (list section
                    (length (nth 1 sections))
                    (length (nth 1 categories))
                    (car (nth 1 categories))
                    (car (last (nth 1 categories))))
              observed))))
  (nreverse observed))"##,
        expect![[
            r#"OK (("Abstract" 8 4 "Stating the aim of your paper and its contribution" "Establishing why your topic X is important") ("Introduction" 8 16 "Using the opinions of others to justify your criticism of someone’s work" "Establishing why your topic X is important") ("Literature Review" 8 0 nil nil) ("Methods" 8 14 "Outlining alternative approaches " "Describing purpose of testing / methods used") ("Results" 8 12 "Outlining undesired or unexpected results" "Describing benefits of your method, equipment etc.") ("Discussion" 8 11 "Announcing your conclusions and summarizing content" "Highlighting significant results and achievements") ("Conclusions" 8 7 "Future work proposed for third parties to carry out" "Announcing your conclusions and summarizing content") ("Acknowledgements" 8 1 "Acknowledgements" "Acknowledgements"))"#
        ]],
    )
}

fn academic_phrases_renders_every_category_and_phrase_without_a_leftover_placeholder()
-> ParityBatchCase {
    ParityBatchCase::value(
        "academic_phrases_renders_every_category_and_phrase_without_a_leftover_placeholder",
        r##"(let* ((pick-first (lambda (candidates) (car candidates)))
       (categories
        (nth 1 (nth 0 (academic-test-offered
                       (list pick-first pick-first pick-first pick-first pick-first)
                       (lambda () (with-temp-buffer (academic-phrases)))))))
       (phrase-total 0)
       (with-placeholder 0)
       (leftovers nil)
       (inserted 0)
       (empty-categories nil))
  (dolist (category categories)
    (let* ((probe
            (academic-test-offered
             (list category pick-first pick-first pick-first pick-first)
             (lambda () (with-temp-buffer (academic-phrases)))))
           (phrases (nth 1 (nth 1 probe))))
      (setq phrase-total (+ phrase-total (length phrases)))
      (unless phrases
        (push category empty-categories))
      (dolist (phrase phrases)
        (when (string-match-p "\\[" phrase)
          (setq with-placeholder (1+ with-placeholder)))
        (when (string-match-p "{[0-9]}" phrase)
          (push phrase leftovers)))
      (with-temp-buffer
        (academic-test-answer
         (list category pick-first pick-first pick-first pick-first)
         (lambda () (academic-phrases)))
        (when (> (buffer-size) 0)
          (setq inserted (1+ inserted)))
        (when (string-match-p "\\[{[0-9]}\\]" (buffer-string))
          (push (buffer-string) leftovers)))))
  (list (length categories)
        phrase-total
        with-placeholder
        inserted
        (nreverse empty-categories)
        (nreverse leftovers)))"##,
        expect!["OK (57 592 592 57 nil nil)"],
    )
}

fn academic_phrases_answers_its_prompts_from_the_real_minibuffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "academic_phrases_answers_its_prompts_from_the_real_minibuffer",
        r##"(let ((buffer (generate-new-buffer "*paper draft*")))
  (unwind-protect
      (progn
        (set-window-buffer (selected-window) buffer)
        (set-buffer buffer)
        (insert "Motivation: ")
        (global-set-key (kbd "C-c C-a") #'academic-phrases)
        (execute-kbd-macro
         (vconcat (kbd "C-c C-a")
                  (string-to-vector "Acknowledgements") [?\r]
                  (string-to-vector "We [thank/are grateful to/gratefully acknowledge] Dr. Y for her [help/valuable suggestions and discussions].") [?\r]
                  (string-to-vector "thank") [?\r]
                  (string-to-vector "help") [?\r]))
        (list (buffer-string) (point)))
    (kill-buffer buffer)))"##,
        expect![[r#"OK ("Motivation: We thank Dr. Y for her help." 41)"#]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        academic_phrases_inserts_a_chosen_phrase_into_a_paper_draft_at_point(),
        academic_phrases_by_section_restricts_the_categories_to_that_paper_section(),
        academic_phrases_fills_every_placeholder_of_a_multiple_choice_template(),
        academic_phrases_offers_the_documented_sections_and_their_own_category_lists(),
        academic_phrases_renders_every_category_and_phrase_without_a_leftover_placeholder(),
        academic_phrases_answers_its_prompts_from_the_real_minibuffer(),
    ]
}
