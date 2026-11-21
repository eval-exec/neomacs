//! Issue #359: matching a font file is insufficient if its raster policy is lost.
//!
//! GNU's pinned src/ftcrfont.c:144-171 passes the substituted Fontconfig
//! pattern to Cairo. Compare real ink as well as opened font metadata, under
//! two explicit policies, so always using full hinting cannot pass.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions, GuiRunStatus,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[test]
fn fontconfig_hintstyle_preserves_normal_and_bold_ink_against_attested_gnu() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let forced = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH")
            .expect("set NEOVM_FORCE_ORACLE_PATH to the pinned GNU GUI executable"),
    );
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "requires the pinned GNU oracle");
    assert_eq!(fs::canonicalize(&forced).unwrap(), oracle.executable_path());
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/font-hinting-weight-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let prefix = forced.parent().unwrap().parent().unwrap();
    let check = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&forced)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .unwrap();
    fs::write(artifacts.join("oracle-check.stdout"), &check.stdout).unwrap();
    fs::write(artifacts.join("oracle-check.stderr"), &check.stderr).unwrap();
    fs::write(artifacts.join("oracle-stamp.txt"), oracle.stamp()).unwrap();
    assert!(check.status.success(), "oracle check failed: {check:?}");
    assert!(String::from_utf8_lossy(&check.stdout).contains("oracle profile: linux-gtk3"));
    eprintln!("{}", String::from_utf8_lossy(&check.stdout));

    let fonts = pinned_plex_fonts(&artifacts);
    // One private server for both editors; never mutate the host's X resources.
    let display = DisplayHarness::Xvfb.start_session(&artifacts).unwrap();
    let resources = artifacts.join("Xresources");
    fs::write(&resources, "Xft.dpi: 96\n").unwrap();
    let xrdb = Command::new("xrdb")
        .arg("-merge")
        .arg(&resources)
        .envs(display.env().iter().cloned())
        .output()
        .unwrap();
    assert!(xrdb.status.success(), "private Xft.dpi setup: {xrdb:?}");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut matrix = Vec::new();
    for policy in ["hintslight", "hintfull"] {
        let directory = artifacts.join(policy);
        fs::create_dir_all(&directory).unwrap();
        let config = fontconfig(&directory, &fonts, policy);
        let gnu = run_editor(&root, &directory, &forced, "gnu", &config, display.env());
        let neo = run_editor(&root, &directory, &binary, "neo", &config, display.env());
        matrix.push(json!({"policy": policy, "gnu": gnu, "neo": neo}));
    }
    // Retain every row and both policy controls before asserting mismatches.
    fs::write(
        artifacts.join("matrix.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
    for cell in &matrix {
        let policy = cell["policy"].as_str().unwrap();
        for (index, (name, weight, filename)) in [
            ("normal", "regular", "IBMPlexSans-Regular.ttf"),
            ("bold", "bold", "IBMPlexSans-Bold.ttf"),
        ]
        .into_iter()
        .enumerate()
        {
            let expected_file = fonts.join(filename);
            for editor in ["gnu", "neo"] {
                let row = &cell[editor]["state"]["rows"][index];
                assert_eq!(row["name"], name, "{cell}; {artifacts:?}");
                assert_eq!(row["family"], "IBM Plex Sans", "{cell}; {artifacts:?}");
                assert_eq!(row["weight"], weight, "{cell}; {artifacts:?}");
                assert_eq!(row["slant"], "normal", "{cell}; {artifacts:?}");
                assert_eq!(row["size"], 16, "{cell}; {artifacts:?}");
                assert_eq!(
                    fs::canonicalize(row["file"].as_str().expect("opened filename")).unwrap(),
                    fs::canonicalize(&expected_file).unwrap(),
                    "{editor}/{policy}/{name} opened a different font; {artifacts:?}"
                );
            }
            let gnu = cell["gnu"]["ink"][index].as_f64().unwrap();
            let neo = cell["neo"]["ink"][index].as_f64().unwrap();
            assert!(gnu > 100.0 && neo > 100.0, "missing text ink: {cell}");
            let relative_error = (neo - gnu).abs() / gnu;
            eprintln!(
                "{policy}/{name}: GNU ink={gnu:.4}, Neo ink={neo:.4}, delta={relative_error:.4}"
            );
            assert!(
                relative_error <= 0.015,
                "{policy}/{name}: same-file integrated ink differs by {:.2}% (GNU={gnu}, Neo={neo}); artifacts: {}",
                relative_error * 100.0,
                artifacts.display()
            );
        }
    }
}

fn pinned_plex_fonts(artifacts: &Path) -> PathBuf {
    let fonts = artifacts.join("plex-fonts");
    fs::create_dir_all(&fonts).unwrap();
    for (filename, sha256) in [
        (
            "IBMPlexSans-Regular.ttf",
            "b47ebd72b5ce53b60f909033ff84d6fc12358cc147e2981f8c321ad3f36246b6",
        ),
        (
            "IBMPlexSans-Bold.ttf",
            "22f59d358e8b9b9fb38ac6ee26a7ea7263ffa1c89a9056c101acff5add91f5d6",
        ),
    ] {
        let url = format!(
            "https://raw.githubusercontent.com/IBM/plex/2e90fe366fdc27c12682db13e5aa20ded3d90908/packages/plex-sans/fonts/complete/ttf/{filename}"
        );
        let pinned = neomacs_infra::pinned::pinned_file(filename, &url, sha256)
            .expect("fetch and verify the official pinned IBM Plex face");
        fs::copy(pinned, fonts.join(filename)).unwrap();
    }
    fonts
}

fn xml_path(path: &Path) -> String {
    path.to_str()
        .unwrap()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
}

fn fontconfig(artifacts: &Path, plex: &Path, hintstyle: &str) -> PathBuf {
    let desktop = neomacs_test_fonts::desktop_fonts().directory();
    let cache = artifacts.join("fc-cache");
    fs::create_dir_all(&cache).unwrap();
    let config = artifacts.join("fonts.conf");
    fs::write(
        &config,
        format!(
            r#"<fontconfig>
<dir>{}</dir><dir>{}</dir><cachedir>{}</cachedir>
<match target="pattern">
<edit name="hinting" mode="assign"><bool>true</bool></edit>
<edit name="hintstyle" mode="assign"><const>{hintstyle}</const></edit>
<edit name="autohint" mode="assign"><bool>false</bool></edit>
<edit name="antialias" mode="assign"><bool>true</bool></edit>
<edit name="rgba" mode="assign"><const>none</const></edit>
<edit name="lcdfilter" mode="assign"><const>lcddefault</const></edit>
</match></fontconfig>"#,
            xml_path(plex),
            xml_path(&desktop),
            xml_path(&cache)
        ),
    )
    .unwrap();
    config
}

fn run_editor(
    root: &Path,
    directory: &Path,
    binary: &Path,
    editor: &str,
    fontconfig: &Path,
    display_env: &[(String, String)],
) -> Value {
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        root,
        directory,
        GuiScenario::new(
            editor,
            root.join("crates/neomacs-gui-tests/fixtures/font-hinting-weight.el"),
        ),
    )
    .with_program(binary)
    .with_env("FONTCONFIG_FILE", fontconfig.display().to_string())
    .with_env("GDK_BACKEND", "x11")
    .with_env("GDK_SCALE", "1")
    .with_env("GDK_DPI_SCALE", "1")
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
    for (key, value) in display_env {
        plan = plan.with_env(key, value);
    }
    let artifacts = GuiArtifactSet::new(directory, GuiBackend::LinuxX11, editor);
    let options = GuiRunOptions::with_timeout(Duration::from_secs(30));
    if editor == "gnu" {
        plan = plan.with_env(
            "NEOMACS_FONT_WEIGHT_GNU_PNG",
            artifacts.png.display().to_string(),
        );
        plan.write_manifest().unwrap();
        let output = ProcessGuiCommandRunner
            .run(&plan.command_spec(), &artifacts, &options)
            .unwrap();
        fs::write(&artifacts.stdout, &output.stdout).unwrap();
        fs::write(&artifacts.stderr, &output.stderr).unwrap();
        assert!(!output.timed_out, "GNU timed out: {directory:?}");
        assert_eq!(output.exit_code, Some(0), "{output:?}; {directory:?}");
    } else {
        let result = plan
            .run_with(&mut ProcessGuiCommandRunner, options)
            .unwrap();
        assert!(!result.timed_out, "{result:#?}");
        assert_eq!(result.exit_code, Some(0), "{result:#?}");
        assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    }
    let state: Value = serde_json::from_slice(&fs::read(&artifacts.gui_state).unwrap()).unwrap();
    let pixels = image::open(&artifacts.png).unwrap().to_rgba8();
    let rows = state["rows"]
        .as_array()
        .expect("actual displayed font rows");
    assert_eq!(rows.len(), 2, "{state}");
    let ink: Vec<f64> = rows
        .iter()
        .map(|row| integrated_ink(&pixels, &row["bounds"]))
        .collect();
    json!({"state": state, "ink": ink, "png": artifacts.png})
}

fn integrated_ink(pixels: &image::RgbaImage, bounds: &Value) -> f64 {
    let edges: Vec<u32> = bounds
        .as_array()
        .unwrap()
        .iter()
        .map(|edge| u32::try_from(edge.as_u64().expect("nonnegative pixel edge")).unwrap())
        .collect();
    assert_eq!(edges.len(), 4);
    let [left, top, right, bottom] = [edges[0], edges[1], edges[2], edges[3]];
    assert!(left < right && top < bottom, "empty row: {bounds}");
    assert!(
        right <= pixels.width() && bottom <= pixels.height(),
        "row outside PNG: {bounds}"
    );
    let mut ink = 0.0;
    for y in top..bottom {
        for x in left..right {
            let [r, g, b, a] = pixels.get_pixel(x, y).0;
            assert_eq!(a, 255, "expected opaque frame readback");
            ink += (765.0 - f64::from(r) - f64::from(g) - f64::from(b)) / 765.0;
        }
    }
    ink
}
