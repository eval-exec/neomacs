#![cfg(target_os = "linux")]
//! GNU oracle for the GUI cursor's x position on right-to-left rows (issue
//! #785 follow-up N-gui-rtl-cursor).
//!
//! GNU chooses the cursor glyph with `set_cursor_from_row`
//! (`src/xdisp.c:18609`); for a reversed row it scans the visually ordered
//! glyph array from the right and computes `x` by summing every glyph to the
//! left of the chosen one (`compute_x`, `src/xdisp.c:19120-19130`).  A GUI box
//! cursor then paints that glyph cell with the frame's cursor colour and the
//! glyph face's background (`x_set_cursor_gc` / `xterm.c`), so a cursor over a
//! space is a solid dark cell.
//!
//! Both arms run the same fixture: the reporter's R2L text (and an all-LTR
//! control) with a red line background and point on a space.  The wrapper
//! screenshots the frame and reads the dark cell inside the red band, so the
//! compared observable is the painted cursor position, not an engine API.
//!
//! Prerequisites: GNU GUI Emacs 31.1 (`NEOMACS_GNU_GUI_BINARY`), Xvfb and
//! ImageMagick `import`, as for the other native GUI suites.

use std::path::{Path, PathBuf};
use std::time::Duration;

use neomacs_gui_tests::{DisplayHarness, GuiBackend};
use std::process::Command;

const FIXTURE: &str = "crates/neomacs-gui-tests/fixtures/bidi-gui-cursor-785.el";

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
/// Latin and FreeSerif for Arabic, exactly like the #785 GUI oracles.
fn write_private_fontconfig(artifacts: &Path) -> PathBuf {
    let fonts = artifacts.join("bidi-cursor-fonts");
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
    let fontconfig = artifacts.join("bidi-cursor-fonts.conf");
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
            artifacts.join("bidi-cursor-fc-cache").display(),
        ),
    )
    .expect("write fonts.conf");
    fontconfig
}

#[derive(Clone, Copy, Debug)]
enum Case {
    /// Auto-detected R2L paragraph (first strong character is Arabic).
    AutoRtl,
    /// An all-LTR line under an explicit right-to-left paragraph: the row is
    /// reversed and right-aligned.
    RightToLeftAscii,
    /// An ordinary LTR row, as a control.
    LeftToRightAscii,
    /// An LTR paragraph holding an RTL run: the row is not reversed, but the
    /// Arabic words are right-to-left internally.
    LeftToRightArabic,
}

impl Case {
    const ALL: [Case; 4] = [
        Case::AutoRtl,
        Case::RightToLeftAscii,
        Case::LeftToRightAscii,
        Case::LeftToRightArabic,
    ];

    fn label(self) -> &'static str {
        match self {
            Case::AutoRtl => "auto-rtl",
            Case::RightToLeftAscii => "right-to-left-ascii",
            Case::LeftToRightAscii => "left-to-right-ascii",
            Case::LeftToRightArabic => "left-to-right-arabic",
        }
    }

    fn direction_env(self) -> &'static str {
        match self {
            Case::AutoRtl => "nil",
            Case::RightToLeftAscii => "right-to-left",
            Case::LeftToRightAscii | Case::LeftToRightArabic => "left-to-right",
        }
    }

    fn text_env(self) -> &'static str {
        match self {
            Case::AutoRtl | Case::LeftToRightArabic => "arabic",
            Case::RightToLeftAscii | Case::LeftToRightAscii => "ascii",
        }
    }

    /// 1-based positions of the two spaces in the fixture text.
    fn points(self) -> [usize; 2] {
        match self {
            Case::AutoRtl
            | Case::RightToLeftAscii
            | Case::LeftToRightAscii
            | Case::LeftToRightArabic => [3, 6],
        }
    }

    /// Whether the row's text starts at the right margin (reversed row).
    fn anchored_right(self) -> bool {
        matches!(self, Case::AutoRtl | Case::RightToLeftAscii)
    }
}

/// The red-backed text extent painted for the fixture line.
#[derive(Clone, Copy, Debug)]
struct Band {
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
}

fn painted_band(image: &image::RgbImage) -> Option<Band> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let red = |x: usize, y: usize| {
        let pixel = image.get_pixel(x as u32, y as u32).0;
        pixel[0] >= 160 && pixel[1] <= 90 && pixel[2] <= 90
    };
    let rows: Vec<usize> = (0..height)
        .filter(|&y| (0..width).filter(|&x| red(x, y)).count() > 8)
        .collect();
    let (y0, y1) = (*rows.first()?, *rows.last()?);
    let columns: Vec<usize> = (0..width)
        .filter(|&x| (y0..=y1).any(|y| red(x, y)))
        .collect();
    let (x0, x1) = (*columns.first()?, *columns.last()?);
    Some(Band { x0, x1, y0, y1 })
}

/// Columns inside the text band whose pixels differ between two frames.
///
/// The only state that changes between the two point positions is the cursor
/// (blink is off), so each run is one painted cursor cell.  Diffing avoids
/// guessing the cursor colour and works for bar or box cursors alike; the
/// first version of this row assumed a filled box and mistook an Arabic
/// letter's vertical stroke for the cursor.
fn changed_cursor_runs(
    first: &image::RgbImage,
    second: &image::RgbImage,
    band: Band,
) -> Vec<(usize, usize)> {
    let differs = |x: usize, y: usize| {
        let a = first.get_pixel(x as u32, y as u32).0;
        let b = second.get_pixel(x as u32, y as u32).0;
        a.iter()
            .zip(b.iter())
            .map(|(a, b)| u32::from(a.abs_diff(*b)))
            .sum::<u32>()
            > 30
    };
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    for x in band.x0..=band.x1 {
        let changed = (band.y0..=band.y1).any(|y| differs(x, y));
        if changed {
            match current.as_mut() {
                Some(run) => run.1 = x,
                None => current = Some((x, x)),
            }
        } else if let Some(run) = current.take() {
            runs.push(run);
        }
    }
    if let Some(run) = current.take() {
        runs.push(run);
    }
    runs
}

/// Screenshot one engine's frame with `point`.  The process is killed by this
/// wrapper, always.
fn capture_frame(
    program: &Path,
    engine: &str,
    root: &Path,
    artifacts: &Path,
    fontconfig: &Path,
    session_env: &[(String, String)],
    case: Case,
    point: usize,
) -> PathBuf {
    let png = artifacts.join(format!("bidi-cursor-{engine}-{}-{point}.png", case.label()));
    let mut command = Command::new(program);
    command
        .arg("-Q")
        .arg("-l")
        .arg(root.join(FIXTURE))
        .env("R2_DIR", case.direction_env())
        .env("R2_REORDER", "t")
        .env("R2_TEXT", case.text_env())
        .env("R2_POINT", point.to_string())
        .env("FONTCONFIG_FILE", fontconfig)
        .env("GDK_BACKEND", "x11");
    for (key, value) in session_env {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("spawn {engine} {}: {error}", case.label()));

    // The red band is the readiness marker: once it is painted, the frame has
    // laid out the fixture line.
    let mut painted = false;
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
            "ImageMagick import failed for {engine} {} point {point} (attempt {})",
            case.label(),
            attempt + 1
        );
        if let Ok(image) = image::open(&png) {
            painted = painted_band(&image.to_rgb8()).is_some();
            if painted {
                break;
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        painted,
        "{engine} {} point {point}: no red text band was painted in {}",
        case.label(),
        png.display()
    );
    png
}

fn open_rgb(png: &Path) -> image::RgbImage {
    image::open(png)
        .unwrap_or_else(|error| panic!("read screenshot {}: {error}", png.display()))
        .to_rgb8()
}

#[test]
// Prerequisites: requires GNU GUI Emacs, Xvfb and ImageMagick import.
fn gui_cursor_on_rtl_rows_matches_gnu() {
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
        let [first_point, second_point] = case.points();
        let capture = |program: &Path, engine: &str, point: usize| {
            capture_frame(
                program,
                engine,
                &root,
                &artifact_root,
                &fontconfig,
                &session_env,
                case,
                point,
            )
        };
        let gnu_first = capture(&gnu_program, "gnu", first_point);
        let gnu_second = capture(&gnu_program, "gnu", second_point);
        let neo_first = capture(&neo_program, "neomacs", first_point);
        let neo_second = capture(&neo_program, "neomacs", second_point);

        let gnu_image = open_rgb(&gnu_first);
        let gnu_band = painted_band(&gnu_image).expect("GNU red band");
        let gnu_runs = changed_cursor_runs(&gnu_image, &open_rgb(&gnu_second), gnu_band);
        let neo_image = open_rgb(&neo_first);
        let neo_band = painted_band(&neo_image).expect("Neo red band");
        let neo_runs = changed_cursor_runs(&neo_image, &open_rgb(&neo_second), neo_band);

        eprintln!(
            "gui-cursor case={} points={first_point},{second_point} gnu_band={gnu_band:?}              gnu_runs={gnu_runs:?} neo_band={neo_band:?} neo_runs={neo_runs:?}",
            case.label(),
        );

        assert_eq!(
            gnu_runs.len(),
            2,
            "{}: expected one changed cursor cell per point, GNU has {gnu_runs:?} \
             (bands {gnu_band:?}/{neo_band:?}); screenshots: {} {}",
            case.label(),
            gnu_first.display(),
            gnu_second.display()
        );
        assert_eq!(
            neo_runs.len(),
            2,
            "{}: expected one changed cursor cell per point, Neo has {neo_runs:?} \
             (bands {gnu_band:?}/{neo_band:?}); screenshots: {} {}",
            case.label(),
            neo_first.display(),
            neo_second.display()
        );

        for ((gnu_x0, gnu_x1), (neo_x0, neo_x1)) in gnu_runs.iter().zip(&neo_runs) {
            assert!(
                (*gnu_x1 - *gnu_x0 + 1) >= 2,
                "{}: GNU's cursor run is too narrow: {gnu_runs:?}",
                case.label()
            );
            assert!(
                (*neo_x1 - *neo_x0 + 1) >= 2,
                "{}: Neo's cursor run is too narrow: {neo_runs:?}",
                case.label()
            );
            // The engines can lay the frame out at different character widths
            // (a 60-column frame is 433px in GNU here and wider in Neo), so
            // compare each cursor cell's offset from the alignment margin of
            // its own painted text band.  A cursor placed by a logical (LTR)
            // walk on a reversed row lands tens of pixels away, not within a
            // cell.
            let gnu_center = (*gnu_x0 + *gnu_x1) as i64 / 2;
            let neo_center = (*neo_x0 + *neo_x1) as i64 / 2;
            let (gnu_offset, neo_offset) = if case.anchored_right() {
                (
                    i64::try_from(gnu_band.x1).unwrap_or(i64::MAX) - gnu_center,
                    i64::try_from(neo_band.x1).unwrap_or(i64::MAX) - neo_center,
                )
            } else {
                (
                    gnu_center - i64::try_from(gnu_band.x0).unwrap_or(0),
                    neo_center - i64::try_from(neo_band.x0).unwrap_or(0),
                )
            };
            assert!(
                (gnu_offset - neo_offset).abs() <= 4,
                "{}: Neo's cursor cell is at a different offset from the text \
                 margin than GNU's: GNU {gnu_runs:?} in {gnu_band:?} vs Neo \
                 {neo_runs:?} in {neo_band:?}",
                case.label()
            );
        }
    }
}
