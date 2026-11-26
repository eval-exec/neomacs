#![cfg(target_os = "linux")]
//! GNU oracle for N-orphan-combining-advance: the automatic-composition
//! gstring of a lone combining mark must carry the font driver's unshaped
//! `text_extents` (GNU `font_fill_lglyph_metrics`, src/font.c) and the
//! matched rule must run through `auto-composition-function`
//! (`autocmp_chars`, src/composite.c).
//!
//! Under a pinned FreeSerif-14 face GNU fills U+0301's glyph with
//! `[0 0 769 708 0 -4 0 9 -7 [4 0 4]]`: width 0, lbearing -4, rbearing 0,
//! ascent 9, descent -7, and `compose-gstring-for-graphic`'s `nchars = 1`
//! branch stores the spacing adjustment `[4 0 4]`.  Those integers decide
//! the displayed advance of a leading mark, which Neomacs previously
//! measured as a full character cell.
//!
//! Both engines run on one X11 display under a private fontconfig holding
//! the pinned fonts; the fixture writes only Lisp-visible integers, so the
//! two result files compare line by line.

use neomacs_gui_tests::{
    CommandSpec, DisplayHarness, GuiArtifactSet, GuiBackend, GuiCommandRunner, GuiRunOptions,
    GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

const FIXTURE: &str = "crates/neomacs-gui-tests/fixtures/composition-gstring-metrics-n-orphan.el";
const SCENARIO: &str = "composition-gstring-metrics-n-orphan";
const RESULT_ENV: &str = "NEOMACS_N_ORPHAN_ORACLE_RESULT";

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
/// Latin and FreeSerif as the pinned fallback.  The fixture sets the frame
/// font to FreeSerif-14 explicitly.
fn write_private_fontconfig(artifacts: &Path) -> PathBuf {
    let fonts = artifacts.join("fonts");
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

#[test]
// Prerequisites: requires GNU GUI Emacs and Xvfb.
fn orphan_combining_mark_gstring_rows_match_gnu() {
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
        .expect("GNU orphan gstring oracle should run");
    assert_eq!(
        gnu_output.exit_code,
        Some(0),
        "GNU orphan gstring oracle failed; stderr:\n{}",
        gnu_output.stderr
    );

    let binary = neomacs_binary(&root);
    assert!(
        binary.exists(),
        "build {binary:?} before running the orphan gstring oracle"
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
        .expect("neomacs orphan gstring oracle should run");
    assert_eq!(
        result.exit_code,
        Some(0),
        "neomacs orphan gstring oracle failed; stderr:\n{}",
        result.artifacts.stderr.display()
    );

    let gnu_result = std::fs::read_to_string(&gnu_result_path)
        .unwrap_or_else(|error| panic!("GNU oracle did not write {gnu_result_path:?}: {error}"));
    let neo_result = std::fs::read_to_string(&neo_result_path).unwrap_or_else(|error| {
        panic!("Neomacs oracle did not write {neo_result_path:?}: {error}")
    });

    let mut diff = String::new();
    for (gnu_line, neo_line) in gnu_result.lines().zip(neo_result.lines()) {
        if gnu_line == neo_line {
            diff.push_str(&format!(" {gnu_line}\n"));
        } else {
            diff.push_str(&format!("-{gnu_line}\n+{neo_line}\n"));
        }
    }
    let _ = std::fs::write(&diff_path, &diff);

    let gnu_lines = gnu_result.lines().collect::<Vec<_>>();
    let neo_lines = neo_result.lines().collect::<Vec<_>>();
    assert_eq!(
        neo_lines.len(),
        gnu_lines.len(),
        "oracle row count differs\nGNU:\n{gnu_result}\nNeomacs:\n{neo_result}"
    );
    let divergences = gnu_lines
        .iter()
        .zip(&neo_lines)
        .filter(|(gnu, neo)| gnu != neo)
        .map(|(gnu, neo)| format!("GNU:     {gnu}\nNeomacs: {neo}"))
        .collect::<Vec<_>>();
    assert!(
        divergences.is_empty(),
        "orphan combining-mark rows diverged from GNU in {} case(s):\n\n{}\n\ndiff: {}",
        divergences.len(),
        divergences.join("\n\n"),
        diff_path.display()
    );
}
