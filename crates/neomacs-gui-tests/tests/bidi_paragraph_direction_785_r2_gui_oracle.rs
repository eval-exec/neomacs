#![cfg(target_os = "linux")]
//! GNU oracle for the displayed paragraph direction (issue #785 R2).
//!
//! GNU reads the buffer-local `bidi-display-reordering` and
//! `bidi-paragraph-direction` per window at `start_display`
//! (`src/xdisp.c:3600-3611`) and stores `it->paragraph_embedding`;
//! `bidi_paragraph_init` (`src/bidi.c:1713`) turns that into the paragraph base
//! level.  A `left-to-right` paragraph therefore keeps the reporter's RTL text
//! flush to the LEFT margin, a `right-to-left` paragraph right-aligns it
//! (`reversed_p`), a nil direction picks the first strong character (P2/P3,
//! right here), and a nil `bidi-display-reordering` leaves the logical order
//! untouched.
//!
//! Neomacs used to resolve every row with UAX#9 automatic detection, so
//! `(setq-default bidi-paragraph-direction 'left-to-right)` still drew the
//! reporter's Arabic file flush right.
//!
//! Both arms run the same fixture with the reporter's exact bytes and the same
//! private fontconfig; each frame is screenshotted as a whole with ImageMagick
//! `import`, and the test reads the x position of the first narrow ink band
//! (the first Arabic line) inside the window's white content box.  A narrow
//! band left of the window midpoint is a left-aligned paragraph, one right of
//! it is right-aligned.  GNU 31.1 displays right, left, right, left for
//! nil / left-to-right / right-to-left / reordering-off, attested on the
//! pinned GUI oracle.
//!
//! Prerequisites: GNU GUI Emacs 31.1 (`NEOMACS_GNU_GUI_BINARY`), Xvfb and
//! ImageMagick `import`, as for the native-menu GUI suites.

use std::path::{Path, PathBuf};
use std::time::Duration;

use neomacs_gui_tests::{DisplayHarness, GuiBackend};
use std::process::Command;

const FIXTURE: &str = "crates/neomacs-gui-tests/fixtures/bidi-paragraph-direction-785-r2-gui.el";

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

/// The private fontconfig both arms resolve fonts through: Ubuntu Mono for
/// Latin and FreeSerif for Arabic, exactly like the #785 composition oracle.
fn write_private_fontconfig(artifacts: &Path) -> PathBuf {
    let fonts = artifacts.join("bidi-r2-fonts");
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
    let fontconfig = artifacts.join("bidi-r2-fonts.conf");
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
            artifacts.join("bidi-r2-fc-cache").display(),
        ),
    )
    .expect("write fonts.conf");
    fontconfig
}

#[derive(Clone, Copy, Debug)]
enum Case {
    Auto,
    LeftToRight,
    RightToLeft,
    Off,
    /// An all-LTR line under an explicit `right-to-left` paragraph: GNU pins
    /// the base to R2L, so the row is right-aligned and its L characters sit
    /// at level 2.  The expectation is taken from the paired GNU arm.
    RightToLeftAscii,
}

impl Case {
    const ALL: [Case; 4] = [Case::Auto, Case::LeftToRight, Case::RightToLeft, Case::Off];

    fn label(self) -> &'static str {
        match self {
            Case::Auto => "direction-nil",
            Case::LeftToRight => "direction-left-to-right",
            Case::RightToLeft => "direction-right-to-left",
            Case::Off => "reordering-nil",
            Case::RightToLeftAscii => "right-to-left-ascii",
        }
    }

    fn direction_env(self) -> &'static str {
        match self {
            Case::Auto | Case::Off => "nil",
            Case::LeftToRight => "left-to-right",
            Case::RightToLeft | Case::RightToLeftAscii => "right-to-left",
        }
    }

    fn text_env(self) -> &'static str {
        match self {
            Case::RightToLeftAscii => "ascii",
            _ => "arabic",
        }
    }

    fn reordering_env(self) -> &'static str {
        match self {
            Case::Off => "nil",
            _ => "t",
        }
    }

    /// Pinned GNU 31.1 GUI: `JA`/`JIM` sit at the right margin for nil and
    /// `right-to-left`, and at the left margin for `left-to-right` and a nil
    /// `bidi-display-reordering`.
    /// Pinned GNU 31.1 expectation, or `None` when the paired GNU arm itself
    /// is the expectation (the all-LTR row under right-to-left).
    fn gnu_alignment(self) -> Option<&'static str> {
        match self {
            Case::Auto | Case::RightToLeft => Some("right"),
            Case::LeftToRight | Case::Off => Some("left"),
            Case::RightToLeftAscii => None,
        }
    }
}

/// Screenshot one engine's whole frame after the fixture has displayed the
/// reporter text.  The child is killed by this wrapper, always.
fn capture_frame(
    program: &Path,
    engine: &str,
    root: &Path,
    artifacts: &Path,
    fontconfig: &Path,
    session_env: &[(String, String)],
    case: Case,
) -> PathBuf {
    let png = artifacts.join(format!("bidi-r2-{engine}-{}.png", case.label()));
    let mut command = Command::new(program);
    command
        .arg("-Q")
        .arg("-l")
        .arg(root.join(FIXTURE))
        .env("R2_DIR", case.direction_env())
        .env("R2_REORDER", case.reordering_env())
        .env("R2_TEXT", case.text_env())
        .env("FONTCONFIG_FILE", fontconfig)
        .env("GDK_BACKEND", "x11");
    for (key, value) in session_env {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("spawn {engine} {}: {error}", case.label()));

    // Wait for the frame and its first redisplay, retrying briefly on a slow
    // host.  The screenshot itself is the observable; nothing depends on an
    // engine-specific ready marker.
    let mut captured_with_window = false;
    for attempt in 0..3 {
        std::thread::sleep(Duration::from_secs(4));
        let status = Command::new("import")
            .arg("-window")
            .arg("root")
            .arg(&png)
            .envs(session_env.iter().map(|(key, value)| (key, value)))
            .status()
            .unwrap_or_else(|error| panic!("run ImageMagick import for {engine}: {error}"));
        assert!(
            status.success(),
            "ImageMagick import failed for {engine} {} (attempt {})",
            case.label(),
            attempt + 1
        );
        captured_with_window = screenshot_has_window(&png);
        if captured_with_window {
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        captured_with_window,
        "{engine} {} never painted an editor window",
        case.label()
    );
    assert!(
        png.exists(),
        "no screenshot was written for {engine} {}",
        case.label()
    );
    png
}

/// Whether the screenshot contains a light editor window at all.
fn screenshot_has_window(png: &Path) -> bool {
    let Ok(image) = image::open(png) else {
        return false;
    };
    let image = image.to_luma8();
    let width = image.width() as usize;
    (0..image.height() as usize).any(|y| {
        (0..width)
            .filter(|&x| image.get_pixel(x as u32, y as u32)[0] >= 250)
            .count()
            > width / 5
    })
}

/// Alignment of the first narrow dark ink band inside the window's white
/// content box, as painted on screen.
fn painted_alignment(png: &Path) -> String {
    let image = image::open(png)
        .unwrap_or_else(|error| panic!("read screenshot {}: {error}", png.display()))
        .to_luma8();
    let (width, height) = (image.width() as usize, image.height() as usize);
    let pixel = |x: usize, y: usize| image.get_pixel(x as u32, y as u32)[0];

    let row_white: Vec<usize> = (0..height)
        .map(|y| (0..width).filter(|&x| pixel(x, y) >= 250).count())
        .collect();
    let box_rows: Vec<usize> = (0..height).filter(|&y| row_white[y] > width / 5).collect();
    assert!(
        !box_rows.is_empty(),
        "no light editor window in screenshot {}",
        png.display()
    );
    let y0 = *box_rows.first().expect("nonempty");
    let y1 = *box_rows.last().expect("nonempty");
    let box_columns: Vec<usize> = (0..width)
        .filter(|&x| (y0..=y1).filter(|&y| pixel(x, y) >= 250).count() > (y1 - y0) / 5)
        .collect();
    assert!(
        !box_columns.is_empty(),
        "no editor columns in screenshot {}",
        png.display()
    );
    let x0 = *box_columns.first().expect("nonempty");
    let x1 = *box_columns.last().expect("nonempty");
    let box_width = x1 - x0 + 1;

    // Group dark pixels into ink bands.  A narrow band is a short run of text
    // such as the two-glyph Arabic line; menus, toolbars, the mode line and
    // echo text are wider than a quarter of the window.
    #[derive(Clone, Copy, Debug)]
    struct InkBand {
        first_row: usize,
        last_row: usize,
        min_x: usize,
        max_x: usize,
        ink: usize,
    }
    let mut bands: Vec<InkBand> = Vec::new();
    let mut current: Option<InkBand> = None;
    for y in y0..=y1 {
        let dark: Vec<usize> = (x0..=x1).filter(|&x| pixel(x, y) <= 160).collect();
        if dark.is_empty() {
            if let Some(band) = current.take() {
                bands.push(band);
            }
            continue;
        }
        let (min_x, max_x) = (dark[0], *dark.last().expect("nonempty"));
        match current.as_mut() {
            Some(band) => {
                band.last_row = y;
                band.min_x = band.min_x.min(min_x);
                band.max_x = band.max_x.max(max_x);
                band.ink += dark.len();
            }
            None => {
                current = Some(InkBand {
                    first_row: y,
                    last_row: y,
                    min_x,
                    max_x,
                    ink: dark.len(),
                })
            }
        }
    }
    if let Some(band) = current.take() {
        bands.push(band);
    }

    // A text band is short both ways: a vertical scrollbar or the frame edge
    // is narrow horizontally but runs down the window, so cap its height too.
    let narrow = bands
        .iter()
        .find(|band| {
            band.max_x - band.min_x + 1 < box_width / 4
                && band.ink >= 3
                && band.last_row - band.first_row <= 40
        })
        .unwrap_or_else(|| {
            panic!(
                "no narrow text band in screenshot {} (bands: {bands:?}, box x {x0}..{x1})",
                png.display()
            )
        });
    if narrow.min_x - x0 < box_width / 2 {
        "left".to_string()
    } else {
        "right".to_string()
    }
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb and ImageMagick import.
fn bidi_paragraph_direction_controls_rtl_alignment_like_gnu() {
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

    for case in Case::ALL {
        let gnu_png = capture_frame(
            &gnu_program,
            "gnu",
            &root,
            &artifact_root,
            &fontconfig,
            &session_env,
            case,
        );
        let gnu_alignment = painted_alignment(&gnu_png);
        if let Some(expected) = case.gnu_alignment() {
            assert_eq!(
                gnu_alignment,
                expected,
                "GNU authority changed for {}; screenshot: {}",
                case.label(),
                gnu_png.display()
            );
        }

        let neo_png = capture_frame(
            &neo_program,
            "neomacs",
            &root,
            &artifact_root,
            &fontconfig,
            &session_env,
            case,
        );
        let neo_alignment = painted_alignment(&neo_png);
        assert_eq!(
            neo_alignment,
            gnu_alignment,
            "Neomacs paragraph alignment differs from GNU for {}:\n  GNU: {}\n  Neo: {}",
            case.label(),
            gnu_png.display(),
            neo_png.display()
        );
    }
}
