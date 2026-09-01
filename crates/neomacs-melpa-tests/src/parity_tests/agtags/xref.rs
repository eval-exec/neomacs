use expect_test::expect;

use super::ParityBatchCase;

/// `M-.' on a symbol with exactly one definition jumps straight there and
/// `M-,' comes back, through the real xref commands and agtags' xref backend.
/// The enumeration side is checked on `log_line', which the fixture defines
/// once per translation unit: a backend that stopped at the first hit would
/// answer one where GNU GLOBAL answers two.  `parser_reset' shows the other
/// half of GNU GLOBAL's model — the prototype in the header counts as a
/// reference, so references outnumber definitions three to one.
fn agtags_xref_jumps_to_a_unique_definition_and_enumerates_every_hit() -> ParityBatchCase {
    ParityBatchCase::value(
        "agtags_xref_jumps_to_a_unique_definition_and_enumerates_every_hit",
        r####"
(let* ((start (neomacs-agtags-test-start "agtags-xref-workflow"))
       (root (car start))
       (tools (cdr start))
       (default-directory root)
       result)
  (unwind-protect
      (progn
        (neomacs-agtags-test-build-database root)
        (let ((buffer (neomacs-agtags-test-visit (expand-file-name "src/main.c" root))))
          (switch-to-buffer buffer)
          (agtags-mode 1)
          (goto-char (point-min))
          (search-forward "return parser_reset")
          (backward-char 3)
          (let* ((backend (run-hook-with-args-until-success 'xref-backend-functions))
                 (identifier (xref-backend-identifier-at-point backend))
                 (started (neomacs-agtags-test-here root))
                 (describe
                  (lambda (item)
                    (list (copy-sequence (xref-item-summary item))
                          (neomacs-agtags-test-where
                           (xref-location-marker (xref-item-location item))
                           root))))
                 (log-definitions
                  (mapcar describe (xref-backend-definitions backend "log_line")))
                 (log-references
                  (mapcar describe (xref-backend-references backend "log_line")))
                 (reset-definitions
                  (mapcar describe (xref-backend-definitions backend "parser_reset")))
                 (reset-references
                  (mapcar describe (xref-backend-references backend "parser_reset")))
                 (unknown
                  (mapcar describe (xref-backend-definitions backend "zzz_absent"))))
            (xref-find-definitions "parser_reset")
            (let ((jumped (neomacs-agtags-test-here root)))
              (xref-go-back)
              (setq result
                    (list backend
                          identifier
                          started
                          (list (length log-definitions) log-definitions)
                          (list (length log-references) log-references)
                          (list (length reset-definitions) reset-definitions)
                          (list (length reset-references) reset-references)
                          unknown
                          jumped
                          (neomacs-agtags-test-here root)
                          (neomacs-agtags-test-trace tools)))))))
    (neomacs-agtags-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (agtags "parser_reset" ("src/main.c" 11 18 "  return parser_reset(input);") (2 (("static int log_line(int value) {" ("src/main.c" 3 0 "static int log_line(int value) {")) ("static int log_line(int value) {" ("src/parser.c" 3 0 "static int log_line(int value) {")))) (1 (("return log_line(seed);" ("src/parser.c" 8 0 "  return log_line(seed);")))) (1 (("int parser_reset(int state) {" ("src/parser.c" 11 0 "int parser_reset(int state) {")))) (3 (("int parser_reset(int state);" ("include/parser.h" 4 0 "int parser_reset(int state);")) ("return parser_reset(input);" ("src/main.c" 11 0 "  return parser_reset(input);")) ("return parser_reset(state - 1);" ("src/parser.c" 18 0 "  return parser_reset(state - 1);")))) nil ("src/parser.c" 11 0 "int parser_reset(int state) {") ("src/main.c" 11 18 "  return parser_reset(input);") "gtags cwd=[ORACLE-SANDBOX]/agtags-xref-workflow <-i>\nglobal cwd=[ORACLE-SANDBOX]/agtags-xref-workflow <-d> <-x> <-a> <log_line>\nglobal cwd=[ORACLE-SANDBOX]/agtags-xref-workflow <-r> <-x> <-a> <log_line>\nglobal cwd=[ORACLE-SANDBOX]/agtags-xref-workflow <-d> <-x> <-a> <parser_reset>\nglobal cwd=[ORACLE-SANDBOX]/agtags-xref-workflow <-r> <-x> <-a> <parser_reset>\nglobal cwd=[ORACLE-SANDBOX]/agtags-xref-workflow <-d> <-x> <-a> <zzz_absent>\nglobal cwd=[ORACLE-SANDBOX]/agtags-xref-workflow <-d> <-x> <-a> <parser_reset>\n")"#
        ]],
    )
}

pub(super) fn xref_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![agtags_xref_jumps_to_a_unique_definition_and_enumerates_every_hit()]
}
