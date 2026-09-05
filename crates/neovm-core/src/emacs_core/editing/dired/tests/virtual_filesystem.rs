use crate::emacs_core::eval::Context;
use crate::emacs_core::fileio::{EditorFileSystem, MemoryFileSystem, TemporaryEntry};
use crate::emacs_core::value::{Value, list_to_vec};
use std::path::Path;

fn virtual_editor() -> Context {
    let filesystem = MemoryFileSystem::new();
    filesystem
        .create_directory(Path::new("/virtual-completion/alpha-dir"), true)
        .unwrap();
    for name in ["alpha.el", "alpha.elc", "beta.el"] {
        filesystem
            .create_temporary(
                &Path::new("/virtual-completion").join(name),
                TemporaryEntry::File(b"nil"),
            )
            .unwrap();
    }
    let mut eval = Context::new();
    eval.install_editor_file_system(Box::new(filesystem));
    eval
}

fn names(value: Value) -> Vec<String> {
    let mut names: Vec<_> = list_to_vec(&value)
        .unwrap()
        .iter()
        .map(|value| value.as_utf8_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn filename_completion_uses_the_same_virtual_directory_as_directory_files() {
    let mut eval = virtual_editor();
    assert_eq!(
        names(
            eval.eval_str(r#"(directory-files "/virtual-completion" nil "^alpha")"#)
                .unwrap()
        ),
        ["alpha-dir", "alpha.el", "alpha.elc"]
    );
    assert_eq!(
        names(
            eval.eval_str(r#"(file-name-all-completions "alpha" "/virtual-completion")"#)
                .unwrap()
        ),
        ["alpha-dir/", "alpha.el", "alpha.elc"]
    );
    assert_eq!(
        eval.eval_str(r#"(file-name-completion "alpha-d" "/virtual-completion")"#)
            .unwrap()
            .as_utf8_str(),
        Some("alpha-dir/")
    );
    assert_eq!(
        eval.eval_str(
            r#"(let ((completion-ignored-extensions '(".elc")))
        (file-name-completion "alpha.e" "/virtual-completion"))"#
        )
        .unwrap()
        .as_utf8_str(),
        Some("alpha.el")
    );
    assert_eq!(
        eval.eval_str(r#"(file-name-completion "alpha" "/virtual-completion" #'file-directory-p)"#)
            .unwrap()
            .as_utf8_str(),
        Some("alpha-dir/")
    );
    assert_eq!(
        names(
            eval.eval_str(
                r#"(let ((completion-regexp-list '("el$")))
        (file-name-all-completions "alpha" "/virtual-completion"))"#
            )
            .unwrap()
        ),
        ["alpha.el"]
    );
    assert_eq!(
        names(
            eval.eval_str(
                r#"(let ((default-directory "/virtual-completion/")
                                         (completion-ignored-extensions '(".elc")))
        (file-name-all-completions "alpha.e" "."))"#
            )
            .unwrap()
        ),
        ["alpha.el", "alpha.elc"]
    );
}

#[test]
fn virtual_completion_does_not_silently_hide_directory_errors() {
    let mut eval = virtual_editor();
    for directory in [
        "/virtual-completion/missing",
        "/virtual-completion/alpha.el",
    ] {
        let result = eval
            .eval_str(&format!(
                r#"(condition-case nil
            (file-name-all-completions "" "{directory}") (file-error 'directory-error))"#
            ))
            .unwrap();
        assert_eq!(result, Value::symbol("directory-error"));
    }
}
