use crate::emacs_core::eval::Context;
use crate::emacs_core::fileio::{EditorFileSystem, MemoryFileSystem, TemporaryEntry};
use std::path::Path;

#[test]
fn integer_predicates_use_virtual_storage_and_require_existing_files() {
    let filesystem = MemoryFileSystem::new();
    filesystem
        .create_directory(Path::new("/virtual-lookup"), true)
        .unwrap();
    filesystem
        .create_temporary(
            Path::new("/virtual-lookup/file"),
            TemporaryEntry::File(b"data"),
        )
        .unwrap();
    let mut eval = Context::new();
    eval.install_editor_file_system(Box::new(filesystem));
    eval.eval_str(
        r#"(progn
      (fset 'deny-magic-access (lambda (&rest _) nil))
      (put 'deny-magic-access 'operations '(file-exists-p file-readable-p file-directory-p))
      (setq file-name-handler-alist '(("/virtual-lookup" . deny-magic-access))))"#,
    )
    .unwrap();
    // Integer predicates must ignore the filename handler above.
    // Memory storage permits reading/writing regular files, but not executing.
    for (mask, expected) in [
        (0, true),
        (1, false),
        (2, true),
        (3, false),
        (4, true),
        (5, false),
        (6, true),
        (7, false),
        (8, false),
        (256, false),
        (2147483648i64, false),
    ] {
        let found = eval
            .eval_str(&format!(
                r#"(locate-file-internal "file" '("/virtual-lookup") nil {mask})"#
            ))
            .unwrap();
        assert_eq!(
            found.as_utf8_str(),
            expected.then_some("/virtual-lookup/file"),
            "mask {mask}"
        );
        for name in ["/virtual-lookup/missing", "/virtual-lookup"] {
            assert!(
                eval.eval_str(&format!(
                    r#"(locate-file-internal "{name}" nil nil {mask})"#
                ))
                .unwrap()
                .is_nil(),
                "mask {mask} must reject {name}"
            );
        }
    }
    let result = eval
        .eval_str(
            r#"(condition-case err (locate-file-internal "/virtual-lookup/file" nil nil -1)
             (invalid-function (car err)))"#,
        )
        .unwrap();
    assert_eq!(
        result,
        crate::emacs_core::value::Value::symbol("invalid-function")
    );
}
