use expect_test::expect;

use super::ParityBatchCase;

fn ac_math_completes_a_backslash_name_into_the_unicode_character() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_math_completes_a_backslash_name_into_the_unicode_character",
        r##"(ac-math-test-in-buffer
 (setq ac-sources '(ac-source-math-unicode))
 (insert "Let \\alpha")
 (let ((candidates (ac-math-test-candidates))
       (prefix ac-prefix))
   (ac-complete)
   (insert " be a root and \\beta")
   (let ((second (ac-math-test-candidates)))
     (ac-complete)
     (list candidates
           prefix
           second
           (ac-math-test-text)
           (point)
           (cdr (assq 'symbol ac-source-math-unicode))
           (cdr (assq 'action ac-source-math-unicode))))))"##,
        expect![[
            r#"OK (("alpha α" "alpha 𝛼") "alpha" ("beta β" "beta 𝛽") "Let α be a root and β" 22 "u" ac-math-action-unicode)"#
        ]],
    )
}

fn ac_math_completes_latex_control_words_from_the_command_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_math_completes_latex_control_words_from_the_command_source",
        r##"(ac-math-test-in-buffer
 (setq ac-sources '(ac-source-latex-commands))
 (insert "\\beg")
 (let ((candidates (ac-math-test-candidates)))
   (ac-complete)
   (insert "{document}\n\\sect")
   (let ((narrowed (ac-math-test-candidates)))
     (ac-complete)
     (list candidates
           narrowed
           (ac-math-test-text)
           (point)
           (length math-symbol-list-latex-commands)
           (cdr (assq 'symbol ac-source-latex-commands))
           (cdr (assq 'prefix ac-source-latex-commands))))))"##,
        expect![[
            r#"OK (("begin") ("section") "\\begin{document}\n\\section" 26 323 "c" ac-math-prefix)"#
        ]],
    )
}

fn ac_math_switches_sources_between_math_and_prose_by_the_face_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_math_switches_sources_between_math_and_prose_by_the_face_at_point",
        r##"(ac-math-test-in-buffer
 (insert "prose \\alpha and $\\beta$")
 (ac-math-test-math-region 18 25)
 (let ((in-prose (progn (goto-char 13)
                        (list (ac-math-latex-math-face-p)
                              (and (ac-math-candidates-latex) t)
                              (and (ac-math-candidates-unicode) t))))
       (in-math (progn (goto-char 22)
                       (list (ac-math-latex-math-face-p)
                             (and (ac-math-candidates-latex) t)
                             (and (ac-math-candidates-unicode) t))))
       (unicode-allowed (let ((ac-math-unicode-in-math-p t))
                          (goto-char 22)
                          (and (ac-math-candidates-unicode) t))))
   (list in-prose in-math unicode-allowed ac-math-unicode-in-math-p)))"##,
        expect!["OK ((nil nil t) (t t nil) t nil)"],
    )
}

fn ac_math_keeps_the_latex_command_when_completing_inside_a_math_environment() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_math_keeps_the_latex_command_when_completing_inside_a_math_environment",
        r##"(ac-math-test-in-buffer
 (setq ac-sources '(ac-source-math-latex))
 (insert "The set $\\alph$ is open")
 (ac-math-test-math-region 9 16)
 (goto-char 15)
 (let ((math-face (ac-math-latex-math-face-p))
       (candidates (seq-take (ac-math-test-candidates) 5)))
   (ac-complete)
   (list math-face
         candidates
         (ac-math-test-text)
         (point)
         (cdr (assq 'symbol ac-source-math-latex))
         (cdr (assq 'action ac-source-math-latex)))))"##,
        expect![[
            r#"OK (t ("alpha α" "alpha 𝛼") "The set $\\alpha$ is open" 16 "l" ac-math-action-latex)"#
        ]],
    )
}

fn ac_math_honours_a_customized_prefix_regexp() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_math_honours_a_customized_prefix_regexp",
        r##"(ac-math-test-in-buffer
 (setq ac-sources '(ac-source-math-unicode))
 (let ((default-regexp ac-math-prefix-regexp)
       (ac-math-prefix-regexp "::\\(.*\\)"))
   (insert "note ::alpha")
   (let ((candidates (ac-math-test-candidates))
         (prefix-position (save-excursion (ac-math-prefix))))
     (ac-complete)
     (list default-regexp
           ac-math-prefix-regexp
           candidates
           prefix-position
           (ac-math-test-text)
           (point)))))"##,
        expect![[r#"OK ("\\\\\\(.*\\)" "::\\(.*\\)" ("alpha α" "alpha 𝛼") 8 "note α" 7)"#]],
    )
}

fn ac_math_candidate_tables_pair_every_name_with_its_character() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_math_candidate_tables_pair_every_name_with_its_character",
        r##"(list
 (length ac-math-symbols-latex)
 (length ac-math-symbols-unicode)
 (seq-take ac-math-symbols-latex 4)
 (seq-take ac-math-symbols-unicode 4)
 (assoc "alpha α" ac-math-symbols-latex)
 (assoc "alpha α" ac-math-symbols-unicode)
 (assoc "infty ∞" ac-math-symbols-unicode)
 (assoc "not-a-symbol" ac-math-symbols-unicode)
 (seq-take math-symbol-list-latex-commands 4)
 ac-math--dummy)"##,
        expect![[
            r#"OK (2824 2774 (("acute ́" . "acute") ("bar ̄" . "bar") ("breve ̆" . "breve") ("check ̌" . "check")) (("acute ́" . "́") ("bar ̄" . "̄") ("breve ̆" . "̆") ("check ̌" . "̌")) ("alpha α" . "alpha") ("alpha α" . "α") ("infty ∞" . "∞") nil ("address" "addtocounter" "addtolength" "addvspace") " ")"#
        ]],
    )
}

fn ac_math_offers_nothing_for_plain_prose_and_leaves_the_buffer_alone() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_math_offers_nothing_for_plain_prose_and_leaves_the_buffer_alone",
        r##"(ac-math-test-in-buffer
 (setq ac-sources '(ac-source-math-unicode ac-source-latex-commands))
 (insert "just prose here")
 (let ((prose (ac-math-test-candidates))
       (prose-prefix ac-prefix)
       (prose-text (ac-math-test-text)))
   (erase-buffer)
   (insert "\\zzzznosuchsymbol")
   (let ((unknown (ac-math-test-candidates)))
     (list prose
           prose-prefix
           prose-text
           unknown
           ac-prefix
           (ac-math-test-text)
           (point)))))"##,
        expect![[
            r#"OK (nil nil "just prose here" nil "zzzznosuchsymbol" "\\zzzznosuchsymbol\n\n\n\n\n\n\n\n\n\n\n" 18)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ac_math_completes_a_backslash_name_into_the_unicode_character(),
        ac_math_completes_latex_control_words_from_the_command_source(),
        ac_math_switches_sources_between_math_and_prose_by_the_face_at_point(),
        ac_math_keeps_the_latex_command_when_completing_inside_a_math_environment(),
        ac_math_honours_a_customized_prefix_regexp(),
        ac_math_candidate_tables_pair_every_name_with_its_character(),
        ac_math_offers_nothing_for_plain_prose_and_leaves_the_buffer_alone(),
    ]
}
