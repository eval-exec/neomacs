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

/// GNU's Windows implementation treats an existing directory as writable,
/// including the trailing-separator form produced by `file-name-directory`,
/// even though opening that directory as a regular file for content writes is
/// invalid (`src/fileio.c:Ffile_writable_p`).  `backup-buffer` relies on this
/// contract to select rename-based backups.
#[test]
fn windows_file_writable_p_accepts_existing_directory_forms() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let mut eval = Context::new();

    for directory_name in [
        directory.path().display().to_string(),
        format!("{}/", directory.path().display()),
    ] {
        let result = builtin_file_writable_p(&mut eval, vec![Value::string(directory_name)])
            .expect("query directory writability");
        assert_eq!(result, Value::T);
    }
}

/// CopyFileW preserves the source modification time.  GNU explicitly resets
/// both destination timestamps to now when copy-file's KEEP-TIME argument is
/// nil (`src/w32.c:w32_copy_file`), so the default copy must not retain an old
/// source timestamp.
#[test]
fn windows_copy_file_without_keep_time_refreshes_timestamps() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let source = directory.path().join("source.txt");
    let destination = directory.path().join("destination.txt");
    std::fs::write(&source, "contents").expect("create copy source");

    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(86_400);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .expect("open copy source")
        .set_times(
            std::fs::FileTimes::new()
                .set_accessed(old)
                .set_modified(old),
        )
        .expect("age copy source");

    let mut eval = Context::new();
    builtin_copy_file(
        &mut eval,
        vec![
            Value::string(source.display().to_string()),
            Value::string(destination.display().to_string()),
        ],
    )
    .expect("copy file without KEEP-TIME");

    let destination_modified = std::fs::metadata(&destination)
        .expect("stat copied file")
        .modified()
        .expect("copied file modification time");
    assert!(
        destination_modified > old + std::time::Duration::from_secs(86_400),
        "copy-file retained the source timestamp despite nil KEEP-TIME: \
         source={old:?}, destination={destination_modified:?}"
    );
}
