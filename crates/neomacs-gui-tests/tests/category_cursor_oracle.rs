//! Category-symbol face change followed by a cursor move, on X11.
//!
//! GNU keeps the current glyph row: `Fput` sets no redisplay flag
//! (fns.c:2729-2737) and `try_cursor_movement` does not rebuild rows
//! (xdisp.c:19867-19902). `window-line-height` reads that matrix
//! (window.c:2082-2089).

#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{fs, path::PathBuf, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CategoryRow {
    attach: i64,
    after_put: i64,
    after_move: i64,
    after_force: i64,
}

#[test]
fn n832_category_face_put_then_cursor_move_keeps_the_row() {
    let backend = GuiBackend::LinuxX11;
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("n832-category-cursor");
    let _ = fs::remove_dir_all(&artifacts);
    fs::create_dir_all(&artifacts).unwrap();
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let session = DisplayHarness::for_backend(backend)
        .start_session(&artifacts)
        .unwrap();
    let gnu = observe(
        &root,
        &artifacts.join("gnu"),
        oracle.executable_path(),
        &session.env(),
    );
    let actual = observe(&root, &artifacts.join("neo"), neo, &session.env());
    assert_eq!(
        gnu.after_move, gnu.attach,
        "GNU rebuilt the category row on a cursor move: {gnu:?}"
    );
    assert_eq!(
        actual.after_move, actual.attach,
        "cursor move re-laid the category row: neo {actual:?} gnu {gnu:?}"
    );
}

fn observe(
    root: &std::path::Path,
    artifacts: &std::path::Path,
    binary: PathBuf,
    session_env: &[(String, String)],
) -> CategoryRow {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/n832-category-cursor-oracle.el");
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        root,
        artifacts,
        GuiScenario::new("n832-category-cursor", &fixture),
    )
    .without_paint_proof()
    .with_program(binary)
    .with_args([
        "-Q".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env("GSETTINGS_BACKEND", "memory")
    .with_env("GDK_BACKEND", "x11");
    for (key, value) in session_env {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(25)),
        )
        .unwrap();
    assert!(!result.timed_out, "{result:#?}");
    assert_eq!(result.exit_code, Some(0), "{result:#?}");
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap();
    parse_category_row(&stderr)
}

fn parse_category_row(stderr: &str) -> CategoryRow {
    let line = stderr
        .lines()
        .find(|line| line.contains("CATEGORY-ROW"))
        .unwrap_or_else(|| panic!("missing CATEGORY-ROW line: {stderr}"));
    CategoryRow {
        attach: first_height(line, "attach"),
        after_put: first_height(line, "put"),
        after_move: first_height(line, "move"),
        after_force: first_height(line, "force"),
    }
}

fn first_height(line: &str, key: &str) -> i64 {
    let rest = line
        .split_once(&format!("{key}="))
        .unwrap_or_else(|| panic!("missing {key} in {line}"))
        .1;
    let token = rest.split_whitespace().next().unwrap_or("");
    let digits: String = token
        .trim_start_matches('(')
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("no height in {key}={token} from {line}"))
}
