//! Regression: an automatic (shaped) Arabic composition whose default face
//! has no Arabic coverage must still draw ink.
//!
//! The reporter's configuration sets a Nerd-Font/Caskaydia-family default
//! face and types Arabic.  Layout correctly resolves Noto Sans Arabic for the
//! composition, but the renderer's multi-glyph subpixel compositor previously
//! read the unused fourth byte of each subpixel mask as alpha and produced an
//! all-zero atlas entry: the Latin text rendered, the Arabic word was blank.
//!
//! This test pins a private fontconfig containing only Ubuntu Mono (no Arabic
//! coverage) and Noto Sans Arabic, forces subpixel masks, and checks the
//! captured surface has ink inside the composed glyph's cell.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn fallback_composition_draws_arabic_ink() {
    let root = neomacs_infra::crate_root!().join("../..");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/font-fallback-composite-ink-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&artifacts).unwrap();

    // Exactly two faces: a Latin-only default and the Arabic fallback.
    // Ubuntu Mono carries no Arabic; Noto Sans Arabic only participates as
    // the fontconfig fallback for the composition's characters.
    let fonts = artifacts.join("fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    std::fs::copy(
        neomacs_test_fonts::desktop_fonts()
            .directory()
            .join("UbuntuMono[wght].ttf"),
        fonts.join("UbuntuMono.ttf"),
    )
    .expect("pinned Ubuntu Mono fixture");
    std::fs::copy(
        neomacs_test_fonts::noto_sans_arabic_variable(),
        fonts.join("NotoSansArabic.ttf"),
    )
    .expect("pinned Noto Sans Arabic fixture");

    let fontconfig = artifacts.join("fonts.conf");
    std::fs::write(
        &fontconfig,
        format!(
            "<?xml version=\"1.0\"?>\n\
             <!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\">\n\
             <fontconfig>\n\
               <dir>{}</dir>\n\
               <cachedir>{}</cachedir>\n\
               <match target=\"pattern\"><edit name=\"rgba\" mode=\"assign\"><const>rgb</const></edit></match>\n\
             </fontconfig>\n",
            fonts.display(),
            artifacts.join("fc-cache").display(),
        ),
    )
    .unwrap();

    let session = neomacs_infra::display::start_sway(
        &artifacts,
        "output * resolution 1280x720\nxwayland disable\nseat seat0 fallback true\n",
    )
    .unwrap();
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        &root,
        &artifacts,
        GuiScenario::new(
            "font-fallback-composite-ink",
            root.join("crates/neomacs-gui-tests/fixtures/font-fallback-composite-ink.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("RUST_LOG", "info")
    .with_env("FONTCONFIG_FILE", fontconfig.to_string_lossy());
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");

    let snapshot: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&result.artifacts.frame_snapshot_json).unwrap(),
    )
    .unwrap();
    let png = image::open(format!(
        "{}.png",
        result.artifacts.frame_snapshot_json.display()
    ))
    .expect("captured surface")
    .to_rgba8();
    let window = &snapshot["frames"][0]["window_matrices"][0];
    let origin = &window["text_pixel_bounds"];
    let origin_x = origin["x"].as_f64().unwrap() as u32;
    let origin_y = origin["y"].as_f64().unwrap() as u32;
    let rows = window["matrix"]["rows"].as_array().expect("matrix rows");

    let mut arabic_cell = None;
    for row in rows {
        let Some(glyphs) = row["glyphs"][1].as_array() else {
            continue;
        };
        let mut x_cursor = 0.0f64;
        for glyph in glyphs {
            let width = glyph["pixel_width"].as_f64().unwrap_or(0.0);
            if let Some(text) = glyph["glyph_type"]["AutomaticComposite"]["text"]
                .as_str()
                .filter(|text| text.contains('س'))
            {
                arabic_cell = Some((
                    origin_x + x_cursor as u32,
                    origin_y + row["pixel_y"].as_f64().unwrap() as u32,
                    width as u32,
                    row["height_px"].as_f64().unwrap() as u32,
                    text.to_owned(),
                ));
                break;
            }
            x_cursor += width;
        }
        if arabic_cell.is_some() {
            break;
        }
    }
    let (cell_x, cell_y, cell_w, cell_h, text) =
        arabic_cell.expect("the matrix must record the Arabic automatic composition");

    let ink = |x0: u32, y0: u32, w: u32, h: u32| {
        let mut count = 0u32;
        for y in y0..(y0 + h).min(png.height()) {
            for x in x0..(x0 + w).min(png.width()) {
                let pixel = png.get_pixel(x, y);
                if pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200 {
                    count += 1;
                }
            }
        }
        count
    };
    let latin_ink = ink(origin_x, cell_y, 100, cell_h);
    let arabic_ink = ink(cell_x, cell_y, cell_w, cell_h);
    assert!(
        latin_ink > 30,
        "the Latin text must be visible, proving the surface readback works (ink={latin_ink})"
    );
    assert!(
        arabic_ink > 30,
        "the fallback-resolved composition {text:?} must draw ink in its cell \
         (x={cell_x}, y={cell_y}, w={cell_w}, h={cell_h}, ink={arabic_ink})"
    );
}
