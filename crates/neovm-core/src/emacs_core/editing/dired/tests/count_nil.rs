use super::*;
use crate::emacs_core::fileio::builtin_directory_files;

fn fixture() -> tempfile::TempDir {
    let root = crate::test_utils::workspace_root().join("tmp");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
#[test]
fn directory_files_accepts_nil_count_and_forwarded_optional_count() {
    crate::test_utils::init_test_tracing();
    let dir = fixture();
    std::fs::write(dir.path().join("entry"), b"").unwrap();
    let path = Value::string(dir.path().to_str().unwrap());
    let mut ctx = Context::new();
    let omitted = builtin_directory_files(&mut ctx, vec![path]).unwrap();
    let explicit = builtin_directory_files(
        &mut ctx,
        vec![path, Value::NIL, Value::NIL, Value::NIL, Value::NIL],
    )
    .unwrap();
    assert_eq!(
        crate::emacs_core::format_eval_result(&Ok(explicit)),
        crate::emacs_core::format_eval_result(&Ok(omitted))
    );
    let invalid = builtin_directory_files(
        &mut ctx,
        vec![
            path,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("invalid-count"),
        ],
    )
    .unwrap_err();
    match invalid.into_kind() {
        FlowKind::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "wrong-type-argument");
            assert_eq!(signal.data[0].as_symbol_name(), Some("wholenump"));
        }
        other => panic!("unexpected {other:?}"),
    }
}
