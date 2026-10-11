use super::*;
use crate::emacs_core::fileio::builtin_directory_files;

fn fixture() -> tempfile::TempDir {
    let root = crate::test_utils::workspace_root().join("tmp");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
#[derive(Clone, Copy)]
enum Operation {
    Names,
    Attributes,
    Completion,
    AllCompletions,
}

#[test]
fn directory_errors_use_separate_action_errno_and_filename_data() {
    crate::test_utils::init_test_tracing();
    let dir = fixture();
    let missing = dir.path().join("missing");
    let file = dir.path().join("file");
    std::fs::write(&file, b"").unwrap();
    let mut ctx = Context::new();
    for (path, symbol, message) in [
        (&missing, "file-missing", "No such file or directory"),
        (&file, "file-error", "Not a directory"),
    ] {
        let filename = Value::string(path.to_str().unwrap());
        for operation in [
            Operation::Names,
            Operation::Attributes,
            Operation::Completion,
            Operation::AllCompletions,
        ] {
            let error = match operation {
                Operation::Names => builtin_directory_files(&mut ctx, vec![filename]),
                Operation::Attributes => {
                    builtin_directory_files_and_attributes(&mut ctx, vec![filename])
                }
                Operation::Completion => {
                    builtin_file_name_completion(&mut ctx, vec![Value::string(""), filename])
                }
                Operation::AllCompletions => {
                    builtin_file_name_all_completions(&mut ctx, vec![Value::string(""), filename])
                }
            }
            .unwrap_err();
            match error.into_kind() {
                FlowKind::Signal(signal) => {
                    assert_eq!(signal.symbol_name(), symbol);
                    assert_eq!(signal.data.len(), 3);
                    assert_eq!(
                        signal.data[0].as_str_owned().as_deref(),
                        Some("Opening directory")
                    );
                    assert_eq!(signal.data[1].as_str_owned().as_deref(), Some(message));
                    assert_eq!(signal.data[2].as_str_owned().as_deref(), path.to_str());
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }
}
