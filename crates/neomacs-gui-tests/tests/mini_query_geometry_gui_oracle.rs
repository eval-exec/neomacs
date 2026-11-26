//! Same-height echo messages from a synchronous window-end fontifier.
//!
//! GNU 31.1 at a360712c uses a stack-local iterator for Fwindow_end
//! (window.c:1831-1897). Fontification runs under dsafe__call's dynamic
//! inhibit-redisplay binding (xdisp.c:3104-3141), which Lisp can rebind. A
//! one-line message succeeds without changing the minibuffer height or
//! forcing the nested complete redisplay that aborts the enclosing iterator.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, process::Command, time::Duration};

#[test]
fn window_end_fontification_message_matches_gnu() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/mini-query-geometry-{}", std::process::id()));
    fs::create_dir_all(&artifacts).expect("create paired mini-query artifacts");
    let gnu = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH")
            .expect("set NEOVM_FORCE_ORACLE_PATH to the checked GNU GUI oracle"),
    );
    let prefix = gnu
        .parent()
        .and_then(|bin| bin.parent())
        .expect("GNU oracle must have a prefix/bin executable");
    let attestation = Command::new(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&gnu)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .expect("attest the pinned GNU oracle before the paired run");
    fs::write(artifacts.join("oracle.stdout"), &attestation.stdout).unwrap();
    fs::write(artifacts.join("oracle.stderr"), &attestation.stderr).unwrap();
    assert!(
        attestation.status.success(),
        "GNU oracle attestation failed: {attestation:?}"
    );
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts)
        .expect("the paired mini-query row requires an X11 display");
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/mini-query-geometry-gui-oracle.el");
    fs::copy(&fixture, artifacts.join("fixture.el")).expect("retain the exact paired form");
    let mut values = Vec::new();
    for (editor, program) in [("gnu", gnu), ("neo", neo)] {
        let result = artifacts.join(format!("{editor}.el"));
        let mut env = session.env().to_vec();
        env.extend([
            ("GDK_BACKEND".into(), "x11".into()),
            // This immutable process policy is read through OnceLock. It must
            // be selected before startup, not by setenv inside the fixture.
            ("NEOMACS_REDISPLAY_GNU_HOOKS".into(), "on".into()),
            (
                "NEOMACS_MINI_QUERY_ORACLE_RESULT".into(),
                result.display().to_string(),
            ),
        ]);
        let output = ProcessGuiCommandRunner
            .run(
                &CommandSpec {
                    program,
                    args: vec!["-Q".into(), "-l".into(), fixture.display().to_string()],
                    env,
                },
                &GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, editor),
                &GuiRunOptions::with_timeout(Duration::from_secs(30)),
            )
            .expect("run the paired single-thread mini-query fixture");
        assert!(!output.timed_out, "{editor}: {output:#?}");
        assert_eq!(output.exit_code, Some(0), "{editor}: {output:#?}");
        values.push(fs::read_to_string(result).expect("editor must emit its query witness"));
    }
    assert_eq!(
        values[0], "(t t (ok \"query fontify\") t nil \"query fontify\")\n",
        "GNU must run the fontifier, accept its message, and complete the query"
    );
    assert_eq!(
        values[1], values[0],
        "mini-query geometry divergence; artifacts: {artifacts:?}"
    );
}
