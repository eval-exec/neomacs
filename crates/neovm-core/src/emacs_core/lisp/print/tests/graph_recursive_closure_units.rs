//! Same-traversal closure slots: GNU print.c:2285-2300, 2611-2613,
//! 2781-2801. Raw escaped strings: print.c:2375-2474.
//! Refresh the saved GNU fixtures with UPDATE_EXPECT=1. Campaign runs wrap
//! the driver with sandbox-run.sh; NEOMACS_PROBE_SANDBOX may wrap refresh too.
use super::super::*;
use crate::emacs_core::value::{LambdaData, LambdaParams};
use std::path::PathBuf;

const CYCLE_FORM: &str = r###"(let* ((binding (cons 'f nil))
       (environment (list binding))
       (closure (make-interpreted-closure nil '(f) environment))
       (print-circle nil))
  (setcdr binding closure)
  (list (prin1-to-string closure)
        (prin1-to-string (vector closure))
        (prin1-to-string (list closure))
        (let ((print-circle t)) (prin1-to-string closure))))
"###;
const RAW_SOURCES: [&str; 5] = [
    "(string 233)",
    "(unibyte-string 255)",
    "(string-as-multibyte (unibyte-string 255))",
    "(string #x110000)",
    "(string #x200000)",
];

fn gnu_output(name: &str, program: &str) -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = root.join(format!("../../tmp/{name}-{}.el", std::process::id()));
    std::fs::write(
        &script,
        format!(";; -*- lexical-binding: t; -*-\n{program}\n"),
    )
    .expect("GNU script");
    let emacs = crate::test_utils::gnu_oracle().executable_path();
    let mut command = match std::env::var_os("NEOMACS_PROBE_SANDBOX") {
        Some(sandbox) => {
            let mut command = std::process::Command::new(sandbox);
            command.arg(emacs);
            command
        }
        None => std::process::Command::new(emacs),
    };
    let output = command
        .args(["-Q", "--batch", "-l"])
        .arg(script)
        .output()
        .expect("GNU oracle");
    assert!(
        output.status.success(),
        "GNU: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn write_fixture(name: &str, expected: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::fs::write(
        root.join(format!("src/emacs_core/lisp/print/tests/{name}.expect")),
        expected,
    )
    .expect("refresh GNU fixture");
}

fn gnu_fixture(name: &str, form: &str, saved: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return saved.to_owned();
    }
    let expected = String::from_utf8(gnu_output(name, &format!("(prin1 {form})")))
        .expect("ASCII/Unicode fixture");
    write_fixture(name, &expected);
    expected
}

fn gnu_stdout_fixture() -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return include_str!("graph_closure_stdout_slots.expect").to_owned();
    }
    // Capture exact GNU stdout bytes, without decoding Unicode or re-encoding
    // function-stream character codes. Source: print.c:310-334, 2458-2466.
    let rows: Vec<String> = RAW_SOURCES
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            let program = format!(
                r#"(let ((print-circle nil) (print-level nil) (print-length nil)
            (print-escape-nonascii nil) (print-escape-multibyte nil)
            (standard-display-table nil))
          (prin1 (make-interpreted-closure nil (list {source}) nil) t))"#
            );
            let bytes = gnu_output(&format!("graph-closure-stdout-{index}"), &program);
            let numbers: Vec<String> = bytes.into_iter().map(|byte| byte.to_string()).collect();
            format!("({})", numbers.join(" "))
        })
        .collect();
    let expected = format!("({})", rows.join(" "));
    write_fixture("graph_closure_stdout_slots", &expected);
    expected
}

#[test]
fn recursive_closure_slots_keep_enclosing_string_and_byte_traversal_prefixes() {
    crate::test_utils::init_test_tracing();
    let saved = gnu_fixture(
        "graph_recursive_closure",
        CYCLE_FORM,
        include_str!("graph_recursive_closure.expect"),
    );
    let mut context = crate::test_utils::runtime_startup_context();
    let fixture = context
        .eval_str(&format!("(quote {saved})"))
        .expect("GNU fixture");
    let expected: Vec<Vec<u8>> = list_to_vec(&fixture)
        .expect("GNU strings")
        .into_iter()
        .map(|value| {
            value
                .as_lisp_string()
                .expect("GNU string")
                .as_bytes()
                .to_vec()
        })
        .collect();
    assert_eq!(expected.len(), 4);
    let binding = Value::cons(Value::symbol("f"), Value::NIL);
    let environment = Value::list(vec![binding]);
    let closure = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: vec![Value::symbol("f")],
        env: Some(environment),
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    binding.set_cdr(closure);
    for (value, expected) in [
        closure,
        Value::vector(vec![closure]),
        Value::list(vec![closure]),
    ]
    .into_iter()
    .zip(&expected[..3])
    {
        assert_eq!(print_value(&value).as_bytes(), expected.as_slice());
        assert_eq!(print_value_bytes(&value).as_slice(), expected.as_slice());
    }
    let circle = PrintOptions {
        print_circle: true,
        ..PrintOptions::default()
    };
    assert_eq!(
        print_value_with_options(&closure, circle).as_bytes(),
        expected[3].as_slice()
    );
    assert_eq!(
        print_value_bytes_with_options(&closure, circle).as_slice(),
        expected[3].as_slice()
    );
}

#[test]
fn closure_byte_slots_preserve_unicode_unibyte_byte8_and_extended_characters() {
    crate::test_utils::init_test_tracing();
    let saved = gnu_stdout_fixture();
    let mut context = crate::test_utils::runtime_startup_context();
    let fixture = context
        .eval_str(&format!("(quote {saved})"))
        .expect("GNU fixture");
    // Own canonical bytes before the next evaluator call may collect fixture values.
    let expected: Vec<Vec<u8>> = list_to_vec(&fixture)
        .expect("GNU rows")
        .into_iter()
        .map(|row| {
            list_to_vec(&row)
                .expect("GNU stdout bytes")
                .into_iter()
                .map(|value| u8::try_from(value.as_fixnum().expect("byte")).expect("u8"))
                .collect()
        })
        .collect();
    let sources = RAW_SOURCES;
    assert_eq!(expected.len(), sources.len());
    for (index, (source, expected)) in sources.into_iter().zip(expected).enumerate() {
        let text = context.eval_str(source).expect("slot string");
        let closure = Value::make_lambda(LambdaData {
            params: LambdaParams::simple(vec![]),
            body: vec![text],
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        });
        assert_eq!(print_value_bytes(&closure), expected, "{source}");
        // These two GNU outputs have Unicode/ASCII-only character streams.
        if index == 0 || index == 2 {
            assert_eq!(
                print_value(&closure).as_bytes(),
                expected.as_slice(),
                "{source}"
            );
        }
    }
}
