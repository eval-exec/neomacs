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

/// This is the public policy seam behind GNU's file-notify backup oracle:
/// with copy backups disabled, `backup-buffer` must rename the visited file
/// and return the metadata needed to finish the save.
#[test]
fn windows_backup_buffer_uses_rename_when_copying_is_disabled() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let file = directory.path().join("visited.txt");
    std::fs::write(&file, "original").expect("create backup fixture");

    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_variable(
        "neovm-test-backup-file",
        Value::string(crate::emacs_core::fileio::host_path_to_lisp_file_name_string(&file)),
    );
    let result = eval
        .eval_str(
            r#"
            (progn
              (require 'files)
              (with-temp-buffer
                (let ((buffer-file-name neovm-test-backup-file)
                      (make-backup-files t)
                      (backup-by-copying nil)
                      (backup-by-copying-when-mismatch nil)
                      (kept-new-versions 1)
                      (delete-old-versions t))
                  (insert "replacement")
                  (backup-buffer))))
            "#,
        )
        .expect("run backup-buffer");

    assert!(
        result.is_cons(),
        "backup-buffer copied instead of renaming the original: {result:?}"
    );
}
