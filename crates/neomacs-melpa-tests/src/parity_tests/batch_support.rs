//! Shared multi-probe batch assertion helper for package parity suites.

use std::borrow::Cow;

use crate::{CachedMelpaOracle, OracleBatchCase, OracleBatchCaseReport, OracleBatchReport};
use expect_test::Expect;
use neomacs_test_oracle::ExpectedOutcome;

/// One named probe: Elisp form, required outcome kind, and expect-test snapshot.
///
/// Prefer building these from small case constructors (`fn opens_…() -> Self`)
/// so a package batch test stays a short list of names rather than a wall of
/// inline raw strings.
pub(crate) struct ParityBatchCase {
    pub id: &'static str,
    pub probe: Cow<'static, str>,
    pub expected_outcome: ExpectedOutcome,
    pub expected: Expect,
    execution: CaseExecution,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaseExecution {
    SharedProcess,
    FreshProcess,
    SetupOutcome,
    DirectCommandLoop,
}

impl ParityBatchCase {
    pub(crate) fn value(
        id: &'static str,
        probe: impl Into<Cow<'static, str>>,
        expected: Expect,
    ) -> Self {
        Self {
            id,
            probe: probe.into(),
            expected_outcome: ExpectedOutcome::Value,
            expected,
            execution: CaseExecution::SharedProcess,
        }
    }

    pub(crate) fn signal(
        id: &'static str,
        probe: impl Into<Cow<'static, str>>,
        expected: Expect,
    ) -> Self {
        Self {
            id,
            probe: probe.into(),
            expected_outcome: ExpectedOutcome::Signal,
            expected,
            execution: CaseExecution::SharedProcess,
        }
    }

    /// Run this case in its own GNU Emacs and Neomacs process pair.
    ///
    /// Use this only when the package cannot reliably restore global editor
    /// state after the probe. Other cases in the same suite remain batched.
    pub(crate) fn fresh_process(mut self) -> Self {
        self.execution = CaseExecution::FreshProcess;
        self
    }

    /// Catch an outcome raised by package setup rather than by the probe.
    ///
    /// Shared batches run package setup before their per-probe catchers. This
    /// mode uses the single-case wrapper so a deliberately expected setup
    /// signal remains observable without misclassifying it as state leakage.
    pub(crate) fn setup_outcome(mut self) -> Self {
        self.execution = CaseExecution::SetupOutcome;
        self
    }

    /// Run this case as a top-level editor script with a real command loop.
    ///
    /// This is reserved for workflows whose public behavior depends on
    /// `recursive-edit`, process sentinels, or other command-loop state that
    /// cannot be nested below the ordinary oracle transport.
    pub(crate) fn direct_command_loop(mut self) -> Self {
        self.execution = CaseExecution::DirectCommandLoop;
        self
    }
}

/// Same as [`assert_oracle_batch`], but takes structured [`ParityBatchCase`] values
/// (from per-case constructor functions).
pub(crate) fn assert_oracle_batch_cases(
    oracle: CachedMelpaOracle,
    batch_name: &str,
    package_label: &str,
    cases: &[ParityBatchCase],
) {
    let mut oracle_errors = if isolation_audit_enabled() {
        audit_batch_isolation(&oracle, batch_name, cases)
    } else {
        Vec::new()
    };
    let shared_cases = cases
        .iter()
        .filter(|case| case.execution == CaseExecution::SharedProcess)
        .collect::<Vec<_>>();
    let mut observed = Vec::with_capacity(cases.len());

    if !shared_cases.is_empty() {
        let batch = shared_cases
            .iter()
            .map(|case| OracleBatchCase {
                id: case.id,
                probe: case.probe.as_ref(),
                expected_outcome: case.expected_outcome,
            })
            .collect::<Vec<_>>();
        match oracle.run_batch(batch_name, &batch) {
            Ok(report) => {
                let OracleBatchReport {
                    cases: reports,
                    failures,
                } = report;
                assert_eq!(
                    reports.len(),
                    shared_cases.len(),
                    "{package_label} batch `{batch_name}` returned {} reports for {} cases",
                    reports.len(),
                    shared_cases.len()
                );
                for (observed_case, case) in reports.into_iter().zip(shared_cases) {
                    assert_eq!(
                        observed_case.id, case.id,
                        "{package_label} batch case order mismatch"
                    );
                    let batch_failures = failures_for_case(&failures, case.id);
                    observed.push((case, observed_case, batch_failures));
                }
            }
            Err(error) => oracle_errors.push(format!("shared batch failed:\n{error}")),
        }
    }

    for case in cases
        .iter()
        .filter(|case| case.execution != CaseExecution::SharedProcess)
    {
        let result = run_isolated_case(&oracle, case);
        match result {
            Ok(report) => {
                let OracleBatchReport {
                    cases: reports,
                    failures,
                } = report;
                let case_failures = failures_for_case(&failures, case.id);
                let observed_case = reports
                    .into_iter()
                    .next()
                    .expect("a one-case batch returns one report");
                observed.push((case, observed_case, case_failures));
            }
            Err(error) => oracle_errors.push(format!("case `{}` failed:\n{error}", case.id)),
        }
    }

    let mut snapshot_mismatches = Vec::new();
    for (case, report, case_failures) in observed {
        oracle_errors.extend(case_failures);
        let gnu_emacs = report.gnu_emacs.to_string();
        let neomacs = report.neomacs.to_string();
        for (editor, actual) in
            snapshot_editor_outputs(&gnu_emacs, &neomacs, std::env::var("UPDATE_EXPECT").is_ok())
        {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                case.expected.assert_eq(actual);
            }))
            .is_err()
            {
                snapshot_mismatches.push(format!("{} ({editor})", case.id));
            }
        }
    }

    if !oracle_errors.is_empty() || !snapshot_mismatches.is_empty() {
        let mut details = oracle_errors;
        if !snapshot_mismatches.is_empty() {
            details.push(format!(
                "snapshot mismatches: {}",
                snapshot_mismatches.join(", ")
            ));
        }
        panic!(
            "{package_label} batch `{batch_name}` failed:\n{}",
            details.join("\n")
        );
    }
}

/// GNU Emacs owns snapshot generation; ordinary verification still checks
/// both editors against that generated contract.
fn snapshot_editor_outputs<'a>(
    gnu_emacs: &'a str,
    neomacs: &'a str,
    updating: bool,
) -> Vec<(&'static str, &'a str)> {
    let mut outputs = vec![("GNU Emacs", gnu_emacs)];
    if !updating {
        outputs.push(("Neomacs", neomacs));
    }
    outputs
}

fn failures_for_case(failures: &[crate::OracleBatchFailure], id: &str) -> Vec<String> {
    failures
        .iter()
        .filter(|failure| failure.id() == id)
        .map(ToString::to_string)
        .collect()
}

fn run_isolated_case(
    oracle: &CachedMelpaOracle,
    case: &ParityBatchCase,
) -> Result<OracleBatchReport, String> {
    match case.execution {
        CaseExecution::DirectCommandLoop => oracle.run_direct_command_loop_probe(
            case.id,
            case.probe.as_ref(),
            case.expected_outcome,
        ),
        CaseExecution::SharedProcess
        | CaseExecution::FreshProcess
        | CaseExecution::SetupOutcome => {
            oracle.run_case(case.id, case.probe.as_ref(), case.expected_outcome)
        }
    }
}

fn audit_batch_isolation(
    oracle: &CachedMelpaOracle,
    batch_name: &str,
    cases: &[ParityBatchCase],
) -> Vec<String> {
    let audit_cases = isolation_audit_cases(cases);
    if audit_cases.is_empty() {
        return Vec::new();
    }
    let batch = audit_cases
        .iter()
        .map(|case| OracleBatchCase {
            id: case.id,
            probe: case.probe.as_ref(),
            expected_outcome: case.expected_outcome,
        })
        .collect::<Vec<_>>();
    let batched = match oracle.run_batch(&format!("{batch_name}-isolation-audit"), &batch) {
        Ok(report) => report.cases,
        Err(error) => return vec![format!("isolation audit batch failed:\n{error}")],
    };

    let mut errors = Vec::new();
    for (case, batched_case) in audit_cases.into_iter().zip(batched) {
        match run_isolated_case(oracle, case) {
            Ok(isolated) => {
                let isolated_case = isolated
                    .cases
                    .into_iter()
                    .next()
                    .expect("a one-case isolation audit returns one report");
                if !same_outcomes(&batched_case, &isolated_case) {
                    errors.push(format!(
                        "case `{}` is not batch-safe:\n  batched GNU Emacs: {}\n  isolated GNU Emacs: {}\n  batched Neomacs: {}\n  isolated Neomacs: {}",
                        case.id,
                        batched_case.gnu_emacs,
                        isolated_case.gnu_emacs,
                        batched_case.neomacs,
                        isolated_case.neomacs,
                    ));
                }
            }
            Err(error) => errors.push(format!(
                "case `{}` failed its isolation audit:\n{error}",
                case.id
            )),
        }
    }
    errors
}

fn isolation_audit_cases(cases: &[ParityBatchCase]) -> Vec<&ParityBatchCase> {
    cases
        .iter()
        .filter(|case| {
            matches!(
                case.execution,
                CaseExecution::SharedProcess | CaseExecution::FreshProcess
            )
        })
        .collect()
}

fn same_outcomes(left: &OracleBatchCaseReport, right: &OracleBatchCaseReport) -> bool {
    left.gnu_emacs == right.gnu_emacs && left.neomacs == right.neomacs
}

fn isolation_audit_enabled() -> bool {
    std::env::var_os("NEOMACS_MELPA_AUDIT_BATCH_ISOLATION").is_some()
}

#[cfg(test)]
mod tests {
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
}
