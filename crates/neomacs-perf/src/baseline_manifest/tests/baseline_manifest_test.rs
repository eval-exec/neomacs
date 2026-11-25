use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::{BaselineManifest, PerfError};

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn committed_v0021_baseline_manifest_loads_and_validates_schema() {
    let path = crate::workspace_root().join(".github/perf-baselines/v0.0.21.json");
    let (manifest, manifest_sha) =
        BaselineManifest::load(&path).expect("committed v0.0.21 manifest must parse");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.version, "v0.0.21");
    assert_eq!(
        manifest.git_commit,
        "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a"
    );
    assert_eq!(manifest.target, "x86_64-unknown-linux-gnu");
    assert_eq!(
        manifest.release_asset.sha256,
        "7c72abbd66640dcf1686a78049204217bc862a439e9c403a8dd769b1eabd627c"
    );
    assert_eq!(
        manifest.binary.sha256,
        "9cf559ca5ab3a2b5af92cd2e037beb61583c7a0e01401f122914aa29e624ec35"
    );
    assert_eq!(
        manifest.pdump.sha256,
        "a851b9a45cb3fb34f54c022a215d8a9830cc6eb07b257e161694e74e6fc1993c"
    );
    assert_eq!(manifest_sha.len(), 64);
}

#[test]
fn baseline_manifest_verifies_binary_and_pdump_and_rejects_tampering() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bin_dir = dir.path().join("bin");
    let pdump_dir = dir.path().join("libexec/neomacs/0.0.21");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(&pdump_dir).expect("pdump dir");

    let bin_path = bin_dir.join("neomacs");
    let pdump_path = pdump_dir.join("neomacs.pdump");
    let bin_bytes = b"fake-v0.0.21-neomacs-binary";
    let pdump_bytes = b"fake-v0.0.21-neomacs-pdump";
    fs::write(&bin_path, bin_bytes).expect("write bin");
    fs::write(&pdump_path, pdump_bytes).expect("write pdump");

    let manifest_path = dir.path().join("v0.0.21.json");
    let manifest_json = serde_json::json!({
        "schema_version": 1,
        "version": "v0.0.21",
        "git_commit": "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
        "target": "x86_64-unknown-linux-gnu",
        "release_asset": {
            "name": "neomacs-0.0.21-x86_64-unknown-linux-gnu.tar.gz",
            "url": "https://example.invalid/v0.0.21.tar.gz",
            "size_bytes": 123,
            "sha256": hex_sha256(b"tarball")
        },
        "binary": {
            "archive_path": "bin/neomacs",
            "size_bytes": bin_bytes.len(),
            "sha256": hex_sha256(bin_bytes)
        },
        "pdump": {
            "archive_path": "libexec/neomacs/0.0.21/neomacs.pdump",
            "relative_to_binary": "../libexec/neomacs/0.0.21/neomacs.pdump",
            "size_bytes": pdump_bytes.len(),
            "sha256": hex_sha256(pdump_bytes)
        }
    });
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest_json).expect("serialize manifest"),
    )
    .expect("write manifest");

    let (manifest, manifest_sha) = BaselineManifest::load(&manifest_path).expect("load manifest");
    let verified = manifest
        .verify_editor(&manifest_path, manifest_sha.clone(), &bin_path)
        .expect("matching binary and pdump verify");
    assert_eq!(verified.version, "v0.0.21");
    assert_eq!(
        verified.binary_path,
        fs::canonicalize(&bin_path).expect("canonical bin path")
    );
    assert!(verified.binary_path.is_absolute());
    assert_eq!(verified.binary_sha256, hex_sha256(bin_bytes));
    assert_eq!(verified.pdump_sha256, hex_sha256(pdump_bytes));

    // Tampered pdump must fail with subject == "pdump".
    fs::write(&pdump_path, b"tampered-pdump").expect("tamper pdump");
    let err = manifest
        .verify_editor(&manifest_path, manifest_sha.clone(), &bin_path)
        .expect_err("tampered pdump must be rejected");
    assert!(
        matches!(
            err,
            PerfError::BaselineManifestMismatch {
                subject: "pdump",
                ..
            }
        ),
        "expected pdump mismatch, got {err:?}"
    );

    // Missing pdump must also fail with subject == "pdump".
    fs::remove_file(&pdump_path).expect("remove pdump");
    let err = manifest
        .verify_editor(&manifest_path, manifest_sha.clone(), &bin_path)
        .expect_err("missing pdump must be rejected");
    assert!(
        matches!(
            err,
            PerfError::BaselineManifestMismatch {
                subject: "pdump",
                ..
            }
        ),
        "expected missing pdump mismatch, got {err:?}"
    );

    // Tampered binary must fail with subject == "binary".
    fs::write(&pdump_path, pdump_bytes).expect("restore pdump");
    fs::write(&bin_path, b"tampered-binary").expect("tamper bin");
    let err = manifest
        .verify_editor(&manifest_path, manifest_sha, &bin_path)
        .expect_err("tampered binary must be rejected");
    assert!(
        matches!(
            err,
            PerfError::BaselineManifestMismatch {
                subject: "binary",
                ..
            }
        ),
        "expected binary mismatch, got {err:?}"
    );
}

#[test]
fn baseline_manifest_rejects_unknown_schema_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad.json");
    let json = serde_json::json!({
        "schema_version": 99,
        "version": "v0.0.21",
        "git_commit": "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
        "target": "x86_64-unknown-linux-gnu",
        "release_asset": {
            "name": "a.tar.gz",
            "url": "https://example.invalid/a.tar.gz",
            "size_bytes": 1,
            "sha256": "0".repeat(64)
        },
        "binary": {
            "archive_path": "bin/neomacs",
            "size_bytes": 1,
            "sha256": "0".repeat(64)
        },
        "pdump": {
            "archive_path": "libexec/neomacs.pdump",
            "relative_to_binary": PathBuf::from("../libexec/neomacs.pdump"),
            "size_bytes": 1,
            "sha256": "0".repeat(64)
        }
    });
    fs::write(&path, serde_json::to_vec(&json).expect("json")).expect("write");
    let err = BaselineManifest::load(&path).expect_err("schema 99 must fail");
    assert!(matches!(err, PerfError::InvalidBaselineManifest { .. }));
}

#[cfg(unix)]
#[test]
fn verify_perf_baseline_shell_script_enforces_manifest_and_env_var_contract() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let script = crate::workspace_root().join("scripts/ci/verify-perf-baseline.sh");
    let committed_manifest = crate::workspace_root().join(".github/perf-baselines/v0.0.21.json");

    let run = |args: &[&str]| {
        Command::new("bash")
            .arg(&script)
            .args(args)
            .output()
            .expect("invoke verify-perf-baseline.sh")
    };

    // Unset baseline fails closed and names the configured env var.
    let unset = run(&[
        "--manifest",
        committed_manifest.to_str().unwrap(),
        "--env-var",
        "NEOMACS_PERF_V0021_BASELINE_EDITOR",
        "--",
        "",
    ]);
    assert!(!unset.status.success(), "unset baseline must fail");
    assert!(
        String::from_utf8_lossy(&unset.stderr).contains("NEOMACS_PERF_V0021_BASELINE_EDITOR"),
        "stderr must name env var: {}",
        String::from_utf8_lossy(&unset.stderr)
    );

    // Dash-prefixed path passed after `--` is treated as a path (not an option)
    // and fails naming the env var.
    let dash = run(&[
        "--manifest",
        committed_manifest.to_str().unwrap(),
        "--env-var",
        "NEOMACS_PERF_V0021_BASELINE_EDITOR",
        "--",
        "-nonexistent-baseline",
    ]);
    assert!(
        !dash.status.success(),
        "dash-prefixed missing path must fail"
    );
    let dash_stderr = String::from_utf8_lossy(&dash.stderr);
    assert!(
        dash_stderr.contains("NEOMACS_PERF_V0021_BASELINE_EDITOR"),
        "stderr must name env var rather than unknown option: {dash_stderr}"
    );
    assert!(
        !dash_stderr.contains("unknown option"),
        "value after `--` must not be parsed as an option: {dash_stderr}"
    );

    // Extra positional argument fails with exit code 2.
    let extra = run(&[
        "--manifest",
        committed_manifest.to_str().unwrap(),
        "--",
        "/a",
        "/b",
    ]);
    assert_eq!(extra.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&extra.stderr).contains("unexpected extra arguments"));

    // Valid temp binary + sibling pdump passes, while tampering either fails.
    let dir = tempfile::tempdir().expect("tempdir");
    let bin_dir = dir.path().join("bin");
    let pdump_dir = dir.path().join("libexec/neomacs/0.0.21");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(&pdump_dir).expect("pdump dir");

    let bin_path = bin_dir.join("neomacs");
    let pdump_path = pdump_dir.join("neomacs.pdump");
    let bin_bytes = b"#!/bin/sh\nexit 0\n";
    let pdump_bytes = b"verified-pdump-payload";
    fs::write(&bin_path, bin_bytes).expect("write bin");
    fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o755)).expect("chmod +x");
    fs::write(&pdump_path, pdump_bytes).expect("write pdump");

    let manifest_path = dir.path().join("manifest.json");
    let make_manifest = |rel_pdump: &str, release_sha: String| {
        serde_json::json!({
            "schema_version": 1,
            "version": "v0.0.21",
            "git_commit": "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
            "target": "x86_64-unknown-linux-gnu",
            "release_asset": {
                "name": "neomacs-0.0.21-x86_64-unknown-linux-gnu.tar.gz",
                "url": "https://example.invalid/v0.0.21.tar.gz",
                "size_bytes": 123,
                "sha256": release_sha
            },
            "binary": {
                "archive_path": "bin/neomacs",
                "size_bytes": bin_bytes.len(),
                "sha256": hex_sha256(bin_bytes)
            },
            "pdump": {
                "archive_path": "libexec/neomacs/0.0.21/neomacs.pdump",
                "relative_to_binary": rel_pdump,
                "size_bytes": pdump_bytes.len(),
                "sha256": hex_sha256(pdump_bytes)
            }
        })
    };
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&make_manifest(
            "../libexec/neomacs/0.0.21/neomacs.pdump",
            hex_sha256(b"tarball"),
        ))
        .unwrap(),
    )
    .expect("write manifest");

    let ok = run(&[
        "--manifest",
        manifest_path.to_str().unwrap(),
        "--env-var",
        "NEOMACS_PERF_V0021_BASELINE_EDITOR",
        "--",
        bin_path.to_str().unwrap(),
    ]);
    assert!(
        ok.status.success(),
        "valid binary + pdump must pass: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(String::from_utf8_lossy(&ok.stdout).contains("baseline verified:"));

    // Tampered pdump fails.
    fs::write(&pdump_path, b"tampered").expect("tamper pdump");
    let bad_pdump = run(&[
        "--manifest",
        manifest_path.to_str().unwrap(),
        "--",
        bin_path.to_str().unwrap(),
    ]);
    assert!(!bad_pdump.status.success(), "tampered pdump must fail");
    assert!(String::from_utf8_lossy(&bad_pdump.stderr).contains("portable dump digest mismatch"));
    fs::write(&pdump_path, pdump_bytes).expect("restore pdump");

    // Tampered binary fails.
    fs::write(&bin_path, b"#!/bin/sh\necho tampered\n").expect("tamper bin");
    let bad_bin = run(&[
        "--manifest",
        manifest_path.to_str().unwrap(),
        "--",
        bin_path.to_str().unwrap(),
    ]);
    assert!(!bad_bin.status.success(), "tampered binary must fail");
    assert!(String::from_utf8_lossy(&bad_bin.stderr).contains("binary digest mismatch"));
    fs::write(&bin_path, bin_bytes).expect("restore bin");

    // Absolute `pdump.relative_to_binary` fails (parity with Rust loader).
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&make_manifest(
            pdump_path.to_str().unwrap(),
            hex_sha256(b"tarball"),
        ))
        .unwrap(),
    )
    .expect("write abs manifest");
    let abs_pdump = run(&[
        "--manifest",
        manifest_path.to_str().unwrap(),
        "--",
        bin_path.to_str().unwrap(),
    ]);
    assert!(
        !abs_pdump.status.success(),
        "absolute relative_to_binary must fail"
    );
    assert!(String::from_utf8_lossy(&abs_pdump.stderr).contains("relative path"));

    // Invalid `release_asset.sha256` fails (parity with Rust loader).
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&make_manifest(
            "../libexec/neomacs/0.0.21/neomacs.pdump",
            "not-hex".to_string(),
        ))
        .unwrap(),
    )
    .expect("write bad release sha manifest");
    let bad_release = run(&[
        "--manifest",
        manifest_path.to_str().unwrap(),
        "--",
        bin_path.to_str().unwrap(),
    ]);
    assert!(
        !bad_release.status.success(),
        "invalid release_asset.sha256 must fail"
    );
    assert!(String::from_utf8_lossy(&bad_release.stderr).contains("release_asset"));
}
