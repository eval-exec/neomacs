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
        let small = if case["policy"] == "hintfull" {
            5.0
        } else {
            7.0
        };
        for (phase, size, advance, average) in [("base", 14, 13.0, 7), ("scaled", 7, small, 3)] {
            for editor in ["gnu", "neo"] {
                let state = &case[editor][phase];
                for (key, expected) in json!({"family": "Noto Sans", "weight": "regular",
                    "slant": "normal", "width": "normal", "size": size,
                    "m_width": advance, "window_width": average, "frame_width": 7})
                .as_object()
                .unwrap()
                {
                    assert_eq!(
                        &state[key], expected,
                        "{editor}/{phase}/{key}: {case}; {dir:?}"
                    );
                }
                assert_eq!(
                    fs::canonicalize(state["file"].as_str().unwrap()).unwrap(),
                    font
                );
                if editor == "gnu" {
                    assert!(state["font_entity"].as_str().unwrap().ends_with(" . 0)"));
                }
            }
            assert_eq!(
                case["gnu"][phase]["m_width"], case["neo"][phase]["m_width"],
                "{case}"
            );
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
