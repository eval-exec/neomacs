use expect_test::expect;

use super::ParityBatchCase;

fn agda_lib_mode_opens_edits_and_saves_a_library_file_through_its_autoload() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_lib_mode_opens_edits_and_saves_a_library_file_through_its_autoload",
        r##"(let* ((root
                                 (expand-file-name
                                  "project"
                                  (getenv
                                   "NEOMACS_TEST_SANDBOX_ROOT")))
                                (file
                                 (expand-file-name
                                  "standard-library.agda-lib"
                                  root))
                                buffer)
                           (make-directory root t)
                           (with-temp-file file
                             (insert
                              "name: standard-library\n"
                              "include: src\n"
                              "-- local development paths\n"))
                           (unwind-protect
                               (progn
                                 (setq buffer
                                       (find-file-noselect file))
                                 (with-current-buffer buffer
                                   (font-lock-ensure)
                                   (goto-char
                                    (point-min))
                                   (search-forward
                                    "include: src")
                                   (insert
                                    " generated")
                                   (save-buffer)
                                   (list
                                    major-mode
                                    (featurep
                                     'agda-lib-mode)
                                    (file-name-nondirectory
                                     buffer-file-name)
                                    (buffer-string)
                                    (mapcar
                                     (lambda (needle)
                                       (goto-char
                                        (point-min))
                                       (search-forward needle)
                                       (get-text-property
                                        (match-beginning 0)
                                        'face))
                                     '("name:"
                                       "include:"
                                       "-- local development paths"))
                                    comment-start
                                    (with-temp-buffer
                                      (insert-file-contents file)
                                      (buffer-string)))))
                             (when
                                 (buffer-live-p buffer)
                               (with-current-buffer buffer
                                 (set-buffer-modified-p nil))
                               (kill-buffer buffer))))"##,
        expect![[
            r#"OK (agda-lib-mode t "standard-library.agda-lib" #("name: standard-library\ninclude: src generated\n-- local development paths\n" 0 5 (face font-lock-keyword-face) 23 31 (face font-lock-keyword-face) 46 72 (face font-lock-comment-face)) (font-lock-keyword-face font-lock-keyword-face font-lock-comment-face) "-- " "name: standard-library\ninclude: src generated\n-- local development paths\n")"#
        ]],
    )
}

fn agda_lib_mode_repairs_comments_and_refontifies_a_real_library_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_lib_mode_repairs_comments_and_refontifies_a_real_library_document",
        r##"(with-temp-buffer
                           (insert
                            "name sample\n"
                            "include: src\n"
                            "depend: base\n")
                           (agda-lib-mode)
                           (goto-char
                            (point-min))
                           (search-forward
                            "name")
                           (insert ":")
                           (forward-line 1)
                           (comment-line 1)
                           (font-lock-ensure)
                           (let ((commented
                                  (buffer-string))
                                 (commented-faces
                                  (mapcar
                                   (lambda (needle)
                                     (goto-char
                                      (point-min))
                                     (search-forward needle)
                                     (get-text-property
                                      (match-beginning 0)
                                      'face))
                                   '("name:"
                                     "-- include:"
                                     "depend:"))))
                             (uncomment-region
                              (save-excursion
                                (goto-char
                                 (point-min))
                                (forward-line 1)
                                (point))
                              (save-excursion
                                (goto-char
                                 (point-min))
                                (forward-line 2)
                                (point)))
                             (font-lock-flush)
                             (font-lock-ensure)
                             (list
                              commented
                              commented-faces
                              (buffer-string)
                              (mapcar
                               (lambda (needle)
                                 (goto-char
                                  (point-min))
                                 (search-forward needle)
                                 (get-text-property
                                  (match-beginning 0)
                                  'face))
                               '("name:"
                                 "include:"
                                 "depend:")))))"##,
        expect![[
            r#"OK (#("name: sample\n-- include: src\ndepend: base\n" 0 5 (face font-lock-keyword-face) 13 28 (face font-lock-comment-face) 29 36 (face font-lock-keyword-face)) (font-lock-keyword-face font-lock-comment-face font-lock-keyword-face) #("name: sample\ninclude: src\ndepend: base\n" 0 5 (face font-lock-keyword-face) 13 21 (face font-lock-keyword-face) 26 33 (face font-lock-keyword-face)) (font-lock-keyword-face font-lock-keyword-face font-lock-keyword-face))"#
        ]],
    )
}

fn agda_lib_mode_fills_and_round_trips_documentation_comments() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_lib_mode_fills_and_round_trips_documentation_comments",
        r##"(with-temp-buffer
                           (insert
                            "-- This library exposes algebraic structures and carefully selected experimental modules.\n"
                            "include: src\n"
                            "depend: base\n")
                           (agda-lib-mode)
                           (setq-local
                            fill-column
                            32)
                           (goto-char 45)
                           (fill-paragraph)
                           (let ((filled
                                  (buffer-string)))
                             (let ((comment-end-marker
                                    (copy-marker
                                     (save-excursion
                                       (goto-char
                                        (point-min))
                                       (forward-line 4)
                                       (point)))))
                               (uncomment-region
                                (point-min)
                                comment-end-marker)
                               (comment-region
                                (point-min)
                                comment-end-marker)
                               (set-marker
                                comment-end-marker
                                nil))
                             (list
                              filled
                              (buffer-string)
                              (line-number-at-pos)
                              comment-start)))"##,
        expect![[
            r#"OK ("-- This library exposes\n-- algebraic structures and\n-- carefully selected\n-- experimental modules.\ninclude: src\ndepend: base\n" "-- This library exposes\n-- algebraic structures and\n-- carefully selected\n-- experimental modules.\ninclude: src\ndepend: base\n" 2 "-- ")"#
        ]],
    )
}

fn agda_lib_mode_highlights_fields_flags_and_comments_in_a_complete_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_lib_mode_highlights_fields_flags_and_comments_in_a_complete_document",
        r##"(with-temp-buffer
                           (insert
                            "name: standard-library\n"
                            "include: src\n"
                            "         experimental\n"
                            "depend: base\n"
                            "flags: --safe --without-K\n"
                            "-- whole-line comment\n"
                            "include: test -- trailing explanation\n")
                           (agda-lib-mode)
                           (font-lock-ensure)
                           (let ((position
                                  (point-min))
                                 runs)
                             (while
                                 (< position
                                    (point-max))
                               (let* ((face
                                       (get-text-property
                                        position
                                        'face))
                                      (next
                                       (next-single-property-change
                                        position
                                        'face
                                        nil
                                        (point-max))))
                                 (when face
                                   (push
                                    (list
                                     position
                                     next
                                     (buffer-substring-no-properties
                                      position next)
                                     face)
                                    runs))
                                 (setq position next)))
                             (nreverse runs)))"##,
        expect![[
            r#"OK ((1 6 "name:" font-lock-keyword-face) (24 32 "include:" font-lock-keyword-face) (59 66 "depend:" font-lock-keyword-face) (72 78 "flags:" font-lock-keyword-face) (98 119 "-- whole-line comment" font-lock-comment-face) (120 128 "include:" font-lock-keyword-face) (133 157 " -- trailing explanation" font-lock-comment-face))"#
        ]],
    )
}

pub(super) fn workflows_agda_lib_mode_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![agda_lib_mode_opens_edits_and_saves_a_library_file_through_its_autoload()]
}

pub(super) fn workflows_agda_lib_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        agda_lib_mode_repairs_comments_and_refontifies_a_real_library_document(),
        agda_lib_mode_fills_and_round_trips_documentation_comments(),
        agda_lib_mode_highlights_fields_flags_and_comments_in_a_complete_document(),
    ]
}
