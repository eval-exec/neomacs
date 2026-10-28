//! GNU 31.1 settings and stream observations through byte-compiled callers.
//! Run the same filter with the default JIT and NEOVM_JIT=0. Mutable
//! evaluator state belongs to the calling test thread; no env writes occur.
use std::path::PathBuf;

fn oracle(name: &str, form: &str, fixture: &str) {
    crate::test_utils::init_test_tracing();
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let probe = root.join(format!(
            "../../tmp/settings-compiled-{name}-{}.el",
            std::process::id()
        ));
        std::fs::write(
            &probe,
            format!(";; -*- lexical-binding: nil; -*-\n(prin1 {form})\n"),
        )
        .expect("GNU probe");
        let emacs = crate::test_utils::gnu_oracle().executable_path();
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
            .arg(probe)
            .output()
            .expect("GNU oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = String::from_utf8(output.stdout).expect("UTF-8 oracle");
        let destination = root
            .join("src/emacs_core/lisp/print/tests")
            .join(format!("compiled_settings_{name}.expect"));
        if std::fs::read_to_string(&destination).ok().as_deref() != Some(expected.as_str()) {
            std::fs::write(destination, &expected).expect("GNU fixture");
        }
        expected
    } else {
        fixture.to_owned()
    };
    let mut context = crate::test_utils::runtime_startup_context();
    // Match the lexical-binding:nil file used to refresh the GNU fixture.
    context.set_lexical_binding(false);
    let actual = crate::emacs_core::format_eval_result(&context.eval_str(form));
    assert_eq!(actual, format!("OK {}", expected.trim_end()), "{name}");
}

#[test]
fn printer_compiled_settings_gnu_compiled_settings() {
    oracle(
        "compiled_settings",
        r###"(let* ((print-length 99) (print-quoted nil) (event nil) (collected nil)
        (printer (byte-compile '(lambda (_object escaping)
                    (setq event (list print-length print-quoted print-escape-nonascii print-unreadable-function escaping))
                    (setq print-quoted nil print-length 1)
                    (unless collected (setq collected t) (garbage-collect))
                    "Q")))
        (render (byte-compile '(lambda (object callback)
                   (prin1-to-string object nil
                     (list (cons 'length 3) (cons 'quoted t)
                           (cons 'unreadable-function callback))))))
        (object (vector (make-marker) '(quote a) '(1 2)))
        (text nil))
   (dotimes (_ 1024) (setq text (funcall render object printer)))
   (list (byte-code-function-p printer) (byte-code-function-p render)
         text event print-length print-quoted))"###,
        include_str!("compiled_settings_compiled_settings.expect"),
    );
}

#[test]
fn printer_compiled_settings_gnu_compiled_streams() {
    oracle(
        "compiled_streams",
        r###"(let* ((seen nil) (print-quoted t) (print-length 99)
        (sink (byte-compile '(lambda (character) (push character seen)
                              (when (= character 233) (garbage-collect)))))
        (princ-driver (byte-compile '(lambda (object target) (princ object target) (terpri target))))
        (prin1-driver (byte-compile '(lambda (object target callback)
                         (prin1 object target
                           (list (cons 'length 3) (cons 'quoted t)
                                 (cons 'unreadable-function callback))))))
        (printer (byte-compile '(lambda (_object escaping)
                     (setq print-quoted nil)
                     (if escaping "S" "s"))))
        (failing (byte-compile '(lambda (_object _escaping)
                     (set-buffer (get-buffer-create "printer-bytecode-callback"))
                     (garbage-collect) (error "compiled callback")))))
   (let ((first (progn (funcall princ-driver (string 233 #x3fff80) sink) (nreverse seen))))
     (setq seen nil)
     (funcall prin1-driver (vector (make-marker) '(quote a)) sink printer)
     (let ((second (apply #'string (nreverse seen))) (origin (current-buffer)))
       (list first second
         (condition-case condition (funcall prin1-driver (make-marker) sink failing) (error condition))
         (eq origin (current-buffer)) print-length print-quoted
         (byte-code-function-p sink) (byte-code-function-p failing)))))"###,
        include_str!("compiled_settings_compiled_streams.expect"),
    );
}

#[test]
fn printer_compiled_settings_gnu_local_number_tables() {
    oracle(
        "local_number_tables",
        r###"(let ((print-circle t) (print-continuous-numbering t) (print-number-table nil))
  (with-temp-buffer
    (setq-local print-number-table (make-hash-table :test 'eq))
    (let* ((origin-table print-number-table)
           (target (generate-new-buffer "printer-table-target"))
           (x (list 1)))
      (unwind-protect
        (progn
          (with-current-buffer target (setq-local print-number-table (make-hash-table :test 'eq)))
          (let ((target-table (with-current-buffer target print-number-table)))
            (prin1 (list x x) target)
            (let ((retained (list (eq origin-table print-number-table) (hash-table-count origin-table)
                                 (with-current-buffer target (eq target-table print-number-table))
                                 (hash-table-count target-table) (with-current-buffer target (buffer-string)))))
              (let ((print-circle nil) (print-continuous-numbering nil)) (prin1 1 target))
              (list retained (eq origin-table print-number-table)
                    (with-current-buffer target print-number-table)))))
        (kill-buffer target)))))"###,
        include_str!("compiled_settings_local_number_tables.expect"),
    );
}

#[test]
fn printer_compiled_settings_gnu_live_cdr_table() {
    oracle(
        "live_cdr_table",
        r###"(list
 (let ((print-circle nil) (print-unreadable-function (lambda (_o _e) (setq print-circle t) "Q")))
   (condition-case e (prin1-to-string (list (make-marker) 1 2)) (error e)))
 (let ((print-circle t) (print-unreadable-function (lambda (_o _e) (let ((print-circle nil)) (prin1 1 #'ignore)) "Q")))
   (condition-case e (prin1-to-string (list (make-marker) 1 2)) (error e))))"###,
        include_str!("compiled_settings_live_cdr_table.expect"),
    );
}

#[test]
fn printer_compiled_settings_gnu_callback_bind_cleanup() {
    oracle(
        "callback_bind_cleanup",
        r###"(let* ((function (lambda (_object _escaping) "unused"))
        (print-unreadable-function function)
        (events nil)
        (watcher (lambda (_symbol value operation _where)
                   (when (memq operation '(let unlet))
                     (push operation events)
                     (cond ((and (eq operation 'let) (null value)) (error "binding failure"))
                           ((eq operation 'unlet) (error "cleanup failure")))))))
   (unwind-protect
       (progn
         (add-variable-watcher 'print-unreadable-function watcher)
         (let ((result (condition-case condition (prin1-to-string (make-marker)) (error condition))))
           (remove-variable-watcher 'print-unreadable-function watcher)
           (list result (nreverse events) (eq function print-unreadable-function))))
     (remove-variable-watcher 'print-unreadable-function watcher)))"###,
        include_str!("compiled_settings_callback_bind_cleanup.expect"),
    );
}
