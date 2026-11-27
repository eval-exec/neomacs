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

    // Dangling value-taking options exit 2 with a clear error message.
    for opt in ["--manifest", "--env-var"] {
        let dangling = run(&[opt]);
        assert_eq!(dangling.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&dangling.stderr).contains("requires a value"),
            "dangling {opt} must explain missing value"
        );
    }

    // Optional provenance fields (`build_profile`, `rustc_version`):
    // empty, whitespace-only, and non-string values must be rejected;
    // explicit `null` and omitted fields must remain accepted (legacy parity).
    let base_valid = make_manifest(
        "../libexec/neomacs/0.0.21/neomacs.pdump",
        hex_sha256(b"tarball"),
    );
    for (field, bad_val) in [
        ("build_profile", serde_json::json!("")),
        ("build_profile", serde_json::json!("   ")),
        ("build_profile", serde_json::json!(123)),
        ("rustc_version", serde_json::json!("")),
        ("rustc_version", serde_json::json!(" \t\n ")),
        ("rustc_version", serde_json::json!(["rustc"])),
    ] {
        let mut doc = base_valid.clone();
        doc[field] = bad_val;
        fs::write(&manifest_path, serde_json::to_vec_pretty(&doc).unwrap())
            .expect("write bad provenance manifest");
        let res = run(&[
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--",
            bin_path.to_str().unwrap(),
        ]);
        assert!(
            !res.status.success(),
            "verify-perf-baseline.sh must reject invalid {field}: stdout={} stderr={}",
            String::from_utf8_lossy(&res.stdout),
            String::from_utf8_lossy(&res.stderr)
        );
        assert!(
            String::from_utf8_lossy(&res.stderr).contains(field),
            "stderr must mention {field}: {}",
            String::from_utf8_lossy(&res.stderr)
        );
    }

    let mut null_doc = base_valid.clone();
    null_doc["build_profile"] = serde_json::Value::Null;
    null_doc["rustc_version"] = serde_json::Value::Null;
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&null_doc).unwrap(),
    )
    .expect("write null provenance manifest");
    let null_ok = run(&[
        "--manifest",
        manifest_path.to_str().unwrap(),
        "--",
        bin_path.to_str().unwrap(),
    ]);
    assert!(
        null_ok.status.success(),
        "null optional provenance fields must pass: {}",
        String::from_utf8_lossy(&null_ok.stderr)
    );
    let null_stdout = String::from_utf8_lossy(&null_ok.stdout);
    assert!(null_stdout.contains("baseline verified:"));
    assert!(!null_stdout.contains("profile="));
    assert!(!null_stdout.contains("rustc="));
}

#[cfg(unix)]
#[test]
fn build_perf_baseline_script_writes_and_verifies_release_profile_manifest() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let script = crate::workspace_root().join("scripts/ci/build-perf-baseline.sh");
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("neomacs-0.0.21-x86_64-unknown-linux-gnu");
    let bin_dir = root.join("bin");
    let pdump_dir = root.join("libexec/neomacs/0.0.21/x86_64-unknown-linux-gnu");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(&pdump_dir).expect("pdump dir");

    let tarball = dir
        .path()
        .join("neomacs-0.0.21-x86_64-unknown-linux-gnu.tar.gz");
    let bin_path = bin_dir.join("neomacs");
    let pdump_path = pdump_dir.join("neomacs.pdump");
    let manifest_path = dir.path().join("v0.0.21-release.json");

    fs::write(&tarball, b"synthetic-release-tarball").expect("write tarball");
    fs::write(&bin_path, b"#!/bin/sh\nexit 0\n").expect("write bin");
    fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o755)).expect("chmod +x");
    fs::write(&pdump_path, b"synthetic-release-pdump").expect("write pdump");

    let output = Command::new("bash")
        .arg(&script)
        .args([
            "--manifest-only",
            "--ref",
            "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
            "--version",
            "v0.0.21-release",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--tarball",
            tarball.to_str().unwrap(),
            "--archive-root",
            "neomacs-0.0.21-x86_64-unknown-linux-gnu",
            "--binary",
            bin_path.to_str().unwrap(),
            "--pdump",
            pdump_path.to_str().unwrap(),
            "--relative-pdump",
            "../libexec/neomacs/0.0.21/x86_64-unknown-linux-gnu/neomacs.pdump",
            "--rustc-version",
            "rustc 1.96.1 (31fca3adb 2026-06-26)",
            "--manifest",
            manifest_path.to_str().unwrap(),
        ])
        .output()
        .expect("invoke build-perf-baseline.sh");

    assert!(
        output.status.success(),
        "build-perf-baseline.sh --manifest-only must succeed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("profile=release"), "stdout: {stdout}");
    assert!(stdout.contains("pgo=none"), "stdout: {stdout}");
    assert!(stdout.contains("rustc 1.96.1"), "stdout: {stdout}");

    let (loaded, sha) = BaselineManifest::load(&manifest_path).expect("load generated manifest");
    assert_eq!(loaded.version, "v0.0.21-release");
    assert_eq!(loaded.build_profile.as_deref(), Some("release"));
    assert_eq!(
        loaded.rustc_version.as_deref(),
        Some("rustc 1.96.1 (31fca3adb 2026-06-26)")
    );
    assert_eq!(loaded.pgo_profile.as_deref(), Some("none"));
    let codegen = loaded
        .codegen_settings
        .as_deref()
        .expect("codegen_settings must be recorded");
    assert!(
        codegen.contains("lto=true") && codegen.contains("-align-all-functions=6"),
        "unexpected codegen_settings: {codegen}"
    );
    let verified = loaded
        .verify_editor(&manifest_path, sha, &bin_path)
        .expect("verify generated manifest");
    assert_eq!(verified.build_profile.as_deref(), Some("release"));
    assert_eq!(
        verified.rustc_version.as_deref(),
        Some("rustc 1.96.1 (31fca3adb 2026-06-26)")
    );
    assert_eq!(verified.pgo_profile.as_deref(), Some("none"));
    assert_eq!(verified.codegen_settings.as_deref(), Some(codegen));
}

#[cfg(unix)]
#[test]
fn build_perf_baseline_script_rejects_unsafe_out_dir_and_manifest_only_failures_atomically() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let repo_root = crate::workspace_root();
    let script = repo_root.join("scripts/ci/build-perf-baseline.sh");
    let dir = tempfile::tempdir().expect("tempdir");

    let run = |args: &[&str]| {
        Command::new("bash")
            .arg(&script)
            .args(args)
            .output()
            .expect("invoke build-perf-baseline.sh")
    };

    // 1. Unsafe `--out-dir` values MUST exit 2 before touching git or rm -rf:
    // `/`, `//`, `///`, `/tmp/..`, relative paths, `$repo_root`, `$repo_root/sub`,
    // ancestors of `$repo_root`, and symlinks resolving to `/`, `$repo_root`,
    // `$repo_root/sub`, or an ancestor of `$repo_root`.
    let dummy_manifest = dir.path().join("never-written.json");
    let repo_str = repo_root.to_str().unwrap().to_owned();
    let repo_sub = repo_root
        .join(".github/perf-baselines")
        .to_str()
        .unwrap()
        .to_owned();
    let repo_parent = repo_root
        .parent()
        .expect("repo parent")
        .to_str()
        .unwrap()
        .to_owned();
    let sym_root = dir.path().join("sym-root");
    let sym_repo = dir.path().join("sym-repo");
    let sym_repo_sub = dir.path().join("sym-repo-sub");
    let sym_repo_parent = dir.path().join("sym-repo-parent");
    std::os::unix::fs::symlink("/", &sym_root).expect("symlink /");
    std::os::unix::fs::symlink(&repo_root, &sym_repo).expect("symlink repo");
    std::os::unix::fs::symlink(repo_root.join(".github"), &sym_repo_sub).expect("symlink repo sub");
    std::os::unix::fs::symlink(repo_root.parent().unwrap(), &sym_repo_parent)
        .expect("symlink repo parent");
    let sym_repo_dotdot = format!("{}/..", sym_repo_sub.to_str().unwrap());

    for bad_out in [
        "/",
        "//",
        "///",
        "/tmp/..",
        "relative/out",
        repo_str.as_str(),
        repo_sub.as_str(),
        repo_parent.as_str(),
        sym_root.to_str().unwrap(),
        sym_repo.to_str().unwrap(),
        sym_repo_sub.to_str().unwrap(),
        sym_repo_parent.to_str().unwrap(),
        sym_repo_dotdot.as_str(),
    ] {
        let res = run(&[
            "--out-dir",
            bad_out,
            "--ref",
            "nonexistent-ref-should-not-be-reached",
            "--manifest",
            dummy_manifest.to_str().unwrap(),
        ]);
        assert_eq!(
            res.status.code(),
            Some(2),
            "unsafe --out-dir {bad_out:?} must exit 2 before git/rm: stdout={} stderr={}",
            String::from_utf8_lossy(&res.stdout),
            String::from_utf8_lossy(&res.stderr)
        );
        assert!(
            !dummy_manifest.exists(),
            "--out-dir failure must not create manifest"
        );
    }

    // 2. Dangling value-taking option, missing `--manifest`, missing `--out-dir`,
    // and unknown option all exit 2 with clear diagnostics.
    let dangling = run(&["--manifest", dummy_manifest.to_str().unwrap(), "--ref"]);
    assert_eq!(dangling.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&dangling.stderr).contains("requires a value"));

    let missing_manifest = run(&["--out-dir", "/opt/neomacs-perf/v0.0.21-release"]);
    assert_eq!(missing_manifest.status.code(), Some(2));

    let missing_out_dir = run(&["--manifest", dummy_manifest.to_str().unwrap()]);
    assert_eq!(missing_out_dir.status.code(), Some(2));

    let unknown_opt = run(&[
        "--bogus-flag",
        "--manifest",
        dummy_manifest.to_str().unwrap(),
    ]);
    assert_eq!(unknown_opt.status.code(), Some(2));

    // 3. `--manifest-only` missing required flags exits 2.
    let missing_flags = run(&[
        "--manifest-only",
        "--manifest",
        dummy_manifest.to_str().unwrap(),
    ]);
    assert_eq!(missing_flags.status.code(), Some(2));

    // Set up valid synthetic tree for atomic failure tests.
    let root = dir.path().join("neomacs-0.0.21-x86_64-unknown-linux-gnu");
    let bin_dir = root.join("bin");
    let pdump_dir = root.join("libexec/neomacs/0.0.21/x86_64-unknown-linux-gnu");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::create_dir_all(&pdump_dir).expect("pdump dir");

    let tarball = dir
        .path()
        .join("neomacs-0.0.21-x86_64-unknown-linux-gnu.tar.gz");
    let bin_path = bin_dir.join("neomacs");
    let pdump_path = pdump_dir.join("neomacs.pdump");
    fs::write(&tarball, b"synthetic-tarball").expect("write tarball");
    fs::write(&bin_path, b"#!/bin/sh\nexit 0\n").expect("write bin");
    fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o755)).expect("chmod +x");
    fs::write(&pdump_path, b"synthetic-pdump").expect("write pdump");

    let assert_no_temp_manifests = |dir_path: &std::path::Path| {
        let leftovers: Vec<_> = fs::read_dir(dir_path)
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".perf-baseline-manifest.")
            })
            .collect();
        assert!(
            leftovers.is_empty(),
            "failed manifest write left behind temp file(s): {leftovers:?}"
        );
    };

    // 4. Failed verification (whitespace `--rustc-version`) on a new path MUST
    // NOT leave `--manifest` or sibling temp files behind on disk.
    let fresh_bad_manifest = dir.path().join("must-not-exist.json");
    let ws_rustc = run(&[
        "--manifest-only",
        "--ref",
        "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
        "--tarball",
        tarball.to_str().unwrap(),
        "--archive-root",
        "neomacs-0.0.21-x86_64-unknown-linux-gnu",
        "--binary",
        bin_path.to_str().unwrap(),
        "--pdump",
        pdump_path.to_str().unwrap(),
        "--relative-pdump",
        "../libexec/neomacs/0.0.21/x86_64-unknown-linux-gnu/neomacs.pdump",
        "--rustc-version",
        "   ",
        "--manifest",
        fresh_bad_manifest.to_str().unwrap(),
    ]);
    assert!(
        !ws_rustc.status.success(),
        "whitespace --rustc-version must fail"
    );
    assert!(
        !fresh_bad_manifest.exists(),
        "failed verification must not leave invalid manifest behind"
    );
    assert_no_temp_manifests(dir.path());

    // 5. Existing `--manifest` MUST NOT be clobbered on any failure path:
    // empty `--rustc-version ""`, bad non-40-hex `--ref`, mismatched
    // `--relative-pdump`, or non-executable `--binary`.
    let existing_manifest = dir.path().join("existing-sentinel.json");
    let sentinel_contents = "{\"sentinel\": \"do-not-clobber\"}\n";
    fs::write(&existing_manifest, sentinel_contents).expect("write sentinel");

    let empty_rustc = run(&[
        "--manifest-only",
        "--ref",
        "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
        "--tarball",
        tarball.to_str().unwrap(),
        "--archive-root",
        "neomacs-0.0.21-x86_64-unknown-linux-gnu",
        "--binary",
        bin_path.to_str().unwrap(),
        "--pdump",
        pdump_path.to_str().unwrap(),
        "--relative-pdump",
        "../libexec/neomacs/0.0.21/x86_64-unknown-linux-gnu/neomacs.pdump",
        "--rustc-version",
        "",
        "--manifest",
        existing_manifest.to_str().unwrap(),
    ]);
    assert!(
        !empty_rustc.status.success(),
        "explicit empty --rustc-version must fail instead of falling back"
    );
    assert_eq!(
        fs::read_to_string(&existing_manifest).unwrap(),
        sentinel_contents
    );
    assert_no_temp_manifests(dir.path());

    let bad_ref = run(&[
        "--manifest-only",
        "--ref",
        "not-a-40-char-sha",
        "--tarball",
        tarball.to_str().unwrap(),
        "--archive-root",
        "neomacs-0.0.21-x86_64-unknown-linux-gnu",
        "--binary",
        bin_path.to_str().unwrap(),
        "--pdump",
        pdump_path.to_str().unwrap(),
        "--relative-pdump",
        "../libexec/neomacs/0.0.21/x86_64-unknown-linux-gnu/neomacs.pdump",
        "--rustc-version",
        "rustc 1.96.1",
        "--manifest",
        existing_manifest.to_str().unwrap(),
    ]);
    assert_eq!(bad_ref.status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(&existing_manifest).unwrap(),
        sentinel_contents
    );
    assert_no_temp_manifests(dir.path());

    let bad_rel_pdump = run(&[
        "--manifest-only",
        "--ref",
        "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
        "--tarball",
        tarball.to_str().unwrap(),
        "--archive-root",
        "neomacs-0.0.21-x86_64-unknown-linux-gnu",
        "--binary",
        bin_path.to_str().unwrap(),
        "--pdump",
        pdump_path.to_str().unwrap(),
        "--relative-pdump",
        "../libexec/wrong/neomacs.pdump",
        "--rustc-version",
        "rustc 1.96.1",
        "--manifest",
        existing_manifest.to_str().unwrap(),
    ]);
    assert!(
        !bad_rel_pdump.status.success(),
        "mismatched --relative-pdump must fail self-check"
    );
    assert_eq!(
        fs::read_to_string(&existing_manifest).unwrap(),
        sentinel_contents
    );
    assert_no_temp_manifests(dir.path());

    fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o644)).expect("chmod -x");
    let non_exec_bin = run(&[
        "--manifest-only",
        "--ref",
        "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
        "--tarball",
        tarball.to_str().unwrap(),
        "--archive-root",
        "neomacs-0.0.21-x86_64-unknown-linux-gnu",
        "--binary",
        bin_path.to_str().unwrap(),
        "--pdump",
        pdump_path.to_str().unwrap(),
        "--relative-pdump",
        "../libexec/neomacs/0.0.21/x86_64-unknown-linux-gnu/neomacs.pdump",
        "--rustc-version",
        "rustc 1.96.1",
        "--manifest",
        existing_manifest.to_str().unwrap(),
    ]);
    assert!(
        !non_exec_bin.status.success(),
        "non-executable --binary must fail self-check"
    );
    assert_eq!(
        fs::read_to_string(&existing_manifest).unwrap(),
        sentinel_contents
    );
    assert_no_temp_manifests(dir.path());

    // Missing input file in `--manifest-only` fails closed without clobbering sentinel.
    let missing_tarball = dir.path().join("does-not-exist.tar.gz");
    let missing_input = run(&[
        "--manifest-only",
        "--ref",
        "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
        "--tarball",
        missing_tarball.to_str().unwrap(),
        "--archive-root",
        "neomacs-0.0.21-x86_64-unknown-linux-gnu",
        "--binary",
        bin_path.to_str().unwrap(),
        "--pdump",
        pdump_path.to_str().unwrap(),
        "--relative-pdump",
        "../libexec/neomacs/0.0.21/x86_64-unknown-linux-gnu/neomacs.pdump",
        "--rustc-version",
        "rustc 1.96.1",
        "--manifest",
        existing_manifest.to_str().unwrap(),
    ]);
    assert!(
        !missing_input.status.success(),
        "missing tarball must fail closed"
    );
    assert_eq!(
        fs::read_to_string(&existing_manifest).unwrap(),
        sentinel_contents
    );
    assert_no_temp_manifests(dir.path());

    // 6. Full-build path end-to-end (mock `cargo` + mock `package-release.sh`
    // in a throwaway git repo with `TMPDIR=/tmp`):
    // - Verifies `git clone --no-checkout --shared` + detached checkout.
    // - Verifies `run_in_allowlisted_build_env` constructs an explicit allowlisted
    //   env (preserving `LD_LIBRARY_PATH` for build-time bootstrap while dropping
    //   all 18 poisoned/unallowlisted vars), logs `build-env-keys:` to stdout, and
    //   that BOTH `cargo` and `package-release.sh` receive the EXACT logged key set.
    // - Verifies the installed manifest has mode `0644` and verifies cleanly.
    let mock_root = dir.path().join("mock-repo");
    fs::create_dir_all(mock_root.join("scripts/ci")).expect("mock scripts/ci");
    fs::copy(&script, mock_root.join("scripts/ci/build-perf-baseline.sh"))
        .expect("copy build-perf-baseline.sh");
    fs::copy(
        repo_root.join("scripts/ci/verify-perf-baseline.sh"),
        mock_root.join("scripts/ci/verify-perf-baseline.sh"),
    )
    .expect("copy verify-perf-baseline.sh");
    fs::set_permissions(
        mock_root.join("scripts/ci/build-perf-baseline.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("chmod build script");
    fs::set_permissions(
        mock_root.join("scripts/ci/verify-perf-baseline.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("chmod verify script");

    let env_capture_dir = dir.path().join("captured-env");
    fs::create_dir_all(&env_capture_dir).expect("capture dir");
    let cargo_env_file = env_capture_dir.join("cargo.env");
    let pkg_env_file = env_capture_dir.join("package.env");
    let mock_bin_dir = dir.path().join("mock-bin");
    fs::create_dir_all(&mock_bin_dir).expect("mock-bin");

    let mock_cargo = mock_bin_dir.join("cargo");
    fs::write(
        &mock_cargo,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nenv | sort > \"{}\"\nmkdir -p target/release\n",
            cargo_env_file.display()
        ),
    )
    .expect("write mock cargo");
    fs::set_permissions(&mock_cargo, fs::Permissions::from_mode(0o755)).expect("chmod mock cargo");

    let mock_pkg = mock_root.join("scripts/package-release.sh");
    fs::write(
        &mock_pkg,
        format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
env | sort > "{}"
target=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --target) target="$2"; shift 2 ;;
    *) shift ;;
  esac
done
root="neomacs-0.0.21-${{target}}"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$root/bin" "$stage/$root/libexec/neomacs/0.0.21/$target"
printf '#!/bin/sh\nexit 0\n' > "$stage/$root/bin/neomacs"
chmod 0755 "$stage/$root/bin/neomacs"
printf 'mock-release-pdump' > "$stage/$root/libexec/neomacs/0.0.21/$target/neomacs.pdump"
mkdir -p dist
tar -czf "dist/${{root}}.tar.gz" -C "$stage" "$root"
"#,
            pkg_env_file.display()
        ),
    )
    .expect("write mock package-release.sh");
    fs::set_permissions(&mock_pkg, fs::Permissions::from_mode(0o755))
        .expect("chmod mock package-release.sh");

    for git_args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "test@example.invalid"],
        vec!["config", "user.name", "Test"],
        vec!["add", "."],
        vec!["commit", "-q", "-m", "mock v0.0.21"],
    ] {
        let st = Command::new("git")
            .current_dir(&mock_root)
            .args(&git_args)
            .status()
            .expect("git setup");
        assert!(st.success(), "git {git_args:?} failed");
    }
    let mock_commit = String::from_utf8(
        Command::new("git")
            .current_dir(&mock_root)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("git rev-parse")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();

    let host_path = std::env::var("PATH").unwrap_or_default();
    let mock_path = format!("{}:{host_path}", mock_bin_dir.display());
    let expected_ld_lib = match std::env::var("LD_LIBRARY_PATH") {
        Ok(existing) if !existing.is_empty() => format!("/mock/lib/needed-for-temacs:{existing}"),
        _ => "/mock/lib/needed-for-temacs".to_owned(),
    };
    let mock_out_dir = dir.path().join("installed-baseline");
    let mock_manifest_out = dir.path().join("mock-v0.0.21-release.json");

    let mut cmd = Command::new("bash");
    cmd.arg(mock_root.join("scripts/ci/build-perf-baseline.sh"))
        .args([
            "--ref",
            &mock_commit,
            "--version",
            "v0.0.21-release",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--rustc-version",
            "rustc 1.96.1 (31fca3adb 2026-06-26)",
            "--out-dir",
            mock_out_dir.to_str().unwrap(),
            "--manifest",
            mock_manifest_out.to_str().unwrap(),
        ])
        .env("PATH", &mock_path)
        .env("TMPDIR", "/tmp")
        .env("LD_LIBRARY_PATH", &expected_ld_lib)
        .env("NEOMACS_RELEASE_DIR", "/tmp/poisoned-pgo-dir")
        .env("CARGO_TARGET_DIR", "/tmp/poisoned-target")
        .env("RUSTFLAGS", "-C profile-use=/tmp/fake.profdata")
        .env(
            "CARGO_ENCODED_RUSTFLAGS",
            "-C\x1fprofile-use=/tmp/fake.profdata",
        )
        .env("CARGO_BUILD_RUSTFLAGS", "-C opt-level=0")
        .env("CARGO_BUILD_TARGET", "aarch64-unknown-linux-gnu")
        .env("CARGO_INCREMENTAL", "1")
        .env("CARGO_PROFILE_RELEASE_LTO", "off")
        .env(
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "-C lto=off",
        )
        .env("RUSTC_WRAPPER", "/bin/false")
        .env("RUSTC_WORKSPACE_WRAPPER", "/bin/false")
        .env("NEOMACS_RUNTIME_ROOT", "/tmp/poisoned-runtime")
        .env("NEOVM_JIT", "0")
        .env("RUST_LOG", "trace")
        .env("LD_PRELOAD", "/tmp/nonexistent-preload.so")
        .env("LD_AUDIT", "/tmp/nonexistent-audit.so")
        .env("DYLD_INSERT_LIBRARIES", "/tmp/nonexistent-insert.dylib")
        .env("UNALLOWLISTED_CUSTOM_VAR", "must-not-leak");

    let e2e = cmd.output().expect("run mock full-build");
    assert!(
        e2e.status.success(),
        "mock full-build must succeed across TMPDIR=/tmp: stdout={} stderr={}",
        String::from_utf8_lossy(&e2e.stdout),
        String::from_utf8_lossy(&e2e.stderr)
    );

    let e2e_stdout = String::from_utf8_lossy(&e2e.stdout);
    let logged_keys_line = e2e_stdout
        .lines()
        .find_map(|line| line.strip_prefix("build-env-keys: "))
        .expect("stdout must log allowlisted build-env-keys");
    let logged_keys: std::collections::BTreeSet<String> = logged_keys_line
        .split_whitespace()
        .map(str::to_owned)
        .collect();

    let parse_env_map = |path: &std::path::Path| -> std::collections::BTreeMap<String, String> {
        fs::read_to_string(path)
            .expect("read captured env")
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    };
    let cargo_env = parse_env_map(&cargo_env_file);
    let pkg_env = parse_env_map(&pkg_env_file);

    // Set-compare: every child env key (excluding bash's implicit `PWD`/`SHLVL`/`_`)
    // MUST equal the logged allowlisted build-env key set.
    let strip_bash_implicit = |map: &std::collections::BTreeMap<String, String>| {
        map.keys()
            .filter(|k| !matches!(k.as_str(), "PWD" | "SHLVL" | "_"))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        strip_bash_implicit(&cargo_env),
        logged_keys,
        "cargo child env keys must equal logged allowlisted build-env-keys"
    );
    assert_eq!(
        strip_bash_implicit(&pkg_env),
        logged_keys,
        "package-release.sh child env keys must equal logged allowlisted build-env-keys"
    );

    // `LD_LIBRARY_PATH` MUST be preserved for build-time bootstrap (`neomacs-temacs`).
    assert_eq!(
        cargo_env.get("LD_LIBRARY_PATH").map(String::as_str),
        Some(expected_ld_lib.as_str())
    );
    assert_eq!(
        pkg_env.get("LD_LIBRARY_PATH").map(String::as_str),
        Some(expected_ld_lib.as_str())
    );

    // Every poisoned / unallowlisted variable MUST be absent from both children.
    for forbidden in [
        "NEOMACS_RELEASE_DIR",
        "CARGO_TARGET_DIR",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
        "CARGO_BUILD_TARGET",
        "CARGO_INCREMENTAL",
        "CARGO_PROFILE_RELEASE_LTO",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "NEOMACS_RUNTIME_ROOT",
        "NEOVM_JIT",
        "RUST_LOG",
        "LD_PRELOAD",
        "LD_AUDIT",
        "DYLD_INSERT_LIBRARIES",
        "UNALLOWLISTED_CUSTOM_VAR",
    ] {
        assert!(
            !cargo_env.contains_key(forbidden),
            "cargo env leaked {forbidden}"
        );
        assert!(
            !pkg_env.contains_key(forbidden),
            "package-release.sh env leaked {forbidden}"
        );
    }

    // Installed manifest must have mode 0644 (not mktemp's 0600) and verify cleanly.
    let mode = fs::metadata(&mock_manifest_out)
        .expect("stat manifest")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o644, "atomic manifest install must set mode 0644");
    let (loaded, sha) = BaselineManifest::load(&mock_manifest_out).expect("load mock manifest");
    loaded
        .verify_editor(&mock_manifest_out, sha, &mock_out_dir.join("bin/neomacs"))
        .expect("verify installed mock baseline");
}

#[test]
fn baseline_manifest_rejects_empty_optional_provenance_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (field, value) in [
        ("build_profile", ""),
        ("rustc_version", "   "),
        ("pgo_profile", "\t\n"),
        ("codegen_settings", "  "),
    ] {
        let path = dir.path().join(format!("empty-{field}.json"));
        let json = serde_json::json!({
            "schema_version": 1,
            "version": "v0.0.21-release",
            "git_commit": "7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a",
            "target": "x86_64-unknown-linux-gnu",
            field: value,
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
                "relative_to_binary": "../libexec/neomacs.pdump",
                "size_bytes": 1,
                "sha256": "0".repeat(64)
            }
        });
        fs::write(&path, serde_json::to_vec(&json).expect("json")).expect("write");
        let err = BaselineManifest::load(&path)
            .expect_err("empty optional provenance field must be rejected");
        assert!(
            matches!(err, PerfError::InvalidBaselineManifest { ref message, .. } if message.contains(field)),
            "expected InvalidBaselineManifest mentioning {field}, got {err:?}"
        );
    }
}
