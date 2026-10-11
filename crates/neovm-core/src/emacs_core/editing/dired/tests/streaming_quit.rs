use super::*;
use crate::emacs_core::fileio::builtin_directory_files;
#[derive(Clone, Copy)]
enum Operation {
    Names,
    Attributes,
    Completion,
    AllCompletions,
}

#[test]
fn directory_scans_poll_pending_quit_on_each_entry() {
    crate::test_utils::init_test_tracing();
    let root = crate::test_utils::workspace_root().join("tmp");
    std::fs::create_dir_all(&root).unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    let filename = Value::string(dir.path().to_str().unwrap());
    for operation in [
        Operation::Names,
        Operation::Attributes,
        Operation::Completion,
        Operation::AllCompletions,
    ] {
        let mut ctx = Context::new();
        ctx.quit_requested.request();
        let result = match operation {
            Operation::Names => builtin_directory_files(
                &mut ctx,
                vec![
                    filename,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::fixnum(1),
                ],
            ),
            Operation::Attributes => {
                builtin_directory_files_and_attributes(&mut ctx, vec![filename])
            }
            Operation::Completion => {
                builtin_file_name_completion(&mut ctx, vec![Value::string(""), filename])
            }
            Operation::AllCompletions => {
                builtin_file_name_all_completions(&mut ctx, vec![Value::string(""), filename])
            }
        };
        match result.unwrap_err().into_kind() {
            FlowKind::Signal(signal) => assert_eq!(signal.symbol_name(), "quit"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(!ctx.quit_requested.is_requested());
    }
}
