use expect_test::expect;

use super::ParityBatchCase;

/// The everyday loop: point sits on `parser_reset' in main.c, `agtags-find-tag'
/// offers it as the default, the hit list is rendered in `*agtags-grep*', and
/// RET on the match jumps to the definition in parser.c.  The same user then
/// searches for a symbol GNU GLOBAL does not know: real global 6.6.14 exits 0
/// with no output, so the result buffer reports no matches rather than an
/// error, and the previous hit list is gone.
fn agtags_find_tag_lists_the_definition_jumps_to_it_and_reports_an_unknown_tag() -> ParityBatchCase
{
    ParityBatchCase::value(
        "agtags_find_tag_lists_the_definition_jumps_to_it_and_reports_an_unknown_tag",
        r####"
(let* ((start (neomacs-agtags-test-start "agtags-find-tag-workflow"))
       (root (car start))
       (tools (cdr start))
       (default-directory root)
       prompts
       result)
  (unwind-protect
      (progn
        (neomacs-agtags-test-build-database root)
        (let ((buffer (neomacs-agtags-test-visit (expand-file-name "src/main.c" root))))
          (with-current-buffer buffer
            (agtags-mode 1)
            (goto-char (point-min))
            (search-forward "return parser_reset")
            (backward-char 3))
          (cl-letf (((symbol-function 'completing-read)
                     (lambda (prompt &rest _arguments)
                       (push (copy-sequence prompt) prompts)
                       "parser_reset")))
            (with-current-buffer buffer
              (agtags-find-tag)))
          (let* ((grep (neomacs-agtags-test-wait-for-buffer (get-buffer "*agtags-grep*")))
                 (listed (list (buffer-name grep)
                               (with-current-buffer grep (list major-mode mode-name))
                               (neomacs-agtags-test-result-text grep))))
            (switch-to-buffer grep)
            (goto-char (point-min))
            (search-forward "src/parser.c:11:")
            (beginning-of-line)
            (compile-goto-error)
            (let ((jumped (list (neomacs-agtags-test-here root)
                                (buffer-name (current-buffer))
                                major-mode)))
              (cl-letf (((symbol-function 'completing-read)
                         (lambda (prompt &rest _arguments)
                           (push (copy-sequence prompt) prompts)
                           "zzz_absent")))
                (agtags-find-tag))
              (let* ((again (neomacs-agtags-test-wait-for-buffer (get-buffer "*agtags-grep*")))
                     (empty (neomacs-agtags-test-result-text again)))
                (setq result
                      (list listed
                            jumped
                            empty
                            (nreverse prompts)
                            (neomacs-agtags-test-trace tools))))))))
    (neomacs-agtags-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (("*agtags-grep*" (agtags-grep-mode "Global Grep") "-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-find-tag-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep parser_reset\nsrc/parser.c:11:int parser_reset(int state) {\n\nGlobal Grep finished with matches found at TIME\n") (("src/parser.c" 11 0 "int parser_reset(int state) {") "parser.c" c-mode) "-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-find-tag-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep zzz_absent\n\nGlobal Grep finished with no matches found at TIME\n" ("Find tag (default parser_reset): " "Find tag (default int): ") "gtags cwd=[ORACLE-SANDBOX]/agtags-find-tag-workflow <-i>\nglobal cwd=[ORACLE-SANDBOX]/agtags-find-tag-workflow <--result=grep> <parser_reset>\nglobal cwd=[ORACLE-SANDBOX]/agtags-find-tag-workflow <--result=grep> <zzz_absent>\n")"#
        ]],
    )
}

fn agtags_find_file_opens_a_listed_file_and_the_next_search_retires_the_list() -> ParityBatchCase {
    ParityBatchCase::value(
        "agtags_find_file_opens_a_listed_file_and_the_next_search_retires_the_list",
        r####"
(let* ((start (neomacs-agtags-test-start "agtags-find-file-workflow"))
       (root (car start))
       (tools (cdr start))
       (default-directory root)
       prompts
       result)
  (unwind-protect
      (progn
        (neomacs-agtags-test-build-database root)
        (let ((buffer (neomacs-agtags-test-visit (expand-file-name "docs/notes.txt" root))))
          (with-current-buffer buffer
            (agtags-mode 1))
          (cl-letf (((symbol-function 'read-from-minibuffer)
                     (lambda (prompt &rest _arguments)
                       (push (copy-sequence prompt) prompts)
                       "\\.c$")))
            (with-current-buffer buffer
              (agtags-find-file)))
          (let* ((paths (neomacs-agtags-test-wait-for-buffer (get-buffer "*agtags-path*")))
                 (listed (list (buffer-name paths)
                               (with-current-buffer paths (list major-mode mode-name))
                               (neomacs-agtags-test-result-text paths))))
            (switch-to-buffer paths)
            (goto-char (point-min))
            (search-forward "src/parser.c")
            (beginning-of-line)
            (compile-goto-error)
            (let ((opened (list (neomacs-agtags-test-here root) major-mode)))
              (cl-letf (((symbol-function 'completing-read)
                         (lambda (prompt &rest _arguments)
                           (push (copy-sequence prompt) prompts)
                           "parser_reset")))
                (agtags-find-tag))
              (let* ((grep (neomacs-agtags-test-wait-for-buffer (get-buffer "*agtags-grep*")))
                     (searched (neomacs-agtags-test-result-text grep))
                     (survivors (mapcar (lambda (name) (and (get-buffer name) t))
                                        '("*agtags-grep*" "*agtags-path*"))))
                (agtags-switch-dwim)
                (setq result
                      (list listed
                            opened
                            searched
                            survivors
                            (buffer-name (current-buffer))
                            (nreverse prompts)
                            (neomacs-agtags-test-trace tools))))))))
    (neomacs-agtags-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (("*agtags-path*" (agtags-path-mode "Global Files") "-*- mode: agtags-path; default-directory: \"[ORACLE-SANDBOX]/agtags-find-file-workflow/\" -*-\nGlobal Files started at TIME\n\nglobal --result=path -P \\\\.c\\$\nsrc/main.c\nsrc/parser.c\n\nGlobal Files finished at TIME\n") (("src/parser.c" 1 0 "#include \"parser.h\"") c-mode) "-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-find-file-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep parser_reset\nsrc/parser.c:11:int parser_reset(int state) {\n\nGlobal Grep finished with matches found at TIME\n" (t nil) "*agtags-grep*" ("Find files: " "Find tag: ") "gtags cwd=[ORACLE-SANDBOX]/agtags-find-file-workflow <-i>\nglobal cwd=[ORACLE-SANDBOX]/agtags-find-file-workflow <--result=path> <-P> <\\.c$>\nglobal cwd=[ORACLE-SANDBOX]/agtags-find-file-workflow <--result=grep> <parser_reset>\n")"##
        ]],
    )
}

fn agtags_search_options_add_global_flags_and_change_which_lines_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "agtags_search_options_add_global_flags_and_change_which_lines_match",
        r####"
(let* ((start (neomacs-agtags-test-start "agtags-options-workflow"))
       (root (car start))
       (tools (cdr start))
       (default-directory root)
       result)
  (unwind-protect
      (progn
        (neomacs-agtags-test-build-database root)
        (let ((buffer (neomacs-agtags-test-visit (expand-file-name "src/main.c" root))))
          (with-current-buffer buffer
            (agtags-mode 1)
            (goto-char (point-min)))
          (cl-letf (((symbol-function 'read-from-minibuffer)
                     (lambda (&rest _arguments) "parser_reset"))
                    ((symbol-function 'completing-read)
                     (lambda (&rest _arguments) "parser_reset")))
            (let* ((source-only
                    (progn
                      (with-current-buffer buffer (agtags-find-with-pattern))
                      (neomacs-agtags-test-result-text
                       (neomacs-agtags-test-wait-for-buffer (get-buffer "*agtags-grep*")))))
                   (with-text
                    (let ((agtags-global-treat-text t))
                      (with-current-buffer buffer (agtags-find-with-pattern))
                      (neomacs-agtags-test-result-text
                       (neomacs-agtags-test-wait-for-buffer (get-buffer "*agtags-grep*")))))
                   (ignoring-case
                    (let ((agtags-global-ignore-case t))
                      (with-current-buffer buffer (agtags-find-tag))
                      (neomacs-agtags-test-result-text
                       (neomacs-agtags-test-wait-for-buffer (get-buffer "*agtags-grep*"))))))
              (cl-letf (((symbol-function 'read-from-minibuffer)
                         (lambda (&rest _arguments) "状態")))
                (with-current-buffer buffer (agtags-find-with-pattern)))
              (let ((japanese
                     (neomacs-agtags-test-result-text
                      (neomacs-agtags-test-wait-for-buffer (get-buffer "*agtags-grep*")))))
                (setq result
                      (list source-only
                            with-text
                            ignoring-case
                            japanese
                            (neomacs-agtags-test-trace tools))))))))
    (neomacs-agtags-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK ("-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-options-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep -g parser_reset\ninclude/parser.h:4:int parser_reset(int state);\nsrc/main.c:11:  return parser_reset(input);\nsrc/parser.c:11:int parser_reset(int state) {\nsrc/parser.c:18:  return parser_reset(state - 1);\n\nGlobal Grep finished with matches found at TIME\n" "-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-options-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep -o -g parser_reset\ndocs/notes.txt:4:parser_reset returns the next state.\ninclude/parser.h:4:int parser_reset(int state);\nsrc/main.c:11:  return parser_reset(input);\nsrc/parser.c:11:int parser_reset(int state) {\nsrc/parser.c:18:  return parser_reset(state - 1);\n\nGlobal Grep finished with matches found at TIME\n" "-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-options-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep -i parser_reset\nsrc/parser.c:11:int parser_reset(int state) {\n\nGlobal Grep finished with matches found at TIME\n" "-*- mode: agtags-grep; default-directory: \"[ORACLE-SANDBOX]/agtags-options-workflow/\" -*-\nGlobal Grep started at TIME\n\nglobal --result=grep -g \\状\\態\nsrc/parser.c:12:  /* 状態をリセットする */\n\nGlobal Grep finished with matches found at TIME\n" "gtags cwd=[ORACLE-SANDBOX]/agtags-options-workflow <-i>\nglobal cwd=[ORACLE-SANDBOX]/agtags-options-workflow <--result=grep> <-g> <parser_reset>\nglobal cwd=[ORACLE-SANDBOX]/agtags-options-workflow <--result=grep> <-o> <-g> <parser_reset>\nglobal cwd=[ORACLE-SANDBOX]/agtags-options-workflow <--result=grep> <-i> <parser_reset>\nglobal cwd=[ORACLE-SANDBOX]/agtags-options-workflow <--result=grep> <-g> <状態>\n")"#
        ]],
    )
}

pub(super) fn search_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        agtags_find_tag_lists_the_definition_jumps_to_it_and_reports_an_unknown_tag(),
        agtags_find_file_opens_a_listed_file_and_the_next_search_retires_the_list(),
        agtags_search_options_add_global_flags_and_change_which_lines_match(),
    ]
}
