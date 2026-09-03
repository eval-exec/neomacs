use super::*;

fn workspace_temp_dir() -> tempfile::TempDir {
    let parent = std::path::Path::new(env!("CARGO_WORKSPACE_DIR"))
        .join("target")
        .join("neovm-core-fileio-tests");
    std::fs::create_dir_all(&parent).expect("create workspace test directory");
    tempfile::Builder::new()
        .prefix("backup-policy-")
        .tempdir_in(parent)
        .expect("create backup policy fixture")
}

fn context_with_gnu_files() -> Context {
    let mut eval = Context::new();
    let lisp = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("lisp");
    eval.set_lexical_binding(true);
    eval.set_variable(
        "load-path",
        Value::list(crate::emacs_core::load::bootstrap_load_path_entries(&lisp)),
    );
    // This is GNU loadup.el's exact source order through files.el.  Loading
    // explicit source paths makes the test independent of ignored .elc files
    // in a developer checkout; runtime_startup_context is unsuitable because
    // it additionally requires every generated charset artifact.
    for relative in [
        "emacs-lisp/debug-early.el",
        "emacs-lisp/byte-run.el",
        "emacs-lisp/backquote.el",
        "subr.el",
        "keymap.el",
        "version.el",
        "widget.el",
        "custom.el",
        "emacs-lisp/map-ynp.el",
        "international/mule.el",
        "international/mule-conf.el",
        "env.el",
        "format.el",
        "bindings.el",
        "window.el",
    ] {
        crate::emacs_core::load::load_file(&mut eval, &lisp.join(relative))
            .unwrap_or_else(|error| panic!("load GNU {relative}: {error:?}"));
    }
    crate::emacs_core::load::load_file(&mut eval, &lisp.join("files.el"))
        .unwrap_or_else(|error| panic!("load GNU files.el: {error:?}"));
    eval
}

/// This is the public policy seam behind GNU's file-notify backup oracle:
/// with copy backups disabled, `backup-buffer` must rename the visited file
/// and return the metadata needed to finish the save.
#[test]
fn backup_buffer_uses_rename_when_copying_is_disabled() {
    crate::test_utils::init_test_tracing();
    let directory = workspace_temp_dir();
    let file = directory.path().join("visited.txt");
    std::fs::write(&file, "original").expect("create backup fixture");

    let mut eval = context_with_gnu_files();
    eval.set_variable(
        "neovm-test-backup-file",
        Value::string(crate::emacs_core::fileio::host_path_to_lisp_file_name_string(&file)),
    );
    let diagnostics = eval
        .eval_str(
            r#"
            (with-temp-buffer
              (let ((buffer-file-name neovm-test-backup-file)
                    (make-backup-files t)
                    (backup-by-copying nil)
                    (backup-by-copying-when-mismatch nil)
                    (kept-new-versions 1)
                    (delete-old-versions t))
                (insert "replacement")
                (let* ((attributes
                        (file-attributes buffer-file-name 'integer))
                       (decision (backup-buffer)))
                  (list
                   (file-writable-p
                    (file-name-directory buffer-file-name))
                   (file-modes buffer-file-name)
                   (file-attribute-link-number attributes)
                   (file-attribute-user-id attributes)
                   (file-attribute-group-id attributes)
                   (user-uid)
                   (group-gid)
                   (file-ownership-preserved-p
                    neovm-test-backup-file t)
                   decision))))
            "#,
        )
        .expect("run backup-buffer");
    let fields = crate::emacs_core::value::list_to_vec(&diagnostics)
        .expect("backup policy diagnostics list");
    let decision = fields.last().expect("backup decision field");

    assert!(
        decision.is_cons(),
        "backup-buffer copied instead of renaming the original; \
         diagnostics=(directory-writable modes links file-uid file-gid user-uid group-gid \
         ownership-preserved decision) {fields:?}"
    );
}
