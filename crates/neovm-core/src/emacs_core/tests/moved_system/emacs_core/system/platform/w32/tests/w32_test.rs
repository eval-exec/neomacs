use neovm_system::emacs_core::w32::shell::rewrite_file_url_document;
#[cfg(windows)]
use neovm_system::emacs_core::w32::shell::w32_shell_execute;
use neovm_system::emacs_core::w32::*;
use neovm_value::symbol::Obarray;
use neovm_value::value::Value;

#[test]
fn windows_symbol_registration_matches_gnu_bootstrap_shape() {
    crate::test_utils::init_test_tracing();
    let mut obarray = Obarray::new();
    register_bootstrap_symbols(&mut obarray);

    assert_eq!(
        obarray.symbol_value_copied("w32-quote-process-args"),
        Some(Value::T)
    );
    assert_eq!(
        obarray.symbol_value_copied("w32-get-true-file-attributes"),
        Some(Value::symbol("local"))
    );
    assert_eq!(
        obarray.symbol_value_copied("x-toolkit-scroll-bars"),
        Some(Value::T)
    );
    assert_eq!(
        obarray.symbol_value_copied("tree-sitter--library-abi"),
        Some(Value::fixnum(tree_sitter::LANGUAGE_VERSION as i64))
    );
    assert!(obarray.is_special("w32-quote-process-args"));
    assert!(obarray.is_special("tree-sitter--library-abi"));
    assert!(obarray.intern_soft("w32-charset-shiftjis").is_some());
}

#[test]
fn undefined_color_has_gnu_error_properties() {
    crate::test_utils::init_test_tracing();
    let mut obarray = Obarray::new();
    register_bootstrap_symbols(&mut obarray);

    assert_eq!(
        obarray.get_property("undefined-color", "error-message"),
        Some(Value::string("Undefined color"))
    );
    assert_eq!(
        obarray.get_property("undefined-color", "error-conditions"),
        Some(Value::list(vec![
            Value::symbol("undefined-color"),
            Value::symbol("error"),
        ]))
    );
}

#[test]
fn shell_execute_rewrites_only_drive_letter_file_urls() {
    assert_eq!(
        rewrite_file_url_document("file:///C:/tmp/a.txt"),
        "C:/tmp/a.txt"
    );
    assert_eq!(
        rewrite_file_url_document("file:///c:\\tmp\\a.txt"),
        "c:\\tmp\\a.txt"
    );
    // No drive letter: a Unix path or a host name is not a local file URL.
    assert_eq!(
        rewrite_file_url_document("file:///tmp/a.txt"),
        "file:///tmp/a.txt"
    );
    assert_eq!(
        rewrite_file_url_document("file://server/share"),
        "file://server/share"
    );
    assert_eq!(
        rewrite_file_url_document("https://example.com/a"),
        "https://example.com/a"
    );
    // The separator after the colon is required.
    assert_eq!(rewrite_file_url_document("file:///C:"), "file:///C:");
}

#[cfg(windows)]
#[test]
fn shell_execute_requires_a_string_document() {
    use crate::emacs_core::error::FlowKind;

    let mut eval = crate::emacs_core::eval_hook_installers::new_context();
    // (OPERATION DOCUMENT ...): DOCUMENT is the second argument and is the
    // only one GNU type-checks.
    let error = w32_shell_execute(&mut eval, vec![Value::string("open"), Value::fixnum(1)])
        .expect_err("a non-string document must signal");
    match error.into_kind() {
        FlowKind::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "wrong-type-argument");
            assert_eq!(signal.data[0], Value::symbol("stringp"));
        }
        other => panic!("expected Flow::Signal, got {other:?}"),
    }
}

#[cfg(windows)]
#[test]
fn shell_execute_missing_local_file_signals_gnu_error() {
    use crate::emacs_core::error::FlowKind;

    let missing = std::env::temp_dir().join(format!(
        "neomacs-w32-shell-execute-missing-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&missing);

    let mut eval = crate::emacs_core::eval_hook_installers::new_context();
    let error = w32_shell_execute(
        &mut eval,
        vec![
            Value::string("open"),
            Value::string(missing.to_string_lossy().to_string()),
        ],
    )
    .expect_err("a missing document must signal");
    match error.into_kind() {
        FlowKind::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "error");
            let message = signal.data[0]
                .as_utf8_str()
                .expect("error message is a string");
            assert!(
                message.starts_with("ShellExecute failed: "),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected Flow::Signal, got {other:?}"),
    }
}

#[cfg(windows)]
#[test]
fn shell_execute_reads_operation_from_the_first_argument() {
    use crate::emacs_core::error::FlowKind;

    let missing = std::env::temp_dir().join(format!(
        "neomacs-w32-shell-execute-op-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&missing);

    let mut eval = crate::emacs_core::eval_hook_installers::new_context();
    // OPERATION is not type-checked; only DOCUMENT is. A non-string first
    // argument must therefore reach ShellExecute instead of signalling
    // wrong-type-argument, proving the first slot is read as OPERATION.
    let error = w32_shell_execute(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::string(missing.to_string_lossy().to_string()),
        ],
    )
    .expect_err("a missing document must signal");
    match error.into_kind() {
        FlowKind::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "error");
            let message = signal.data[0]
                .as_utf8_str()
                .expect("error message is a string");
            assert!(
                message.starts_with("ShellExecute failed: "),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected Flow::Signal, got {other:?}"),
    }
}

#[cfg(not(windows))]
#[test]
fn non_windows_context_does_not_leak_w32_surface() {
    let eval = crate::emacs_core::eval_hook_installers::new_context();
    let features = eval
        .obarray()
        .symbol_value_copied("features")
        .expect("features should be bound");
    let feature_list =
        neovm_value::value::list_to_vec(&features).expect("features should be a list");

    assert!(
        eval.obarray()
            .intern_soft("w32-quote-process-args")
            .is_none()
    );
    assert!(eval.obarray().intern_soft("w32-shell-execute").is_none());
    assert!(!feature_list.contains(&Value::symbol("w32")));
}
