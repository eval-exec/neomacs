use super::*;

fn workspace_temp_dir() -> tempfile::TempDir {
    let parent = std::path::Path::new(env!("CARGO_WORKSPACE_DIR"))
        .join("target")
        .join("neovm-core-fileio-tests");
    std::fs::create_dir_all(&parent).expect("create workspace test directory");
    tempfile::Builder::new()
        .prefix("windows-fileio-")
        .tempdir_in(parent)
        .expect("create Windows fileio fixture")
}

/// GNU opens the target with FILE_WRITE_ATTRIBUTES rather than generic write
/// access (src/w32.c:5998-6034), so the Windows read-only attribute does not
/// prevent `set-file-times' from updating timestamps.
#[test]
fn windows_set_file_times_does_not_require_content_write_access() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let file = directory.path().join("read-only.txt");
    std::fs::write(&file, "contents").expect("create file-time fixture");

    let mut eval = Context::new();
    let file_name = Value::string(file.display().to_string());
    builtin_set_file_modes(
        &mut eval,
        vec![file_name, Value::fixnum(0), Value::symbol("nofollow")],
    )
    .expect("make fixture read-only");

    let result = builtin_set_file_times(
        &mut eval,
        vec![file_name, Value::fixnum(0), Value::symbol("nofollow")],
    );

    // Restore writability before asserting so a red test still cleans up on
    // Windows, where deleting a read-only file can otherwise fail.
    builtin_set_file_modes(
        &mut eval,
        vec![file_name, Value::fixnum(0o600), Value::symbol("nofollow")],
    )
    .expect("restore fixture writability");
    assert_eq!(result.expect("set times on read-only file"), Value::T);
}
