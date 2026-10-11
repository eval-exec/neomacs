//! Language/coding preferences govern real terminal file opening and saving.
#![cfg(unix)]
use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory, TuiTerminalConfig};
use std::{fs, path::Path, time::Duration};

fn open_edit_save(binary: &Path, preference: &str, initial: &[u8], text: &str) {
    let directory = TuiTempDirectory::new("coding-detection-");
    let file = directory.path().join("coding-detection.txt");
    fs::write(&file, initial).unwrap();
    let launch = TuiLaunch::new(binary.as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("--eval")
        .arg(format!(
            "(progn {preference} (setq make-backup-files nil create-lockfiles nil))"
        ))
        .arg(&file)
        .env("HOME", directory.path());
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        "coding detection",
        TuiTerminalConfig::new("xterm-256color", 30, 100),
    );
    session.read_until(Duration::from_secs(20), |grid| {
        grid.iter().any(|row| row.contains("coding-detection.txt"))
    });
    assert!(
        session.text_grid().iter().any(|row| row.contains(text)),
        "incorrectly decoded text: {:?}",
        session.text_grid()
    );
    session.send_keys("M->");
    session.send(b"abc\r");
    session.read_until(Duration::from_secs(10), |grid| {
        grid.iter().any(|row| row.contains("abc"))
    });
    assert!(session.text_grid().iter().any(|row| row.contains("abc")));
    session.send_keys("C-x C-s");
    session.read_until(Duration::from_secs(10), |grid| {
        grid.iter().any(|row| row.contains("Wrote "))
    });
    assert!(session.text_grid().iter().any(|row| row.contains("Wrote ")));
    let mut expected = initial.to_vec();
    expected.extend_from_slice(b"abc\n");
    assert_eq!(fs::read(file).unwrap(), expected);
    session.send_keys("C-x C-c");
}

#[test]
fn preferred_gbk_and_windows_1252_files_match_gnu_in_terminal() {
    let gnu = std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "emacs".into());
    for binary in [gnu, neomacs_tui_tests::neomacs_binary()] {
        open_edit_save(
            &binary,
            "(set-language-environment 'Chinese-GB)",
            &[0x86, 0xaa, 0xe0, 0xc2, 0x0a],
            "啰嗦",
        );
        open_edit_save(
            &binary,
            "(prefer-coding-system 'windows-1252)",
            &[0x80, 0x20, 0x31, 0x30, 0x0a],
            "€ 10",
        );
    }
}
