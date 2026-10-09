//! Terminal callers retain the full Emacs character validation contract.
#![cfg(unix)]
use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory, TuiTerminalConfig};
use std::{path::PathBuf, time::Duration};

#[test]
fn font_coverage_validates_non_unicode_characters_before_frame_in_terminal() {
    let gnu = std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "emacs".into());
    for binary in [gnu, neomacs_tui_tests::neomacs_binary()] {
        let home = TuiTempDirectory::new("font-coverage-");
        let launch = TuiLaunch::new(binary.as_os_str())
            .arg("-nw")
            .arg("-Q")
            .arg("--eval")
            .arg(
                r#"(progn
              (switch-to-buffer (get-buffer-create "font-coverage"))
              (erase-buffer)
              (insert (format "FONT-COVERAGE: %S"
                (mapcar (lambda (ch)
                          (condition-case err (font-has-char-p (font-spec) ch 1)
                            (wrong-type-argument (cdr err))))
                        '(#xd800 #x110000 #x3fffff))))
              (set-buffer-modified-p nil))"#,
            )
            .env("HOME", home.path());
        let mut session = TuiSession::spawn_launch_on_terminal(
            launch,
            "font coverage",
            TuiTerminalConfig::new("xterm-256color", 30, 100),
        );
        let expected = "FONT-COVERAGE: ((framep 1) (framep 1) (framep 1))";
        session.read_until(Duration::from_secs(20), |grid| {
            grid.iter().any(|row| row.contains(expected))
        });
        assert!(
            session.text_grid().iter().any(|row| row.contains(expected)),
            "{}: {:?}",
            binary.display(),
            session.text_grid()
        );
        session.send_keys("C-x C-c");
    }
}
