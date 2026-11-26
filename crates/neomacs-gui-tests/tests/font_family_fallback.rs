//! GNU font.c:3115,3219 and xfaces.c:3956,6008 distinguish requested
//! family preferences from mandatory specs and realized families.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiCommandRunner, GuiRunOptions, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MONO: &str = "DejaVu Sans Mono";
const ALPHA: &str = "Neomacs Missing Family Alpha";
const BETA: &str = "Neomacs Missing Family Beta";
const PHASES: &[&str] = &[
    "initial",
    "redisplayed",
    "settled",
    "remapped",
    "remapped-settled",
];

#[test]
fn default_family_fallback_realization_and_height_remap_match_attested_gnu() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let gnu = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH")
            .expect("set NEOVM_FORCE_ORACLE_PATH to the pinned GNU GUI executable"),
    );
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "requires the pinned GNU oracle");
    assert_eq!(fs::canonicalize(&gnu).unwrap(), oracle.executable_path());
    let root = neomacs_infra::workspace_root();
    let dir = root.join(format!(
        "tmp/neomacs-gui-tests/font-family-fallback-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    attest(&root, &dir, &gnu);
    fs::write(dir.join("oracle-stamp.txt"), oracle.stamp()).unwrap();
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    for (name, binary) in [("gnu", &gnu), ("neo", &neo)] {
        let version = Command::new(binary).arg("--version").output().unwrap();
        fs::write(dir.join(format!("{name}-version.stdout")), &version.stdout).unwrap();
        fs::write(dir.join(format!("{name}-version.stderr")), &version.stderr).unwrap();
        assert!(version.status.success(), "{version:?}");
    }
    let configs = [
        fontconfig(&dir.join("mono-noto"), false),
        fontconfig(&dir.join("same-foundry"), true),
    ];
    let display = DisplayHarness::Xvfb.start_session(&dir).unwrap();
    let resources = dir.join("Xresources");
    fs::write(&resources, "Xft.dpi: 96\n").unwrap();
    let xrdb = Command::new("xrdb")
        .arg("-merge")
        .arg(resources)
        .envs(display.env().iter().cloned())
        .output()
        .unwrap();
    assert!(xrdb.status.success(), "{xrdb:?}");
    let mut matrix = Vec::new();
    for (name, family, initial, scope, operation) in [
        ("known-mono", MONO, MONO, "selected", "family"),
        ("missing-default", "default", MONO, "selected", "family"),
        ("missing-alpha", ALPHA, MONO, "selected", "family"),
        ("missing-beta", BETA, MONO, "selected", "family"),
        ("all-frames", ALPHA, MONO, "nil", "family"),
        ("new-frame-default", BETA, MONO, "t", "family"),
        ("mandatory-family", ALPHA, MONO, "selected", "font-spec"),
        (
            "mandatory-name",
            "Missing Font-17",
            MONO,
            "selected",
            "font-name",
        ),
        ("from-sans", ALPHA, "DejaVu Sans", "selected", "family"),
        ("from-serif", ALPHA, "DejaVu Serif", "selected", "family"),
    ] {
        let config = &configs[usize::from(initial != MONO)];
        let mut results = Vec::new();
        for (editor, binary) in [("gnu", &gnu), ("neo", &neo)] {
            let mut plan = GuiTestPlan::new(
                GuiBackend::LinuxX11,
                &root,
                dir.join(name),
                GuiScenario::new(
                    editor,
                    root.join("crates/neomacs-gui-tests/fixtures/font-family-fallback.el"),
                ),
            )
            .with_program(binary)
            .with_env("FONTCONFIG_FILE", config.display().to_string())
            .with_env("GDK_BACKEND", "x11")
            .with_env("GDK_SCALE", "1")
            .with_env("GDK_DPI_SCALE", "1")
            .with_env("GSETTINGS_BACKEND", "memory")
            .with_env("FONT_FALLBACK_FAMILY", family)
            .with_env("FONT_FALLBACK_INITIAL", initial)
            .with_env("FONT_FALLBACK_SCOPE", scope)
            .with_env("FONT_FALLBACK_OPERATION", operation);
            for (key, value) in display.env() {
                plan = plan.with_env(key, value);
            }
            results.push(run_editor(plan));
        }
        matrix.push(json!({"case": name, "family": family, "initial": initial,
            "scope": scope, "operation": operation, "gnu": results[0], "neo": results[1]}));
    }
    fs::write(
        dir.join("matrix.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
    // Save all rows before comparison.
    for row in &matrix {
        compare(row, &dir);
    }
    assert!(
        matrix
            .iter()
            .filter(|r| r["initial"] != MONO)
            .any(|r| r["gnu"]["phases"]["redisplayed"]["family"] != r["initial"]),
        "same-foundry controls must select beyond the old font"
    );
}

fn attest(root: &Path, dir: &Path, gnu: &Path) {
    let prefix = gnu.parent().unwrap().parent().unwrap();
    let output = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(gnu)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .unwrap();
    fs::write(dir.join("oracle-check.stdout"), &output.stdout).unwrap();
    fs::write(dir.join("oracle-check.stderr"), &output.stderr).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("oracle profile: linux-gtk3"));
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
}

fn fontconfig(dir: &Path, expanded: bool) -> PathBuf {
    let fonts = dir.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    let dejavu = neomacs_test_fonts::dejavu_2_37();
    let mut files = vec![
        ("DejaVuSansMono.ttf", dejavu.mono_regular()),
        ("NotoSans.ttf", neomacs_test_fonts::noto_sans_variable()),
        (
            "NotoSansMono.ttf",
            neomacs_test_fonts::noto_sans_mono_variable(),
        ),
    ];
    if expanded {
        files.extend([
            ("DejaVuSans.ttf", dejavu.sans_regular()),
            ("DejaVuSerif.ttf", dejavu.serif_regular()),
        ]);
    }
    // Fixture accessors verify release and file hashes.
    for (name, source) in files {
        fs::copy(source, fonts.join(name)).unwrap();
    }
    let cache = dir.join("fc-cache");
    fs::create_dir_all(&cache).unwrap();
    let escape = |p: &Path| {
        p.to_str()
            .unwrap()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
    };
    let config = dir.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",
            escape(&fonts),
            escape(&cache)
        ),
    )
    .unwrap();
    config
}

fn run_editor(plan: GuiTestPlan) -> Value {
    // Font and geometry APIs need no GPU readback.
    let files = plan.write_manifest().unwrap();
    let command = plan.command_spec();
    let output = ProcessGuiCommandRunner
        .run(
            &command,
            &files,
            &GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    fs::write(&files.stdout, &output.stdout).unwrap();
    fs::write(&files.stderr, &output.stderr).unwrap();
    assert!(!output.timed_out, "{files:?}");
    assert_eq!(output.exit_code, Some(0), "{files:?}");
    let state: Value = serde_json::from_slice(&fs::read(&files.gui_state).unwrap()).unwrap();
    fs::write(
        &files.json,
        serde_json::to_vec_pretty(&json!({"program": command.program,
        "args": command.args, "env": command.env, "exit_code": output.exit_code,
        "timed_out": output.timed_out, "state": state}))
        .unwrap(),
    )
    .unwrap();
    state
}

fn comparable(state: &Value) -> Value {
    let mut state = state.clone();
    // Preserve object diagnostics outside the comparison.
    for (phase, value) in state["phases"].as_object_mut().unwrap() {
        if phase == "immediate" {
            value.as_object_mut().unwrap().remove("font");
        } else if let Some(value) = value.as_object_mut() {
            value.remove("frame_font");
            for field in ["raw", "raw_after_queries"] {
                value[field].as_object_mut().unwrap().remove("font");
            }
            assert_eq!(
                value["raw"], value["raw_after_queries"],
                "query changed raw face attributes"
            );
        }
    }
    for error in state["errors"].as_array_mut().unwrap() {
        error.as_object_mut().unwrap().remove("printed");
    }
    state
}

fn compare(row: &Value, dir: &Path) {
    let expected = &row["gnu"];
    let label = format!("{}; artifacts: {}", row["case"], dir.display());
    assert_eq!(comparable(&row["neo"]), comparable(expected), "{label}");
    assert_eq!(
        expected["errors"].as_array().unwrap().len(),
        usize::from(row["operation"] != "family"),
        "{label}"
    );
    for phase in PHASES {
        let state = &expected["phases"][phase];
        assert_eq!(
            state["size"],
            if phase.starts_with("remapped") { 7 } else { 14 },
            "{label}"
        );
        assert!(state["m_width"].as_f64().unwrap() > 0.0, "{label}");
        let file = Path::new(state["file"].as_str().unwrap());
        assert!(
            file.is_file() && file.starts_with(dir),
            "opened unpinned font: {label}"
        );
    }
}
