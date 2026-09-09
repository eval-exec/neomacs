use neomacs_android::environment::AndroidSessionEnvironment;
use neovm_core::emacs_core::{Context, Value};

#[test]
fn android_session_paths_are_visible_to_lisp_without_mutating_host_environment() {
    let host_home = std::env::var_os("HOME");
    let mut context = Context::new();
    let environment = AndroidSessionEnvironment::new(
        "/data/user/0/org.neomacs/files",
        "/data/user/0/org.neomacs/cache",
    )
    .unwrap();
    environment.install(&mut context);
    assert_eq!(
        context
            .eval_str(
                r##"(list (getenv-internal "HOME")
                                  (expand-file-name "~/.emacs.d/")
                                  (getenv-internal "TMPDIR")
                                  temporary-file-directory
                                  shell-file-name)"##
            )
            .unwrap(),
        Value::list(vec![
            Value::string("/data/user/0/org.neomacs/files"),
            Value::string("/data/user/0/org.neomacs/files/.emacs.d/"),
            Value::string("/data/user/0/org.neomacs/cache"),
            Value::string("/data/user/0/org.neomacs/cache/"),
            Value::string("/system/bin/sh"),
        ]),
    );
    assert_eq!(std::env::var_os("HOME"), host_home);
}

#[test]
fn android_environment_preserves_inherited_entries_and_initial_snapshot() {
    let mut context = Context::new();
    for variable in ["initial-environment", "process-environment"] {
        context.set_variable(
            variable,
            Value::list(vec![
                Value::string("HOME=/dump-host"),
                Value::string("PATH=/system/bin"),
            ]),
        );
    }
    AndroidSessionEnvironment::new("/app/files", "/app/cache")
        .unwrap()
        .install(&mut context);
    assert_eq!(
        context.eval_str(r##"(getenv-internal "PATH")"##).unwrap(),
        Value::string("/system/bin")
    );
    context
        .eval_str(r##"(setcar process-environment "HOME=/user-choice")"##)
        .unwrap();
    assert_eq!(
        context.eval_str(r##"(getenv-internal "HOME")"##).unwrap(),
        Value::string("/user-choice")
    );
    assert_eq!(
        context.eval_str("(car initial-environment)").unwrap(),
        Value::string("HOME=/app/files")
    );
}

#[test]
fn android_session_rejects_relative_or_nul_paths_before_startup() {
    assert!(AndroidSessionEnvironment::new("files", "/app/cache").is_err());
    assert!(AndroidSessionEnvironment::new("/app/files", "cache").is_err());
    assert!(AndroidSessionEnvironment::new("/app/\0files", "/app/cache").is_err());
}
