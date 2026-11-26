#![cfg(target_os = "linux")]
//! GNU Emacs 31.1 GUI oracle for merged face attributes, font-at selection,
//! overlay/hl-line/remapping precedence, and box-cursor inverse paint (#853).
//!
//! State-gated on `window-setup-hook` -> `(redisplay t)` -> immediate write
//! (zero sleeps, timers, or wall-clock latency assertions). Compares:
//! 1. Elisp merged-attribute introspection (`face-at-point`, `face-at-point nil t`,
//!    `get-text-property`, `get-char-property`, `font-at` weight/slant,
//!    `cursor-color` frame parameter, and `cursor` face `:background`) identically
//!    between the attested GNU Emacs 31.1 binary and Neomacs.
//! 2. Rendered colors across all 15 merged-face rows: GNU's actual rendered
//!    `x-export-frames` PNG pixels (dominant background and peak ink RGB per row)
//!    against Neomacs's finalized `FrameSnapshot` face foreground/background RGB
//!    (linear -> sRGB) and `phys_cursor` (`FilledBox` color and inverse `cursor_fg`).

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const FIXTURE: &str = "crates/neomacs-gui-tests/fixtures/merged-face-attributes-gui-oracle.el";
const SCENARIO: &str = "merged-face-attributes-oracle";

fn neomacs_binary(root: &Path) -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"))
}

fn linear_to_srgb_byte(c: f64) -> u8 {
    let srgb = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn snapshot_color_rgb(color: &serde_json::Value) -> [u8; 3] {
    [
        linear_to_srgb_byte(color["r"].as_f64().expect("color.r")),
        linear_to_srgb_byte(color["g"].as_f64().expect("color.g")),
        linear_to_srgb_byte(color["b"].as_f64().expect("color.b")),
    ]
}

fn parse_hex_rgb(hex: &str) -> [u8; 3] {
    let hex = hex
        .strip_prefix('#')
        .unwrap_or_else(|| panic!("expected #rrggbb hex color, got {hex:?}"));
    assert_eq!(hex.len(), 6, "expected 6 hex digits, got {hex:?}");
    [
        u8::from_str_radix(&hex[0..2], 16).unwrap(),
        u8::from_str_radix(&hex[2..4], 16).unwrap(),
        u8::from_str_radix(&hex[4..6], 16).unwrap(),
    ]
}

fn fmt_rgb(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

fn rgb_close(a: [u8; 3], b: [u8; 3], max_channel_diff: u8) -> bool {
    a[0].abs_diff(b[0]) <= max_channel_diff
        && a[1].abs_diff(b[1]) <= max_channel_diff
        && a[2].abs_diff(b[2]) <= max_channel_diff
}

/// Extract the dominant background RGB and peak ink RGB (color farthest from
/// background) from GNU's rendered PNG inside `[x0..x1, y0..y1)`.
fn gnu_row_bg_and_ink(
    pixels: &image::RgbaImage,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
) -> ([u8; 3], [u8; 3]) {
    let mut counts: HashMap<[u8; 3], usize> = HashMap::new();
    for y in y0.min(pixels.height())..y1.min(pixels.height()) {
        for x in x0.min(pixels.width())..x1.min(pixels.width()) {
            let [r, g, b, _] = pixels.get_pixel(x, y).0;
            *counts.entry([r, g, b]).or_insert(0) += 1;
        }
    }
    let (&bg, _) = counts
        .iter()
        .max_by_key(|&(_, count)| count)
        .expect("row region must contain pixels");
    let (&ink, _) = counts
        .iter()
        .max_by_key(|&(rgb, _)| {
            let dr = i32::from(rgb[0]) - i32::from(bg[0]);
            let dg = i32::from(rgb[1]) - i32::from(bg[1]);
            let db = i32::from(rgb[2]) - i32::from(bg[2]);
            dr * dr + dg * dg + db * db
        })
        .expect("row region must contain ink pixels");
    (bg, ink)
}

#[test]
// Prerequisites: requires the attested GNU 31.1 GUI oracle binary and Xvfb.
fn merged_face_attributes_and_rendered_colors_match_gnu() {
    let root = neomacs_infra::workspace_root();
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "requires the attested GNU 31.1 oracle"
    );
    eprintln!("{}", oracle.stamp());

    let backend = GuiBackend::LinuxX11;
    let artifact_root = root.join(format!("tmp/{SCENARIO}-{}", std::process::id()));
    fs::create_dir_all(&artifact_root).expect("artifact dir");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("X11 display session should start");

    let fixture_path = root.join(FIXTURE);
    let gnu_json_path = artifact_root.join("gnu-state.json");
    let gnu_png_path = artifact_root.join("gnu-export.png");
    let neo_json_path = artifact_root.join("neo-state.json");
    let neo_snap_path = artifact_root.join("neo-snapshot.json");

    // 1. Run fixture under the attested GNU Emacs 31.1 oracle.
    let mut gnu_env = vec![
        ("GDK_BACKEND".to_string(), "x11".to_string()),
        ("GSETTINGS_BACKEND".to_string(), "memory".to_string()),
        (
            "NEOMACS_MERGED_FACE_ORACLE_JSON".to_string(),
            gnu_json_path.display().to_string(),
        ),
        (
            "NEOMACS_MERGED_FACE_GNU_PNG".to_string(),
            gnu_png_path.display().to_string(),
        ),
    ];
    gnu_env.extend(session.env().iter().cloned());
    let gnu_cmd = CommandSpec {
        program: oracle.executable().to_path_buf(),
        args: vec![
            "-Q".to_string(),
            "-l".to_string(),
            fixture_path.display().to_string(),
        ],
        env: gnu_env,
    };
    let mut runner = ProcessGuiCommandRunner;
    let gnu_out = runner
        .run(
            &gnu_cmd,
            &GuiArtifactSet::new(&artifact_root, backend, "gnu"),
            &GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .expect("GNU merged-face oracle should run");
    assert_eq!(
        gnu_out.exit_code,
        Some(0),
        "GNU merged-face oracle failed; stderr:\n{}",
        gnu_out.stderr
    );

    // 2. Run the exact same fixture under Neomacs.
    let binary = neomacs_binary(&root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the merged-face GUI oracle"
    );
    let scenario = GuiScenario::new(SCENARIO, &fixture_path);
    let mut plan = GuiTestPlan::new(backend, &root, &artifact_root, scenario)
        .with_program(binary)
        .with_env("GDK_BACKEND", "x11")
        .with_env(
            "NEOMACS_MERGED_FACE_ORACLE_JSON",
            neo_json_path.display().to_string(),
        )
        .with_env(
            "NEOMACS_MERGED_FACE_SNAP_JSON",
            neo_snap_path.display().to_string(),
        );
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let neo_out = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(30)),
        )
        .expect("Neomacs merged-face oracle should run");
    assert_eq!(
        neo_out.exit_code,
        Some(0),
        "Neomacs merged-face oracle failed; stderr: {:?}",
        neo_out.artifacts.stderr
    );

    let gnu: serde_json::Value =
        serde_json::from_slice(&fs::read(&gnu_json_path).expect("GNU state JSON"))
            .expect("parse GNU state JSON");
    let neo: serde_json::Value =
        serde_json::from_slice(&fs::read(&neo_json_path).expect("Neomacs state JSON"))
            .expect("parse Neomacs state JSON");
    let snap: serde_json::Value =
        serde_json::from_slice(&fs::read(&neo_snap_path).expect("Neomacs snapshot JSON"))
            .expect("parse Neomacs snapshot JSON");
    let gnu_png = image::open(&gnu_png_path)
        .expect("GNU x-export-frames PNG must exist")
        .to_rgba8();

    assert_eq!(gnu["native_engine"], serde_json::json!(false), "{gnu}");
    assert_eq!(neo["native_engine"], serde_json::json!(true), "{neo}");
    assert_eq!(gnu["graphic"], serde_json::json!(true), "{gnu}");
    assert_eq!(neo["graphic"], serde_json::json!(true), "{neo}");

    // Cursor face -> frame parameter propagation parity.
    assert_eq!(
        neo["cursor_color_param"], gnu["cursor_color_param"],
        "cursor-color frame parameter mismatch: neo={neo} gnu={gnu}"
    );
    assert_eq!(
        neo["cursor_face_bg"], gnu["cursor_face_bg"],
        "cursor face :background mismatch: neo={neo} gnu={gnu}"
    );

    // 3. Compare Elisp merged attributes AND rendered RGB colors per row.
    let gnu_cases = gnu["cases"].as_array().expect("gnu.cases");
    let neo_cases = neo["cases"].as_array().expect("neo.cases");
    assert_eq!(gnu_cases.len(), neo_cases.len());

    let gnu_edges = gnu["window_edges"].as_array().expect("gnu.window_edges");
    let ex0 = gnu_edges[0].as_u64().unwrap() as u32;
    let ey0 = gnu_edges[1].as_u64().unwrap() as u32;

    let frame = &snap["frames"][0];
    let faces = frame["faces"]
        .as_object()
        .expect("snapshot.frames[0].faces");
    let rows = frame["window_matrices"][0]["matrix"]["rows"]
        .as_array()
        .expect("snapshot rows");

    let mut gnu_hl_line_bg: Option<[u8; 3]> = None;

    for (gc, nc) in gnu_cases.iter().zip(neo_cases.iter()) {
        let name = gc["name"].as_str().expect("case name");
        assert_eq!(nc["name"], gc["name"]);
        for field in [
            "face_at_point",
            "faces_at_point",
            "text_prop_face",
            "char_prop_face",
            "font_weight",
            "font_slant",
        ] {
            assert_eq!(
                nc[field], gc[field],
                "Elisp merged-attribute mismatch for case `{name}` field `{field}`: neo={} gnu={}",
                nc[field], gc[field]
            );
        }

        // Compare rendered colors for every non-cursor cell directly against
        // GNU's rendered `x-export-frames` PNG pixels.
        if name == "cursor-on-hl-line" {
            continue;
        }
        let row_idx = gc["row"].as_u64().expect("row index") as usize;
        let x0 = ex0 + gc["x"].as_u64().unwrap() as u32;
        let y0 = ey0 + gc["y"].as_u64().unwrap() as u32;
        let y1 = ey0
            + gc["next_y"]
                .as_u64()
                .unwrap()
                .max(gc["y"].as_u64().unwrap() + 18) as u32;
        let x1 = x0 + 50;
        let (gnu_bg, gnu_ink) = gnu_row_bg_and_ink(&gnu_png, x0, y0, x1, y1);
        if name == "neighbor-on-hl-line" {
            gnu_hl_line_bg = Some(gnu_bg);
        }

        let glyph_col = if name == "neighbor-on-hl-line" { 4 } else { 0 };
        let glyph = &rows[row_idx]["glyphs"][1][glyph_col];
        let face_id = glyph["face_id"]
            .as_u64()
            .expect("glyph face_id")
            .to_string();
        let face = faces
            .get(&face_id)
            .unwrap_or_else(|| panic!("missing face_id {face_id} for case `{name}`"));
        let neo_fg = snapshot_color_rgb(&face["foreground"]);
        let neo_bg = snapshot_color_rgb(&face["background"]);

        assert!(
            rgb_close(neo_bg, gnu_bg, 1),
            "case `{name}` rendered background mismatch: neo={} gnu={}",
            fmt_rgb(neo_bg),
            fmt_rgb(gnu_bg)
        );
        assert!(
            rgb_close(neo_fg, gnu_ink, 1),
            "case `{name}` rendered foreground mismatch: neo={} gnu={}",
            fmt_rgb(neo_fg),
            fmt_rgb(gnu_ink)
        );
    }

    // 4. Verify box-cursor inverse paint on the live `hl-line-mode` row:
    //    cursor box color matches GNU's `cursor` face `:background` and
    //    inverse glyph foreground matches GNU's rendered `hl-line` background
    //    per GNU `x_set_cursor_gc`.
    let expected_cursor_bg = parse_hex_rgb(
        gnu["cursor_face_bg"]
            .as_str()
            .expect("gnu cursor_face_bg hex"),
    );
    let expected_cursor_fg = gnu_hl_line_bg.expect("neighbor-on-hl-line GNU rendered bg");
    let phys_cursor = &frame["phys_cursor"];
    assert_eq!(phys_cursor["row"], serde_json::json!(14));
    assert_eq!(phys_cursor["col"], serde_json::json!(0));
    assert_eq!(phys_cursor["style"], serde_json::json!("FilledBox"));
    let cursor_bg = snapshot_color_rgb(&phys_cursor["color"]);
    let cursor_fg = snapshot_color_rgb(&phys_cursor["cursor_fg"]);
    assert!(
        rgb_close(cursor_bg, expected_cursor_bg, 1),
        "box cursor color mismatch: neo={} gnu={}",
        fmt_rgb(cursor_bg),
        fmt_rgb(expected_cursor_bg)
    );
    assert!(
        rgb_close(cursor_fg, expected_cursor_fg, 1),
        "box cursor inverse glyph foreground must match GNU merged hl-line background: neo={} gnu={}",
        fmt_rgb(cursor_fg),
        fmt_rgb(expected_cursor_fg)
    );

    let _ = fs::remove_dir_all(&artifact_root);
}
