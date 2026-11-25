//! GNU Emacs 31.1 `gif_load` disposal, transparency, and bounds parity (F6).
//!
//! GNU `gif_load` (`src/image.c:10013-10263`) checks all subimage bounds up
//! front, composites GIF frames onto one retained pixmap without clearing
//! disposed frames afterwards, and paints transparent pixels with `:background`
//! when specified or `FRAME_BACKGROUND_PIXEL` otherwise (ignoring both
//! `face-remapping-alist` and the displaying face's own background), producing
//! an opaque image with `(image-mask-p img) => nil` unless `:mask heuristic` is
//! requested.
//!
//! Both GNU Emacs (via `x-export-frames`) and Neomacs (via surface readback)
//! render the same code-generated GIFs, and every swatch's exact pixel counts
//! are compared directly between GNU and Neomacs.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiRunStatus, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

const FRAME_BG: [u8; 3] = [0xe8, 0xd8, 0xc8];
const REMAP_BG: [u8; 3] = [0xf5, 0xf5, 0xf5];
const FACE_DECOY: [u8; 3] = [0x00, 0xff, 0xff];
const SPEC_BG: [u8; 3] = [0x12, 0x34, 0x56];
const RED: [u8; 3] = [0xff, 0x00, 0x00];
const GREEN: [u8; 3] = [0x00, 0xff, 0x00];
const MAGENTA: [u8; 3] = [0xff, 0x00, 0xff];
const ORANGE: [u8; 3] = [0xff, 0x80, 0x00];
const YELLOW: [u8; 3] = [0xff, 0xff, 0x00];

/// Uncompressed 4-color LZW encoder (min_code_size=2, 3-bit codes with CLEAR
/// every 2 pixels so the code width stays fixed at 3 bits). Generates tiny
/// valid GIF fixtures in memory without git-tracking binary files.
fn lzw_encode_2bit(pixels: &[u8]) -> Vec<u8> {
    let mut bit_buf: u32 = 0;
    let mut bit_len: u32 = 0;
    let mut stream = Vec::with_capacity(pixels.len() / 2 + 16);
    let mut emit = |code: u32| {
        bit_buf |= code << bit_len;
        bit_len += 3;
        while bit_len >= 8 {
            stream.push((bit_buf & 0xff) as u8);
            bit_buf >>= 8;
            bit_len -= 8;
        }
    };
    emit(4); // CLEAR
    let mut since_clear = 0u32;
    for &p in pixels {
        emit(u32::from(p & 3));
        since_clear += 1;
        if since_clear == 2 {
            emit(4); // CLEAR
            since_clear = 0;
        }
    }
    emit(5); // EOI
    if bit_len > 0 {
        stream.push((bit_buf & 0xff) as u8);
    }

    let mut out = Vec::with_capacity(stream.len() + stream.len() / 255 + 4);
    out.push(2); // LZW minimum code size
    for chunk in stream.chunks(255) {
        out.push(chunk.len() as u8);
        out.extend_from_slice(chunk);
    }
    out.push(0); // block terminator
    out
}

fn gif_gce(disposal: u8, transparent: Option<u8>, delay_cs: u16) -> [u8; 8] {
    let (flag, trans_idx) = match transparent {
        Some(idx) => (1u8, idx),
        None => (0u8, 0u8),
    };
    let packed = ((disposal & 7) << 2) | flag;
    let [dl, dh] = delay_cs.to_le_bytes();
    [0x21, 0xf9, 0x04, packed, dl, dh, trans_idx, 0x00]
}

fn gif_image_descriptor(left: u16, top: u16, width: u16, height: u16) -> [u8; 10] {
    let [ll, lh] = left.to_le_bytes();
    let [tl, th] = top.to_le_bytes();
    let [wl, wh] = width.to_le_bytes();
    let [hl, hh] = height.to_le_bytes();
    [0x2c, ll, lh, tl, th, wl, wh, hl, hh, 0x00]
}

/// Palette: 0=red, 1=green, 2=magenta, 3=black/transparent.
const TEST_PALETTE: [u8; 12] = [255, 0, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0];
const ORANGE_PALETTE: [u8; 12] = [255, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const YELLOW_PALETTE: [u8; 12] = [255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn gif_header(width: u16, height: u16, palette: &[u8; 12]) -> Vec<u8> {
    let [wl, wh] = width.to_le_bytes();
    let [hl, hh] = height.to_le_bytes();
    let mut out = Vec::with_capacity(25);
    out.extend_from_slice(b"GIF89a");
    // Global color table flag=1, color res=7, sort=0, size=1 (4 entries).
    out.extend_from_slice(&[wl, wh, hl, hh, 0xf1, 0x00, 0x00]);
    out.extend_from_slice(palette);
    out
}

fn build_disposal_gif() -> Vec<u8> {
    let mut out = gif_header(80, 80, &TEST_PALETTE);
    // Frame 0: 80x80 red, DISPOSE_BACKGROUND (2).
    out.extend_from_slice(&gif_gce(2, None, 10));
    out.extend_from_slice(&gif_image_descriptor(0, 0, 80, 80));
    out.extend_from_slice(&lzw_encode_2bit(&vec![0u8; 80 * 80]));
    // Frame 1: 40x40 green at (20, 20), DISPOSE_DO_NOT (1).
    out.extend_from_slice(&gif_gce(1, None, 10));
    out.extend_from_slice(&gif_image_descriptor(20, 20, 40, 40));
    out.extend_from_slice(&lzw_encode_2bit(&vec![1u8; 40 * 40]));
    out.push(0x3b);
    out
}

fn build_transparent_gif() -> Vec<u8> {
    let mut out = gif_header(48, 48, &TEST_PALETTE);
    out.extend_from_slice(&gif_gce(2, Some(3), 10));
    out.extend_from_slice(&gif_image_descriptor(0, 0, 48, 48));
    out.extend_from_slice(&lzw_encode_2bit(&vec![3u8; 48 * 48]));
    out.push(0x3b);
    out
}

fn build_framed_transparent_gif() -> Vec<u8> {
    let mut pixels = vec![3u8; 48 * 48];
    for y in 0..48 {
        for x in 0..48 {
            if x < 4 || x >= 44 || y < 4 || y >= 44 {
                pixels[y * 48 + x] = 2; // magenta border
            }
        }
    }
    let mut out = gif_header(48, 48, &TEST_PALETTE);
    out.extend_from_slice(&gif_gce(2, Some(3), 10));
    out.extend_from_slice(&gif_image_descriptor(0, 0, 48, 48));
    out.extend_from_slice(&lzw_encode_2bit(&pixels));
    out.push(0x3b);
    out
}

fn build_oob_single_gif() -> Vec<u8> {
    // 48x48 logical screen, single 48x48 orange subimage at left=10 (right=58 > 48).
    let mut out = gif_header(48, 48, &ORANGE_PALETTE);
    out.extend_from_slice(&gif_gce(1, None, 10));
    out.extend_from_slice(&gif_image_descriptor(10, 0, 48, 48));
    out.extend_from_slice(&lzw_encode_2bit(&vec![0u8; 48 * 48]));
    out.push(0x3b);
    out
}

fn build_oob_frame1_gif() -> Vec<u8> {
    // 48x48 logical screen: frame 0 fits (yellow), frame 1 overflows (left=10, width=48).
    let mut out = gif_header(48, 48, &YELLOW_PALETTE);
    out.extend_from_slice(&gif_gce(1, None, 10));
    out.extend_from_slice(&gif_image_descriptor(0, 0, 48, 48));
    out.extend_from_slice(&lzw_encode_2bit(&vec![0u8; 48 * 48]));
    out.extend_from_slice(&gif_gce(1, None, 10));
    out.extend_from_slice(&gif_image_descriptor(10, 0, 48, 48));
    out.extend_from_slice(&lzw_encode_2bit(&vec![0u8; 48 * 48]));
    out.push(0x3b);
    out
}

fn count_pixels(pixels: &image::RgbaImage, colour: [u8; 3]) -> usize {
    pixels
        .pixels()
        .filter(|pixel| {
            let [r, g, b, _] = pixel.0;
            [r, g, b] == colour
        })
        .count()
}

fn color_bounding_box(pixels: &image::RgbaImage, colour: [u8; 3]) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (x, y, pixel) in pixels.enumerate_pixels() {
        let [r, g, b, _] = pixel.0;
        if [r, g, b] == colour {
            bounds = Some(match bounds {
                Some((min_x, min_y, max_x, max_y)) => {
                    (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                }
                None => (x, y, x, y),
            });
        }
    }
    bounds
}

/// Exact pixel breakdown inside the 4px magenta border of `frame-bg.gif`:
/// `(frame_bg, remap_bg, face_decoy)`.
fn framed_interior_counts(pixels: &image::RgbaImage, label: &str) -> (usize, usize, usize) {
    let (min_x, min_y, max_x, max_y) = color_bounding_box(pixels, MAGENTA)
        .unwrap_or_else(|| panic!("{label}: magenta border of frame-bg.gif must be visible"));
    assert_eq!(
        (max_x - min_x + 1, max_y - min_y + 1),
        (48, 48),
        "{label}: expected 48x48 magenta border box, got ({min_x},{min_y})..=({max_x},{max_y})"
    );
    let mut frame_bg = 0usize;
    let mut remap_bg = 0usize;
    let mut face_decoy = 0usize;
    for y in (min_y + 4)..=(max_y - 4) {
        for x in (min_x + 4)..=(max_x - 4) {
            let [r, g, b, _] = pixels.get_pixel(x, y).0;
            match [r, g, b] {
                c if c == FRAME_BG => frame_bg += 1,
                c if c == REMAP_BG => remap_bg += 1,
                c if c == FACE_DECOY => face_decoy += 1,
                _ => {}
            }
        }
    }
    (frame_bg, remap_bg, face_decoy)
}

#[test]
fn gif_disposal_and_transparency_match_gnu_gif_load() {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/gif-disposal-oracle-{}", std::process::id()));
    let gif_dir = artifacts.join("gifs");
    fs::create_dir_all(&gif_dir).unwrap();
    fs::write(gif_dir.join("disposal.gif"), build_disposal_gif()).unwrap();
    fs::write(gif_dir.join("spec-bg.gif"), build_transparent_gif()).unwrap();
    fs::write(gif_dir.join("frame-bg.gif"), build_framed_transparent_gif()).unwrap();
    fs::write(gif_dir.join("oob-single.gif"), build_oob_single_gif()).unwrap();
    fs::write(gif_dir.join("oob-frame1.gif"), build_oob_frame1_gif()).unwrap();

    let fixture = root.join("crates/neomacs-gui-tests/fixtures/gif-disposal-oracle/init.el");

    // 1. GNU Emacs oracle run: captures both Elisp state and `x-export-frames` PNG.
    let gnu_program = std::env::var_os("NEOVM_FORCE_ORACLE_PATH")
        .or_else(|| std::env::var_os("NEOMACS_GNU_EMACS_BINARY"))
        .map(PathBuf::from)
        .unwrap_or_else(|| "emacs".into());
    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu"))
        .unwrap();
    let gnu_state_path = artifacts.join("gnu-state.json");
    let gnu_png_path = artifacts.join("gnu-export.png");
    let mut gnu_cmd = CommandSpec {
        program: gnu_program,
        args: vec!["-Q".into(), "--load".into(), fixture.display().to_string()],
        env: vec![
            ("GDK_BACKEND".into(), "x11".into()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            (
                "NEOMACS_GIF_DISPOSAL_DIR".into(),
                gif_dir.display().to_string(),
            ),
            (
                "NEOMACS_GUI_STATE_JSON".into(),
                gnu_state_path.display().to_string(),
            ),
            (
                "NEOMACS_GNU_EXPORT_PNG".into(),
                gnu_png_path.display().to_string(),
            ),
        ],
    };
    gnu_cmd.env.extend(gnu_session.env().iter().cloned());
    let gnu_output = ProcessGuiCommandRunner
        .run(
            &gnu_cmd,
            &GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, "gnu"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert!(!gnu_output.timed_out, "GNU: {gnu_output:#?}");
    assert_eq!(gnu_output.exit_code, Some(0), "GNU: {gnu_output:#?}");
    let gnu: serde_json::Value =
        serde_json::from_slice(&fs::read(&gnu_state_path).unwrap()).unwrap();
    assert_eq!(gnu["native-engine"], serde_json::json!(false), "{gnu}");
    assert_eq!(gnu["graphic"], serde_json::json!(true), "{gnu}");
    assert_eq!(gnu["disposal-width"], serde_json::json!(80), "{gnu}");
    assert_eq!(gnu["disposal-height"], serde_json::json!(80), "{gnu}");
    assert_eq!(gnu["mask-default"], serde_json::json!(false), "{gnu}");
    assert_eq!(gnu["mask-heuristic"], serde_json::json!(true), "{gnu}");
    assert_eq!(gnu["oob-single-loaded"], serde_json::json!(false), "{gnu}");
    assert_eq!(
        gnu["oob-frame1-0-loaded"],
        serde_json::json!(false),
        "GNU rejects the whole GIF when frame 1 is out of screen: {gnu}"
    );
    assert_eq!(
        gnu["oob-frame1-1-loaded"],
        serde_json::json!(false),
        "{gnu}"
    );
    assert_eq!(gnu["oob-single-meta"], serde_json::json!(false), "{gnu}");
    assert_eq!(gnu["oob-frame1-0-meta"], serde_json::json!(false), "{gnu}");

    let gnu_pixels = image::open(&gnu_png_path)
        .expect("GNU x-export-frames PNG must exist")
        .to_rgba8();

    // 2. Neomacs run: captures both Elisp state and surface readback PNG.
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
    let mut plan = GuiTestPlan::new(
        backend,
        &root,
        &artifacts,
        GuiScenario::new("gif-disposal-oracle", &fixture),
    )
    .with_program(binary)
    .with_env("NEOMACS_GIF_DISPOSAL_DIR", gif_dir.display().to_string())
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000");
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .unwrap();
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    assert_eq!(result.exit_code, Some(0));

    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(&result.artifacts.gui_state).unwrap()).unwrap();
    assert_eq!(state["native-engine"], serde_json::json!(true), "{state}");
    assert_eq!(state["graphic"], serde_json::json!(true), "{state}");
    for key in [
        "disposal-width",
        "disposal-height",
        "mask-default",
        "mask-heuristic",
        "oob-single-loaded",
        "oob-frame1-0-loaded",
        "oob-frame1-1-loaded",
        "oob-single-meta",
        "oob-frame1-0-meta",
    ] {
        assert_eq!(
            state[key], gnu[key],
            "Elisp state mismatch for {key}: neo={state} gnu={gnu}"
        );
    }

    let neo_pixels = image::open(&result.artifacts.png)
        .expect("Neomacs readback PNG exists")
        .to_rgba8();

    // Direct GNU vs Neomacs pixel parity across every swatch:
    // Swatch 1: 80x80 disposal GIF at :index 1 retains 4800 red outer pixels
    // beside the 1600 green inner pixels.
    let gnu_disposal = (
        count_pixels(&gnu_pixels, RED),
        count_pixels(&gnu_pixels, GREEN),
    );
    let neo_disposal = (
        count_pixels(&neo_pixels, RED),
        count_pixels(&neo_pixels, GREEN),
    );
    assert_eq!(
        gnu_disposal,
        (4800, 1600),
        "GNU gif_load disposal oracle counts"
    );
    assert_eq!(
        neo_disposal, gnu_disposal,
        "Neomacs disposal pixels must equal GNU exactly"
    );

    // Swatch 2: 48x48 transparent GIF with :background "#123456" paints 2304
    // pixels of #123456 under the cyan face decoy.
    let gnu_spec_bg = count_pixels(&gnu_pixels, SPEC_BG);
    let neo_spec_bg = count_pixels(&neo_pixels, SPEC_BG);
    assert_eq!(
        gnu_spec_bg,
        48 * 48,
        "GNU gif_load :background override oracle count"
    );
    assert_eq!(
        neo_spec_bg, gnu_spec_bg,
        "Neomacs :background pixels must equal GNU exactly"
    );

    // Swatch 3: 48x48 magenta-framed transparent GIF without :background paints
    // FRAME_BACKGROUND_PIXEL (#e8d8c8) across all 40x40 = 1600 interior pixels,
    // ignoring both face-remapping-alist (#f5f5f5) and face :background (#00ffff).
    let gnu_interior = framed_interior_counts(&gnu_pixels, "GNU");
    let neo_interior = framed_interior_counts(&neo_pixels, "Neomacs");
    assert_eq!(
        gnu_interior,
        (1600, 0, 0),
        "GNU gif_load FRAME_BACKGROUND_PIXEL oracle counts"
    );
    assert_eq!(
        neo_interior, gnu_interior,
        "Neomacs transparent interior pixels must equal GNU exactly"
    );

    // Swatches 4 & 5: out-of-bounds single-frame (orange) and multi-frame :index 0
    // (yellow) GIFs must fail to load and paint zero pixels in both GNU and Neomacs.
    let gnu_oob = (
        count_pixels(&gnu_pixels, ORANGE),
        count_pixels(&gnu_pixels, YELLOW),
    );
    let neo_oob = (
        count_pixels(&neo_pixels, ORANGE),
        count_pixels(&neo_pixels, YELLOW),
    );
    assert_eq!(
        gnu_oob,
        (0, 0),
        "GNU must reject both out-of-screen GIFs without painting any pixels"
    );
    assert_eq!(
        neo_oob, gnu_oob,
        "Neomacs must reject both out-of-screen GIFs without falling back to a still decoder"
    );
}
