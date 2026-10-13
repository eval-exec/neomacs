#![cfg(unix)]
use neomacs_melpa_test_support::{EmacsRuntime, GNU31_TREE_SITTER_GRAMMARS};
use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory, TuiTerminalConfig};
use std::time::Duration;

#[test]
fn go_grammar_loads_and_multibyte_mode_line_renders_with_debug_logging() {
    let gnu = EmacsRuntime::gnu_emacs();
    let grammar = GNU31_TREE_SITTER_GRAMMARS
        .iter()
        .find(|case| case.language == "go")
        .unwrap()
        .prepare(&gnu)
        .unwrap();
    let fixture = neomacs_infra::workspace_root()
        .join("crates/neomacs-gui-tests/fixtures/treesit-debug-mode-line.el");
    for (name, program) in [
        ("GNU", gnu.executable),
        ("Neomacs", neomacs_tui_tests::neomacs_binary()),
    ] {
        let directory = TuiTempDirectory::new("issue543-");
        let ready = directory.path().join("ready");
        let launch = TuiLaunch::new(program)
            .args(["-nw", "-Q", "-no-comp-spawn", "-l"])
            .arg(&fixture)
            .env("RUST_LOG", "debug")
            .env("NEOMACS_TEST_GRAMMAR_USER_DIR", grammar.parent().unwrap())
            .env("NEOMACS_TEST_TREESIT_READY", &ready);
        let mut session = TuiSession::spawn_launch_on_terminal(
            launch,
            name,
            TuiTerminalConfig::new("screen-256color", 30, 200),
        );
        session.read_until(Duration::from_secs(30), |grid| {
            ready.exists()
                && grid
                    .iter()
                    .any(|row| row.contains("Parsed go: source_file"))
                && grid.iter().any(|row| row.contains("…END"))
        });
        let grid = session.text_grid();
        assert!(ready.exists(), "{name}: {}", grid.join("\n"));
        assert!(
            grid.iter()
                .any(|row| row.contains("Parsed go: source_file")),
            "{name}: {grid:?}"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains(&format!("{}…END", "a".repeat(118)))),
            "{name}: {grid:?}"
        );
    }
}
