use expect_test::expect;

use super::ParityBatchCase;

fn ast_grep_rewrite_json_parser_preserves_full_multiline_ranges_and_replacements() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_rewrite_json_parser_preserves_full_multiline_ranges_and_replacements",
        r##"(mapcar
          (lambda (line)
            (ast-grep--parse-rewrite-line line))
          (list
           "{\"file\":\"src/a.js\",\"range\":{\"start\":{\"line\":3,\"column\":2},\"end\":{\"line\":5,\"column\":7}},\"text\":\"old(\\n x\\n)\",\"replacement\":\"new(x)\"}"
           "{\"file\":\"src/unicode.rs\",\"range\":{\"start\":{\"line\":0,\"column\":1},\"end\":{\"line\":0,\"column\":4}},\"text\":\"α界\",\"replacement\":\"beta\"}"
           "{\"file\":\"missing-end\",\"range\":{\"start\":{\"line\":1,\"column\":0}},\"text\":\"x\",\"replacement\":null}"
           ""
           "malformed"))"##,
        expect![[
            r#"OK ((:file "src/a.js" :start-line 3 :start-column 2 :end-line 5 :end-column 7 :text "old(\n x\n)" :replacement "new(x)") (:file "src/unicode.rs" :start-line 0 :start-column 1 :end-line 0 :end-column 4 :text "α界" :replacement "beta") (:file "missing-end" :start-line 1 :start-column 0 :end-line nil :end-column nil :text "x" :replacement :null) nil nil)"#
        ]],
    )
}

fn ast_grep_collect_rewrites_runs_real_cli_with_exact_pattern_rewrite_and_directory()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_collect_rewrites_runs_real_cli_with_exact_pattern_rewrite_and_directory",
        r##"(let* ((work (ast-grep-test-path "rewrite-project"))
               (log (ast-grep-test-path "rewrite-argv.log"))
               (program
                (ast-grep-test-make-executable
                 "sg-rewrite"
                 (format
                  "printf '%%s\\n' \"$@\" > %s\nprintf '%%s\\n' '{\"file\":\"src/a.js\",\"range\":{\"start\":{\"line\":0,\"column\":4},\"end\":{\"line\":0,\"column\":10}},\"text\":\"old(x)\",\"replacement\":\"new(x)\"}' 'invalid-json'"
                  (shell-quote-argument log))))
               (ast-grep-executable program))
          (make-directory work t)
          (let ((matches
                 (ast-grep--collect-rewrites
                  "old($X)"
                  "new($X)"
                  work)))
            (list
             (replace-regexp-in-string
              (regexp-quote work) "$WORK"
              (ast-grep-test-read-file log))
             matches)))"##,
        expect![[
            r#"OK ("run\n--pattern=old($X)\n--rewrite=new($X)\n--json=stream\n$WORK\n" ((:file "src/a.js" :start-line 0 :start-column 4 :end-line 0 :end-column 10 :text "old(x)" :replacement "new(x)")))"#
        ]],
    )
}

fn ast_grep_match_region_uses_character_coordinates_for_multiline_unicode_source() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ast_grep_match_region_uses_character_coordinates_for_multiline_unicode_source",
        r##"(with-temp-buffer
          (insert "zero\n\tα界 old(one)\nnext old(two)\n")
          (mapcar
           (lambda (match)
             (let ((region (ast-grep--match-region match)))
               (list
                region
                (buffer-substring-no-properties
                 (car region)
                 (cdr region)))))
           '((:start-line 1 :start-column 4
              :end-line 1 :end-column 12)
             (:start-line 1 :start-column 4
              :end-line 2 :end-column 8)
             (:start-line 2 :start-column 5
              :end-line 2 :end-column 13))))"##,
        expect![[
            r#"OK (((10 . 18) "old(one)") ((10 . 27) "old(one)\nnext old") ((24 . 32) "old(two)"))"#
        ]],
    )
}

fn ast_grep_rewrite_sort_orders_files_ascending_and_positions_descending() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_rewrite_sort_orders_files_ascending_and_positions_descending",
        r##"(mapcar
          (lambda (match)
            (list
             (plist-get match :file)
             (plist-get match :start-line)
             (plist-get match :start-column)
             (plist-get match :text)))
          (ast-grep--rewrite-sort
           '((:file "b.rs" :start-line 1 :start-column 2 :text "b1")
             (:file "a.rs" :start-line 0 :start-column 1 :text "a0")
             (:file "a.rs" :start-line 3 :start-column 0 :text "a3")
             (:file "b.rs" :start-line 1 :start-column 9 :text "b9")
             (:file "a.rs" :start-line 3 :start-column 7 :text "a37"))))"##,
        expect![[
            r#"OK (("a.rs" 3 7 "a37") ("a.rs" 3 0 "a3") ("a.rs" 0 1 "a0") ("b.rs" 1 9 "b9") ("b.rs" 1 2 "b1"))"#
        ]],
    )
}

fn ast_grep_apply_rewrites_bang_updates_all_files_in_reverse_offset_order_without_saving()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_apply_rewrites_bang_updates_all_files_in_reverse_offset_order_without_saving",
        r##"(let* ((file-a
               (ast-grep-test-write-file
                 "rewrite/a.txt"
                 "old(one) + old(two)\n"))
               (file-b
                (ast-grep-test-write-file
                 "rewrite/b.txt"
                 "old(three)\n"))
               (matches
                (list
                 (list :file file-a :start-line 0 :start-column 0
                       :end-line 0 :end-column 8
                       :text "old(one)" :replacement "new(one)")
                 (list :file file-a :start-line 0 :start-column 11
                       :end-line 0 :end-column 19
                       :text "old(two)" :replacement "new(two)")
                 (list :file file-b :start-line 0 :start-column 0
                       :end-line 0 :end-column 10
                       :text "old(three)" :replacement "new(three)")))
               (choices '(?!))
               messages)
          ;; Open both buffers before intercepting `message': Neomacs may load
          ;; ordinary file-visiting support lazily, while the assertion below
          ;; intentionally observes only ast-grep's own rewrite summary.
          (find-file-noselect file-a)
          (find-file-noselect file-b)
          (unwind-protect
              (cl-letf (((symbol-function 'pop-to-buffer)
                         (lambda (buffer &rest _)
                           (set-buffer buffer)
                           buffer))
                        ((symbol-function 'read-char-choice)
                         (lambda (_prompt _characters)
                           (prog1 (car choices)
                             (setq choices (cdr choices)))))
                        ((symbol-function 'message)
                         (lambda (format-string &rest args)
                           (push
                            (apply #'format format-string args)
                            messages))))
                (ast-grep--apply-rewrites matches)
                (list
                 (mapcar
                  (lambda (file)
                    (with-current-buffer (find-buffer-visiting file)
                      (list
                       (file-name-nondirectory file)
                       (buffer-string)
                       (buffer-modified-p))))
                  (list file-a file-b))
                 (mapcar #'ast-grep-test-read-file (list file-a file-b))
                 (nreverse messages)
                 choices))
            (ast-grep-test-kill-file-buffer file-a)
            (ast-grep-test-kill-file-buffer file-b)))"##,
        expect![[
            r#"OK ((("a.txt" "new(one) + new(two)\n" t) ("b.txt" "new(three)\n" t)) ("old(one) + old(two)\n" "old(three)\n") (#("Replaced 3 match(es) in 2 file(s); skipped 0; use C-x s to save" 50 55 (font-lock-face help-key-binding face help-key-binding))) nil)"#
        ]],
    )
}

fn ast_grep_apply_rewrites_yes_skip_and_quit_follow_query_replace_semantics() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_apply_rewrites_yes_skip_and_quit_follow_query_replace_semantics",
        r##"(let* ((file
                (ast-grep-test-write-file
                 "rewrite/choices.el"
                 "old-a old-b old-c old-d\n"))
               (matches
                (list
                 (list :file file :start-line 0 :start-column 0
                       :end-line 0 :end-column 5
                       :text "old-a" :replacement "new-a")
                 (list :file file :start-line 0 :start-column 6
                       :end-line 0 :end-column 11
                       :text "old-b" :replacement "new-b")
                 (list :file file :start-line 0 :start-column 12
                       :end-line 0 :end-column 17
                       :text "old-c" :replacement "new-c")
                 (list :file file :start-line 0 :start-column 18
                       :end-line 0 :end-column 23
                       :text "old-d" :replacement "new-d")))
               (choices '(?q))
               prompts
               final-message)
          ;; The sort is position-descending, so quitting on old-d leaves all
          ;; four edits untouched and proves traversal really follows offsets.
          (unwind-protect
              (cl-letf (((symbol-function 'pop-to-buffer)
                         (lambda (buffer &rest _)
                           (set-buffer buffer)
                           buffer))
                        ((symbol-function 'read-char-choice)
                         (lambda (prompt _characters)
                           (push prompt prompts)
                           (prog1 (car choices)
                             (setq choices (cdr choices)))))
                        ((symbol-function 'message)
                         (lambda (format-string &rest args)
                           (setq final-message
                                 (apply #'format format-string args)))))
                (ast-grep--apply-rewrites matches)
                (with-current-buffer (find-buffer-visiting file)
                  (list
                   (buffer-string)
                   (buffer-modified-p)
                   (nreverse prompts)
                   final-message)))
            (ast-grep-test-kill-file-buffer file)))"##,
        expect![[
            r#"OK (#("old-a old-b old-c old-d\n" 0 24 (fontified nil)) nil ("Replace `old-d' with `new-d'? (y/n/!/q) ") "Replaced 0 match(es) in 0 file(s); skipped 0 (quit)")"#
        ]],
    )
}

fn ast_grep_apply_rewrites_mixed_yes_and_skip_keeps_offsets_and_counts_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_apply_rewrites_mixed_yes_and_skip_keeps_offsets_and_counts_exact",
        r##"(let* ((file
                (ast-grep-test-write-file
                 "rewrite/mixed.txt"
                 "aa bb cc\n"))
               (matches
                (list
                 (list :file file :start-line 0 :start-column 0
                       :end-line 0 :end-column 2
                       :text "aa" :replacement "AAAA")
                 (list :file file :start-line 0 :start-column 3
                       :end-line 0 :end-column 5
                       :text "bb" :replacement "BBBB")
                 (list :file file :start-line 0 :start-column 6
                       :end-line 0 :end-column 8
                       :text "cc" :replacement "CCCC")))
               ;; Reverse traversal: cc yes, bb no, aa yes.
               (choices '(?y ?n ?y))
               final-message)
          (unwind-protect
              (cl-letf (((symbol-function 'pop-to-buffer)
                         (lambda (buffer &rest _)
                           (set-buffer buffer)
                           buffer))
                        ((symbol-function 'read-char-choice)
                         (lambda (_prompt _characters)
                           (prog1 (car choices)
                             (setq choices (cdr choices)))))
                        ((symbol-function 'message)
                         (lambda (format-string &rest args)
                           (setq final-message
                                 (apply #'format format-string args)))))
                (ast-grep--apply-rewrites matches)
                (with-current-buffer (find-buffer-visiting file)
                  (list
                   (buffer-string)
                   (buffer-modified-p)
                   final-message
                   choices)))
            (ast-grep-test-kill-file-buffer file)))"##,
        expect![[
            r#"OK ("AAAA bb CCCC\n" t #("Replaced 2 match(es) in 1 file(s); skipped 1; use C-x s to save" 50 55 (font-lock-face help-key-binding face help-key-binding)) nil)"#
        ]],
    )
}

fn ast_grep_rewrite_command_prompts_collects_and_applies_real_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_rewrite_command_prompts_collects_and_applies_real_workflow",
        r##"(let* ((program
                (ast-grep-test-make-executable
                 "sg-present"
                 "exit 0"))
               (ast-grep-executable program)
               (answers '("old($X)" "new($X)"))
               calls)
          (cl-letf (((symbol-function 'read-string)
                     (lambda (prompt &rest args)
                       (push (list :prompt prompt :args args) calls)
                       (prog1 (car answers)
                         (setq answers (cdr answers)))))
                    ((symbol-function 'ast-grep--collect-rewrites)
                     (lambda (pattern rewrite directory)
                       (push
                        (list :collect pattern rewrite directory)
                        calls)
                       '((:file "a" :start-line 0 :start-column 0))))
                    ((symbol-function 'ast-grep--apply-rewrites)
                     (lambda (matches)
                       (push (list :apply matches) calls)
                       :applied)))
            (list
             (ast-grep-rewrite "/fixture/project/")
             (nreverse calls)
             answers)))"##,
        expect![[
            r#"OK (:applied ((:prompt "ast-grep pattern: " :args (nil ast-grep-history)) (:prompt "Rewrite `old($X)' with: " :args (nil ast-grep-rewrite-history)) (:collect "old($X)" "new($X)" "/fixture/project/") (:apply ((:file "a" :start-line 0 :start-column 0)))) nil)"#
        ]],
    )
}

fn ast_grep_rewrite_command_reports_no_matches_without_entering_editor_loop() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_rewrite_command_reports_no_matches_without_entering_editor_loop",
        r##"(let* ((program
                (ast-grep-test-make-executable
                 "sg-present"
                 "exit 0"))
               (ast-grep-executable program)
               (answers '("find-me" "replace-me"))
               messages
               applied)
          (cl-letf (((symbol-function 'read-string)
                     (lambda (&rest _)
                       (prog1 (car answers)
                         (setq answers (cdr answers)))))
                    ((symbol-function 'ast-grep--collect-rewrites)
                     (lambda (&rest _) nil))
                    ((symbol-function 'ast-grep--apply-rewrites)
                     (lambda (&rest _)
                       (setq applied t)))
                    ((symbol-function 'message)
                     (lambda (format-string &rest args)
                       (push
                        (apply #'format format-string args)
                        messages))))
            (list
             (ast-grep-rewrite "/fixture/")
             (nreverse messages)
             applied)))"##,
        expect![[r#"OK (#1=("No matches for pattern: find-me") #1# nil)"#]],
    )
}

pub(super) fn rewrite_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ast_grep_rewrite_json_parser_preserves_full_multiline_ranges_and_replacements(),
        ast_grep_collect_rewrites_runs_real_cli_with_exact_pattern_rewrite_and_directory(),
        ast_grep_match_region_uses_character_coordinates_for_multiline_unicode_source(),
        ast_grep_rewrite_sort_orders_files_ascending_and_positions_descending(),
        ast_grep_apply_rewrites_bang_updates_all_files_in_reverse_offset_order_without_saving(),
        ast_grep_apply_rewrites_yes_skip_and_quit_follow_query_replace_semantics(),
        ast_grep_apply_rewrites_mixed_yes_and_skip_keeps_offsets_and_counts_exact(),
        ast_grep_rewrite_command_prompts_collects_and_applies_real_workflow(),
        ast_grep_rewrite_command_reports_no_matches_without_entering_editor_loop(),
    ]
}
