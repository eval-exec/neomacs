//! GNU character.c:773-835 and print.c:472-495 preserve ordinary Emacs
//! character spans while converting raw byte8 pairs into octal ASCII.
//! Compare the public error path and the shared encoder with GNU-derived data.
use std::path::PathBuf;

use super::super::{PrincStringOutput, princ_string_bytes, print_value};
use crate::emacs_core::value::list_to_vec;

const SOURCES: &str = r#"(let* ((all (apply #'unibyte-string (number-sequence 0 255)))
       (low (string-to-multibyte (unibyte-string 128)))
       (high (string-to-multibyte (unibyte-string 255)))
       (unicode (string 233 955 #x1f600))
       (extended (concat (make-string 1 #x110000 t) (make-string 1 #x200000 t)))
       (specs (list
               (list 'all-256-unibyte all)
               (list 'all-256-promoted (string-to-multibyte all))
               (list 'all-high-promoted (string-to-multibyte (substring all 128)))
               (list 'c0-alone low)
               (list 'c1-alone high)
               (list 'sparse-mixed (concat unicode extended "abc" low unicode "z" high extended))
               (list 'adjacent-mixed (concat low high unicode extended low high))
               (list 'ascii-multibyte (make-string 64 97 t))
               (list 'unicode-extended-only (concat unicode extended))
               (list 'empty-multibyte (make-string 0 97 t)))))
  (dolist (n '(15 16 17 31 32 33))
    (setq specs
          (append specs
                  (list
                   (list (list 'end n) (concat (make-string (- n 2) 97 t) low))
                   (list (list 'start n) (concat high (make-string (- n 2) 97 t)))
                   (list (list 'head-at-boundary n) (concat (make-string n 97 t) low high))
                   (list (list 'adjacent-at-ends n) (concat low (make-string (- n 4) 97 t) high))
                   (list (list 'dense n) (concat (apply #'concat (make-list (/ n 2) low))
                                                (if (= (% n 2) 0) "" "a")))))))
  specs)"#;

fn program() -> String {
    format!(
        r#"(let ((print-circle nil) (print-length 0) (print-level 0)
      (print-escape-nonascii nil) (print-escape-multibyte nil)
      (print-escape-newlines t) (print-unreadable-function nil)
      (rows nil))
  (dolist (spec {SOURCES})
    (dolist (properties '(nil t))
      (let ((text (copy-sequence (cadr spec))))
        (when properties (put-text-property 0 (length text) 'face 'bold text))
        (let ((rendered (with-temp-buffer
                          (set-buffer-multibyte t)
                          (princ text (current-buffer))
                          (buffer-string)))
              (diagnostic (error-message-string (list 'end-of-file text))))
          (push (list (car spec) properties (append rendered nil)
                      (text-properties-at 0 rendered) (append diagnostic nil)
                      (text-properties-at (max 0 (1- (length diagnostic))) diagnostic))
                rows)))))
  (nreverse rows))"#
    )
}

fn expectation() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("src/emacs_core/lisp/print/tests/princ_octal_spans_gnu.expect");
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return std::fs::read_to_string(fixture).expect("refresh GNU fixture with UPDATE_EXPECT=1");
    }
    let script = root.join("../../tmp").join(format!(
        "princ-octal-spans-oracle-{}.el",
        std::process::id()
    ));
    std::fs::write(
        &script,
        format!(";;; -*- lexical-binding: nil; -*-\n(prin1 {})\n", program()),
    )
    .expect("GNU probe file");
    let emacs = crate::test_utils::gnu_oracle().executable_path();
    // The campaign encloses the whole nextest driver in its PID sandbox;
    // callers outside that driver may supply an explicit child wrapper.
    let mut command = if let Some(wrapper) = std::env::var_os("NEOMACS_PROBE_SANDBOX") {
        let mut command = std::process::Command::new(wrapper);
        command.arg(&emacs);
        command
    } else {
        std::process::Command::new(&emacs)
    };
    let output = command
        .args(["-Q", "--batch", "-l"])
        .arg(script)
        .output()
        .expect("GNU oracle child");
    assert!(
        output.status.success(),
        "GNU oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = String::from_utf8(output.stdout).expect("ASCII character-code fixture");
    std::fs::write(fixture, &expected).expect("GNU fixture");
    expected
}

#[test]
fn princ_octal_spans_preserve_gnu_all_byte_domains_and_boundaries() {
    crate::test_utils::init_test_tracing();
    let expected = expectation();
    let mut context = crate::test_utils::runtime_startup_context();
    context.set_lexical_binding(false);
    let actual = context.eval_str(&program()).expect("public printer oracle");
    assert_eq!(print_value(&actual), expected.trim_end());

    let fixture = context
        .eval_str(&format!("(quote {expected})"))
        .expect("GNU rows");
    // Own all expected bytes before another evaluator call can collect values.
    let expected_bytes: Vec<Vec<u8>> = list_to_vec(&fixture)
        .expect("GNU rows")
        .into_iter()
        .map(|row| {
            let fields = list_to_vec(&row).expect("GNU row");
            let mut bytes = Vec::new();
            for code in list_to_vec(&fields[2]).expect("GNU text-stream characters") {
                let code = u32::try_from(code.as_fixnum().expect("GNU character code"))
                    .expect("Emacs character range");
                let mut encoded = [0; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
                let len = crate::emacs_core::emacs_char::char_string(code, &mut encoded);
                bytes.extend_from_slice(&encoded[..len]);
            }
            bytes
        })
        .collect();
    assert_eq!(expected_bytes.len(), 80);
    let sources = context.eval_str(SOURCES).expect("encoder sources");
    let sources = list_to_vec(&sources).expect("source shapes");
    assert_eq!(sources.len() * 2, expected_bytes.len());
    // No Lisp/callback/GC occurs below; each source borrow ends before the next.
    for (spec, expected_pair) in sources.into_iter().zip(expected_bytes.chunks_exact(2)) {
        let fields = list_to_vec(&spec).expect("source shape");
        let source = fields[1].as_lisp_string().expect("source text");
        let encoded = princ_string_bytes(source, PrincStringOutput::OctalByte8);
        assert_eq!(encoded.as_ref(), expected_pair[0].as_slice());
        assert_eq!(encoded.as_ref(), expected_pair[1].as_slice());
    }
}
