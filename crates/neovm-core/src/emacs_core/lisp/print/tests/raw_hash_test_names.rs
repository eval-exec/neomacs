//! Hash-test names use the same byte-safe symbol protocol as all printed symbols.
use super::super::*;
use crate::emacs_core::intern::{
    intern_lisp_string, intern_uninterned, intern_uninterned_lisp_string,
};
use crate::heap_types::LispString;

fn character_codes(bytes: Vec<u8>) -> Value {
    let mut codes = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let (code, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
        codes.push(Value::fixnum(i64::from(code.code())));
        pos += len;
    }
    Value::list(codes)
}

#[test]
fn custom_hash_test_names_preserve_raw_bytes_escaping_and_symbol_identity() {
    crate::test_utils::init_test_tracing();
    let expected = gnu_expectation();
    let ctx = crate::test_utils::runtime_startup_context();
    let name = LispString::from_emacs_bytes(vec![0xc0, 0x80]);
    let ids = [
        intern_lisp_string(&name),
        intern_uninterned_lisp_string(&name),
        intern_uninterned("a b"),
        intern_uninterned("eql"),
    ];
    let actual = Value::list(
        ids.into_iter()
            .map(|id| {
                // The Lisp define-hash-table-test boundary still rejects raw names;
                // model GNU's resulting heap object directly to isolate this printer.
                let table = Value::hash_table(HashTableTest::Eq);
                table
                    .with_hash_table_mut(|table| table.test_name = Some(id))
                    .unwrap();
                let options = PrintOptions::default();
                let noescape = PrintOptions {
                    print_noescape: true,
                    print_escape_nonascii: true,
                    ..options
                };
                let escaped = print_value_bytes_with_options(&table, options);
                let escaped_stateful =
                    print_value_stateful_bytes_with_buffers(&table, None, options);
                assert_eq!(escaped, escaped_stateful, "Both printer entrypoints");
                Value::list(vec![
                    Value::T,
                    character_codes(escaped),
                    character_codes(print_value_bytes_with_options(&table, noescape)),
                    character_codes(
                        crate::emacs_core::error::print_value_bytes_in_state_with_options(
                            &ctx, &table, noescape,
                        ),
                    ),
                ])
            })
            .collect(),
    );
    assert_eq!(print_value(&actual), expected.trim_end());
}

fn gnu_expectation() -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("src/emacs_core/lisp/print/tests/raw_hash_test_names.expect");
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let script = root
            .join("../../tmp")
            .join(format!("raw-hash-test-names-{}.el", std::process::id()));
        let form = r#"(mapcar (lambda (name) (define-hash-table-test name 'eq 'sxhash-eq) (let ((h (make-hash-table :test name))) (list (eq (hash-table-test h) name) (string-to-list (prin1-to-string h)) (string-to-list (prin1-to-string h t)) (string-to-list (format "%s" h))))) (list (intern (string-to-multibyte "\x80")) (make-symbol (string-to-multibyte "\x80")) (make-symbol "a b") (make-symbol "eql")))"#;
        std::fs::write(
            &script,
            format!(";;; -*- lexical-binding: nil; -*-\n(prin1 {form})\n"),
        )
        .unwrap();
        // Campaign nextest runs already enclose this child in a PID sandbox.
        let mut command = crate::test_utils::gnu_probe_command();
        let result = command
            .args(["-Q", "--batch", "-l"])
            .arg(&script)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "GNU oracle: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let expected = String::from_utf8(result.stdout).unwrap();
        std::fs::write(&fixture, &expected).unwrap();
        expected
    } else {
        std::fs::read_to_string(&fixture).expect("refresh from GNU with UPDATE_EXPECT=1")
    }
}
