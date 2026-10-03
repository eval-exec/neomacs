use super::*;

#[test]
fn command_uses_matching_debug_target_and_serial_full_suite() {
    assert_eq!(BuildProfile::Test.target_subdir(), "debug");
    assert_eq!(
        TEST_ARGS,
        [
            "test",
            "--locked",
            "-p",
            "neomacs",
            "--test",
            "daemon_lifecycle"
        ]
    );
    assert_eq!(
        BOOTSTRAP_ARGS,
        ["--batch", "-l", "loadup", "--temacs=pbootstrap"]
    );
    assert!(usage_text().contains("cargo xtask test-daemon-lifecycle"));
}

#[test]
fn options_are_rejected_before_runtime_mutation() {
    let root = tempfile::tempdir().unwrap();
    let error = run(root.path().to_owned(), [OsString::from("--release")]).unwrap_err();
    assert!(error.to_string().contains("takes no arguments"));
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}
