//! GUI regression for issue #414: `minibuffer-line' must render its status
//! text in the minibuffer window of a graphical frame.
//!
//! The package's whole mechanism rests on the miniwindow displaying the
//! permanently-inactive minibuffer buffer `" *Minibuf-0*"' (GNU:
//! `init_minibuf_once' creates it at startup; `minibuffer_unwind' points the
//! miniwindow back at it after every minibuffer exit).  The mode writes
//! `format-mode-line' output into that buffer, and redisplay must surface it
//! in the miniwindow.  This test loads the real GNU ELPA package source
//! (fixtures/minibuffer-line.el), enables the mode with a deterministic
//! format, and asserts the *rendered* frame snapshot — what redisplay
//! actually produced — shows the marker on the miniwindow row.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use neomacs_infra::packages;
use std::{fs, path::PathBuf, time::Duration};

#[test]
fn minibuffer_line_renders_in_the_graphical_miniwindow() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let backend = if cfg!(target_os = "macos") {
        GuiBackend::Macos
    } else {
        GuiBackend::LinuxX11
    };
    let artifacts = root.join("target/neomacs-gui-tests/minibuffer-line");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .expect("display session should start");
    let program = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    // Provision the package source through the shared cache so this suite
    // exercises the same pinned bytes as the TUI parity tests.
    let source =
        std::fs::read_to_string(root.join("crates/neomacs-gui-tests/fixtures/minibuffer-line.el"))
            .expect("read GNU ELPA minibuffer-line source");
    let provisioned = packages::source_file("minibuffer-line", &source)
        .expect("provision minibuffer-line source");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new(
            "minibuffer-line-gui",
            root.join("crates/neomacs-gui-tests/fixtures/minibuffer-line-gui.el"),
        ),
    )
    .with_program(program)
    .with_env(
        "NEOMACS_PACKAGE_SOURCE",
        provisioned.path().to_string_lossy(),
    )
    .with_env("RUST_LOG", "warn");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .expect("GUI run should produce artifacts");
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    assert!(
        !result.timed_out && result.exit_code == Some(0),
        "{result:#?}\n{stderr}"
    );
    assert!(stderr.contains("MBL-GUI-PASS"), "{stderr}");

    // Display oracle: the marker must be present in the rendered frame
    // snapshot, not merely in the minibuffer-line buffer's own text.
    let snapshot = fs::read_to_string(&result.artifacts.frame_snapshot_txt)
        .expect("frame snapshot text artifact");
    assert!(
        snapshot.contains("MBL-GUI-MARKER"),
        "minibuffer-line marker missing from the rendered frame snapshot:\n{snapshot}"
    );
}
