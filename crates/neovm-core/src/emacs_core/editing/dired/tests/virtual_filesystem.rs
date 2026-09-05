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
fn virtual_attributes_support_gnu_ls_lisp_numeric_columns() {
    let mut eval = virtual_editor();
    assert_eq!(
        eval.eval_str(r##"(let ((a (file-attributes "/virtual-completion/alpha.el")))
          (format "%d %d %d" (nth 1 a) (nth 2 a) (nth 3 a)))"##)
            .unwrap().as_utf8_str(),
        Some("1 0 0")
    );
}

#[test]
fn file_attributes_describe_virtual_files_without_host_metadata() {
    let mut eval = virtual_editor();
    assert_eq!(
        eval.eval_str(
            r##"(let ((a (file-attributes "/virtual-completion/alpha.el")))
          (list (length a) (car a) (nth 7 a)
                (car (file-attributes "/virtual-completion/alpha-dir"))
                (file-attributes "/virtual-completion/missing")))"##
        )
        .unwrap(),
        Value::list(vec![
            Value::fixnum(12),
            Value::NIL,
            Value::fixnum(3),
            Value::T,
            Value::NIL
        ])
    );
    assert_eq!(
        eval.eval_str(
            r##"(let ((a (file-attributes "/virtual-completion/alpha.el" 'string)))
          (list (nth 1 a) (nth 2 a) (nth 3 a) (nth 4 a)
                (consp (nth 5 a)) (nth 6 a) (nth 8 a) (nth 10 a) (nth 11 a)))"##
        )
        .unwrap(),
        Value::list(vec![
            Value::fixnum(1),
            Value::string("virtual"),
            Value::string("virtual"),
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::string("-?????????"),
            Value::NIL,
            Value::NIL
        ])
    );
    assert_eq!(
        eval.eval_str(r##"(equal
          (cdr (assoc "alpha.el" (directory-files-and-attributes "/virtual-completion" nil "^alpha")))
          (file-attributes "/virtual-completion/alpha.el"))"##).unwrap(),
        Value::T
    );
}

#[test]
fn file_attributes_include_mount_ancestors_and_mounted_entries() {
    use crate::emacs_core::fileio::MountTableFileSystem;
    let mut mounts = MountTableFileSystem::new();
    mounts
        .mount(
            Path::new("/virtual-mount/home"),
            Box::new(MemoryFileSystem::new()),
        )
        .unwrap();
    let mut eval = Context::new();
    eval.install_editor_file_system(Box::new(mounts));
    assert_eq!(
        eval.eval_str(
            r##"(list (car (file-attributes "/"))
          (car (file-attributes "/virtual-mount"))
          (car (file-attributes "/virtual-mount/home")))"##
        )
        .unwrap(),
        Value::list(vec![Value::T, Value::T, Value::T])
    );
}

#[cfg(unix)]
#[test]
fn mounted_native_attributes_preserve_links_permissions_and_identity() {
    use crate::emacs_core::fileio::{MountTableFileSystem, NativeFileSystem};
    use std::os::unix::fs::PermissionsExt;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).unwrap();
    let fixture = tempfile::Builder::new()
        .prefix("native-attributes-")
        .tempdir_in(root)
        .unwrap();
    let path = fixture.path().canonicalize().unwrap().join("file");
    std::fs::write(&path, b"hello").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    std::fs::hard_link(&path, path.with_file_name("alias")).unwrap();
    let mut mounts = MountTableFileSystem::new();
    mounts
        .mount(Path::new("/host"), Box::new(NativeFileSystem))
        .unwrap();
    let mut eval = Context::new();
    eval.install_editor_file_system(Box::new(mounts));
    let filename = format!("/host{}", path.display());
    assert_eq!(
        eval.eval_str(&format!(
            r##"(let ((a (file-attributes {filename:?})))
      (list (nth 1 a) (nth 7 a) (nth 8 a) (integerp (nth 2 a))
        (> (nth 10 a) 0) (integerp (nth 11 a))))"##
        ))
        .unwrap(),
        Value::list(vec![
            Value::fixnum(2),
            Value::fixnum(5),
            Value::string("-rw-r-----"),
            Value::T,
            Value::T,
            Value::T
        ])
    );
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
