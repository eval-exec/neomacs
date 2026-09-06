use std::str::FromStr;

use std::num::NonZeroU32;

use super::{CrossEditorParityMetric, Frontend, MetricName, ScenarioId, scenario, scenarios};

#[test]
fn catalog_exposes_the_rust_lsp_typing_workload_as_a_typed_scenario() {
    let scenarios = scenarios();
    assert_eq!(scenarios.len(), 14);

    let rust_lsp = scenario(ScenarioId::RustLspTyping);
    assert_eq!(rust_lsp.id, ScenarioId::RustLspTyping);
    assert_eq!(rust_lsp.id.to_string(), "rust-lsp-typing");
    assert_eq!(
        ScenarioId::from_str("rust-lsp-typing"),
        Ok(ScenarioId::RustLspTyping)
    );
    assert_eq!(
        rust_lsp.default_frontend,
        Frontend::Tui {
            rows: 40,
            columns: 120,
        }
    );
    assert!(rust_lsp.description.contains("Tree-sitter"));
    assert!(rust_lsp.description.contains("LSP Mode"));
    assert_eq!(
        rust_lsp.default_iterations,
        NonZeroU32::new(100).expect("non-zero default")
    );
    assert_eq!(rust_lsp.primary_metric, MetricName::PerEditCpuTime);
}

#[test]
fn catalog_exposes_sustained_native_video_on_a_real_gui() {
    let video = scenario(ScenarioId::SustainedNativeVideo);

    assert_eq!(video.id.to_string(), "sustained-native-video");
    assert_eq!(
        ScenarioId::from_str("sustained-native-video"),
        Ok(ScenarioId::SustainedNativeVideo)
    );
    assert_eq!(
        video.default_frontend,
        Frontend::Gui {
            width: 1920,
            height: 1080,
        }
    );
    assert_eq!(
        video.default_iterations,
        NonZeroU32::new(300).expect("30 seconds at one 100 ms sample tick")
    );
    assert_eq!(
        video.primary_metric,
        MetricName::P99VideoPresentationInterval
    );
}

#[test]
fn catalog_commits_the_editor_workflow_scenario_family() {
    let expected = [
        ("editing-simulation", ScenarioId::EditingSimulation),
        ("startup", ScenarioId::Startup),
        ("sustained-editing", ScenarioId::SustainedEditing),
        ("gui-input-latency", ScenarioId::GuiInputLatency),
        ("org-editing", ScenarioId::OrgEditing),
        ("magit-status", ScenarioId::MagitStatus),
        ("large-file-editing", ScenarioId::LargeFileEditing),
        ("indentation", ScenarioId::Indentation),
        ("regex-search", ScenarioId::RegexSearch),
    ];

    for (name, id) in expected {
        assert_eq!(ScenarioId::from_str(name), Ok(id));
        assert_eq!(scenario(id).id, id);
    }
    assert_eq!(
        scenario(ScenarioId::GuiInputLatency).default_frontend,
        Frontend::Gui {
            width: 1200,
            height: 800,
        }
    );
    assert_eq!(
        scenario(ScenarioId::GuiInputLatency).primary_metric,
        MetricName::P99InputToRedisplayLatency
    );
    assert_eq!(
        scenario(ScenarioId::Startup).primary_metric,
        MetricName::ProcessWallTime
    );
}

#[test]
fn catalog_exposes_the_pure_interpreter_bytecode_call_loop() {
    let bytecode_calls = scenario(ScenarioId::BytecodeCallLoop);
    assert_eq!(bytecode_calls.id.to_string(), "bytecode-call-loop");
    assert_eq!(
        ScenarioId::from_str("bytecode-call-loop"),
        Ok(ScenarioId::BytecodeCallLoop)
    );
    assert_eq!(bytecode_calls.default_frontend, Frontend::Batch);
    assert_eq!(
        bytecode_calls.default_iterations,
        NonZeroU32::new(20_000_000).expect("non-zero default")
    );
    assert_eq!(
        bytecode_calls.primary_metric,
        MetricName::PerBytecodeCallCpuTime
    );
    assert!(bytecode_calls.description.contains("Tier-0"));
}

#[test]
fn catalog_exposes_mx_tab_as_a_real_completion_window_workload() {
    let mx_tab = scenario(ScenarioId::MxTabCompletion);
    assert_eq!(mx_tab.id.to_string(), "mx-tab-completion");
    assert_eq!(
        ScenarioId::from_str("mx-tab-completion"),
        Ok(ScenarioId::MxTabCompletion)
    );
    assert_eq!(
        mx_tab.default_frontend,
        Frontend::Tui {
            rows: 40,
            columns: 120,
        }
    );
    assert_eq!(
        mx_tab.default_iterations,
        NonZeroU32::new(5).expect("non-zero default")
    );
    assert_eq!(mx_tab.primary_metric, MetricName::PerCompletionCpuTime);
    assert_eq!(
        mx_tab.cross_editor_parity_metrics,
        &[CrossEditorParityMetric::CompletionCandidateCount]
    );
    assert!(mx_tab.description.contains("M-x TAB"));
}

#[test]
fn catalog_exposes_the_org_journal_open_workload() {
    let org_journal = scenario(ScenarioId::OrgJournalOpen);
    assert_eq!(org_journal.id.to_string(), "org-journal-open");
    assert_eq!(
        ScenarioId::from_str("org-journal-open"),
        Ok(ScenarioId::OrgJournalOpen)
    );
    assert_eq!(org_journal.default_frontend, Frontend::Batch);
    assert_eq!(
        org_journal.default_iterations,
        NonZeroU32::new(5).expect("non-zero default")
    );
    assert_eq!(org_journal.primary_metric, MetricName::PerOperationWallTime);
    assert!(org_journal.description.contains("org-journal"));
    assert!(org_journal.description.contains("git-gutter"));
}

#[test]
fn unknown_scenario_names_are_rejected_instead_of_silently_falling_back() {
    let error = ScenarioId::from_str("rust-typing").expect_err("unknown scenario must fail");
    assert!(error.to_string().contains("rust-typing"));
}
