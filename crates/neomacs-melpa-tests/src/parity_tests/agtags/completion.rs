use expect_test::expect;

use super::ParityBatchCase;

/// Mid-edit completion: the user is typing a call, asks for the tags starting
/// with `parser_', and agtags answers from GNU GLOBAL with a " Gtags"
/// annotation.  Asking the same question again must not run GNU GLOBAL a
/// second time — agtags caches one query — while a different prefix must.  The
/// trace is the witness: it holds one `-c parser_' line and one `-c parse'
/// line, so a broken cache shows up as an extra invocation.
fn agtags_completion_at_point_offers_global_tags_and_runs_global_once_per_prefix() -> ParityBatchCase
{
    ParityBatchCase::value(
        "agtags_completion_at_point_offers_global_tags_and_runs_global_once_per_prefix",
        r####"
(let* ((start (neomacs-agtags-test-start "agtags-completion-workflow"))
       (root (car start))
       (tools (cdr start))
       (default-directory root)
       result)
  (unwind-protect
      (progn
        (neomacs-agtags-test-build-database root)
        (let ((buffer (neomacs-agtags-test-visit (expand-file-name "src/parser.c" root))))
          (switch-to-buffer buffer)
          (agtags-mode 1)
          (goto-char (point-max))
          (insert "\nint parse_retry(int state) {\n  return parser_")
          (let* ((capf (run-hook-with-args-until-success 'completion-at-point-functions))
                 (prefix (buffer-substring-no-properties (nth 0 capf) (nth 1 capf)))
                 (table (nth 2 capf))
                 (annotate (plist-get (nthcdr 3 capf) :annotation-function))
                 (first (all-completions prefix table))
                 (first-cache (copy-tree agtags--global-to-list-cache))
                 (repeat (all-completions prefix table))
                 (repeat-cache (copy-tree agtags--global-to-list-cache))
                 (shorter (all-completions "parse" table))
                 (shorter-cache (copy-tree agtags--global-to-list-cache))
                 (nothing (all-completions "parser_f" table)))
            (setq result
                  (list prefix
                        (list first (mapcar annotate first))
                        (plist-get (nthcdr 3 capf) :exclusive)
                        first-cache
                        repeat
                        (equal first-cache repeat-cache)
                        shorter
                        shorter-cache
                        nothing
                        (neomacs-agtags-test-trace tools))))))
    (neomacs-agtags-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK ("parser_" (("parser_init" "parser_reset") (" Gtags" " Gtags")) no ("[ORACLE-SANDBOX]/agtags-completion-workflow/$-c$parser_" "parser_init" "parser_reset") ("parser_init" "parser_reset") t ("parse_request" "parser_init" "parser_reset") ("[ORACLE-SANDBOX]/agtags-completion-workflow/$-c$parse" "parse_request" "parser_init" "parser_reset") nil "gtags cwd=[ORACLE-SANDBOX]/agtags-completion-workflow <-i>\nglobal cwd=[ORACLE-SANDBOX]/agtags-completion-workflow <-c> <parser_>\nglobal cwd=[ORACLE-SANDBOX]/agtags-completion-workflow <-c> <parse>\nglobal cwd=[ORACLE-SANDBOX]/agtags-completion-workflow <-c> <parser_f>\n")"#
        ]],
    )
}

fn agtags_open_file_follows_a_path_candidate_but_not_a_bare_base_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "agtags_open_file_follows_a_path_candidate_but_not_a_bare_base_name",
        r####"
(let* ((start (neomacs-agtags-test-start "agtags-open-file-workflow"))
       (root (car start))
       (tools (cdr start))
       (default-directory root)
       prompts
       result)
  (unwind-protect
      (progn
        (neomacs-agtags-test-build-database root)
        (let ((buffer (neomacs-agtags-test-visit (expand-file-name "src/parser.c" root))))
          (switch-to-buffer buffer)
          (agtags-mode 1)
          (let* ((open
                  (lambda (typed)
                    (let (offered)
                      (cl-letf (((symbol-function 'completing-read)
                                 (lambda (prompt collection &rest _arguments)
                                   (push (copy-sequence prompt) prompts)
                                   (setq offered
                                         (copy-sequence (all-completions typed collection)))
                                   (or (car offered) ""))))
                        (agtags-open-file))
                      (list offered
                            (file-relative-name (buffer-file-name) root)
                            (and (file-exists-p (buffer-file-name)) t)
                            (buffer-size)
                            (copy-sequence (substring-no-properties (buffer-string)))))))
                 (header (funcall open "inc"))
                 (base (funcall open "main")))
            (setq result
                  (list header
                        base
                        (nreverse prompts)
                        (neomacs-agtags-test-trace tools))))))
    (neomacs-agtags-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK ((("include/parser.h") "include/parser.h" t 100 "#pragma once\n\nint parser_init(int seed);\nint parser_reset(int state);\nint parse_request(int state);\n") (("main.c") "main.c" nil 0 "") ("Open file: " "Open file: ") "gtags cwd=[ORACLE-SANDBOX]/agtags-open-file-workflow <-i>\nglobal cwd=[ORACLE-SANDBOX]/agtags-open-file-workflow <-c> <-P> <inc>\nglobal cwd=[ORACLE-SANDBOX]/agtags-open-file-workflow <-c> <-P> <main>\n")"##
        ]],
    )
}

pub(super) fn completion_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        agtags_completion_at_point_offers_global_tags_and_runs_global_once_per_prefix(),
        agtags_open_file_follows_a_path_candidate_but_not_a_bare_base_name(),
    ]
}
