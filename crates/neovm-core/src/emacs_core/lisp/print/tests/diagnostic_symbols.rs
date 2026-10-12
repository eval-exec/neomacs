//! JIT fingerprints retain GNU print.c:2470-2530 symbol spelling and escapes.
use super::super::*;
use std::path::PathBuf;

const SYMBOLS: &str = r#"(let* ((name (copy-sequence "before")) (live (make-symbol name)))
  (aset name 0 ?a)
  (list nil t live (make-symbol "") (make-symbol "a b")
        (make-symbol "123") (make-symbol "?x") (make-symbol ".")
        (make-symbol "a,b") (make-symbol "a\\b") (make-symbol "a\nb")
        (make-symbol "λ😀") (make-symbol "a b") (make-symbol ":keyword")))"#;

#[test]
fn diagnostic_symbol_string_matches_gnu_live_names_and_quoting() {
    crate::test_utils::init_test_tracing();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = root.join("../../tmp").join(format!(
        "diagnostic-symbol-oracle-{}.el",
        std::process::id()
    ));
    let program = format!(
        "(let ((print-gensym nil) (print-circle nil) (print-level nil) (print-length nil))\n  (dolist (symbol {SYMBOLS}) (prin1 (append (prin1-to-string symbol) nil)) (terpri)))\n"
    );
    std::fs::write(&script, program).expect("GNU diagnostic symbol probe");
    let emacs = std::env::var_os("NEOVM_FORCE_ORACLE_PATH").unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join(".local/bin/emacs")
            .into_os_string()
    });
    // Campaign nextest runs inside the PID/memory sandbox, including its GNU child.
    let mut command = if let Some(wrapper) = std::env::var_os("NEOMACS_PROBE_SANDBOX") {
        let mut command = std::process::Command::new(wrapper);
        command.arg(&emacs);
        command
    } else {
        std::process::Command::new(&emacs)
    };
    let output = command
        .args(["-Q", "--batch", "-l"])
        .arg(&script)
        .output()
        .expect("GNU diagnostic symbol oracle");
    assert!(output.status.success(), "GNU: {:?}", output.stderr);
    let expected = String::from_utf8(output.stdout).expect("GNU character-code rows");
    let mut context = crate::test_utils::runtime_startup_context();
    let symbols = context.eval_str(SYMBOLS).expect("diagnostic symbols");
    let mut actual = String::new();
    for value in list_to_vec(&symbols).expect("symbol list") {
        let spelling = diagnostic_symbol_string(value.as_bare_symbol_id().expect("bare symbol"));
        let codes = spelling
            .chars()
            .map(|character| u32::from(character).to_string())
            .collect::<Vec<_>>();
        if codes.is_empty() {
            actual.push_str("nil\n");
        } else {
            actual.push_str(&format!("({})\n", codes.join(" ")));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn diagnostic_symbol_string_preserves_general_renderer_domains_and_extent() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    let symbols = context
        .eval_str(
            "(list (make-symbol (unibyte-string 128 255 97))
                   (make-symbol (string-to-multibyte (unibyte-string 128 255 97)))
                   (make-symbol (make-string 2 #x110000 t))
                   (make-symbol (make-string 2 #x200000 t)))",
        )
        .expect("non-Unicode symbol names");
    let values = list_to_vec(&symbols).expect("symbol list");
    for value in [Value::NIL, Value::T, Value::UNBOUND]
        .into_iter()
        .chain(values)
    {
        let id = value.as_bare_symbol_id().expect("bare symbol");
        reset_print_number_index();
        next_print_number_index();
        let expected = print_value(&value);
        let expected_depth = PRINT_CALL_DEPTH.with(Cell::get);
        let expected_index = PRINT_NUMBER_INDEX.with(Cell::get);
        reset_print_number_index();
        next_print_number_index();
        assert_eq!(diagnostic_symbol_string(id), expected);
        assert_eq!(PRINT_CALL_DEPTH.with(Cell::get), expected_depth);
        assert_eq!(PRINT_NUMBER_INDEX.with(Cell::get), expected_index);
    }
}
