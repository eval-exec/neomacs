//! Issue #359: preserve GNU ftcrfont.c:144-171 raster policy for the same font.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiScenario, GuiTestPlan, HangCap,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

const FONTS: [(&str, &str, &str, &str); 2] = [
    (
        "normal",
        "regular",
        "IBMPlexSans-Regular.ttf",
        "b47ebd72b5ce53b60f909033ff84d6fc12358cc147e2981f8c321ad3f36246b6",
    ),
    (
        "bold",
        "bold",
        "IBMPlexSans-Bold.ttf",
        "22f59d358e8b9b9fb38ac6ee26a7ea7263ffa1c89a9056c101acff5add91f5d6",
    ),
];

const FIRA_FONTS: [(&str, &str, &str, &str); 3] = [
    (
        "semi-light",
        "light",
        "FiraCodeNerdFont-Light.ttf",
        "9b4dfa715e8ab7e0da28577fbc289a182cba9a6050ef50a8fce7b2f06051e99e",
    ),
    (
        "bold",
        "bold",
        "FiraCodeNerdFont-Bold.ttf",
        "594a51c86afb58c1160df286e45dc551eeeba4d5f0e6edb1c304fc43ab8a0a09",
    ),
    // Keep Regular available: semi-light must select Light by GNU distance.
    (
        "normal",
        "regular",
        "FiraCodeNerdFont-Regular.ttf",
        "29b619655612cb273e034737408b9508a04beb63c1ddbdfaa9a6846c409c7a2e",
    ),
];

#[derive(Clone, Copy)]
enum FamilyCase {
    Plex,
    Fira,
}

impl FamilyCase {
    fn family(self) -> &'static str {
        match self {
            Self::Plex => "IBM Plex Sans",
            Self::Fira => "FiraCode Nerd Font",
        }
    }
    fn height(self) -> u32 {
        match self {
            Self::Plex => 120,
            Self::Fira => 135,
        }
    }
    fn fonts(self) -> &'static [(&'static str, &'static str, &'static str, &'static str)] {
        match self {
            Self::Plex => &FONTS,
            Self::Fira => &FIRA_FONTS,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum DisplayResources {
    Absent,
    HintStyle(&'static str),
}

#[test]
fn fontconfig_hintstyle_preserves_normal_and_bold_ink_against_attested_gnu() {
    run_ink_matrix(
        FamilyCase::Plex,
        &[
            ("hintslight", DisplayResources::HintStyle("hintslight")),
            ("hintfull", DisplayResources::HintStyle("hintfull")),
        ],
    );
}

#[test]
fn x11_display_hintstyle_overrides_pattern_ink_against_attested_gnu() {
    run_ink_matrix(
        FamilyCase::Plex,
        &[
            ("hintslight", DisplayResources::HintStyle("hintfull")),
            ("hintslight", DisplayResources::HintStyle("hintslight")),
        ],
    );
}

#[test]
fn x11_font_ink_uses_xlib_defaults_without_xft_resources_against_attested_gnu() {
    run_ink_matrix(
        FamilyCase::Plex,
        &[("hintslight", DisplayResources::Absent)],
    );
}

#[test]
fn fira_code_semi_light_ink_matches_attested_gnu_on_x11() {
    run_ink_matrix(
        FamilyCase::Fira,
        &[
            ("hintslight", DisplayResources::Absent),
            ("hintslight", DisplayResources::HintStyle("hintslight")),
        ],
    );
}

fn run_ink_matrix(family: FamilyCase, cases: &[(&str, DisplayResources)]) {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let forced = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH").expect("set NEOVM_FORCE_ORACLE_PATH"),
    );
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "attested oracle required");
    assert_eq!(fs::canonicalize(&forced).unwrap(), oracle.executable_path());
    let root = neomacs_infra::workspace_root();
    let nonce = neomacs_gui_tests::unique_nonce();
    let artifacts = create_dir(root.join(format!(
        "tmp/neomacs-gui-tests/font-hinting-weight-{}-{nonce}",
        std::process::id()
    )));
    let prefix = forced.parent().unwrap().parent().unwrap();
    let check = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&forced)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .unwrap();
    for (name, bytes) in [("stdout", &check.stdout), ("stderr", &check.stderr)] {
        fs::write(artifacts.join(format!("oracle-check.{name}")), bytes).unwrap();
    }
    fs::write(artifacts.join("oracle-stamp.txt"), oracle.stamp()).unwrap();
    assert!(check.status.success(), "oracle check failed: {check:?}");
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("oracle profile: linux-gtk3"));
    eprintln!("{stdout}");

    let fonts = pinned_fonts(&artifacts, family);
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut matrix = Vec::new();
    for (index, &(policy, resources)) in cases.iter().enumerate() {
        let dir = create_dir(artifacts.join(format!("{index}-{policy}")));
        // Each cell gets a fresh server: the no-resource case cannot inherit
        // another cell's Xft values or a desktop's XSETTINGS manager.
        let display = DisplayHarness::Xvfb.start_session(&dir).unwrap();
        set_display_resources(&dir, display.env(), resources);
        let config = fontconfig(&dir, &fonts, policy);
        let gnu = run_editor(&root, &dir, &forced, "gnu", &config, display.env(), family);
        let neo = run_editor(&root, &dir, &binary, "neo", &config, display.env(), family);
        matrix.push(json!({"policy": policy, "display_resources":format!("{resources:?}"), "gnu": gnu, "neo": neo}));
    }
    // Save the complete matrix before asserting.
    fs::write(
        artifacts.join("matrix.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();
    for cell in &matrix {
        assert_policy(cell, &fonts, &artifacts, family);
    }
}

fn assert_policy(cell: &Value, fonts: &Path, artifacts: &Path, family: FamilyCase) {
    let policy = cell["policy"].as_str().unwrap();
    for (index, &(name, weight, filename, _)) in family.fonts()[..2].iter().enumerate() {
        let expected_file = fs::canonicalize(fonts.join(filename)).unwrap();
        let expected = json!({
            "name": name, "family": family.family(), "weight": weight,
            "slant": "normal", "size": match family {FamilyCase::Plex => 16, FamilyCase::Fira => 18},
        });
        for editor in ["gnu", "neo"] {
            let row = &cell[editor]["state"]["rows"][index];
            for (key, value) in expected.as_object().unwrap() {
                assert_eq!(&row[key], value, "{key}: {cell}; {artifacts:?}");
            }
            assert_eq!(
                fs::canonicalize(row["file"].as_str().unwrap()).unwrap(),
                expected_file,
                "{editor}/{policy}/{name} wrong font; {artifacts:?}"
            );
        }
        let gnu = cell["gnu"]["ink"][index].as_f64().unwrap();
        let neo = cell["neo"]["ink"][index].as_f64().unwrap();
        assert!(gnu > 100.0 && neo > 100.0, "missing text ink: {cell}");
        let error = (neo - gnu).abs() / gnu;
        eprintln!("{policy}/{name}: GNU ink={gnu:.4}, Neo ink={neo:.4}, delta={error:.4}");
        assert!(
            error <= 0.015,
            "{policy}/{name}: ink delta={error:.4}, GNU={gnu}, Neo={neo}; {artifacts:?}"
        );
    }
}

fn create_dir(path: PathBuf) -> PathBuf {
    fs::create_dir_all(&path).unwrap();
    path
}

fn set_display_resources(dir: &Path, env: &[(String, String)], resources: DisplayResources) {
    let text = match resources {
        DisplayResources::Absent => String::new(),
        DisplayResources::HintStyle(style) => format!(
            "Xft.dpi: 96\nXft.hinting: 1\nXft.hintstyle: {style}\nXft.antialias: 1\nXft.rgba: none\n"
        ),
    };
    let path = dir.join("Xresources");
    fs::write(&path, text).unwrap();
    let result = Command::new("xrdb")
        .arg("-load")
        .arg(path)
        .envs(env.iter().cloned())
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let query = Command::new("xrdb")
        .arg("-query")
        .envs(env.iter().cloned())
        .output()
        .unwrap();
    assert!(query.status.success(), "{query:?}");
    fs::write(dir.join("xrdb-query.txt"), &query.stdout).unwrap();
    if matches!(resources, DisplayResources::Absent) {
        assert!(
            !String::from_utf8_lossy(&query.stdout).contains("Xft."),
            "{query:?}"
        );
    }
}

fn pinned_fonts(artifacts: &Path, family: FamilyCase) -> PathBuf {
    let fonts = create_dir(artifacts.join("fonts"));
    for &(_, _, filename, sha256) in family.fonts() {
        let url = match family {
            FamilyCase::Plex => format!(
                "https://raw.githubusercontent.com/IBM/plex/2e90fe366fdc27c12682db13e5aa20ded3d90908/packages/plex-sans/fonts/complete/ttf/{filename}"
            ),
            FamilyCase::Fira => {
                let style = filename
                    .strip_prefix("FiraCodeNerdFont-")
                    .unwrap()
                    .strip_suffix(".ttf")
                    .unwrap();
                format!(
                    "https://raw.githubusercontent.com/ryanoasis/nerd-fonts/fa7b859994228a9c8759f99c55a8d31ee92a1b5e/patched-fonts/FiraCode/{style}/{filename}"
                )
            }
        };
        let pinned = neomacs_infra::pinned::pinned_file(filename, &url, sha256)
            .expect("pinned reporter font");
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
    let cache = create_dir(artifacts.join("fc-cache"));
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
    dir: &Path,
    binary: &Path,
    editor: &str,
    fontconfig: &Path,
    display_env: &[(String, String)],
    family: FamilyCase,
) -> Value {
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        root,
        dir,
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
    .with_env("NEOMACS_FONT_WEIGHT_FAMILY", family.family())
    .with_env("NEOMACS_FONT_WEIGHT_HEIGHT", family.height().to_string())
    .with_env("NEOMACS_FONT_WEIGHT_FIRST_WEIGHT", family.fonts()[0].0)
    .with_env(
        "NEOMACS_FONT_WEIGHT_PIXEL_SIZE",
        match family {
            FamilyCase::Plex => "16",
            FamilyCase::Fira => "18",
        },
    );
    for (key, value) in display_env {
        plan = plan.with_env(key, value);
    }
    let files = GuiArtifactSet::new(dir, GuiBackend::LinuxX11, editor);
    plan.write_manifest().unwrap();
    let mut command = plan.command_spec();
    command.env.retain(|(key, _)| {
        !matches!(
            key.as_str(),
            "NEOMACS_DEBUG_FIRST_FRAME_READBACK"
                | "NEOMACS_DEBUG_SURFACE_READBACK"
                | "NEOMACS_DEBUG_SURFACE_READBACK_PNG"
        )
    });
    fs::write(
        dir.join(format!("{editor}-capture-command.json")),
        serde_json::to_vec_pretty(&json!({
            "program":command.program, "args":command.args, "env":command.env
        }))
        .unwrap(),
    )
    .unwrap();
    // GNU export redraws on an image surface, whose font options differ from
    // the visible Xlib surface. Grab both actual windows through the same API.
    let mut child = OwnedEditor(
        Command::new(&command.program)
            .args(&command.args)
            .envs(command.env.iter().cloned())
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env_remove("NEOMACS_FONT_WEIGHT_GNU_PNG")
            .env_remove("NEOMACS_DEBUG_FIRST_FRAME_READBACK")
            .env_remove("NEOMACS_DEBUG_SURFACE_READBACK")
            .env_remove("NEOMACS_DEBUG_SURFACE_READBACK_PNG")
            .stdin(Stdio::null())
            .stdout(fs::File::create(&files.stdout).unwrap())
            .stderr(fs::File::create(&files.stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = neomacs_gui_tests::HangCap::new(Duration::from_secs(25));
    let state = loop {
        if let Ok(bytes) = fs::read(&files.gui_state)
            && let Ok(state) = serde_json::from_slice::<Value>(&bytes)
        {
            break state;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "editor exited before state: {dir:?}"
        );
        assert!(!deadline.is_expired(), "state timeout: {dir:?}");
        thread::sleep(Duration::from_millis(10));
    };
    let rows = state["rows"].as_array().expect("displayed font rows");
    assert_eq!(rows.len(), 2, "{state}");
    let window = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--pid",
            &child.0.id().to_string(),
        ])
        .envs(display_env.iter().cloned())
        .output()
        .unwrap();
    assert!(window.status.success(), "{window:?}; {dir:?}");
    let window = String::from_utf8(window.stdout).unwrap();
    let window = window.lines().next().expect("visible editor window");
    let capture_cap = HangCap::from_secs(10);
    let mut previous = None;
    let ink = loop {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "editor exited before capture: {dir:?}"
        );
        let capture = Command::new("import")
            .arg("-window")
            .arg(window)
            .arg(&files.png)
            .envs(display_env.iter().cloned())
            .output()
            .unwrap();
        assert!(capture.status.success(), "{capture:?}; {dir:?}");
        let pixels = image::open(&files.png).unwrap().to_rgba8();
        let ink: Vec<f64> = rows
            .iter()
            .map(|row| integrated_ink(&pixels, &row["bounds"]))
            .collect();
        if ink.iter().all(|value| *value > 100.0) && previous.as_ref() == Some(&ink) {
            break ink;
        }
        capture_cap.assert_not_expired(format!("visible glyphs did not settle: {dir:?}"));
        previous = Some(ink);
        thread::sleep(Duration::from_millis(50));
    };
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "{status}; {dir:?}");
            break;
        }
        assert!(!deadline.is_expired(), "exit timeout: {dir:?}");
        thread::sleep(Duration::from_millis(10));
    }
    json!({"state": state, "ink": ink, "png": files.png})
}

struct OwnedEditor(Child);
impl Drop for OwnedEditor {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn integrated_ink(pixels: &image::RgbaImage, bounds: &Value) -> f64 {
    let edges: Vec<u32> = serde_json::from_value(bounds.clone()).expect("nonnegative pixel edges");
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
            assert_eq!(a, 255, "opaque frame readback");
            ink += (765.0 - f64::from(r) - f64::from(g) - f64::from(b)) / 765.0;
        }
    }
    ink
}
