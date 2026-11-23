//! #832: GNU completes body rows before chrome Lisp mutates their source.
//! Source contract: pinned xdisp.c:21419-21483, display_mode_lines after body;
//! native cursor/matrix queries do not rerun chrome, fresh visibility does.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[test]
fn late_header_source_mutation_preserves_body_and_queries_like_gnu() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let gnu = fs::canonicalize(PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH").expect("set pinned GNU executable"),
    ))
    .expect("GNU executable");
    let neo = PathBuf::from(
        std::env::var_os("NEOMACS_GUI_TEST_BINARY").expect("set already-built Neo executable"),
    );
    assert!(neo.is_file(), "missing Neo executable: {neo:?}");
    let root = neomacs_infra::workspace_root();
    let runtime = std::env::var_os("NEOMACS_RUNTIME_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/chrome-source-832-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let prefix = gnu.parent().unwrap().parent().unwrap();
    let check = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&gnu)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .expect("GNU attestation command");
    fs::write(artifacts.join("oracle-check.stdout"), &check.stdout).unwrap();
    fs::write(artifacts.join("oracle-check.stderr"), &check.stderr).unwrap();
    assert!(check.status.success(), "GNU attestation failed: {check:?}");
    let proof = String::from_utf8_lossy(&check.stdout);
    assert!(proof.contains("oracle profile: linux-gtk3"), "{proof}");
    eprintln!("{proof}");
    let fonts = artifacts.join("fonts.conf");
    fs::create_dir_all(artifacts.join("fc-cache")).unwrap();
    fs::write(
        &fonts,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",
            neomacs_test_fonts::desktop_fonts().directory().display(),
            artifacts.join("fc-cache").display(),
        ),
    )
    .unwrap();
    // The pinned oracle recipe is GTK3/X11. The Neo arm explicitly exercises
    // native Wayland; this does not claim GNU PGTK/Wayland equivalence.
    let gnu_observation = run_editor(
        &root,
        &artifacts,
        &fonts,
        &runtime,
        "gnu",
        &gnu,
        GuiBackend::LinuxX11,
    );
    let neo_observation = run_editor(
        &root,
        &artifacts,
        &fonts,
        &runtime,
        "neo",
        &neo,
        GuiBackend::LinuxWayland,
    );
    assert_eq!(
        gnu_observation, neo_observation,
        "paired presence/query contract"
    );
}

fn run_editor(
    root: &Path,
    artifacts: &Path,
    fonts: &Path,
    runtime: &Path,
    editor: &str,
    binary: &Path,
    backend: GuiBackend,
) -> Vec<Vec<Value>> {
    let owned = artifacts.join(editor);
    fs::create_dir_all(&owned).unwrap();
    let session = DisplayHarness::for_backend(backend)
        .start_session(&owned.join("display"))
        .expect("owned display prerequisite");
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        &owned,
        GuiScenario::new(
            format!("{editor}-chrome-source-832"),
            root.join("crates/neomacs-gui-tests/fixtures/chrome-source-acceptance.el"),
        ),
    )
    .with_program(binary)
    .with_env(
        "NEOMACS_CHROME_SOURCE_RESULT",
        owned.join("observations.json").to_string_lossy(),
    )
    .with_env("FONTCONFIG_FILE", fonts.to_string_lossy())
    .with_env("GSETTINGS_BACKEND", "memory")
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("RUST_LOG", "warn");
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "TMPDIR",
    ] {
        let directory = owned.join(key.to_ascii_lowercase());
        fs::create_dir_all(&directory).unwrap();
        plan = plan.with_env(key, directory.to_string_lossy());
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    if backend == GuiBackend::LinuxX11 {
        plan = plan.with_env("GDK_BACKEND", "x11");
    }
    if editor == "neo" {
        plan = plan.with_env("NEOMACS_RUNTIME_ROOT", runtime.to_string_lossy());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .expect("owned editor process");
    assert!(
        !result.timed_out && result.exit_code == Some(0),
        "{editor}: {result:#?}"
    );
    assert_eq!(result.status, GuiRunStatus::Passed, "{editor}: {result:#?}");
    for path in [
        &result.artifacts.stdout,
        &result.artifacts.stderr,
        &result.artifacts.neomacs_log,
    ] {
        let log = fs::read_to_string(path).unwrap_or_default();
        for rejection in [
            "layout failed to converge",
            "LogicalInputsChanged",
            "DuplicateWindow",
        ] {
            assert!(
                !log.contains(rejection),
                "{editor}: {}: {log}",
                path.display()
            );
        }
    }
    let receipt: Value = serde_json::from_slice(
        &fs::read(owned.join("observations.json")).expect("completed fixture receipt"),
    )
    .unwrap();
    assert!(receipt["error"].is_null(), "{editor}: {receipt}");
    let samples = receipt["samples"]
        .as_array()
        .expect("actual redisplay samples");
    assert_eq!(samples.len(), 2, "{editor}: {receipt}");
    let fields = [
        "survived",
        "body_seen",
        "cursor_present",
        "line_height_present",
        "line_dimensions_present",
        "ends_match",
        "fresh_query_visible",
        "position_visible",
        "body_hit",
    ];
    let mut normalized = Vec::new();
    for sample in samples {
        assert!(
            sample["header_evaluations"].as_u64().is_some_and(|n| n > 0),
            "{editor}: {sample}"
        );
        for field in fields {
            assert_eq!(sample[field], true, "{editor}: {field}: {sample}");
        }
        assert_eq!(sample["matrix_query_evaluations"], 0, "{editor}: {sample}");
        assert_eq!(sample["fresh_query_evaluations"], 1, "{editor}: {sample}");
        assert!(
            sample["window_system"]
                .as_str()
                .is_some_and(|name| !name.is_empty() && name != "nil")
        );
        normalized.push(fields.iter().map(|field| sample[*field].clone()).collect());
    }
    let png = image::open(&result.artifacts.png)
        .expect("actual GUI paint PNG")
        .to_rgba8();
    assert!(png.width() > 200 && png.height() > 100);
    let first = *png.get_pixel(0, 0);
    assert!(
        png.pixels().any(|pixel| *pixel != first),
        "{editor}: blank paint readback"
    );
    normalized
}
