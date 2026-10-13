//! GNU31.1 retains the global label index with continuous numbering and a
//! nonnil table, including a newly supplied empty table (print.c:1296-1312).
//! Repeated objects and continuous gensyms increment it (print.c:1448-1461).
//! The original legacy forms are retained unchanged. Every Context and its
//! existing thread-local printer counters are owned by the running test thread.
use std::path::PathBuf;

fn oracle(name: &str, form: &str, fixture: &str) {
    crate::test_utils::init_test_tracing();
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let probe = root.join(format!(
            "../../tmp/continuous-numbering-gnu-{name}-{}.el",
            std::process::id()
        ));
        std::fs::write(
            &probe,
            format!(";;; -*- lexical-binding: nil; -*-\n(prin1 {form})\n"),
        )
        .expect("GNU probe");
        let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".local/bin/emacs")
                .into_os_string()
        });
        let mut command = match std::env::var_os("NEOMACS_PROBE_SANDBOX") {
            Some(wrapper) => {
                let mut command = std::process::Command::new(wrapper);
                command.arg(emacs);
                command
            }
            None => std::process::Command::new(emacs),
        };
        let output = command
            .args(["-Q", "--batch", "-l"])
            .arg(&probe)
            .output()
            .expect("GNU oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = String::from_utf8(output.stdout).expect("UTF-8 GNU oracle");
        let destination = root
            .join("src/emacs_core/lisp/native/builtins/tests")
            .join(format!("continuous_numbering_{name}.expect"));
        if std::fs::read_to_string(&destination).ok().as_deref() != Some(expected.as_str()) {
            std::fs::write(destination, &expected).expect("GNU fixture");
        }
        expected
    } else {
        fixture.to_owned()
    };
    let mut context = crate::emacs_core::eval::Context::new();
    context.set_lexical_binding(false);
    let actual = crate::emacs_core::format_eval_result(&context.eval_str(form));
    assert_eq!(actual, format!("OK {}", expected.trim_end()), "{name}");
}

#[test]
fn prin1_to_string_continuous_numbering_keeps_global_label_index() {
    oracle(
        "global_index",
        r###"(progn
              (let ((print-circle t)
                    (print-continuous-numbering nil))
                (let ((x (list 0)))
                  (prin1-to-string (list x x))))
              (let ((print-circle t)
                    (print-continuous-numbering t))
                (concat
                 (let ((print-number-table (make-hash-table :test 'eq)))
                   (let ((x (list 1)))
                     (prin1-to-string (list x x))))
                 "\n"
                 (let ((print-number-table (make-hash-table :test 'eq)))
                   (let ((x (list 2)))
                     (prin1-to-string (list x x)))))))"###,
        include_str!("continuous_numbering_global_index.expect"),
    );
}

#[test]
fn prin1_to_string_continuous_gensym_gets_print_number_label() {
    oracle(
        "gensym",
        r###"(progn
              (let ((print-circle t)
                    (print-continuous-numbering nil))
                (let ((x (list 0)))
                  (prin1-to-string (list x x))))
              (let* ((s (make-symbol "g"))
                     (print-gensym t)
                     (print-circle t)
                     (print-continuous-numbering t)
                     (print-number-table (make-hash-table :test 'eq)))
                (prin1-to-string s)))"###,
        include_str!("continuous_numbering_gensym.expect"),
    );
}
