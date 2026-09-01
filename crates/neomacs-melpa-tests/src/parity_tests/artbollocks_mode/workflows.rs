use expect_test::expect;

use super::ParityBatchCase;

fn documented_text_mode_hook_reviews_a_selected_draft_with_the_bound_metric_commands()
-> ParityBatchCase {
    ParityBatchCase::value(
        "documented_text_mode_hook_reviews_a_selected_draft_with_the_bound_metric_commands",
        r##"(let ((text-mode-hook nil))
         (add-hook 'text-mode-hook #'artbollocks-mode)
         (with-temp-buffer
           (text-mode)
           (insert
            "Gallery notes are deliberately excluded.\n\n"
            "The installation fills the room. Visitors move slowly.\n\n"
            "Unpublished interview material stays outside the review.")
           (goto-char (point-min))
           (search-forward "The installation")
           (set-mark (match-beginning 0))
           (search-forward "slowly.")
           (let ((transient-mark-mode t)
                 (mark-active t)
                 (selection
                  (list (region-beginning)
                        (region-end)))
                 (selected-text
                  (buffer-substring-no-properties
                   (region-beginning)
                   (region-end)))
                 results)
             (dolist (key '("C-c [" "C-c ]" "C-c \\" "C-c /" "C-c ="))
               (let ((binding (key-binding (kbd key))))
                 (push
                  (list key binding (call-interactively binding))
                  results)))
             (font-lock-ensure (point-min) (point-max))
             (goto-char (point-min))
             (search-forward "deliberately")
             (let ((outside-face
                    (get-text-property
                     (match-beginning 0)
                     'face)))
               (search-forward "installation")
               (list
                major-mode
                artbollocks-mode
                (assq 'artbollocks-mode minor-mode-alist)
                (nreverse results)
                selection
                selected-text
                outside-face
                (get-text-property (match-beginning 0) 'face)
                (buffer-modified-p))))))"##,
        expect![
            r#"OK (text-mode t (artbollocks-mode " AB") (("C-c [" artbollocks-word-count 8) ("C-c ]" artbollocks-sentence-count 2) ("C-c \\" artbollocks-readability-index "Readability index: 7.063749999999999") ("C-c /" artbollocks-reading-ease "Reading ease: 44.149") ("C-c =" artbollocks-grade-level "Grade level: 8.094999999999999")) (43 97) "The installation fills the room. Visitors move slowly." nil nil t)"#
        ],
    )
}

fn editing_lisp_prose_marks_real_writing_problems_but_not_matching_code() -> ParityBatchCase {
    ParityBatchCase::value(
        "editing_lisp_prose_marks_real_writing_problems_but_not_matching_code",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert
          "(defun review-installation (narrative very contextual discourse)\n"
          "  \"The the narrative was written in a very contextual discourse.\"\n"
          "  (list narrative very contextual discourse))\n"
          ";; Many works were completed by a fairly normative paradigm.\n")
         (let* ((package-faces
                 '(artbollocks-lexical-illusions-face
                   artbollocks-passive-voice-face
                   artbollocks-weasel-words-face
                   artbollocks-face))
                (collect-highlights
                 (lambda ()
                   (let ((position (point-min))
                         highlights)
                     (while (< position (point-max))
                       (let* ((next
                               (or
                                (next-single-property-change
                                 position 'face nil (point-max))
                                (point-max)))
                              (face (get-text-property position 'face))
                              (package-face
                               (cond
                                ((memq face package-faces) face)
                                ((listp face)
                                 (catch 'found
                                   (dolist (candidate face)
                                     (when (memq candidate package-faces)
                                       (throw 'found candidate))))))))
                         (when package-face
                           (push
                            (list
                             (buffer-substring-no-properties position next)
                             package-face
                             position
                             next)
                            highlights))
                         (setq position next)))
                     (nreverse highlights)))))
           (artbollocks-mode 1)
           (font-lock-ensure (point-min) (point-max))
           (list
            (funcall collect-highlights)
            artbollocks-mode
            (buffer-substring-no-properties
             (point-min)
             (point-max))
            (buffer-modified-p))))"##,
        expect![[
            r#"OK ((("the" artbollocks-lexical-illusions-face 73 76) ("narrative" artbollocks-face 77 86) ("was written" artbollocks-passive-voice-face 87 98) ("very" artbollocks-weasel-words-face 104 108) ("contextual" artbollocks-face 109 119) ("discourse" artbollocks-face 120 129) ("Many" artbollocks-weasel-words-face 181 185) ("works" artbollocks-face 186 191) ("fairly" artbollocks-weasel-words-face 212 218) ("normative" artbollocks-face 219 228) ("paradigm" artbollocks-face 229 237)) t "(defun review-installation (narrative very contextual discourse)\n  \"The the narrative was written in a very contextual discourse.\"\n  (list narrative very contextual discourse))\n;; Many works were completed by a fairly normative paradigm.\n" t)"#
        ]],
    )
}

fn a_team_customizes_its_editorial_policy_and_refontifies_the_open_review() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_team_customizes_its_editorial_policy_and_refontifies_the_open_review",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert
          ";; Perhaps the release was shipped around the north star.\n"
          ";; The team found sort of synergy in the launch narrative.\n")
         (let* ((artbollocks-lexical-illusions nil)
                (artbollocks-passive-voice t)
                (artbollocks-weasel-words t)
                (artbollocks-jargon t)
                (artbollocks-passive-voice-words '("shipped"))
                (artbollocks-weasel-words-list '("perhaps" "sort of"))
                (artbollocks-jargon-words '("north star" "synergy"))
                (package-faces
                 '(artbollocks-passive-voice-face
                   artbollocks-weasel-words-face
                   artbollocks-face))
                (collect-highlights
                 (lambda ()
                   (let ((position (point-min))
                         highlights)
                     (while (< position (point-max))
                       (let* ((next
                               (or
                                (next-single-property-change
                                 position 'face nil (point-max))
                                (point-max)))
                              (face (get-text-property position 'face))
                              (package-face
                               (cond
                                ((memq face package-faces) face)
                                ((listp face)
                                 (catch 'found
                                   (dolist (candidate face)
                                     (when (memq candidate package-faces)
                                       (throw 'found candidate))))))))
                         (when package-face
                           (push
                            (list
                             (buffer-substring-no-properties position next)
                             package-face)
                            highlights))
                         (setq position next)))
                     (nreverse highlights)))))
           (artbollocks-mode 1)
           (font-lock-ensure (point-min) (point-max))
           (let ((team-policy (funcall collect-highlights)))
             (setq artbollocks-passive-voice-words '("found")
                   artbollocks-weasel-words-list '("around")
                   artbollocks-jargon-words '("launch narrative"))
             (font-lock-flush)
             (font-lock-ensure (point-min) (point-max))
             (list
              team-policy
              (funcall collect-highlights)
              (buffer-substring-no-properties
               (point-min)
               (point-max))
              artbollocks-mode
              (buffer-modified-p)))))"##,
        expect![
            r#"OK ((("Perhaps" artbollocks-weasel-words-face) ("was shipped" artbollocks-passive-voice-face) ("north star" artbollocks-face) ("sort of" artbollocks-weasel-words-face) ("synergy" artbollocks-face)) (("around" artbollocks-weasel-words-face) ("launch narrative" artbollocks-face)) ";; Perhaps the release was shipped around the north star.\n;; The team found sort of synergy in the launch narrative.\n" t t)"#
        ],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        documented_text_mode_hook_reviews_a_selected_draft_with_the_bound_metric_commands(),
        editing_lisp_prose_marks_real_writing_problems_but_not_matching_code(),
        a_team_customizes_its_editorial_policy_and_refontifies_the_open_review(),
    ]
}
