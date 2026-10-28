//! GNU 31.1-backed stream and raw-character printing regressions.
use crate::test_utils::runtime_startup_context;

fn assert_gnu(name: &str, form: &str) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root
        .join("src/emacs_core/lisp/native/builtins/tests/printer_streams")
        .join(format!("{name}.expect"));
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let script = root
            .join("../../tmp")
            .join(format!("streams-{name}-{}.el", std::process::id()));
        std::fs::write(&script, format!(";;; -*- lexical-binding: nil; -*-\n(let ((v {form})) (princ \"\nGDK-RESULT:\") (prin1 v))\n")).unwrap();
        let emacs = crate::test_utils::gnu_oracle().executable_path();
        // Campaign runs sandbox the entire nextest driver. Standalone oracle
        // refreshes can supply an explicit wrapper without a host-specific path.
        let mut command = if let Some(wrapper) = std::env::var_os("NEOMACS_PROBE_SANDBOX") {
            let mut command = std::process::Command::new(wrapper);
            command.arg(emacs);
            command
        } else {
            std::process::Command::new(emacs)
        };
        let output = command
            .args(["-Q", "--batch", "-l"])
            .arg(script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "GNU {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = String::from_utf8(output.stdout).unwrap();
        let expected = output
            .rsplit_once("GDK-RESULT:")
            .expect("GNU result marker")
            .1
            .to_owned();
        std::fs::write(&fixture, &expected).unwrap();
        expected
    } else {
        std::fs::read_to_string(fixture).expect("GNU fixture: refresh with UPDATE_EXPECT=1")
    };
    let mut eval = runtime_startup_context();
    let result = eval.eval_str(form).expect("printer Lisp evaluation");
    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        expected.trim_end(),
        "GNU {name}"
    );
}

#[test]
fn raw_noescape_result_is_well_formed() {
    assert_gnu(
        "raw-noescape",
        r#"(let ((s (prin1-to-string "\x80" t))) (list (length s) (string-bytes s) (multibyte-string-p s) (string-to-list s) (string-to-list (prin1-to-string (list "\x80" "é") t))))"#,
    );
}

#[test]
fn aggregate_raw_symbol_name_is_byte_faithful() {
    assert_gnu(
        "raw-symbol",
        r#"(let ((s (make-symbol (string-to-multibyte "\x80")))) (list (string-to-list (format "%s" (list s))) (string-to-list (with-output-to-string (princ (list s)))) (string-to-list (prin1-to-string s t)) (string-to-list (format "%s" s))))"#,
    );
}

#[test]
fn buffer_and_marker_escape_byte8_strings() {
    assert_gnu(
        "buffer-byte8",
        r#"(list (string-to-list (with-output-to-string (princ (string-to-multibyte "\x80")))) (with-temp-buffer (princ "\x80" (current-buffer)) (string-to-list (buffer-string))) (with-temp-buffer (let ((m (point-marker))) (princ "\x80" m)) (string-to-list (buffer-string))) (string-to-list (with-output-to-string (princ "\x80"))) (with-temp-buffer (set-buffer-multibyte nil) (princ "\x80" (current-buffer)) (string-to-list (buffer-string))) (string-to-list (with-output-to-string (princ "é"))))"#,
    );
}

#[test]
fn callable_stream_distinguishes_bytes_from_characters() {
    assert_gnu(
        "callback-byte8",
        r#"(mapcar (lambda (s) (let (out) (princ s (lambda (c) (push c out))) (nreverse out))) (list "\x80" (string-to-unibyte "\351") (string-to-multibyte "\x80") "é" "ascii" (list "\x80") (vector "\351") (propertize "\x80" 'foo 'bar)))"#,
    );
}

#[test]
fn terpri_ensure_uses_destination_point() {
    assert_gnu(
        "terpri-ensure",
        r#"(list (with-temp-buffer (insert "a") (list (terpri (current-buffer) t) (terpri (current-buffer) t) (buffer-string))) (with-temp-buffer (list (terpri (current-buffer) t) (buffer-string))) (condition-case e (terpri (lambda (_c)) t) (error (list (car e) (cadr e)))) (with-temp-buffer (insert "a\nb") (goto-char 3) (let ((m (copy-marker 2))) (list (terpri m t) (marker-position m) (point) (buffer-string)))))"#,
    );
}

#[test]
fn string_stream_is_callable_even_when_buffer_exists() {
    assert_gnu(
        "string-stream",
        r#"(with-temp-buffer (rename-buffer "streams-named") (list (condition-case e (prin1 'x "streams-named") (error e)) (condition-case e (princ 'x "streams-named") (error e)) (condition-case e (let ((standard-output "streams-named")) (print 'x)) (error e)) (condition-case e (write-char 97 "streams-named") (error e)) (condition-case e (terpri "streams-named") (error e)) (buffer-string)))"#,
    );
}

#[test]
fn terpri_ensure_validates_marker_before_checking_line_start() {
    assert_gnu(
        "terpri-markers",
        r#"(list (condition-case e (terpri (make-marker) t) (error (list (car e) (cadr e)))) (with-temp-buffer (insert "abc") (let ((m (copy-marker 1))) (narrow-to-region 2 4) (condition-case e (terpri m t) (error (list (car e) (cadr e)))))))"#,
    );
}

#[test]
fn resolved_callable_stream_survives_standard_output_replacement_and_gc() {
    assert_gnu(
        "callback-root",
        r#"(mapcar (lambda (printer) (let ((captured (list nil))) (let ((standard-output (eval (read "(lambda (c) (setcar captured (cons c (car captured))))") (list (cons 'captured captured)))) (print-unreadable-function (lambda (_o _e) (setq standard-output nil) (garbage-collect) "Q"))) (funcall printer (make-marker))) (nreverse (car captured)))) '(princ prin1 print))"#,
    );
}

#[test]
fn resolved_marker_stream_survives_standard_output_replacement_and_gc() {
    assert_gnu(
        "marker-root",
        r#"(mapcar (lambda (printer) (with-temp-buffer (insert "a") (let ((standard-output (point-marker)) (print-unreadable-function (lambda (_o _e) (setq standard-output nil) (garbage-collect) "Q"))) (funcall printer (make-marker))) (buffer-string))) '(princ prin1 print))"#,
    );
}

#[test]
fn error_message_string_preserves_noescape_object_encoding() {
    assert_gnu(
        "error-message-objects",
        r#"(let* ((raw (make-symbol (string-to-multibyte (unibyte-string 128))))
       (table (make-hash-table))
       (strings (list "ascii" (string-as-multibyte "ascii") "é" (unibyte-string 128) (string-to-multibyte (unibyte-string 128)))))
  (puthash 1 "x" table)
  (let ((values (append strings (mapcar (lambda (s) (propertize s 'face 'bold)) strings)
                       (list raw (list raw "é" (unibyte-string 128)) (vector raw "é" (unibyte-string 128)) table))))
    (mapcar (lambda (condition)
              (mapcar (lambda (value)
                        (string-to-list (error-message-string (list condition value)))) values))
            '(end-of-file error))))"#,
    );
}

#[test]
fn print_prepare_saves_buffer_after_overrides_before_escape_bindings() {
    assert_gnu(
        "prepare-watch-order",
        r#"(let ((caller (current-buffer))
      (other (generate-new-buffer " *streams-watch-other*")))
  (unwind-protect
      (list
       (let ((watch (lambda (_symbol _value operation _where)
                      (when (eq operation 'let) (set-buffer other)))))
         (unwind-protect
             (progn
               (add-variable-watcher 'print-length watch)
               (prin1 '(a b) (lambda (_c)) '((length . 1)))
               (eq (current-buffer) other))
           (remove-variable-watcher 'print-length watch)
           (set-buffer caller)))
       (let ((print-escape-nonascii nil)
             (watch (lambda (_symbol _value operation _where)
                      (when (eq operation 'let) (set-buffer other)))))
         (unwind-protect
             (progn
               (add-variable-watcher 'print-escape-nonascii watch)
               (princ "x" caller)
               (eq (current-buffer) caller))
           (remove-variable-watcher 'print-escape-nonascii watch)
           (set-buffer caller))))
    (kill-buffer other)))"#,
    );
}
