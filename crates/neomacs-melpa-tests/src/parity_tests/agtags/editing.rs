use expect_test::expect;

use super::ParityBatchCase;

/// The user adds `parser_flush' to parser.c and saves.  `agtags-mode' puts
/// `agtags--auto-update' on `before-save-hook' so GNU GLOBAL re-indexes the
/// saved file, and the visible half of that works: the tag history and the
/// completion cache are dropped on every save.  The indexing does not.
///
/// `agtags--auto-update' builds its argument inside `with-temp-buffer', where
/// `buffer-file-name' is nil, so what GNU GLOBAL receives is a bare
/// `--single-update=' with no path — visible in the trace below.  Real GNU
/// GLOBAL 6.6.14 answers that with exit 1 and "gtags: path '<cwd>' is out of
/// the project." and changes nothing, so the database never learns about the
/// edit: saving twice does not help, `parser_flush' completes to nothing, and
/// a tag search for it reports no matches.
///
/// The last step is the baseline for correct: the same GNU GLOBAL, given the
/// update agtags meant to send, does index the file, and `parser_flush' then
/// completes, is found by a tag search, and appears as a fourth reference to
/// `parser_reset'.  Without that contrast "nothing was found" would read as
/// simply how the package works.  This is upstream behaviour and identical in
/// both editors.
fn saving_an_edited_source_clears_the_cache_but_never_updates_the_database() -> ParityBatchCase {
    ParityBatchCase::value(
        "saving_an_edited_source_clears_the_cache_but_never_updates_the_database",
        r####"
(let* ((start (neomacs-agtags-test-start "agtags-editing-workflow"))
       (root (car start))
       (tools (cdr start))
       (default-directory root)
       (source (expand-file-name "src/parser.c" root))
       result)
  (unwind-protect
      (progn
        (neomacs-agtags-test-build-database root)
        (let ((buffer (neomacs-agtags-test-visit source)))
          (switch-to-buffer buffer)
          (agtags-mode 1)
          (let* ((backend (run-hook-with-args-until-success 'xref-backend-functions))
                 (table agtags--completion-table)
                 (look (lambda (prefix) (copy-sequence (all-completions prefix table))))
                 (references
                  (lambda ()
                    (mapcar (lambda (item)
                              (neomacs-agtags-test-where
                               (xref-location-marker (xref-item-location item))
                               root))
                            (xref-backend-references backend "parser_reset"))))
                 (search
                  (lambda ()
                    (cl-letf (((symbol-function 'completing-read)
                               (lambda (&rest _arguments) "parser_flush")))
                      (with-current-buffer buffer (agtags-find-tag)))
                    (neomacs-agtags-test-result-text
                     (neomacs-agtags-test-wait-for-buffer (get-buffer "*agtags-grep*")))))
                 (primed (funcall look "parser_"))
                 (before (progn
                           (setq agtags--history-list '("parser_reset"))
                           (list agtags--history-list
                                 (copy-tree agtags--global-to-list-cache)))))
            (with-current-buffer buffer
              (goto-char (point-max))
              (insert neomacs-agtags-test-flush-text)
              (save-buffer))
            (let* ((after-first-save
                    (list agtags--history-list
                          agtags--global-to-list-cache
                          (buffer-modified-p buffer)
                          (funcall look "parser_f")))
                   (first-search (funcall search)))
              (with-current-buffer buffer
                (goto-char (point-max))
                (insert neomacs-agtags-test-audit-text)
                (save-buffer))
              (let* ((after-second-save (funcall look "parser_f"))
                     (second-search (funcall search))
                     (still-stale (funcall references))
                     ;; What agtags meant to run: the same tool, the same
                     ;; working directory, with the file name it dropped.
                     (repaired
                      (let ((default-directory root))
                        (with-temp-buffer
                          (cd root)
                          (call-process "global" nil nil nil
                                        "-u" (concat "--single-update=" source)))))
                     (after-repair (funcall look "parser_f"))
                     (repaired-search (funcall search))
                     (grown (funcall references)))
                (setq result
                      (list primed
                            before
                            after-first-save
                            first-search
                            after-second-save
                            second-search
                            (list (length still-stale) still-stale)
                            repaired
                            after-repair
                            (funcall look "parse")
                            repaired-search
                            (list (length grown) grown)
                            (neomacs-agtags-test-file-string source)
                            (neomacs-agtags-test-trace tools))))))))
    (neomacs-agtags-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (("parser_init" "parser_reset") (("parser_reset") ("[ORACLE-SANDBOX]/agtags-editing-workflow/$-c$parser_" "parser_init" "parser_reset")) (nil nil nil nil) "-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-editing-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep parser_flush\n\nGlobal Grep finished with no matches found at TIME\n" nil "-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-editing-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep parser_flush\n\nGlobal Grep finished with no matches found at TIME\n" (3 (("include/parser.h" 4 0 "int parser_reset(int state);") ("src/main.c" 11 0 "  return parser_reset(input);") ("src/parser.c" 18 0 "  return parser_reset(state - 1);"))) 0 ("parser_flush") ("parse_request" "parser_flush" "parser_init" "parser_reset") "-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-editing-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep parser_flush\nsrc/parser.c:21:int parser_flush(int state) {\n\nGlobal Grep finished with matches found at TIME\n" (4 (("include/parser.h" 4 0 "int parser_reset(int state);") ("src/main.c" 11 0 "  return parser_reset(input);") ("src/parser.c" 18 0 "  return parser_reset(state - 1);") ("src/parser.c" 22 0 "  return parser_reset(state);"))) "#include \"parser.h\"\n\nstatic int log_line(int value) {\n  return value;\n}\n\nint parser_init(int seed) {\n  return log_line(seed);\n}\n\nint parser_reset(int state) {\n  /* 状態をリセットする */\n  if (state < 0) return 0;\n  return state + 1;\n}\n\nint parse_request(int state) {\n  return parser_reset(state - 1);\n}\n\nint parser_flush(int state) {\n  return parser_reset(state);\n}\n\n/* TODO: audit the flush path. */\n" "gtags cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-i>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-c> <parser_>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-u> <--single-update=>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-c> <parser_f>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <--result=grep> <parser_flush>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-u> <--single-update=>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-c> <parser_f>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <--result=grep> <parser_flush>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-r> <-x> <-a> <parser_reset>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-u> <--single-update=[ORACLE-SANDBOX]/agtags-editing-workflow/src/parser.c>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-c> <parser_f>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <--result=grep> <parser_flush>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-r> <-x> <-a> <parser_reset>\nglobal cwd=[ORACLE-SANDBOX]/agtags-editing-workflow <-c> <parse>\n")"##
        ]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![saving_an_edited_source_clears_the_cache_but_never_updates_the_database()]
}
