//! GNU print.c:2470-2530 prints exact live symbol names, gates #: on gensym,
//! and gates reader escapes on escapeflag. character.h:464-483 converts unibyte
//! symbol names to byte8 even for callable streams, unlike top-level strings.
//! A preservation oracle; no regression/red result is implied.
use std::path::PathBuf;

const PROGRAM: &str = r#"(let ((print-circle nil) (print-continuous-numbering nil)
      (print-level nil) (print-length nil) (rows nil)
      (snapshot
       (lambda (object)
         (list
          (with-temp-buffer (set-buffer-multibyte t) (princ object (current-buffer))
                            (append (buffer-string) nil))
          (with-temp-buffer (set-buffer-multibyte nil) (princ object (current-buffer))
                            (append (buffer-string) nil))
          (let (characters) (princ object (lambda (character) (push character characters)))
               (nreverse characters))
          (append (format "%s" object) nil)
          (append (format "%s" (list object)) nil)
          (append (prin1-to-string object t) nil)))))
  (dolist (gensym '(nil t))
    (let ((print-gensym gensym))
      (dolist (spec (list
                    (list 'empty-unibyte (unibyte-string))
                    (list 'empty-multibyte (make-string 0 97 t))
                    (list 'ascii-unibyte (unibyte-string 97 32 98 92 63))
                    (list 'ascii-multibyte (string-to-multibyte (unibyte-string 97 32 98 92 63)))
                    (list 'unicode (string 233 955 #x1f600))
                    (list 'raw-unibyte (unibyte-string 128 255 97))
                    (list 'raw-multibyte (string-to-multibyte (unibyte-string 128 255 97)))
                    (list 'extended-110000 (make-string 2 #x110000 t))
                    (list 'extended-200000 (make-string 2 #x200000 t))))
        (dolist (interned '(nil t))
          (let ((symbol (if interned (intern (copy-sequence (car (cdr spec))))
                         (make-symbol (copy-sequence (car (cdr spec)))))))
            (push (list 'symbol (car spec) gensym interned (funcall snapshot symbol)) rows))))
      (dolist (domain '(live-unibyte live-multibyte))
        (let* ((name (if (eq domain 'live-unibyte) (unibyte-string 97 98)
                       (string 955 98)))
               (symbol (make-symbol name)))
          ;; Warm the printer before mutating the very same Lisp-visible name.
          (funcall snapshot symbol)
          (aset (symbol-name symbol) (if (eq domain 'live-unibyte) 0 1)
                (if (eq domain 'live-unibyte) 128 99))
          (push (list 'live-symbol domain gensym (funcall snapshot symbol)) rows)))))
  (let ((print-gensym nil) (name (unibyte-string 128 255)))
    (push (list 'string-contrast
                (let (characters) (princ (make-symbol name) (lambda (character) (push character characters)))
                     (nreverse characters))
                (let (characters) (princ name (lambda (character) (push character characters)))
                     (nreverse characters))) rows))
  (let ((print-gensym t))
    (dolist (name '("" "a b" "123" "?x" "."))
      (dolist (interned '(nil t))
        (let* ((symbol (if interned (intern name) (make-symbol name)))
               (text (prin1-to-string symbol))
               (restored (car (read-from-string text))))
          (push (list 'read-roundtrip (append name nil) interned
                      (append text nil) (symbolp restored)
                      (append (symbol-name restored) nil)) rows)))))
  (nreverse rows))"#;

fn expectation() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("src/emacs_core/lisp/print/tests/symbol_noescape_gnu.expect");
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let script = root
            .join("../../tmp")
            .join(format!("symbol-noescape-oracle-{}.el", std::process::id()));
        std::fs::write(&script, format!("(prin1 {PROGRAM})\n")).expect("GNU probe file");
        // Campaign nextest already encloses oracle children in the PID sandbox.
        let mut command = crate::test_utils::gnu_probe_command();
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
fn symbol_noescape_gnu_domains_gensym_live_names_and_streams() {
    crate::test_utils::init_test_tracing();
    let expected = expectation();
    let mut context = crate::test_utils::runtime_startup_context();
    let actual = context
        .eval_str(PROGRAM)
        .expect("symbol noescape oracle evaluation");
    assert_eq!(super::super::print_value(&actual), expected.trim_end());
}
