use expect_test::expect;

use super::{ParityBatchCase, isolation_audit_cases, snapshot_editor_outputs};

#[test]
fn batch_cases_accept_forms_built_at_runtime() {
    let form = format!("(+ {} {})", 20, 22);
    let case = ParityBatchCase::value("dynamic-form", form, expect!["OK 42"]);

    assert_eq!(case.probe.as_ref(), "(+ 20 22)");
}

#[test]
fn snapshot_updates_use_gnu_emacs_as_the_single_source_of_truth() {
    assert_eq!(
        snapshot_editor_outputs("gnu", "neomacs", true),
        [("GNU Emacs", "gnu")]
    );
    assert_eq!(
        snapshot_editor_outputs("gnu", "neomacs", false),
        [("GNU Emacs", "gnu"), ("Neomacs", "neomacs")]
    );
}

#[test]
fn isolation_audit_includes_batchable_quarantines_but_not_setup_outcomes() {
    let cases = [
        ParityBatchCase::value("shared", "1", expect!["OK 1"]),
        ParityBatchCase::value("fresh", "2", expect!["OK 2"]).fresh_process(),
        ParityBatchCase::signal(
            "setup-signal",
            "3",
            expect![[r#"ERR (void-function dependency)"#]],
        )
        .setup_outcome(),
        ParityBatchCase::value("command-loop", "4", expect!["OK 4"]).direct_command_loop(),
    ];

    assert_eq!(
        isolation_audit_cases(&cases)
            .into_iter()
            .map(|case| case.id)
            .collect::<Vec<_>>(),
        ["shared", "fresh"]
    );
}
