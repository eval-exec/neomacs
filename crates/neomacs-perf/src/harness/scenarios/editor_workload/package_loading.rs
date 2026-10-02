//! Package file selection for shared editor-workload scenarios.

use neomacs_melpa_test_support::LoadSuffixes;

use crate::ScenarioId;

/// Which of a package's files the performance scenarios load.
///
/// The board's package scenarios have always forced `load-suffixes '(".el")`,
/// inherited from the MELPA parity tests, so magit and org-journal ran their
/// packages interpreted -- a configuration no user runs.  Setting
/// `NEOMACS_PERF_LOAD_COMPILED=1` measures the byte-compiled files instead.
/// The default is unchanged so the published series stays comparable.
pub(crate) fn scenario_load_suffixes(scenario: ScenarioId) -> LoadSuffixes {
    // The `-compiled` rows exist precisely to load byte-code, so the choice is
    // part of their identity rather than an ambient setting.
    if matches!(
        scenario,
        ScenarioId::MagitStatusCompiled
            | ScenarioId::OrgJournalOpenCompiled
            | ScenarioId::MagitStatusHeavy
    ) {
        return LoadSuffixes::EmacsDefault;
    }
    // The escape hatch stays for measuring an existing row both ways without
    // adding a scenario.
    match std::env::var("NEOMACS_PERF_LOAD_COMPILED").as_deref() {
        Ok("1") => LoadSuffixes::EmacsDefault,
        _ => LoadSuffixes::Source,
    }
}
