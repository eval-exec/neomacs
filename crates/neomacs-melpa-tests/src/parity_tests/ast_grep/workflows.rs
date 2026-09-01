use expect_test::expect;

use super::ParityBatchCase;

/// Searching a real project through the documented `ast-grep-search' command.
///
/// The corpus this sits beside asserts the search pipeline by redefining
/// `ast-grep--run-search' and friends, which cannot check the argument vector
/// the tool is actually given, nor that the package can parse what the tool
/// actually emits.  Here nothing is redefined at all:
/// everything runs: real command construction, real `call-process',
/// and the real NDJSON of ast-grep 0.40.0 replayed byte for byte.
///
/// The pipeline is entered at `ast-grep--run-command' rather than at
/// `ast-grep-search', because the public command reads its pattern from the
/// minibuffer and then selects through `completing-read' -- two minibuffer
/// interactions, which catalogue entry 1 makes unusable in Neomacs batch.
/// Entering one level down keeps the boundary this is about (command
/// construction, the subprocess, the parser) entirely real and puts the known
/// divergence out of reach rather than re-witnessing it.
///
/// That JSON shape is the point.  Each line carries `text', a `range' with both
/// a `byteOffset' and zero-based line/column pairs, the absolute `file', the
/// whole source `lines', a `charCount' and the detected `language'.  A parser
/// tested against hand-written JSON is tested against whatever its author
/// believed those field names to be.
///
/// Both files in the project match, and one of them matches three times at
/// different columns, so a parser that dropped duplicates, lost the column, or
/// stopped at the first file could not produce this list.
///
/// The nil end-line and end-column are the search parser's own shape, not a
/// gap: `ast-grep--parse-stream-line' records only the start of each match
/// even though the JSON carries `range.end'.  The rewrite workflow, which uses
/// the other parser, shows the end positions populated from the same field.
fn searching_a_real_project_parses_ast_greps_own_json_stream_into_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "searching_a_real_project_parses_ast_greps_own_json_stream_into_candidates",
        r##"(let* ((project (ast-grep-test-project))
       (records (ast-grep-test-install project))
       (default-directory project))
  (let ((candidates
         (ast-grep--parse-stream-output
          (ast-grep--run-command "console.log" project))))
    (list :records records
          :count (length candidates)
          :matches (mapcar #'ast-grep-test-match-summary candidates)
          :calls (ast-grep-test-calls-made)
          :unrecorded (ast-grep-test-unrecorded))))"##,
        expect![[
            r#"OK (:records 3 :count 4 :matches (("[ORACLE-SANDBOX]/proj/src/other.js" 0 0 nil nil "console.log" nil) ("[ORACLE-SANDBOX]/proj/src/app.js" 0 23 nil nil "console.log" nil) ("[ORACLE-SANDBOX]/proj/src/app.js" 1 0 nil nil "console.log" nil) ("[ORACLE-SANDBOX]/proj/src/app.js" 3 2 nil nil "console.log" nil)) :calls ("run|--pattern=console.log|--json=stream|<project>/|") :unrecorded nil)"#
        ]],
    )
}

fn rewriting_asks_the_tool_for_replacements_and_pairs_them_with_each_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "rewriting_asks_the_tool_for_replacements_and_pairs_them_with_each_match",
        r##"(let* ((project (ast-grep-test-project))
       (records (ast-grep-test-install project))
       (default-directory project))
  (let ((matches (ast-grep--collect-rewrites "console.log" "logger.info" project)))
    (list :count (length matches)
          :matches
          (mapcar (lambda (m)
                    (list (file-name-nondirectory (plist-get m :file))
                          (plist-get m :start-line) (plist-get m :start-column)
                          (plist-get m :end-line) (plist-get m :end-column)
                          (plist-get m :text) (plist-get m :replacement)))
                  matches)
          :calls (ast-grep-test-calls-made)
          :unrecorded (ast-grep-test-unrecorded))))"##,
        expect![[
            r#"OK (:count 4 :matches (("app.js" 0 23 0 34 "console.log" "logger.info") ("app.js" 1 0 1 11 "console.log" "logger.info") ("app.js" 3 2 3 13 "console.log" "logger.info") ("other.js" 0 0 0 11 "console.log" "logger.info")) :calls ("run|--pattern=console.log|--rewrite=logger.info|--json=stream|<project>/|") :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

fn the_outline_feature_calls_a_subcommand_ast_grep_does_not_have() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_outline_feature_calls_a_subcommand_ast_grep_does_not_have",
        r##"(let* ((project (ast-grep-test-project))
       (records (ast-grep-test-install project))
       (file (expand-file-name "src/app.js" project))
       (default-directory project))
  (list :command (ast-grep--build-outline-command file)
        :outcome (ast-grep-test-error-data
                  (lambda () (ast-grep--run-outline file)))
        :calls (ast-grep-test-calls-made)
        :unrecorded (ast-grep-test-unrecorded)))"##,
        expect![[
            r#"OK (:command ("ast-grep" "outline" "--json=stream" "[ORACLE-SANDBOX]/proj/src/app.js") :outcome (:error error ("The ast-grep failed with exit code 2: error: unrecognized subcommand 'outline'\n\nUsage: ast-grep [OPTIONS] <COMMAND>\n\nFor more information, try '--help'.")) :calls ("outline|--json=stream|<project>/src/app.js|") :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        searching_a_real_project_parses_ast_greps_own_json_stream_into_candidates(),
        rewriting_asks_the_tool_for_replacements_and_pairs_them_with_each_match(),
        the_outline_feature_calls_a_subcommand_ast_grep_does_not_have(),
    ]
}
