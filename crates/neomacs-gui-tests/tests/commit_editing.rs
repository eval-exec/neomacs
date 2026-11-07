//! Real command-input and small-font newline regressions, including macOS.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

/// Honor explicit compositor selection, defaulting to the host's native backend.
fn commit_probe_backend(configured: Option<&str>) -> GuiBackend {
    match configured {
        Some("wayland" | "linux-wayland") => GuiBackend::LinuxWayland,
        Some("x11" | "linux-x11") => GuiBackend::LinuxX11,
        Some("macos") => GuiBackend::Macos,
        None if cfg!(target_os = "macos") => GuiBackend::Macos,
        None => GuiBackend::LinuxX11,
        Some(other) => panic!("unsupported NEOMACS_GUI_TEST_BACKEND={other:?}"),
    }
}

/// Run a native commit-editing fixture and require its explicit success marker.
fn run_commit_probe(probe: &str) {
    run_fixture_probe(probe, "commit-editing.el");
}

/// Run an isolated native fixture, retaining diagnostics and checking its marker.
fn run_fixture_probe(probe: &str, fixture: &str) {
    let root = neomacs_infra::crate_root!().join("../..");
    let configured = std::env::var("NEOMACS_GUI_TEST_BACKEND").ok();
    let backend = commit_probe_backend(configured.as_deref());
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join(format!("commit-{probe}"));
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let program = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let missing_telemetry = probe == "rtl-missing-telemetry";
    let stats_path = if missing_telemetry {
        artifacts
            .join(format!("missing-{}", std::process::id()))
            .join("layout-stats.log")
    } else {
        artifacts.join("layout-stats.log")
    };
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            probe,
            root.join("crates/neomacs-gui-tests/fixtures").join(fixture),
        ),
    )
    .with_program(program)
    .with_env("NEOMACS_COMMIT_PROBE", probe)
    .with_env(
        "NEOMACS_LAYOUT_STATS_FILE",
        stats_path.to_string_lossy().into_owned(),
    )
    .with_env("RUST_LOG", "warn");
    if missing_telemetry {
        assert!(!stats_path.exists());
        // Exercise Neomacs's mandatory-telemetry contract on either executable.
        plan = plan.with_args([
            "-Q".to_string(),
            "--eval".to_string(),
            "(unless (fboundp 'neomacs--frame-snapshot) (fset 'neomacs--frame-snapshot #'ignore))"
                .to_string(),
            "-l".to_string(),
            root.join("crates/neomacs-gui-tests/fixtures")
                .join(fixture)
                .to_string_lossy()
                .into_owned(),
        ]);
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(20)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    assert!(!result.timed_out, "{result:#?}\n{stderr}");
    if missing_telemetry {
        assert_eq!(result.exit_code, Some(1), "{result:#?}\n{stderr}");
        assert!(stderr.contains("Missing layout telemetry"), "{stderr}");
        assert!(
            !stderr.contains("CURSOR-REPLAY-REFERENCE-GEOMETRY-ONLY"),
            "{stderr}"
        );
        return;
    }
    assert_eq!(result.exit_code, Some(0), "{result:#?}\n{stderr}");
    assert!(stderr.contains("COMMIT-EDITING-PASS"), "{stderr}");
    if probe == "org-table-whitespace" {
        let rendered = stderr.contains("ORG-TABLE-RENDERED-FACES-PASS");
        let source_only = stderr.contains("ORG-TABLE-REFERENCE-SOURCE-ONLY");
        assert_ne!(
            rendered, source_only,
            "missing or ambiguous Org evidence: {stderr}"
        );
        if source_only {
            eprintln!("Org reference control: source setup only; rendered faces were not asserted");
        }
    }
    if (probe.ends_with("newline-cursor")
        || probe == "rtl-replacement-cursor"
        || probe.starts_with("stretch-policy-"))
        && !stderr.contains("CURSOR-REPLAY-REFERENCE-GEOMETRY-ONLY")
    {
        let marker = stderr
            .lines()
            .find_map(|line| line.strip_prefix("CURSOR-REPLAY-STATS "))
            .expect("missing replay telemetry interval");
        let bounds: Vec<usize> = marker
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(bounds.len(), 2, "{marker}");
        let stats = fs::read_to_string(artifacts.join("layout-stats.log")).unwrap();
        let interval = stats
            .get(bounds[0]..bounds[1])
            .expect("invalid replay telemetry interval");
        assert!(
            cursor_replay_reused_rows(interval),
            "point move did not reuse rows through cursor-only replay: {interval}"
        );
    }
}

/// Require the marked redisplay to reuse body rows rather than rebuilding them.
fn cursor_replay_reused_rows(stats: &str) -> bool {
    stats.lines().any(|line| {
        let counter = |name: &str| {
            line.split_whitespace().find_map(|entry| {
                entry
                    .strip_prefix(name)
                    .and_then(|value| value.parse::<usize>().ok())
            })
        };
        counter("cursor_only=").is_some_and(|value| value > 0)
            && counter("reused=").is_some_and(|value| value > 0)
            && counter("relaid_body=") == Some(0)
    })
}

#[test]
fn replay_telemetry_rejects_full_layout_and_missing_reuse() {
    assert!(cursor_replay_reused_rows(
        "cursor_only=1 reused=3 relaid_body=0"
    ));
    for stats in [
        "",
        "full=1 reused=3 relaid_body=0",
        "cursor_only=1 reused=0 relaid_body=0",
        "cursor_only=1 reused=3 relaid_body=1",
    ] {
        assert!(!cursor_replay_reused_rows(stats), "{stats}");
    }
}

/// Backend aliases and platform defaults must not require a live compositor.
#[test]
fn commit_probe_respects_requested_backend() {
    for name in ["wayland", "linux-wayland"] {
        assert_eq!(commit_probe_backend(Some(name)), GuiBackend::LinuxWayland);
    }
    for name in ["x11", "linux-x11"] {
        assert_eq!(commit_probe_backend(Some(name)), GuiBackend::LinuxX11);
    }
    assert_eq!(commit_probe_backend(Some("macos")), GuiBackend::Macos);
    assert_eq!(
        commit_probe_backend(None),
        if cfg!(target_os = "macos") {
            GuiBackend::Macos
        } else {
            GuiBackend::LinuxX11
        }
    );
}

#[test]
#[should_panic(expected = "unsupported NEOMACS_GUI_TEST_BACKEND")]
fn commit_probe_rejects_unknown_backend() {
    commit_probe_backend(Some("unknown"));
}

/// Neomacs verifies rendered faces; GNU explicitly reports a source-only control.
#[test]
fn org_table_probe_reports_rendered_faces_or_reference_source() {
    run_fixture_probe("org-table-whitespace", "org-table-whitespace.el");
}

/// Fresh and actual cursor-only replay obey narrow/wide stretch policy in RTL.
#[test]
fn stretch_cursor_replay_obeys_width_policy() {
    for probe in [
        "stretch-policy-narrow",
        "stretch-policy-wide",
        "stretch-policy-narrow-enabled",
        "stretch-policy-wide-enabled",
    ] {
        run_fixture_probe(probe, "cursor-stretch-policy.el");
    }
}

#[test]
fn first_commit_character_uses_the_asynchronously_selected_buffer() {
    run_commit_probe("async-map");
}

#[test]
fn delayed_timer_service_waits_for_commit_commands() {
    run_commit_probe("async-map-delayed");
}

#[test]
fn overlay_separator_uses_its_small_font_height() {
    run_commit_probe("overlay-separator");
}

#[test]
fn log_edit_separator_uses_its_small_font_height() {
    run_commit_probe("separator");
}

#[test]
fn replacement_newline_uses_its_own_font_height() {
    run_fixture_probe("replacement-newline", "newline-geometry.el");
}

#[test]
fn tall_overlay_newline_grows_the_empty_row() {
    run_fixture_probe("tall-overlay-newline", "newline-geometry.el");
}

#[test]
fn short_newline_cursor_replay_uses_full_row_bounds() {
    run_fixture_probe("short-newline-cursor", "newline-geometry.el");
}

#[test]
fn mixed_newline_cursor_matches_gnu_row_geometry() {
    for probe in [
        "mixed-newline-cursor",
        "mixed-small-newline-cursor",
        "mixed-spaced-newline-cursor",
        "mixed-spaced-small-newline-cursor",
    ] {
        run_fixture_probe(probe, "newline-geometry.el");
    }
}

#[test]
fn rtl_missing_telemetry_is_an_explicit_failure() {
    run_fixture_probe("rtl-missing-telemetry", "cursor-rtl.el");
}

#[test]
fn rtl_replacement_cursor_uses_one_glyph_for_placement_and_metrics() {
    run_fixture_probe("rtl-replacement-cursor", "cursor-rtl.el");
}

#[test]
fn raised_text_keeps_the_original_descent() {
    run_fixture_probe("raised-text", "newline-geometry.el");
}

#[test]
fn lowered_text_keeps_the_original_ascent() {
    run_fixture_probe("lowered-text", "newline-geometry.el");
}
