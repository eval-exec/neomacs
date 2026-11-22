//! GNU ftcrfont.c:156-163,255 and :99-103 use the selected hinting policy.
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

#[test]
fn fontconfig_hintstyle_preserves_scaled_noto_advances_against_attested_gnu() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let gnu = PathBuf::from(std::env::var_os("NEOVM_FORCE_ORACLE_PATH").unwrap());
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "attested GNU oracle required");
    assert_eq!(fs::canonicalize(&gnu).unwrap(), oracle.executable_path());
    let root = neomacs_infra::workspace_root();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = root.join(format!(
        "tmp/neomacs-gui-tests/font-hinting-advances-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    attest(&root, &dir, &gnu);
    fs::write(dir.join("oracle-stamp.txt"), oracle.stamp()).unwrap();
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let fonts = dir.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    // This accessor verifies NotoSans-variable.ttf SHA bfb7bb691513f12e...
    let font = fonts.join("NotoSans-variable.ttf");
    fs::copy(neomacs_test_fonts::noto_sans_variable(), &font).unwrap();
    let font = fs::canonicalize(font).unwrap();
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
    for policy in ["hintfull", "hintslight"] {
        let case = dir.join(policy);
        fs::create_dir_all(&case).unwrap();
        let config = fontconfig(&case, &fonts, policy);
        let mut states = Vec::new();
        for (editor, binary) in [("gnu", &gnu), ("neo", &neo)] {
            let mut plan = GuiTestPlan::new(
                GuiBackend::LinuxX11,
                &root,
                &case,
                GuiScenario::new(
                    editor,
                    root.join("crates/neomacs-gui-tests/fixtures/font_hinting_advances.el"),
                ),
            )
            .with_program(binary)
            .with_env("FONTCONFIG_FILE", config.display().to_string())
            .with_env("GDK_BACKEND", "x11")
            .with_env("GDK_SCALE", "1")
            .with_env("GDK_DPI_SCALE", "1")
            .with_env("GSETTINGS_BACKEND", "memory");
            for (key, value) in display.env() {
                plan = plan.with_env(key, value);
            }
            states.push(run_editor(plan));
        }
        matrix.push(json!({"policy": policy, "gnu": states[0], "neo": states[1]}));
    }
    fs::write(
        dir.join("matrix.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
    for case in &matrix {
        // FreeType builds may produce different hinted advances, including in
        // GNU. Compare the live oracle on this host rather than captured pixels.
        for (phase, size) in [("base", 14), ("scaled", 7)] {
            for editor in ["gnu", "neo"] {
                let state = &case[editor][phase];
                for (key, expected) in json!({"family": "Noto Sans", "weight": "regular",
                    "slant": "normal", "width": "normal", "size": size})
                .as_object()
                .unwrap()
                {
                    assert_eq!(
                        &state[key], expected,
                        "{editor}/{phase}/{key}: {case}; {dir:?}"
                    );
                }
                let average = state["font_average_width"].as_i64().unwrap();
                assert!(average > 0, "{editor}/{phase}: {case}; {dir:?}");
                assert!(state["font_max_width"].as_i64().unwrap() >= average);
                assert!(state["m_width"].as_f64().unwrap() > 0.0);
                assert_eq!(state["window_width"], json!(average), "{case}");
                // A buffer-local remap changes window metrics, while the frame
                // continues to use its unremapped default font's native average.
                assert_eq!(
                    state["frame_width"], case[editor]["base"]["font_average_width"],
                    "{editor}/{phase}: {case}; {dir:?}"
                );
                assert_eq!(
                    fs::canonicalize(state["file"].as_str().unwrap()).unwrap(),
                    font
                );
                if editor == "gnu" {
                    assert!(state["font_entity"].as_str().unwrap().ends_with(" . 0)"));
                }
            }
            for key in [
                "m_width",
                "font_average_width",
                "font_max_width",
                "window_width",
                "frame_width",
            ] {
                assert_eq!(
                    case["neo"][phase][key], case["gnu"][phase][key],
                    "{phase}/{key}: {case}; {dir:?}"
                );
            }
        }
    }
}

fn attest(root: &Path, dir: &Path, gnu: &Path) {
    let prefix = gnu.parent().unwrap().parent().unwrap();
    let check = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(gnu)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .unwrap();
    fs::write(dir.join("oracle-check.stdout"), &check.stdout).unwrap();
    fs::write(dir.join("oracle-check.stderr"), &check.stderr).unwrap();
    assert!(check.status.success(), "{check:?}");
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("oracle profile: linux-gtk3"));
    eprintln!("{stdout}");
}

fn fontconfig(dir: &Path, fonts: &Path, hintstyle: &str) -> PathBuf {
    let cache = dir.join("fc-cache");
    fs::create_dir_all(&cache).unwrap();
    let escape = |path: &Path| {
        path.to_str()
            .unwrap()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
    };
    let config = dir.join("fonts.conf");
    fs::write(
        &config,
        format!(
            r#"<fontconfig><dir>{}</dir><cachedir>{}</cachedir>
<match target="pattern">
<edit name="hinting" mode="assign"><bool>true</bool></edit>
<edit name="hintstyle" mode="assign"><const>{hintstyle}</const></edit>
<edit name="autohint" mode="assign"><bool>false</bool></edit>
<edit name="antialias" mode="assign"><bool>true</bool></edit>
<edit name="rgba" mode="assign"><const>none</const></edit>
</match></fontconfig>"#,
            escape(fonts),
            escape(&cache)
        ),
    )
    .unwrap();
    config
}

fn run_editor(plan: GuiTestPlan) -> Value {
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
    serde_json::from_slice(&fs::read(&files.gui_state).unwrap()).unwrap()
}

/// A family name does not turn per-glyph advances into frame columns. GNU
/// xterm.c:27153 always publishes the opened font's native average_width.
#[test]
fn fontconfig_frame_columns_follow_native_average_width_against_attested_gnu() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let gnu = PathBuf::from(std::env::var_os("NEOVM_FORCE_ORACLE_PATH").unwrap());
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "attested GNU oracle required");
    assert_eq!(fs::canonicalize(&gnu).unwrap(), oracle.executable_path());
    let root = neomacs_infra::workspace_root();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = root.join(format!(
        "tmp/neomacs-gui-tests/frame-native-average-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    attest(&root, &dir, &gnu);
    fs::write(dir.join("oracle-stamp.txt"), oracle.stamp()).unwrap();
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    for (editor, binary) in [("gnu", &gnu), ("neo", &neo)] {
        let version = Command::new(binary).arg("--version").output().unwrap();
        fs::write(
            dir.join(format!("{editor}-version.stdout")),
            &version.stdout,
        )
        .unwrap();
        fs::write(
            dir.join(format!("{editor}-version.stderr")),
            &version.stderr,
        )
        .unwrap();
        assert!(version.status.success(), "{version:?}");
    }
    let fonts = dir.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    let font = fonts.join("NotoSansMono-variable.ttf");
    // The accessor verifies the pinned release bytes and SHA256.
    fs::copy(neomacs_test_fonts::noto_sans_mono_variable(), &font).unwrap();
    let font = fs::canonicalize(font).unwrap();
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
    for policy in ["hintfull", "hintslight"] {
        let case = dir.join(policy);
        fs::create_dir_all(&case).unwrap();
        let config = fontconfig(&case, &fonts, policy);
        let mut states = Vec::new();
        for (editor, binary) in [("gnu", &gnu), ("neo", &neo)] {
            let mut plan = GuiTestPlan::new(
                GuiBackend::LinuxX11,
                &root,
                &case,
                GuiScenario::new(
                    editor,
                    root.join("crates/neomacs-gui-tests/fixtures/frame_native_average_width.el"),
                ),
            )
            .with_program(binary)
            .with_env("FONTCONFIG_FILE", config.display().to_string())
            .with_env("GDK_BACKEND", "x11")
            .with_env("GDK_SCALE", "1")
            .with_env("GDK_DPI_SCALE", "1")
            .with_env("GSETTINGS_BACKEND", "memory");
            for (key, value) in display.env() {
                plan = plan.with_env(key, value);
            }
            states.push(run_editor(plan));
        }
        matrix.push(json!({"policy":policy,"gnu":states[0],"neo":states[1]}));
    }
    // Keep every policy/editor result even when the first parity check fails.
    fs::write(
        dir.join("matrix.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
    for case in &matrix {
        for (phase, size) in [("size14", 14), ("size13", 13)] {
            for editor in ["gnu", "neo"] {
                let state = &case[editor][phase];
                for (key, expected) in json!({"family":"Noto Sans Mono","weight":"regular",
                    "slant":"normal","width":"normal","size":size,"columns":80})
                .as_object()
                .unwrap()
                {
                    assert_eq!(
                        &state[key], expected,
                        "{editor}/{phase}/{key}: {case}; {dir:?}"
                    );
                }
                let average = state["font_average_width"].as_i64().unwrap();
                assert!(average > 0, "{editor}/{phase}: {case}; {dir:?}");
                assert!(state["font_max_width"].as_i64().unwrap() >= average);
                for key in [
                    "window_width",
                    "immediate_frame_width",
                    "frame_width_before_queries",
                    "frame_width",
                ] {
                    assert_eq!(
                        state[key],
                        json!(average),
                        "{editor}/{phase}/{key}: {case}; {dir:?}"
                    );
                }
                assert_eq!(state["text_pixels"], json!(80 * average), "{case}");
                let advances = state["advances"].as_array().unwrap();
                assert_eq!(advances.len(), 4);
                assert!(advances.iter().all(|width| width.as_i64().unwrap() > 0));
                assert_eq!(
                    fs::canonicalize(state["file"].as_str().unwrap()).unwrap(),
                    font
                );
                if editor == "gnu" {
                    assert!(state["font_entity"].as_str().unwrap().ends_with(" . 0)"));
                }
            }
            for key in [
                "font_average_width",
                "font_max_width",
                "advances",
                "window_width",
                "immediate_frame_width",
                "frame_width_before_queries",
                "frame_width",
                "text_pixels",
                "frame_pixels",
            ] {
                assert_eq!(
                    case["neo"][phase][key], case["gnu"][phase][key],
                    "{phase}/{key}: {case}; {dir:?}"
                );
            }
        }
    }
}
