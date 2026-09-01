use std::path::PathBuf;

use super::{
    ArtifactFile, ArtifactKind, CorrectnessMismatch, Frontend, RunArtifact, RunVerdict, ScenarioId,
};

#[test]
fn artifact_json_preserves_a_correctness_failure_as_a_failed_verdict() {
    let artifact = RunArtifact {
        schema_version: 1,
        run_id: "rust-lsp-typing-42".to_string(),
        scenario: ScenarioId::RustLspTyping,
        frontend: Frontend::Batch,
        editor: PathBuf::from("target/release/neomacs"),
        iterations: 10,
        started_unix_ms: 42,
        total_elapsed_us: 900,
        verdict: RunVerdict::CorrectnessMismatch {
            mismatches: vec![CorrectnessMismatch {
                invariant: "final-buffer-text".to_string(),
                expected: "unchanged".to_string(),
                actual: "changed".to_string(),
            }],
        },
        files: vec![ArtifactFile {
            kind: ArtifactKind::ScenarioResult,
            path: PathBuf::from("scenario-result.json"),
        }],
    };

    let json = serde_json::to_string_pretty(&artifact).expect("serialize artifact");
    assert!(json.contains(r#""verdict": {"#));
    assert!(json.contains(r#""kind": "correctness-mismatch""#));
    assert!(json.contains("final-buffer-text"));

    let decoded: RunArtifact = serde_json::from_str(&json).expect("deserialize artifact");
    assert_eq!(decoded, artifact);
    assert!(!decoded.verdict.is_valid());
    assert!(decoded.verdict.measurements().is_none());
}

#[test]
fn artifact_json_rejects_measurements_outside_a_valid_verdict() {
    let raw = r##"{
      "schema_version": 1,
      "run_id": "rust-lsp-typing-42",
      "scenario": "rust-lsp-typing",
      "frontend": "batch",
      "editor": "target/release/neomacs",
      "iterations": 10,
      "started_unix_ms": 42,
      "total_elapsed_us": 900,
      "verdict": {
        "kind": "correctness-mismatch",
        "mismatches": [{
          "invariant": "final-buffer-text",
          "expected": "unchanged",
          "actual": "changed"
        }]
      },
      "measurements": [{
        "name": "workload-cpu-time",
        "value": 800.0,
        "unit": "microseconds"
      }],
      "files": []
    }"##;

    let error = serde_json::from_str::<RunArtifact>(raw)
        .expect_err("invalid runs must not carry measurements");
    assert!(error.to_string().contains("unknown field `measurements`"));
}
