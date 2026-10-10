//! A golden sheet for COLRv1 emoji: one cell average per glyph, compared
//! against a committed reference.
//!
//! The hue assertions in `colrv1_emoji` answer "is this cell painted at all";
//! they cannot see *how* it is painted.  The regressions that mattered (the
//! clip applied to its own fill, a dropped paint transform) moved edge
//! coverage, which barely moves a hue but moves a cell average.  Cell
//! averages are also stable across renderers, unlike pixel-exact images, so
//! the golden survives different GPUs and driver versions.
//!
//! Regenerate after intentional rendering changes:
//!
//! ```text
//! NEOMACS_GOLDEN_UPDATE=1 cargo nextest run -p neomacs-gui-tests --test golden_emoji_sheet
//! ```
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

/// The sheet, in draw order — the same list the fixture inserts.  Four
/// glyphs because the readback diagnostics report at most four boxes.
const SHEET: &[char] = &[
    '\u{1F347}', // grape
    '\u{1F34E}', // apple
    '\u{1F308}', // rainbow
    '\u{1F33B}', // sunflower
];

/// Per-channel tolerance on a cell average.  Antialiasing and driver rounding
/// move averages by fractions of a level; a real regression moves them by
/// whole levels.
const TOLERANCE: f32 = 3.0;

/// Average RGBA of the readback box reported for `ch`, if it was reported.
fn glyph_box_average(result: &neomacs_gui_tests::GuiRunResult, ch: char) -> Option<[f32; 4]> {
    let needle = format!("glyph_box='{ch}'");
    let line = result
        .readback_diagnostics
        .iter()
        .find(|line| line.contains(&needle))?;
    let boxed = &line[line.find(&needle)?..];
    let average = boxed.split("avg=(").nth(1)?.split(')').next()?;
    let channels: Vec<f32> = average
        .split(',')
        .filter_map(|channel| channel.trim().parse().ok())
        .collect();
    assert_eq!(channels.len(), 4, "unparsable readback line: {line}");
    Some([channels[0], channels[1], channels[2], channels[3]])
}

#[test]
fn colrv1_emoji_sheet_matches_its_golden() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/golden-emoji-{}", std::process::id()));
    let fonts = artifacts.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    fs::copy(
        neomacs_test_fonts::spleen_2_2_0().otb(),
        fonts.join("spleen.otb"),
    )
    .unwrap();
    fs::copy(
        neomacs_test_fonts::noto_color_emoji_colrv1(),
        fonts.join("emoji.ttf"),
    )
    .unwrap();
    let config = artifacts.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir><alias><family>monospace</family><prefer><family>Spleen</family></prefer></alias></fontconfig>",
            fonts.display(),
            artifacts.join("fontcache").display()
        ),
    )
    .unwrap();

    let backend = match std::env::var("NEOMACS_GUI_TEST_BACKEND").as_deref() {
        Ok("wayland") => GuiBackend::LinuxWayland,
        _ => GuiBackend::LinuxX11,
    };
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts.join("native"))
        .unwrap();
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/golden-emoji/init.el");
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new("golden-emoji", &fixture),
    )
    .with_program(binary)
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "4");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert_eq!(
        result.status,
        neomacs_gui_tests::GuiRunStatus::Passed,
        "{result:#?}"
    );
    assert_eq!(result.exit_code, Some(0), "{result:#?}");

    let mut measured = serde_json::Map::new();
    for &ch in SHEET {
        let average = glyph_box_average(&result, ch).unwrap_or_else(|| {
            panic!(
                "readback reported no box for U+{:04X}: {result:#?}",
                ch as u32
            )
        });
        let rounded: Vec<f64> = average
            .iter()
            .map(|v| (f64::from(*v) * 100.0).round() / 100.0)
            .collect();
        measured.insert(format!("{:x}", ch as u32), serde_json::json!(rounded));
    }

    let golden_path = root.join("crates/neomacs-gui-tests/fixtures/golden-emoji/sheet.json");
    if std::env::var_os("NEOMACS_GOLDEN_UPDATE").is_some() {
        fs::write(
            &golden_path,
            serde_json::to_string_pretty(&serde_json::Value::Object(measured)).unwrap() + "\n",
        )
        .unwrap();
        eprintln!("golden updated: {}", golden_path.display());
        return;
    }

    let golden: serde_json::Value = serde_json::from_slice(
        &fs::read(&golden_path).expect("the committed golden sheet is readable"),
    )
    .expect("the golden sheet is valid JSON");
    let mut worst = 0.0f32;
    let mut failures = Vec::new();
    for (key, actual) in &measured {
        let expected = golden
            .get(key)
            .unwrap_or_else(|| panic!("the golden has no entry for {key}"));
        for channel in 0..4 {
            let expected = expected[channel].as_f64().expect("golden channel") as f32;
            let actual = actual[channel].as_f64().expect("measured channel") as f32;
            let delta = (expected - actual).abs();
            worst = worst.max(delta);
            if delta > TOLERANCE {
                failures.push(format!(
                    "{key} channel {channel}: golden {expected:.2}, measured {actual:.2}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the emoji sheet drifted from its golden (worst channel delta {worst:.2}, tolerance \
         {TOLERANCE}):\n{}\nRegenerate with NEOMACS_GOLDEN_UPDATE=1 if the change is intended",
        failures.join("\n")
    );
}
