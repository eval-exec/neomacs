#![cfg(target_os = "linux")]
//! GNU oracle coverage for automatic composition spans, shaped gstrings and
//! the one-width contract of issue #785 on a graphical frame.
//!
//! FreeSerif Arabic (JA = U+062C U+0627, lone JIM) and Devanagari (STE inside
//! NAMASTE) run under GNU Emacs and Neomacs on one X11 display, under a
//! private fontconfig holding exactly one Latin-only mono default and
//! FreeSerif as the only Arabic/Devanagari-capable fallback.
//!
//! The contract pinned here, from GNU 31.1 `composite.c`: `find-composition`
//! falls back to `find_automatic_composition` when the displayed buffer has
//! no stored `composition` property, returning `(START END GSTRING)` with a
//! SHAPED gstring; and the width an automatic composition contributes to
//! `string-pixel-width` is the sum of that gstring's glyph WIDTH slots.
//! Neomacs previously returned nil from `find-composition` everywhere and
//! measured automatic compositions by a uniform per-character plan, so
//! `string-pixel-width` and the layout matrix disagreed with each other and
//! with GNU (issue #785).
//!
//! Rows are engine-internal facts only (spans, glyph columns, sum-equality
//! booleans): the two engines may resolve a different pixel size for the
//! same face, so pixel literals are deliberately not compared.
//!
//! This is a real GNU-parity contract: it is not to be blessed from
//! Neomacs, weakened, or skipped.

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

const FIXTURE: &str = "crates/neomacs-gui-tests/fixtures/composite-785-gui-oracle.el";
const SCENARIO: &str = "composite-785-oracle";
const RESULT_ENV: &str = "NEOMACS_785_ORACLE_RESULT";

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

/// The private fontconfig both arms resolve fonts through: Ubuntu Mono for
/// Latin, FreeSerif for everything Arabic or Devanagari.  Returns the
/// `FONTCONFIG_FILE` path.
fn write_private_fontconfig(artifacts: &Path) -> PathBuf {
    let fonts = artifacts.join("fonts");
    std::fs::create_dir_all(&fonts).expect("fonts dir");
    std::fs::copy(
        neomacs_test_fonts::desktop_fonts()
            .directory()
            .join("UbuntuMono[wght].ttf"),
        fonts.join("UbuntuMono.ttf"),
    )
    .expect("pinned Ubuntu Mono fixture");
    std::fs::copy(
        neomacs_test_fonts::free_serif(),
        fonts.join("FreeSerif.ttf"),
    )
    .expect("pinned FreeSerif fixture");
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
    .expect("write fonts.conf");
    fontconfig
}

fn write_diff(path: &Path, gnu: &str, neo: &str) {
    let mut diff = String::new();
    for (gnu_line, neo_line) in gnu.lines().zip(neo.lines()) {
        if gnu_line == neo_line {
            diff.push_str(&format!(" {gnu_line}\n"));
        } else {
            diff.push_str(&format!("-{gnu_line}\n+{neo_line}\n"));
        }
    }
    let _ = std::fs::write(path, diff);
}

#[test]
// Prerequisites: requires GNU GUI Emacs and Xvfb.
fn composite_785_rows_match_gnu() {
    let root = neomacs_infra::crate_root!().join("../..");
    let backend = GuiBackend::LinuxX11;
    let artifact_root = root.join("target/neomacs-gui-tests");
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifact_root)
        .expect("display session should start");
    let fontconfig = write_private_fontconfig(&artifact_root);

    let scenario = GuiScenario::new(SCENARIO, root.join(FIXTURE));
    let artifacts = GuiArtifactSet::new(&artifact_root, backend, SCENARIO);
    let gnu_result_path = artifact_root.join(format!("{SCENARIO}.gnu-result.el"));
    let neo_result_path = artifact_root.join(format!("{SCENARIO}.neomacs-result.el"));
    let diff_path = artifact_root.join(format!("{SCENARIO}.diff"));
    let _ = std::fs::remove_file(&gnu_result_path);
    let _ = std::fs::remove_file(&neo_result_path);

    // GNU Emacs writes the oracle file itself; the neomacs side gets the same
    // variable through the plan below.
    let mut gnu_env = vec![
        (
            RESULT_ENV.to_string(),
            gnu_result_path.display().to_string(),
        ),
        ("GDK_BACKEND".to_string(), "x11".to_string()),
        (
            "FONTCONFIG_FILE".to_string(),
            fontconfig.display().to_string(),
        ),
    ];
    gnu_env.extend(session.env().iter().cloned());
    let gnu_command = CommandSpec {
        program: gnu_binary(),
        args: vec![
            "-Q".to_string(),
            "-l".to_string(),
            root.join(FIXTURE).display().to_string(),
        ],
        env: gnu_env,
    };
    let mut runner = ProcessGuiCommandRunner;
    let gnu_output = runner
        .run(
            &gnu_command,
            &artifacts,
            &GuiRunOptions::with_timeout(Duration::from_secs(60)),
        )
        .expect("GNU composite oracle should run");
    assert_eq!(
        gnu_output.exit_code,
        Some(0),
        "GNU composite oracle failed; stderr:\n{}",
        gnu_output.stderr
    );

    let binary = neomacs_binary(&root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the composite oracle"
    );
    let mut plan = GuiTestPlan::new(backend, &root, &artifact_root, scenario)
        .with_program(binary)
        .with_env(RESULT_ENV, neo_result_path.display().to_string())
        .with_env("GDK_BACKEND", "x11")
        .with_env("FONTCONFIG_FILE", fontconfig.display().to_string());
    for (key, value) in session.env() {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut runner,
            GuiRunOptions::with_timeout(Duration::from_secs(60)),
        )
        .expect("neomacs composite oracle should run");
    assert_eq!(
        result.exit_code,
        Some(0),
        "neomacs composite oracle failed; stderr:\n{}",
        result.artifacts.stderr.display()
    );

    let gnu_result = std::fs::read_to_string(&gnu_result_path)
        .unwrap_or_else(|error| panic!("GNU oracle did not write {gnu_result_path:?}: {error}"));
    let neo_result = std::fs::read_to_string(&neo_result_path).unwrap_or_else(|error| {
        panic!("Neomacs oracle did not write {neo_result_path:?}: {error}")
    });

    write_diff(&diff_path, &gnu_result, &neo_result);

    let gnu_lines = gnu_result.lines().collect::<Vec<_>>();
    let neo_lines = neo_result.lines().collect::<Vec<_>>();
    assert_eq!(
        neo_lines.len(),
        gnu_lines.len(),
        "oracle case count differs\nGNU:\n{gnu_result}\nNeomacs:\n{neo_result}"
    );

    let mut divergences = Vec::new();
    for (gnu_line, neo_line) in gnu_lines.iter().zip(&neo_lines) {
        if neo_line != gnu_line {
            divergences.push(format!("GNU:     {gnu_line}\nNeomacs: {neo_line}"));
        }
    }
    assert!(
        divergences.is_empty(),
        "composite #785 rows diverged from GNU in {} case(s):\n\n{}\n\ndiff: {}",
        divergences.len(),
        divergences.join("\n\n"),
        diff_path.display()
    );
}
