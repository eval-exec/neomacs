//! GNU print.c:1046-1061 renders errors through the multibyte scratch buffer.
//! No-escape strings ignore print limits/properties (2376-2380), retain the
//! byte8 protocol (472-495), and remain circle candidates (1329-1336,2300-2329).
//! This oracle covers an optimization invariant; the unoptimized printer
//! should already pass it. Expectations are refreshed only from GNU 31.1.
use std::path::PathBuf;

const PROGRAM: &str = r#"(let (rows)
  (dolist (configuration '(default irrelevant circle))
    (let ((print-circle (eq configuration 'circle))
          (print-continuous-numbering nil)
          (print-number-table nil)
          (print-level (if (eq configuration 'default) nil 0))
          (print-length (if (eq configuration 'default) nil 0))
          (print-escape-newlines (not (eq configuration 'default)))
          (print-escape-nonascii (not (eq configuration 'default)))
          (print-escape-multibyte (not (eq configuration 'default)))
          (print-escape-control-characters (not (eq configuration 'default)))
          (print-charset-text-property nil)
          (print-unreadable-function
           (lambda (_object _escaping) (error "String called unreadable printer"))))
      (dolist (spec (list
                     (list 'empty-unibyte (unibyte-string))
                     (list 'empty-multibyte (make-string 0 97 t))
                     (list 'ascii-unibyte (unibyte-string 97 10 9 92 34))
                     (list 'ascii-multibyte (string-to-multibyte (unibyte-string 97 10 9 92 34)))
                     (list 'unicode (concat (string 233 955) (make-string 1 #x1f600 t)))
                     (list 'raw-unibyte (unibyte-string 128 255 97))
                     (list 'raw-multibyte (string-to-multibyte (unibyte-string 128 255 97)))
                     (list 'extended-110000 (make-string 2 #x110000 t))
                     (list 'extended-200000 (make-string 2 #x200000 t))))
        (dolist (properties '(nil t))
          (let ((text (copy-sequence (car (cdr spec)))))
            (when properties
              (put-text-property 0 (length text) 'face 'bold text))
            (let ((result (error-message-string (list 'end-of-file text))))
              (push (list configuration (car spec) properties
                          (append result nil) (text-properties-at 0 result))
                    rows)))))))
  (nreverse rows))"#;

fn expectation() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("src/emacs_core/lisp/print/tests/error_string_leaf_gnu.expect");
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let script = root.join("../../tmp").join(format!(
            "error-string-leaf-oracle-{}.el",
            std::process::id()
        ));
        std::fs::write(&script, format!("(prin1 {PROGRAM})\n")).expect("GNU probe file");
        let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".local/bin/emacs")
                .into_os_string()
        });
        // Campaign nextest already encloses oracle children in the PID sandbox.
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
        std::fs::write(&fixture, &expected).expect("GNU fixture");
        expected
    } else {
        std::fs::read_to_string(fixture).expect("refresh from GNU with UPDATE_EXPECT=1")
    }
}

#[test]
fn error_string_leaf_gnu_domains_settings_and_circle_fallback() {
    crate::test_utils::init_test_tracing();
    let expected = expectation();
    let mut context = crate::test_utils::runtime_startup_context();
    let actual = context
        .eval_str(PROGRAM)
        .expect("error string leaf oracle evaluation");
    assert_eq!(super::super::print_value(&actual), expected.trim_end());
}
