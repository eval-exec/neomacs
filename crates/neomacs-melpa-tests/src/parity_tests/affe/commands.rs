use expect_test::expect;

use super::ParityBatchCase;

fn affe_command_appends_paths_when_command_has_no_placeholder() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_command_appends_paths_when_command_has_no_placeholder",
        r##"(list
               (affe--command
                "rg --color=never --files"
                '("src" "docs/space name"))
               (affe--command "find -type f" nil)
               (affe--command "printf \"%s\\n\""
                              '("α" "β")))"##,
        expect![[
            r#"OK (("rg" "--color=never" "--files" "src" "docs/space name") ("find" "-type" "f") ("printf" "%s\n" "α" "β"))"#
        ]],
    )
}

fn affe_command_expands_every_dot_placeholder_and_preserves_quoted_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_command_expands_every_dot_placeholder_and_preserves_quoted_arguments",
        r##"(list
               (affe--command
                "rg --glob \"space name\" . --and ."
                '("one" "two words"))
               (affe--command "."
                              '("a" "b"))
               (affe--command
                "tool --literal=./child ."
                '("root")))"##,
        expect![[
            r#"OK (("rg" "--glob" "space name" "one" "two words" "--and" "one" "two words") ("a" "b") ("tool" "--literal=./child" "root"))"#
        ]],
    )
}

fn affe_command_reports_split_errors_and_boundary_argument_types() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_command_reports_split_errors_and_boundary_argument_types",
        r##"(mapcar
               (lambda (case)
                 (condition-case error-data
                     (apply #'affe--command case)
                   (error
                    (list 'signal
                          (car error-data)
                          (cdr error-data)))))
               '(("unterminated \"quote" ("root"))
                 (nil ("root"))
                 ("rg ." nil)))"##,
        expect![[
            r#"OK ((signal end-of-file nil) (signal wrong-type-argument (stringp nil)) ("rg"))"#
        ]],
    )
}

pub(super) fn commands_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        affe_command_appends_paths_when_command_has_no_placeholder(),
        affe_command_expands_every_dot_placeholder_and_preserves_quoted_arguments(),
        affe_command_reports_split_errors_and_boundary_argument_types(),
    ]
}
