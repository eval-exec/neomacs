//! Bounded GNU 31.1 constructor and allocation-delivery observations.
//! Huge accepted ASCII-string/bool-vector requests are deliberately absent.

fn assert_bounded_case(form: &str, frozen: &str) {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    context
        .eval_str(
            r#"(setq memory-signal-data
                 (list 'error
                       (substitute-command-keys "Memory exhausted--use \\[save-some-buffers] then exit and restart Emacs")))"#,
        )
        .expect("GNU command-line memory-signal-data initialization");
    let actual = context.eval_str(form);
    assert_eq!(
        crate::emacs_core::format_eval_result(&actual),
        format!("OK {}", frozen.trim_end_matches('\n')),
    );
}

#[test]
fn bounded_vector_size_rejection() {
    assert_bounded_case(
        r#"(condition-case e (make-vector most-positive-fixnum 0) (error e))"#,
        include_str!("bounded/vector-size-rejection.expect"),
    );
}

#[test]
fn bounded_string_byte_overflow() {
    assert_bounded_case(
        r#"(condition-case e (make-string most-positive-fixnum ?é) (error e))"#,
        include_str!("bounded/string-byte-overflow.expect"),
    );
}

#[test]
fn bounded_constructor_small() {
    assert_bounded_case(
        r#"(list (make-vector 0 7) (make-vector 3 7) (make-string 3 ?é) (make-string 0 ?é) (make-bool-vector 0 nil) (length (make-bool-vector 65 t)) (aref (make-bool-vector 65 nil) 64) (aref (make-bool-vector 65 t) 64))"#,
        include_str!("bounded/constructor-small.expect"),
    );
}

#[test]
fn bounded_live_memory_binding() {
    assert_bounded_case(
        r#"(let ((memory-signal-data (list 'error "owned allocation exhausted"))) (condition-case e (make-vector most-positive-fixnum 0) (error (list e (eq e memory-signal-data)))))"#,
        include_str!("bounded/live-memory-binding.expect"),
    );
}

#[test]
fn bounded_buffer_local_memory_binding() {
    assert_bounded_case(
        r#"(with-temp-buffer (make-local-variable 'memory-signal-data) (setq memory-signal-data (list 'error "buffer-local exhausted")) (condition-case e (make-vector most-positive-fixnum 0) (error (list e (eq e memory-signal-data)))))"#,
        include_str!("bounded/buffer-local-memory-binding.expect"),
    );
}

#[test]
fn bounded_memory_dotted_identity_and_callbacks() {
    assert_bounded_case(
        r#"(let* ((memory-signal-data (cons 'error 'owned-tail)) (gdl-hook-count 0) (gdl-debugger-count 0) (internal-when-entered-debugger -1) (inhibit-debugger nil) (debug-ignored-errors nil) (debug-on-error t) (debug-on-signal t) (debugger (lambda (&rest _) (setq gdl-debugger-count (1+ gdl-debugger-count)))) (signal-hook-function (lambda (&rest _) (setq gdl-hook-count (1+ gdl-hook-count))))) (list (condition-case e (make-vector most-positive-fixnum 0) (error (list e (eq e memory-signal-data)))) gdl-hook-count gdl-debugger-count))"#,
        include_str!("bounded/memory-dotted-identity-and-callbacks.expect"),
    );
}

#[test]
fn bounded_ordinary_error_callback_control() {
    assert_bounded_case(
        r#"(let* ((gdl-hook-count 0) (gdl-debugger-count 0) (internal-when-entered-debugger -1) (inhibit-debugger nil) (debug-ignored-errors nil) (debug-on-error t) (debug-on-signal t) (debugger (lambda (&rest _) (setq gdl-debugger-count (1+ gdl-debugger-count)))) (signal-hook-function (lambda (&rest _) (setq gdl-hook-count (1+ gdl-hook-count))))) (list (condition-case e (error "ordinary") (error e)) gdl-hook-count gdl-debugger-count))"#,
        include_str!("bounded/ordinary-error-callback-control.expect"),
    );
}

#[test]
fn bounded_undefined_memory_condition() {
    assert_bounded_case(
        r#"(let* ((memory-signal-data (cons 'gdl-audit-undefined-condition 'owned-tail)) (gdl-hook-count 0) (gdl-debugger-count 0) (internal-when-entered-debugger -1) (inhibit-debugger nil) (debug-ignored-errors nil) (debug-on-error t) (debug-on-signal t) (debugger (lambda (&rest _) (setq gdl-debugger-count (1+ gdl-debugger-count)))) (signal-hook-function (lambda (&rest _) (setq gdl-hook-count (1+ gdl-hook-count))))) (list (condition-case e (make-vector most-positive-fixnum 0) (error (list e (eq e memory-signal-data)))) gdl-hook-count gdl-debugger-count))"#,
        include_str!("bounded/undefined-memory-condition.expect"),
    );
}

#[test]
fn bounded_malformed_memory_data() {
    assert_bounded_case(
        r#"(let ((out nil)) (dolist (datum (list nil t "oom" (cons 17 'owned-tail))) (let* ((memory-signal-data datum) (gdl-hook-count 0) (signal-hook-function (lambda (&rest _) (setq gdl-hook-count (1+ gdl-hook-count))))) (push (list (condition-case e (make-vector most-positive-fixnum 0) (error e)) gdl-hook-count) out))) (nreverse out))"#,
        include_str!("bounded/malformed-memory-data.expect"),
    );
}

#[test]
fn bounded_aggregate_string_width_overflow() {
    assert_bounded_case(
        r#"(let ((width (number-to-string most-positive-fixnum))) (list (condition-case e (format (concat "a%" width "s") "a") (error e)) (condition-case e (format-message (concat "a%" width "s") "a") (error e))))"#,
        include_str!("bounded/aggregate-string-width-overflow.expect"),
    );
}

#[test]
fn bounded_aggregate_conversion_order() {
    assert_bounded_case(
        r#"(let ((width (number-to-string most-positive-fixnum))) (list (condition-case e (format (concat "a%" width "d") 'bad) (error e)) (condition-case e (format (concat "a%" width "c") "bad") (error e)) (condition-case e (format (concat "a%" width "q") 1) (error e)) (condition-case e (format (concat "a%" width "d")) (error e))))"#,
        include_str!("bounded/aggregate-conversion-order.expect"),
    );
}

#[test]
fn bounded_format_saturation_small_output() {
    assert_bounded_case(
        r#"(list (format "%.999999999999999999999999999s" "a") (format "%.18446744073709551616s" "a") (condition-case e (format "%999999999999999999999999999$s" 1) (error e)))"#,
        include_str!("bounded/format-saturation-small-output.expect"),
    );
}

#[test]
fn bounded_format_encoding_storage_and_props() {
    assert_bounded_case(
        r#"(let* ((raw (unibyte-string 255)) (src (propertize "é" 'face 'bold))) (list (string-to-list (format "[%s]" raw)) (multibyte-string-p (format "[%s]" raw)) (string-to-list (format "[%s]" src)) (get-text-property 1 'face (format "[%s]" src)) (multibyte-string-p (format "%%" "é")) (let ((s (propertize "plain" 'face 'bold))) (eq s (format s))) (let ((s (propertize "text" 'face 'bold))) (eq s (format "%s" s)))))"#,
        include_str!("bounded/format-encoding-storage-and-props.expect"),
    );
}

#[test]
fn bounded_bool_vector_bounded_zero_and_truthy() {
    assert_bounded_case(
        r#"(mapcar (lambda (n) (let ((a (make-bool-vector n nil)) (b (make-bool-vector n 'truthy))) (list (length a) (length b) (if (= n 0) nil (list (aref a 0) (aref a (1- n)) (aref b 0) (aref b (1- n))))))) '(0 1 63 64 65 127 128 129 100000))"#,
        include_str!("bounded/bool-vector-bounded-zero-and-truthy.expect"),
    );
}

#[test]
fn bounded_vector_bounded_initialization_and_identity() {
    assert_bounded_case(
        r#"(let* ((cell (list 'value)) (a (make-vector 3 cell)) (b (make-vector 3 nil))) (aset a 1 'replacement) (list (eq cell (aref a 0)) (eq cell (aref a 2)) (aref a 1) b))"#,
        include_str!("bounded/vector-bounded-initialization-and-identity.expect"),
    );
}
