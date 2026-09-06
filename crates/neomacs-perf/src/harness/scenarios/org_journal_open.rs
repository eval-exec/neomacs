//! The org-journal-open scenario: revision-pinned org-journal,
//! org-superstar, and git-gutter over a deterministic yearly journal
//! inside a Git repository whose base commit predates today's entry.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use neomacs_melpa_test_support::{
    EmacsRuntime, MelpaSandbox, PreparedPackageSet, locked_melpa_sources,
    prepare_cached_locked_melpa_package,
};
use serde::{Deserialize, Serialize};

use crate::harness::{
    CorrectnessMismatch, EditorProvenance, HostProvenance, Measurement, MetricName, MetricUnit,
    PackageProvenance, PreparedScenario, PreparedWorkload, RunRequest,
    SCENARIO_RESULT_SCHEMA_VERSION, ScenarioId, ScenarioOutcome, ScenarioStatus,
    benchmark_passthrough_environment, collect_editor_provenance, collect_host_provenance,
    deserialize_optional_error, mismatch, prepare_gui_runtime_directory, require_positive_phase,
    scenario_outcome, sha256_file,
};

pub(crate) fn prepare(
    workspace_root: &Path,
    request: &RunRequest,
    run_directory: &Path,
) -> Result<PreparedScenario, String> {
    let sandbox = MelpaSandbox::new(&format!("perf-{}", request.scenario))?;
    let editor = collect_editor_provenance(request.editor(), &sandbox)?;
    let fixture_source = workspace_root.join("crates/neomacs-perf/fixtures/org-journal-open.el");
    if !fixture_source.is_file() {
        return Err(format!(
            "missing committed performance fixture {}",
            fixture_source.display()
        ));
    }
    let fixture = run_directory.join("org-journal-open.el");
    fs::copy(&fixture_source, &fixture).map_err(|error| {
        format!(
            "failed to copy performance fixture {} to {}: {error}",
            fixture_source.display(),
            fixture.display()
        )
    })?;

    let sources = locked_melpa_sources()?;
    let locked_package = |name: &str| {
        sources
            .iter()
            .find(|source| source.package().0 == name)
            .copied()
            .ok_or_else(|| format!("the MELPA source lock does not contain {name}"))
    };
    let org_journal = locked_package("org-journal")?;
    let org_superstar = locked_package("org-superstar")?;
    let git_gutter = locked_package("git-gutter")?;
    let gnu_emacs = EmacsRuntime::gnu_emacs();
    let mut packages =
        PreparedPackageSet::from_locked_melpa(&gnu_emacs, org_journal.package(), "org-journal.el")?;
    for dependency in [org_superstar, git_gutter] {
        let directory = prepare_cached_locked_melpa_package(&gnu_emacs, dependency.package())?;
        packages = packages.with_prepared_dependency(dependency.package(), directory)?;
    }
    let startup = packages.write_startup_file(run_directory)?;

    let (journal_directory, journal_input, external_journal) =
        prepare_org_journal_repository(run_directory, request.journal_file.as_deref())?;

    let provenance = run_directory.join("input-provenance.json");
    let provenance_manifest = OrgJournalInputProvenanceManifest {
        editor,
        host: collect_host_provenance(request.machine_policy()),
        scenario: request.scenario,
        workload_fixture_sha256: sha256_file(&fixture_source)?,
        packages: [org_journal, org_superstar, git_gutter]
            .into_iter()
            .map(|source| PackageProvenance {
                name: source.package().0,
                version: source.package().1,
                repository: source.repository(),
                revision: source.revision(),
                upstream_repository: source.upstream_repository(),
                upstream_revision: source.upstream_revision(),
            })
            .collect(),
        journal: journal_input,
        environment_policy: "closed-v1",
        passthrough_environment: benchmark_passthrough_environment()
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string_lossy().into_owned()))
            .collect(),
    };
    let provenance_json = serde_json::to_vec_pretty(&provenance_manifest)
        .map_err(|error| format!("failed to serialize input provenance: {error}"))?;
    fs::write(&provenance, provenance_json).map_err(|error| {
        format!(
            "failed to write input provenance {}: {error}",
            provenance.display()
        )
    })?;

    Ok(PreparedScenario {
        fixture,
        provenance,
        result: run_directory.join("scenario-result.json"),
        sentinel: run_directory.join("completed"),
        terminal_bytes: run_directory.join("terminal.ansi"),
        gui_app_log: run_directory.join("gui-app.log"),
        gui_weston_log: run_directory.join("weston.log"),
        gui_runtime_directory: prepare_gui_runtime_directory(workspace_root)?,
        sandbox,
        workload: PreparedWorkload::OrgJournalOpen {
            startup,
            journal_directory,
            external_journal,
            packages: Box::new(packages),
        },
    })
}

/// An external journal source the scenario works from by copying, never by
/// opening in place: the path plus the identity re-verified after the run.
#[derive(Clone)]
pub(crate) struct ExternalJournalInput {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
    pub(crate) size_bytes: u64,
}

/// Shape of the journal the scenario runs against, recorded in the run's
/// input provenance.
#[derive(Serialize)]
struct JournalInputProvenance {
    /// `"synthetic"` (generated below the run directory) or
    /// `"external-copy"` (copied from the caller's `--journal-file`).
    source: &'static str,
    /// Calendar year the yearly org-journal file is named for.
    year: i64,
    external_path: Option<String>,
    size_bytes: u64,
    sha256: String,
    /// Entry (`**`) heading count; present for generated journals only.
    entries: Option<u64>,
}

/// Build the journal fixture: a yearly org-journal file inside a fresh Git
/// repository whose single base commit deliberately predates today's entry,
/// so the first `org-journal-new-entry` produces a working-tree diff and
/// git-gutter populates its overlays exactly like the real workload.
///
/// With `journal_file` the caller's file is **copied** in and the source is
/// never opened for writing; `verify_external_journal_unchanged` re-checks
/// that after the run.
fn prepare_org_journal_repository(
    run_directory: &Path,
    journal_file: Option<&Path>,
) -> Result<
    (
        PathBuf,
        JournalInputProvenance,
        Option<ExternalJournalInput>,
    ),
    String,
> {
    let journal_directory = run_directory.join("journal");
    fs::create_dir_all(&journal_directory).map_err(|error| {
        format!(
            "failed to create journal fixture directory {}: {error}",
            journal_directory.display()
        )
    })?;
    let (year, day_of_year) = current_year_and_day_of_year();
    let journal_name = format!("{year}.org");
    let journal_path = journal_directory.join(&journal_name);
    let (external, provenance) = match journal_file {
        Some(source) => {
            fs::copy(source, &journal_path).map_err(|error| {
                format!(
                    "failed to copy journal input {} to {}: {error}",
                    source.display(),
                    journal_path.display()
                )
            })?;
            let sha256 = sha256_file(source)?;
            let size_bytes = fs::metadata(source)
                .map_err(|error| {
                    format!(
                        "failed to inspect journal input {}: {error}",
                        source.display()
                    )
                })?
                .len();
            let external = ExternalJournalInput {
                path: source.to_path_buf(),
                sha256: sha256.clone(),
                size_bytes,
            };
            let provenance = JournalInputProvenance {
                source: "external-copy",
                year,
                external_path: Some(source.display().to_string()),
                size_bytes,
                sha256,
                entries: None,
            };
            (Some(external), provenance)
        }
        None => {
            // Days elapsed before today: the generated base must not
            // already contain today's entry.
            let (content, entries) =
                generate_synthetic_journal(year, day_of_year.saturating_sub(1));
            fs::write(&journal_path, &content).map_err(|error| {
                format!(
                    "failed to write synthetic journal {}: {error}",
                    journal_path.display()
                )
            })?;
            let provenance = JournalInputProvenance {
                source: "synthetic",
                year,
                external_path: None,
                size_bytes: content.len() as u64,
                sha256: sha256_file(&journal_path)?,
                entries: Some(entries),
            };
            (None, provenance)
        }
    };
    for arguments in [
        vec!["init", "--quiet"],
        vec!["add", journal_name.as_str()],
        vec![
            "-c",
            "user.name=neomacs-perf",
            "-c",
            "user.email=perf@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "journal base",
        ],
    ] {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&journal_directory)
            .output()
            .map_err(|error| format!("failed to launch git for journal fixture: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "failed to prepare journal fixture repository: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    Ok((journal_directory, provenance, external))
}

/// The yearly org-journal file below the fixture directory, if one exists.
pub(crate) fn journal_file_in_directory(journal_directory: &Path) -> Option<PathBuf> {
    fs::read_dir(journal_directory)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("org"))
        })
}

/// Generate a deterministic yearly journal for the org-journal-open
/// workload.
///
/// Deliberately heavier than the real stall this scenario guards (~330 KB /
/// ~4,330 lines): day headings on most days, 3-4 timed `**` entries per
/// day, and multi-sentence body lines, landing around ~550 KB / ~5,500
/// lines at day 249 so the font-lock, overlay, and marker costs are
/// unmistakable on any machine.  The constant seed makes every machine
/// generate a byte-identical journal for the same elapsed-day count, which
/// is what cross-machine reproduction needs;
/// `synthetic_journal_generator_is_deterministic_and_heavier_than_the_real_workload`
/// pins both properties.
pub(crate) fn generate_synthetic_journal(year: i64, days: u32) -> (String, u64) {
    const DAY_NAMES: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    const SUBJECTS: [&str; 10] = [
        "morning notes",
        "standup summary",
        "reading log",
        "bug hunt",
        "pairing session",
        "design review",
        "refactor pass",
        "release prep",
        "research spike",
        "evening wrap-up",
    ];
    const BODIES: [&str; 8] = [
        "Worked through the review comments on the pull request, rebased the branch onto main, \
         and pushed the fixes after the second round of feedback came in.",
        "Long discussion about the module boundary and who owns the validation step; we \
         sketched three alternatives on the whiteboard but walked away without a decision.",
        "Profiled the slow path again with the new sampling build; the cache is doing its \
         job and the remaining time is all in the text conversion helpers.",
        "Read three chapters of the design book and took structured notes in the reading \
         file, splitting observations about data layout from the ones about interfaces.",
        "Fixed the flaky integration test by pinning the fixture ordering instead of \
         sorting inside the assertion, then re-ran the suite twice to be sure.",
        "Sketched the data flow on paper before touching code, marked the two places \
         where a retry could duplicate a side effect, and only then opened the editor.",
        "Helped debug the rendering pipeline with a fresh trace; the frame that dropped \
         was queued behind a texture upload nobody had profiled before.",
        "Cleaned up the backlog, closed two stale tickets that the reorg made moot, and \
         wrote up the remaining one well enough for anyone to pick up.",
    ];
    let mut state: u64 = 0x5EED_CAFE;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize
    };
    let january_first = days_from_civil(year, 1, 1);
    let mut journal = String::with_capacity(384 * 1024);
    let mut entries = 0u64;
    for index in 0..u64::from(days) {
        // A few days stay empty, like any real journal; the LCG decides
        // deterministically which ones.
        if next() % 100 < 30 {
            continue;
        }
        let day = january_first + index as i64;
        let (_, month, mday) = civil_from_days(day);
        // 1970-01-01 was a Thursday.
        let weekday = DAY_NAMES[(((day + 4) % 7 + 7) % 7) as usize];
        let _ = write!(journal, "* {year:04}-{month:02}-{mday:02}, {weekday}\n");
        for _ in 0..(3 + next() % 2) {
            let hour = 7 + next() % 13;
            let minute = next() % 60;
            let second = next() % 60;
            let _ = writeln!(
                journal,
                "** {hour:02}:{minute:02}:{second:02} {}",
                SUBJECTS[next() % SUBJECTS.len()]
            );
            for _ in 0..(6 + next() % 4) {
                let _ = writeln!(journal, "{}", BODIES[next() % BODIES.len()]);
            }
            entries += 1;
        }
        journal.push('\n');
    }
    (journal, entries)
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Civil date for days since 1970-01-01 (Howard Hinnant's algorithm).
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Current UTC year and 1-based day of year.  UTC is deliberate: journal
/// naming only needs a stable year, and the value is recorded in the run's
/// input provenance.
fn current_year_and_day_of_year() -> (i64, u32) {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let (year, _, _) = civil_from_days(days);
    let day_of_year = days - days_from_civil(year, 1, 1) + 1;
    (year, day_of_year as u32)
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "OrgJournalOpenResultWire")]
pub(crate) struct OrgJournalOpenResult {
    schema_version: u32,
    scenario: ScenarioId,
    outcome: ScenarioOutcome,
    iterations: u32,
    /// Read by the harness's `#[cfg(test)]` elapsed-time helper.
    pub(crate) elapsed_us: u64,
    elapsed_wall_us: u64,
    operation_count: u64,
    open_phase_us: u64,
    fontify_phase_us: u64,
    settle_phase_us: u64,
    expected_major_mode: String,
    actual_major_mode: String,
    org_superstar_active: bool,
    git_gutter_active: bool,
    overlay_count_min: u64,
    overlay_count_final: u64,
    stable_checksum: bool,
    entry_created: bool,
    journal_line_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgJournalOpenResultWire {
    schema_version: u32,
    scenario: ScenarioId,
    status: ScenarioStatus,
    iterations: u32,
    elapsed_us: u64,
    elapsed_wall_us: u64,
    operation_count: u64,
    open_phase_us: u64,
    fontify_phase_us: u64,
    settle_phase_us: u64,
    expected_major_mode: String,
    actual_major_mode: String,
    org_superstar_active: bool,
    git_gutter_active: bool,
    overlay_count_min: u64,
    overlay_count_final: u64,
    stable_checksum: bool,
    entry_created: bool,
    journal_line_count: u64,
    #[serde(deserialize_with = "deserialize_optional_error", rename = "error")]
    error: Option<String>,
}

impl TryFrom<OrgJournalOpenResultWire> for OrgJournalOpenResult {
    type Error = String;

    fn try_from(wire: OrgJournalOpenResultWire) -> Result<Self, Self::Error> {
        Ok(Self {
            schema_version: wire.schema_version,
            scenario: wire.scenario,
            outcome: scenario_outcome(wire.status, wire.error)?,
            iterations: wire.iterations,
            elapsed_us: wire.elapsed_us,
            elapsed_wall_us: wire.elapsed_wall_us,
            operation_count: wire.operation_count,
            open_phase_us: wire.open_phase_us,
            fontify_phase_us: wire.fontify_phase_us,
            settle_phase_us: wire.settle_phase_us,
            expected_major_mode: wire.expected_major_mode,
            actual_major_mode: wire.actual_major_mode,
            org_superstar_active: wire.org_superstar_active,
            git_gutter_active: wire.git_gutter_active,
            overlay_count_min: wire.overlay_count_min,
            overlay_count_final: wire.overlay_count_final,
            stable_checksum: wire.stable_checksum,
            entry_created: wire.entry_created,
            journal_line_count: wire.journal_line_count,
        })
    }
}

#[derive(Serialize)]
struct OrgJournalInputProvenanceManifest<'a> {
    editor: EditorProvenance,
    host: HostProvenance,
    scenario: ScenarioId,
    workload_fixture_sha256: String,
    packages: Vec<PackageProvenance<'a>>,
    journal: JournalInputProvenance,
    environment_policy: &'a str,
    passthrough_environment: BTreeMap<String, String>,
}

/// Invariants of a valid org-journal-open run.
///
/// The synthetic journal guarantees today's entry is missing from the Git
/// base, so a correct run must create it and must produce at least one
/// git-gutter overlay from the resulting working-tree diff.  An external
/// journal may legitimately already contain today's entry (in which case
/// no cycle creates one, and an unchanged file yields no overlays), so
/// those two invariants are relaxed on the `--journal-file` path.
pub(crate) fn validate_org_journal_open_result(
    request: &RunRequest,
    result: &OrgJournalOpenResult,
) -> Vec<CorrectnessMismatch> {
    let synthetic = request.journal_file.is_none();
    let mut mismatches = Vec::new();
    mismatch(
        &mut mismatches,
        "scenario-result-schema",
        SCENARIO_RESULT_SCHEMA_VERSION,
        result.schema_version,
    );
    mismatch(
        &mut mismatches,
        "scenario-id",
        request.scenario,
        result.scenario,
    );
    mismatch(
        &mut mismatches,
        "scenario-outcome",
        &ScenarioOutcome::Ok,
        &result.outcome,
    );
    mismatch(
        &mut mismatches,
        "iterations",
        request.iterations.get(),
        result.iterations,
    );
    mismatch(
        &mut mismatches,
        "operation-count",
        u64::from(request.iterations.get()),
        result.operation_count,
    );
    mismatch(
        &mut mismatches,
        "major-mode",
        "org-journal-mode",
        result.actual_major_mode.as_str(),
    );
    mismatch(
        &mut mismatches,
        "expected-major-mode",
        result.expected_major_mode.as_str(),
        result.actual_major_mode.as_str(),
    );
    mismatch(
        &mut mismatches,
        "org-superstar-active",
        true,
        result.org_superstar_active,
    );
    mismatch(
        &mut mismatches,
        "git-gutter-active",
        true,
        result.git_gutter_active,
    );
    mismatch(
        &mut mismatches,
        "stable-checksum",
        true,
        result.stable_checksum,
    );
    if synthetic {
        mismatch(&mut mismatches, "entry-created", true, result.entry_created);
        if result.overlay_count_min == 0 {
            mismatches.push(CorrectnessMismatch {
                invariant: "overlay-count".to_string(),
                expected: "positive on the synthetic journal".to_string(),
                actual: result.overlay_count_min.to_string(),
            });
        }
    }
    if result.journal_line_count == 0 {
        mismatches.push(CorrectnessMismatch {
            invariant: "journal-line-count".to_string(),
            expected: "positive".to_string(),
            actual: "0".to_string(),
        });
    }
    for (name, value) in [
        ("elapsed-time", result.elapsed_us),
        ("elapsed-wall-time", result.elapsed_wall_us),
        ("open-phase-time", result.open_phase_us),
        ("fontify-phase-time", result.fontify_phase_us),
        ("settle-phase-time", result.settle_phase_us),
    ] {
        require_positive_phase(&mut mismatches, name, value);
    }
    mismatches
}

pub(crate) fn valid_org_journal_open_measurements(
    result: &OrgJournalOpenResult,
    wall_elapsed_us: u128,
) -> Vec<Measurement> {
    vec![
        Measurement {
            name: MetricName::ProcessWallTime,
            value: wall_elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadCpuTime,
            value: result.elapsed_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::WorkloadWallTime,
            value: result.elapsed_wall_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::PerOperationCpuTime,
            value: result.elapsed_us as f64 / result.operation_count.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerOperation,
        },
        Measurement {
            name: MetricName::PerOperationWallTime,
            value: result.elapsed_wall_us as f64 / result.operation_count.max(1) as f64,
            unit: MetricUnit::MicrosecondsPerOperation,
        },
        Measurement {
            name: MetricName::ModePhaseCpuTime,
            value: result.open_phase_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::FontifyPhaseCpuTime,
            value: result.fontify_phase_us as f64,
            unit: MetricUnit::Microseconds,
        },
        Measurement {
            name: MetricName::OverlayCount,
            value: result.overlay_count_final as f64,
            unit: MetricUnit::Count,
        },
    ]
}
