use super::*;
use crate::emacs_core::fileio::builtin_directory_files;

fn fixture() -> tempfile::TempDir {
    let root = crate::test_utils::workspace_root().join("tmp");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
#[test]
fn zero_count_opens_the_directory_and_checks_matching_entries() {
    crate::test_utils::init_test_tracing();
    let dir = fixture();
    let missing = dir.path().join("missing");
    let file = dir.path().join("file");
    std::fs::write(&file, b"").unwrap();
    let mut ctx = Context::new();
    for (path, symbol) in [(&missing, "file-missing"), (&file, "file-error")] {
        for attributes in [false, true] {
            let mut args = vec![
                Value::string(path.to_str().unwrap()),
                Value::NIL,
                Value::NIL,
                Value::NIL,
            ];
            if attributes {
                args.push(Value::NIL);
            }
            args.push(Value::fixnum(0));
            let result = if attributes {
                builtin_directory_files_and_attributes(&mut ctx, args)
            } else {
                builtin_directory_files(&mut ctx, args)
            };
            match result.unwrap_err().into_kind() {
                FlowKind::Signal(signal) => assert_eq!(signal.symbol_name(), symbol),
                other => panic!("unexpected {other:?}"),
            }
        }
    }
    let path = Value::string(dir.path().to_str().unwrap());
    assert!(
        builtin_directory_files(
            &mut ctx,
            vec![path, Value::NIL, Value::NIL, Value::NIL, Value::fixnum(0)]
        )
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_directory_files(
            &mut ctx,
            vec![
                path,
                Value::NIL,
                Value::string("["),
                Value::NIL,
                Value::fixnum(0)
            ]
        )
        .is_err()
    );
}
