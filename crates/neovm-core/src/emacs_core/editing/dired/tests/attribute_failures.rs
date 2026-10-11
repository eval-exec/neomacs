use super::*;
#[test]
fn file_attributes_signals_errors_other_than_missing_metadata() {
    crate::test_utils::init_test_tracing();
    let root = crate::test_utils::workspace_root().join("tmp");
    std::fs::create_dir_all(&root).unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    std::os::unix::fs::symlink("loop", dir.path().join("loop")).unwrap();
    let path = dir.path().join("loop/entry");
    let mut ctx = Context::new();
    let error =
        builtin_file_attributes(&mut ctx, vec![Value::string(path.to_str().unwrap())]).unwrap_err();
    match error.into_kind() {
        FlowKind::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "file-error");
            assert_eq!(
                signal.data[0].as_str_owned().as_deref(),
                Some("Getting attributes")
            );
            assert_eq!(signal.data[2].as_str_owned().as_deref(), path.to_str());
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(
        builtin_file_attributes(
            &mut ctx,
            vec![Value::string(dir.path().join("missing").to_str().unwrap())]
        )
        .unwrap()
        .is_nil()
    );
}
#[test]
fn directory_attribute_permission_failure_is_signalled_before_count() {
    use std::os::unix::fs::PermissionsExt;
    crate::test_utils::init_test_tracing();
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let root = crate::test_utils::workspace_root().join("tmp");
    std::fs::create_dir_all(&root).unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    std::fs::write(dir.path().join("entry"), b"").unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o444)).unwrap();
    let mut ctx = Context::new();
    let result = builtin_directory_files_and_attributes(
        &mut ctx,
        vec![
            Value::string(dir.path().to_str().unwrap()),
            Value::NIL,
            Value::string("\\`entry\\'"),
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
        ],
    );
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    match result.unwrap_err().into_kind() {
        FlowKind::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "permission-denied");
            assert_eq!(
                signal.data[0].as_str_owned().as_deref(),
                Some("Getting attributes")
            );
            assert_eq!(signal.data[2].as_str_owned().as_deref(), Some("entry"));
        }
        other => panic!("unexpected {other:?}"),
    }
}
