//! Live paired image contracts (GNU 31.1 src/image.c:1655-1733).
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[test]
fn image_size_svg_and_scaled_pixels_match_gnu() {
    compare("size");
}

#[test]
fn image_metadata_gif_extensions_match_gnu() {
    compare("metadata");
}

#[test]
fn image_decode_completion_reflows_without_buffer_edit_like_gnu() {
    compare("reflow");
}

#[test]
fn image_metadata_webp_property_order_matches_gnu() {
    compare("webp");
}

#[test]
fn image_metadata_tiff_pages_and_dimensions_match_gnu() {
    // Classic data plus both BigTIFF byte orders, from data and unknown-extension files.
    compare("tiff");
}

#[test]
fn image_mask_bitmap_policy_matches_gnu() {
    compare("mask");
}

#[test]
fn image_metadata_tiff_damaged_links_preserve_readable_pages_like_gnu() {
    compare("tiff-links");
}

fn compare(case: &str) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root.join(format!("tmp/image-api-{case}-{}", std::process::id()));
    fs::create_dir_all(&artifacts).unwrap();
    let session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts)
        .expect("image APIs require an X11 display");
    let gnu = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH")
            .expect("set NEOVM_FORCE_ORACLE_PATH to the checked GNU GUI oracle"),
    );
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/image-api-parity.el");
    let mut values = Vec::new();
    for (editor, binary) in [("gnu", gnu), ("neo", neo)] {
        let result = artifacts.join(format!("{editor}.el"));
        let _ = fs::remove_file(&result);
        let mut env = session.env().to_vec();
        env.extend([
            ("GDK_BACKEND".into(), "x11".into()),
            ("NEOMACS_IMAGE_API_CASE".into(), case.into()),
            (
                "NEOMACS_GUI_IMAGE_RESULT".into(),
                result.display().to_string(),
            ),
        ]);
        let output = ProcessGuiCommandRunner
            .run(
                &CommandSpec {
                    program: binary,
                    args: vec!["-Q".into(), "-l".into(), fixture.display().to_string()],
                    env,
                },
                &GuiArtifactSet::new(&artifacts, GuiBackend::LinuxX11, editor),
                &GuiRunOptions::with_timeout(Duration::from_secs(30)),
            )
            .expect("run paired image fixture");
        assert!(!output.timed_out, "{editor}: {output:#?}");
        assert_eq!(output.exit_code, Some(0), "{editor}: {output:#?}");
        values.push(fs::read_to_string(&result).expect("editor must write image API result"));
    }
    if case == "reflow" {
        assert_eq!(
            values[0], "(t t)\n",
            "GNU must observe the decoded geometry"
        );
    }
    if case == "tiff" {
        let classic = "((2 . 1) (count 2)) ((3 . 2) (count 2))";
        let pages = "(((2 . 1) (count 2)) ((3 . 2) (count 2)))";
        let expected = format!("({classic} ({pages} {pages}) ({pages} {pages}))");
        assert_eq!(
            values[0].trim(),
            expected,
            "GNU must decode both pages of classic TIFF and both BigTIFF byte orders, from data and unknown-extension files"
        );
    }
    assert_eq!(values[1], values[0], "{case}; artifacts: {artifacts:?}");
}
