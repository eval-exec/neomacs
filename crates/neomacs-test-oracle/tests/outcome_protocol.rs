use neomacs_test_oracle::{
    BatchProbe, CapturedEvaluation, EvalOutcome, ExpectedOutcome, extract_marked_batch_protocol,
    extract_marked_outcome, validate_batch_case_id, wrap_elisp_batch_outcomes, wrap_elisp_outcome,
};

const MARKER: &str = "NEOMACS-TEST-OUTCOME:";
const BEGIN_MARKER: &str = "NEOMACS-TEST-BEGIN:";
const COMPLETE_MARKER: &str = "NEOMACS-TEST-COMPLETE:";

#[test]
fn parses_values_and_signals_from_editor_output() {
    assert_eq!(
        extract_marked_outcome("package noise\nNEOMACS-TEST-OUTCOME:OK (2 4 6 8)\n", MARKER)
            .unwrap(),
        EvalOutcome::Value("(2 4 6 8)".to_string())
    );
    assert_eq!(
        extract_marked_outcome(
            "NEOMACS-TEST-OUTCOME:ERR (wrong-type-argument numberp \"x\")\n",
            MARKER,
        )
        .unwrap(),
        EvalOutcome::Signal("(wrong-type-argument numberp \"x\")".to_string())
    );
}

#[test]
fn captured_evaluation_separates_ordinary_stdout_from_the_typed_outcome() {
    let evaluation = CapturedEvaluation::from_marked_streams(
        "Enter (default 42): Enter: ",
        "diagnostic noise\nNEOMACS-TEST-OUTCOME:OK (end-of-file end-of-file)\n",
        MARKER,
    )
    .unwrap();

    assert_eq!(evaluation.stdout(), "Enter (default 42): Enter: ");
    assert_eq!(
        evaluation.outcome(),
        &EvalOutcome::Value("(end-of-file end-of-file)".to_string())
    );
    assert_eq!(
        evaluation.legacy_transcript(),
        "Enter (default 42): Enter: OK (end-of-file end-of-file)"
    );
}

#[test]
fn expected_outcome_kind_matches_only_its_protocol_variant() {
    let value = EvalOutcome::Value("42".into());
    let signal = EvalOutcome::Signal("(error \"boom\")".into());

    assert!(ExpectedOutcome::Value.matches(&value));
    assert!(!ExpectedOutcome::Value.matches(&signal));
    assert!(ExpectedOutcome::Signal.matches(&signal));
    assert!(!ExpectedOutcome::Signal.matches(&value));
}

#[test]
fn rejects_missing_or_malformed_outcomes() {
    assert!(extract_marked_outcome("ordinary output", MARKER).is_err());
    assert!(extract_marked_outcome("NEOMACS-TEST-OUTCOME:MAYBE t", MARKER).is_err());
}

#[test]
fn elisp_wrapper_captures_the_last_value_and_errors() {
    let wrapper = wrap_elisp_outcome("(message \"setup\")", "(list 1 2 3)", MARKER);

    assert!(wrapper.contains(r##"(message "setup")"##));
    assert!(wrapper.contains("(list 1 2 3)"));
    assert!(wrapper.contains("(condition-case"));
    assert!(wrapper.contains("OK "));
    assert!(wrapper.contains("ERR "));
    assert!(wrapper.contains(MARKER));
    assert!(wrapper.contains("NEOMACS_TEST_SANDBOX_ROOT"));
    assert!(wrapper.contains("NEOMACS_TEST_WORKSPACE_ROOT"));
    assert!(wrapper.contains("neomacs--test-oracle-normalized"));
    assert!(wrapper.contains("(print-escape-newlines t)"));
    assert!(wrapper.contains("(print-escape-control-characters t)"));
}

#[test]
fn outcome_wrappers_terminate_every_record_with_a_newline() {
    let single = wrap_elisp_outcome("", "42", MARKER);
    assert_eq!(
        single
            .matches("(terpri 'external-debugging-output)")
            .count(),
        2
    );

    let batch = wrap_elisp_batch_outcomes(
        "",
        &[
            BatchProbe {
                id: "first",
                probe: "1",
            },
            BatchProbe {
                id: "second",
                probe: "2",
            },
        ],
        BEGIN_MARKER,
        COMPLETE_MARKER,
        MARKER,
    )
    .unwrap();
    assert_eq!(
        batch.matches("(terpri 'external-debugging-output)").count(),
        6
    );
}

#[test]
fn rejects_malformed_batch_records_even_when_all_expected_records_are_present() {
    let stream = "\
NEOMACS-TEST-BEGIN:alpha
NEOMACS-TEST-OUTCOME:alpha:OK 1
NEOMACS-TEST-COMPLETE:alpha
NEOMACS-TEST-BEGIN:beta
NEOMACS-TEST-OUTCOME:broken OK 2
NEOMACS-TEST-OUTCOME:beta:OK 3
NEOMACS-TEST-COMPLETE:beta
";

    let error =
        extract_marked_batch_protocol(stream, BEGIN_MARKER, MARKER, COMPLETE_MARKER).unwrap_err();
    assert!(
        error.contains("malformed batch outcome record"),
        "unexpected error: {error}"
    );
}

#[test]
fn batch_wrapper_runs_setup_once_and_names_each_probe() {
    let cases = [
        BatchProbe {
            id: "reads",
            probe: "(+ 1 2)",
        },
        BatchProbe {
            id: "writes",
            probe: "(list 'a 'b)",
        },
    ];
    let wrapper =
        wrap_elisp_batch_outcomes("(setq x 1)", &cases, BEGIN_MARKER, COMPLETE_MARKER, MARKER)
            .unwrap();
    assert!(wrapper.contains("(setq x 1)"));
    assert!(wrapper.contains("(+ 1 2)"));
    assert!(wrapper.contains("(list 'a 'b)"));
    assert!(wrapper.contains(r#""reads""#));
    assert!(wrapper.contains(r#""writes""#));
    // One shared setup progn, two per-case condition-case forms.
    assert_eq!(wrapper.matches("(condition-case").count(), 2);
    assert!(wrapper.contains("neomacs--test-oracle-normalized"));
    assert_eq!(wrapper.matches(BEGIN_MARKER).count(), 1);
    assert_eq!(wrapper.matches(COMPLETE_MARKER).count(), 1);
    assert!(wrapper.contains("(terpri 'external-debugging-output)"));
}

#[test]
fn strict_batch_protocol_requires_outcome_between_begin_and_complete() {
    let valid = "\
NEOMACS-TEST-BEGIN:first
NEOMACS-TEST-OUTCOME:first:OK 1
NEOMACS-TEST-COMPLETE:first
NEOMACS-TEST-BEGIN:second
NEOMACS-TEST-OUTCOME:second:ERR (user-error \"nope\")
NEOMACS-TEST-COMPLETE:second
";
    let protocol =
        extract_marked_batch_protocol(valid, BEGIN_MARKER, MARKER, COMPLETE_MARKER).unwrap();
    assert_eq!(protocol.case_ids, ["first", "second"]);
    assert_eq!(protocol.outcomes.len(), 2);
    assert_eq!(protocol.unfinished_case_id, None);

    let partial = extract_marked_batch_protocol(
        "NEOMACS-TEST-BEGIN:hanging\n",
        BEGIN_MARKER,
        MARKER,
        COMPLETE_MARKER,
    )
    .unwrap();
    assert_eq!(partial.case_ids, ["hanging"]);
    assert_eq!(partial.unfinished_case_id.as_deref(), Some("hanging"));

    for malformed in [
        "NEOMACS-TEST-BEGIN:bad id\n",
        "NEOMACS-TEST-OUTCOME:first:OK 1\n",
        "NEOMACS-TEST-BEGIN:first\nNEOMACS-TEST-COMPLETE:first\nNEOMACS-TEST-OUTCOME:first:OK 1\n",
        "NEOMACS-TEST-BEGIN:first\nNEOMACS-TEST-OUTCOME:second:OK 1\nNEOMACS-TEST-COMPLETE:first\n",
        "NEOMACS-TEST-BEGIN:first\nNEOMACS-TEST-OUTCOME:first:OK 1\nNEOMACS-TEST-OUTCOME:first:OK 2\nNEOMACS-TEST-COMPLETE:first\n",
    ] {
        assert!(
            extract_marked_batch_protocol(malformed, BEGIN_MARKER, MARKER, COMPLETE_MARKER)
                .is_err(),
            "accepted malformed protocol: {malformed}"
        );
    }
}

#[test]
fn strict_batch_protocol_ignores_marker_text_in_noise_and_outcome_values() {
    let stream = "\
ordinary noise mentions NEOMACS-TEST-BEGIN:not-a-record
NEOMACS-TEST-BEGIN:actual
NEOMACS-TEST-OUTCOME:actual:OK (\"NEOMACS-TEST-BEGIN:data\" \"NEOMACS-TEST-COMPLETE:data\")
more noise mentions NEOMACS-TEST-OUTCOME:not-a-record:ERR data
NEOMACS-TEST-COMPLETE:actual
";

    let protocol =
        extract_marked_batch_protocol(stream, BEGIN_MARKER, MARKER, COMPLETE_MARKER).unwrap();
    assert_eq!(protocol.case_ids, ["actual"]);
    assert_eq!(
        protocol.outcomes[0].outcome,
        EvalOutcome::Value(r#"("NEOMACS-TEST-BEGIN:data" "NEOMACS-TEST-COMPLETE:data")"#.into())
    );
    assert_eq!(protocol.unfinished_case_id, None);
}

#[test]
fn batch_case_ids_reject_colon_and_whitespace() {
    assert!(validate_batch_case_id("ok_id").is_ok());
    assert!(validate_batch_case_id("bad:id").is_err());
    assert!(validate_batch_case_id("bad id").is_err());
    assert!(validate_batch_case_id("").is_err());
}
