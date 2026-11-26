#![cfg(target_os = "linux")]
//! GNU oracle for the GUI ink of zero-width characters (issue #785 follow-up
//! N-gui-zero-width-ink).
//!
//! GNU's GUI glyph production gives a zero-advance character a zero-width
//! glyph and then advances the pen by one pixel (`src/xdisp.c:33755-33760`,
//! applied after `append_glyph`); format controls take the default
//! `glyphless-char-display-control` method `thin-space`, a one-pixel advance
//! (`src/xdisp.c:33450-33480`, `append_glyphless_glyph`).  A Neo glyph that
//! falls back to a full character cell for any of these opens a visible gap.
//!
//! The fixture paints one line with a red background.  For each variant the
//! test measures the deltas of the painted band's start and width against a
//! control line ("ab") in the same engine; comparing deltas rather than
//! absolute widths cancels the engines' different default font metrics.
//! GNU's own deltas are the authority; Neo's must match them.
//!
//! Prerequisites: GNU GUI Emacs 31.1 (`NEOMACS_GNU_GUI_BINARY`), Xvfb and
//! ImageMagick `import`, as for the other native GUI suites.

use std::path::{Path, PathBuf};
use std::time::Duration;

use neomacs_gui_tests::{DisplayHarness, GuiBackend};
use std::process::Command;

const FIXTURE: &str = "crates/neomacs-gui-tests/fixtures/bidi-gui-zero-width-785.el";

fn gnu_binary() -> PathBuf {
    std::env::var_os("NEOMACS_GNU_GUI_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"))
}

fn neomacs_binary(root: &Path) -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"))
}

fn copy_fixture(source: &Path, destination: &Path, label: &str) {
    let _ = std::fs::remove_file(destination);
    std::fs::copy(source, destination).unwrap_or_else(|error| panic!("{label}: {error}"));
}

/// The private fontconfig both arms resolve fonts through, as in the other
/// #785 GUI oracles.
fn write_private_fontconfig(artifacts: &Path) -> PathBuf {
    let fonts = artifacts.join("bidi-zero-width-fonts");
    std::fs::create_dir_all(&fonts).expect("fonts dir");
    copy_fixture(
        &neomacs_test_fonts::desktop_fonts()
            .directory()
            .join("UbuntuMono[wght].ttf"),
        &fonts.join("UbuntuMono.ttf"),
        "pinned Ubuntu Mono fixture",
    );
    copy_fixture(
        neomacs_test_fonts::free_serif(),
        &fonts.join("FreeSerif.ttf"),
        "pinned FreeSerif fixture",
    );
    let fontconfig = artifacts.join("bidi-zero-width-fonts.conf");
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
            artifacts.join("bidi-zero-width-fc-cache").display(),
        ),
    )
    .expect("write fonts.conf");
    fontconfig
}

#[derive(Clone, Copy, Debug)]
struct Band {
    x0: usize,
    x1: usize,
}

fn painted_band(image: &image::RgbImage) -> Option<Band> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let red = |x: usize, y: usize| {
        let pixel = image.get_pixel(x as u32, y as u32).0;
        pixel[0] >= 160 && pixel[1] <= 90 && pixel[2] <= 90
    };
    let rows: Vec<usize> = (0..height)
        .filter(|&y| (0..width).filter(|&x| red(x, y)).count() > 2)
        .collect();
    let (y0, y1) = (*rows.first()?, *rows.last()?);
    let columns: Vec<usize> = (0..width)
        .filter(|&x| (y0..=y1).any(|y| red(x, y)))
        .collect();
    let (x0, x1) = (*columns.first()?, *columns.last()?);
    Some(Band { x0, x1 })
}

/// Screenshot one engine's frame for `text`.  The process is killed by this
/// wrapper, always.
fn capture_frame(
    program: &Path,
    engine: &str,
    root: &Path,
    artifacts: &Path,
    fontconfig: &Path,
    session_env: &[(String, String)],
    text: &str,
) -> image::RgbImage {
    let png = artifacts.join(format!("bidi-zero-width-{engine}-{text}.png"));
    let mut command = Command::new(program);
    command
        .arg("-Q")
        .arg("-l")
        .arg(root.join(FIXTURE))
        .env("R2_TEXT", text)
        .env("FONTCONFIG_FILE", fontconfig)
        .env("GDK_BACKEND", "x11");
    for (key, value) in session_env {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("spawn {engine} {text}: {error}"));

    let mut image = None;
    for attempt in 0..5 {
        std::thread::sleep(if attempt == 0 {
            Duration::from_secs(3)
        } else {
            Duration::from_millis(900)
        });
        let status = Command::new("import")
            .arg("-window")
            .arg("root")
            .arg(&png)
            .envs(session_env.iter().map(|(key, value)| (key, value)))
            .status()
            .unwrap_or_else(|error| panic!("run ImageMagick import for {engine}: {error}"));
        assert!(
            status.success(),
            "ImageMagick import failed for {engine} {text} (attempt {})",
            attempt + 1
        );
        if let Ok(shot) = image::open(&png) {
            let shot = shot.to_rgb8();
            if painted_band(&shot).is_some() {
                image = Some(shot);
                break;
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    image.unwrap_or_else(|| {
        panic!(
            "{engine} {text}: no red text band was painted in {}",
            png.display()
        )
    })
}

/// Painted start and width of the line called `text` in one engine.
fn band_for(
    program: &Path,
    engine: &str,
    root: &Path,
    artifacts: &Path,
    fontconfig: &Path,
    session_env: &[(String, String)],
    text: &str,
) -> Band {
    let image = capture_frame(
        program,
        engine,
        root,
        artifacts,
        fontconfig,
        session_env,
        text,
    );
    painted_band(&image).expect("capture returns a painted band")
}

// Format controls (the peer's N-gui-zero-width-ink scope) plus a composed
// cluster control.  A leading/orphan combining mark is deliberately NOT here:
// see the commit message; GNU advances it by a font-metric fallback (6 px at
// this size) that Neo does not reproduce, which is a separate divergence.
const VARIANTS: [&str; 5] = ["rlm", "lrm", "zwnj", "zwj", "combining-after"];

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb and ImageMagick import.
fn gui_zero_width_characters_paint_like_gnu() {
    let root = neomacs_infra::crate_root!().join("../..");
    let backend = GuiBackend::LinuxX11;
    let artifact_root = root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");
    let fontconfig = write_private_fontconfig(&artifact_root);
    let session_env: Vec<(String, String)> = session.env().to_vec();

    let gnu_program = gnu_binary();
    let neo_program = neomacs_binary(&root);
    assert!(
        neo_program.exists(),
        "Neomacs binary missing: {}",
        neo_program.display()
    );

    let measure = |program: &Path, engine: &str| {
        let plain = band_for(
            program,
            engine,
            &root,
            &artifact_root,
            &fontconfig,
            &session_env,
            "plain",
        );
        assert!(
            plain.x1 > plain.x0,
            "{engine}: the control line painted no width: {plain:?}"
        );
        let mut deltas = Vec::new();
        for variant in VARIANTS {
            let band = band_for(
                program,
                engine,
                &root,
                &artifact_root,
                &fontconfig,
                &session_env,
                variant,
            );
            // Delta of the painted start and of the painted width relative to
            // the same engine's control line.
            deltas.push((
                variant,
                band.x0 as i64 - plain.x0 as i64,
                (band.x1 - band.x0) as i64 - (plain.x1 - plain.x0) as i64,
            ));
        }
        (plain, deltas)
    };

    let (gnu_plain, gnu_deltas) = measure(&gnu_program, "gnu");
    let (neo_plain, neo_deltas) = measure(&neo_program, "neomacs");
    eprintln!(
        "zero-width bands: gnu plain {gnu_plain:?} deltas {gnu_deltas:?}; \
         neo plain {neo_plain:?} deltas {neo_deltas:?}"
    );

    for ((variant, gnu_x0, gnu_width), (_, neo_x0, neo_width)) in gnu_deltas.iter().zip(&neo_deltas)
    {
        // A full character cell for a zero-width character moves the painted
        // start or width by one cell (many pixels); a correct one-pixel pen
        // advance is within the small tolerance.
        assert!(
            (gnu_x0 - neo_x0).abs() <= 2,
            "{variant}: Neo's painted start delta differs from GNU's: \
             GNU ({gnu_x0}, {gnu_width}) vs Neo ({neo_x0}, {neo_width})"
        );
        assert!(
            (gnu_width - neo_width).abs() <= 2,
            "{variant}: Neo's painted width delta differs from GNU's: \
             GNU ({gnu_x0}, {gnu_width}) vs Neo ({neo_x0}, {neo_width})"
        );
    }
}
