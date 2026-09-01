use std::str::FromStr;

use std::num::NonZeroU32;

use super::{CrossEditorParityMetric, Frontend, MetricName, ScenarioId, scenario, scenarios};

#[test]
fn catalog_exposes_the_rust_lsp_typing_workload_as_a_typed_scenario() {
    let scenarios = scenarios();
    assert_eq!(scenarios.len(), 3);

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
fn unknown_scenario_names_are_rejected_instead_of_silently_falling_back() {
    let error = ScenarioId::from_str("rust-typing").expect_err("unknown scenario must fail");
    assert!(error.to_string().contains("rust-typing"));
}
