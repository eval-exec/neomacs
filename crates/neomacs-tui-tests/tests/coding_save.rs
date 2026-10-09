//! A physical C-x C-s preserves a Shift_JIS file's charset-only character.
#![cfg(unix)]
use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory, TuiTerminalConfig};
use std::{fs, path::PathBuf, time::Duration};

fn save_in_terminal(binary: PathBuf) -> Vec<u8> {
    let directory = TuiTempDirectory::new("shift-jis-save-");
    let file = directory.path().join("cp932.txt");
    fs::write(&file, [0x8a, 0xbf, 0x8e, 0x9a, 0x87, 0x40, 0x0a]).unwrap();
    let launch = TuiLaunch::new(binary.as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("--eval")
        .arg("(setq make-backup-files nil create-lockfiles nil)")
        .arg(&file)
        .env("HOME", directory.path());
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        "Shift_JIS save",
        TuiTerminalConfig::new("xterm-256color", 30, 100),
    );
    session.read_until(Duration::from_secs(20), |grid| {
        grid.iter().any(|row| row.contains("cp932.txt"))
    });
    assert!(
        session
            .text_grid()
            .iter()
            .any(|row| row.contains("cp932.txt")),
        "file did not open"
    );
    session.send_keys("M->");
    session.send(b"abc\r");
    session.read_until(Duration::from_secs(10), |grid| {
        grid.iter().any(|row| row.contains("abc"))
    });
    assert!(
        session.text_grid().iter().any(|row| row.contains("abc")),
        "edit did not appear"
    );
    session.send_keys("C-x C-s");
    session.read_until(Duration::from_secs(10), |grid| {
        grid.iter().any(|row| row.contains("Wrote "))
    });
    let bytes = fs::read(file).unwrap();
    session.send_keys("C-x C-c");
    bytes
}

#[test]
fn shift_jis_save_matches_gnu_after_keyboard_edit_and_save() {
    let gnu = std::env::var_os("NEOMACS_GNU_EMACS_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "emacs".into());
    let expected = vec![
        0x8a, 0xbf, 0x8e, 0x9a, 0x87, 0x40, 0x0a, 0x61, 0x62, 0x63, 0x0a,
    ];
    assert_eq!(save_in_terminal(gnu), expected);
    assert_eq!(
        save_in_terminal(neomacs_tui_tests::neomacs_binary()),
        expected
    );
}
