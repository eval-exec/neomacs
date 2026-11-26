//! Applied native allocations retire sent intent before a later font resize.
use neomacs_gui_tests::{
    GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use neomacs_infra::display::{WestonBenchConfig, WestonBenchSession, WestonDesktop};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[test]
fn x11_rejected_resize_restores_native_grid_before_font_change_like_gnu() {
    paired_rejection(GuiBackend::LinuxX11);
}

#[test]
fn wayland_rejected_resize_restores_native_grid_before_font_change_like_gnu() {
    paired_rejection(GuiBackend::LinuxWayland);
}

fn paired_rejection(backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "target/neomacs-gui-tests/native-resize-rejection-{}-{}-{}",
        backend.slug(),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    assert!(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH").is_some(),
        "set NEOVM_FORCE_ORACLE_PATH to the prescribed tmp/gnu-emacs-oracle/bin/emacs"
    );
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "requires the pinned GNU GUI oracle");
    let gnu = oracle.executable_path();
    let prefix = gnu.parent().unwrap().parent().unwrap();
    let attestation = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&gnu)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .expect("attest the prescribed GNU binary");
    fs::write(artifacts.join("oracle.stdout"), &attestation.stdout).unwrap();
    fs::write(artifacts.join("oracle.stderr"), &attestation.stderr).unwrap();
    assert!(attestation.status.success(), "{attestation:?}");
    let fontconfig = crate::desktop_font_pins::desktop_font_fontconfig(&artifacts);
    let session = WestonBenchSession::start(
        &artifacts.join("display"),
        WestonBenchConfig {
            width: 1920,
            height: 1080,
            xwayland: true,
            desktop: WestonDesktop::Solid,
        },
    )
    .expect("owned Weston with Xwayland is required for the GNU control");
    let reference = observe(
        &root,
        &artifacts.join("gnu"),
        &gnu,
        GuiBackend::LinuxX11,
        &session,
        &fontconfig,
    );
    let neomacs = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let actual = observe(
        &root,
        &artifacts.join("neo"),
        &neomacs,
        backend,
        &session,
        &fontconfig,
    );
    for observations in [&reference, &actual] {
        let baseline = phase(observations, "baseline");
        assert_eq!(baseline["columns"], 91, "{observations}");
        assert_eq!(
            phase(observations, "after_duplicate_requests")["native_geometry_unchanged"],
            true,
            "the window manager must reject the fullscreen size requests: {observations}"
        );
        let restored = phase(observations, "restored_before_font");
        for field in ["native_width", "native_height", "columns", "rows"] {
            assert_eq!(restored[field], baseline[field], "{field}: {observations}");
        }
        let enlarged = phase(observations, "font_change_settled");
        for field in ["columns", "rows", "parameter_width", "parameter_height"] {
            assert_eq!(enlarged[field], baseline[field], "{field}: {observations}");
        }
        for field in ["char_width", "char_height", "native_width", "native_height"] {
            assert!(
                enlarged[field].as_u64().unwrap() > baseline[field].as_u64().unwrap(),
                "the font phase must change {field}: {observations}"
            );
        }
    }
    // Intermediate fullscreen geometry belongs to the toolkit/WM, and a
    // suppressed unchanged X11 configure need not produce a completion.
    // Compare only after the authoritative restore and settled font resize.
    for stage in ["restored_before_font", "font_change_settled"] {
        for field in ["columns", "rows", "parameter_width", "parameter_height"] {
            assert_eq!(
                phase(&actual, stage)[field],
                phase(&reference, stage)[field],
                "{stage}/{field}; artifacts={}",
                artifacts.display()
            );
        }
    }
}

fn observe(
    root: &Path,
    artifacts: &Path,
    binary: &Path,
    backend: GuiBackend,
    session: &WestonBenchSession,
    fontconfig: &Path,
) -> Value {
    let scenario = "native-resize-rejection-oracle";
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/native-resize-rejection-oracle.el");
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new(scenario, &fixture),
    )
    .without_paint_proof()
    .with_program(binary)
    .with_args([
        "-Q".to_string(),
        "--font=DejaVu Sans Mono-10".to_string(),
        "--geometry=80x35".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env("FONTCONFIG_FILE", fontconfig.display().to_string())
    .with_env("GDK_BACKEND", "x11")
    .with_env("GSETTINGS_BACKEND", "memory")
    .with_env("RUST_LOG", "info");
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    let paths = GuiArtifactSet::new(artifacts, backend, scenario);
    fs::create_dir_all(paths.json.parent().unwrap()).unwrap();
    let command = plan.command_spec();
    // The shared runner removes WAYLAND_DISPLAY and WAYLAND_SOCKET for X11,
    // or DISPLAY for Wayland; this controls winit's actual backend selection.
    let output = ProcessGuiCommandRunner
        .run(
            &command,
            &paths,
            &GuiRunOptions::with_timeout(Duration::from_secs(55)),
        )
        .unwrap();
    fs::write(&paths.stdout, &output.stdout).unwrap();
    fs::write(&paths.stderr, &output.stderr).unwrap();
    fs::write(
        &paths.json,
        serde_json::to_vec_pretty(&serde_json::json!({
            "backend": backend.slug(), "program": binary, "args": command.args,
            "exit_code": output.exit_code, "timed_out": output.timed_out,
            "state": paths.gui_state,
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(
        !output.timed_out,
        "artifacts={}: {output:?}",
        artifacts.display()
    );
    assert_eq!(
        output.exit_code,
        Some(0),
        "artifacts={}: {output:?}",
        artifacts.display()
    );
    let state: Value = serde_json::from_slice(&fs::read(&paths.gui_state).unwrap()).unwrap();
    let initial = phase(&state, "initial");
    match backend {
        GuiBackend::LinuxX11 => {
            assert_eq!(initial["wayland_display"], "UNSET");
            assert_eq!(initial["wayland_socket"], "UNSET");
            assert_ne!(initial["display"], "UNSET");
        }
        GuiBackend::LinuxWayland => {
            assert_ne!(initial["wayland_display"], "UNSET");
            assert_eq!(initial["display"], "UNSET");
        }
        _ => unreachable!("this pair requires the owned Linux compositor"),
    }
    // A watchdog/error observation cannot masquerade as a completed fixture.
    phase(&state, "font_change_settled");
    state
}

fn phase<'a>(state: &'a Value, name: &str) -> &'a Value {
    state["observations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["phase"] == name)
        .unwrap_or_else(|| panic!("missing phase {name}: {state}"))
}
