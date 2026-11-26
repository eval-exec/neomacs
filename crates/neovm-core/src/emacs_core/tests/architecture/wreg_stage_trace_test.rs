//! W-REG R1: installed SYSTEM stage trace gate.
//!
//! Libval carry-forward: each stage registrar records `(crate, segment id,
//! row count)`.  This test compares that installed sequence with the crate's
//! stage table and reports a missing or extra segment directly by name,
//! crate and row count — before the frozen pre-move trace comparison, so an
//! omitted stage call in core's sequencing chain is not reported as a
//! generic name diff.

#[test]
fn installed_system_stage_trace_matches_the_stage_table() {
    use neovm_eval::emacs_core::subr::{
        reset_registered_subr_stage_trace_for_test, take_registered_subr_stage_trace_for_test,
    };
    use neovm_system::emacs_core::stages::startup_stages_for_test;

    let expected: Vec<(&'static str, usize)> = startup_stages_for_test()
        .iter()
        .map(|(_, name, batch)| (*name, batch.specs().len()))
        .collect();
    assert!(
        !expected.is_empty(),
        "the SYSTEM stage table must not be empty"
    );

    reset_registered_subr_stage_trace_for_test();
    let context = crate::test_utils::new_context();
    let actual = take_registered_subr_stage_trace_for_test();
    drop(context);

    let installed: Vec<(&'static str, usize)> = actual
        .iter()
        .map(|stage| {
            assert_eq!(
                stage.owner, "neovm-system",
                "the stage trace must name the owning crate"
            );
            (stage.segment, stage.rows)
        })
        .collect();

    // Direct diagnostic first: name the segment, crate and row count.
    for (index, expected_row) in expected.iter().enumerate() {
        let Some(installed_row) = installed.get(index) else {
            panic!(
                "missing SYSTEM stage {:?} (owner neovm-system, {} rows): core's \
                 sequencing chain did not install it",
                expected_row.0, expected_row.1
            );
        };
        assert_eq!(
            installed_row,
            expected_row,
            "SYSTEM stage mismatch at position {} (owner neovm-system): expected {} rows",
            index + 1,
            expected_row.1
        );
    }
    assert_eq!(
        installed.len(),
        expected.len(),
        "unexpected extra SYSTEM stages beyond the stage table: {:?}",
        &installed[expected.len()..]
    );
    assert_eq!(
        installed, expected,
        "installed SYSTEM stage trace diverges from the crate stage table"
    );
}

/// W-REG R1 (lead #6781 §5): the 19 closure rows are the only names whose
/// layer could have changed by moving a core registration into the SYSTEM
/// stage table.  Measurement: the upper-ledger layer is the descriptor's
/// `ABSENT-LAYER` marker, not the registration site, so the regenerated
/// upper snapshot and generated source are byte-identical and the layer does
/// NOT move.  This test itemizes exactly those 19 names and asserts they (and
/// every other name) keep their frozen layer/arity/cfg — any change other
/// than an itemized `core -> system` move is a failure.
#[test]
fn upper_ledger_layer_moves_are_exactly_the_closure_allowlist() {
    use std::collections::BTreeMap;

    const FROZEN: &str = include_str!("fixtures/wreg/W-REG-PRE-MOVE-NAMES.tsv");
    const UPPER: &str = include_str!("../../../../../neovm-eval/src/upper_subrs.snapshot.tsv");

    /// The 19 closure-registered SYSTEM rows (stages 3, 6, 13, 17).  Checked
    /// explicitly; none of them changed layer (descriptor metadata wins).
    const CLOSURE_ROWS: &[&str] = &[
        "neomacs-tls-available-p",
        "float-time",
        "file-name-concat",
        "file-name-absolute-p",
        "directory-name-p",
        "make-temp-name",
        "next-read-file-uses-dialog-p",
        "set-binary-mode",
        "default-file-modes",
        "set-default-file-modes",
        "time-add",
        "time-subtract",
        "time-less-p",
        "time-equal-p",
        "current-time-string",
        "current-time-zone",
        "encode-time",
        "decode-time",
        "set-time-zone-rule",
    ];

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Row {
        min: u16,
        max: Option<u16>,
        layer: String,
        cfg: String,
    }

    fn parse(text: &str) -> BTreeMap<String, Row> {
        let mut rows = BTreeMap::new();
        for line in text.lines().skip(1).filter(|line| !line.is_empty()) {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 5, "malformed ledger row: {line}");
            rows.insert(
                fields[0].to_owned(),
                Row {
                    min: fields[1].parse().expect("ledger min"),
                    max: if fields[2] == "-" {
                        None
                    } else {
                        Some(fields[2].parse().expect("ledger max"))
                    },
                    layer: fields[3].to_owned(),
                    cfg: fields[4].to_owned(),
                },
            );
        }
        rows
    }

    let frozen = parse(FROZEN);
    let upper = parse(UPPER);

    for (name, frozen_row) in &frozen {
        let Some(upper_row) = upper.get(name) else {
            // eval-layer names are intentionally absent from the upper ledger.
            assert_eq!(
                frozen_row.layer, "eval",
                "{name}: an upper name vanished from the upper ledger"
            );
            continue;
        };
        assert_eq!(
            upper_row, frozen_row,
            "{name}: W-REG R1 must not change the upper-ledger row \
             (the only permitted delta, a core->system layer move for the 19 \
             closure rows, did not occur: the layer is descriptor metadata)"
        );
    }

    for name in CLOSURE_ROWS {
        let frozen_row = frozen
            .get(*name)
            .unwrap_or_else(|| panic!("{name}: closure row missing from the frozen ledger"));
        assert_ne!(
            frozen_row.layer, "system",
            "{name}: closure row was already system-layer; the list is stale"
        );
        let upper_row = upper
            .get(*name)
            .unwrap_or_else(|| panic!("{name}: closure row missing from the upper ledger"));
        assert_eq!(
            upper_row.layer, frozen_row.layer,
            "{name}: unexpected layer move for a moved closure row"
        );
    }
}
