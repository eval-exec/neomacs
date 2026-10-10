use crate::common::{
    assert_oracle_parity, assert_oracle_parity_expect,
    return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn take_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (take (expt 2 100) '(1 2)) (ntake (expt 2 100) (list 1 2)) (take (- (expt 2 100)) '(1 2)) (ntake (- (expt 2 100)) (list 1 2)) (butlast '(1 2 3) (expt 2 100)) (take (1+ most-positive-fixnum) nil) (condition-case e (take (expt 2 100) 'x) (error e)) (condition-case e (take 1.0 '(1 2)) (error e)))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((1 2) (1 2) nil nil nil nil (wrong-type-argument listp x) (wrong-type-argument integerp 1.0))""#
        ]],
    );
}

#[test]
fn ntake_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (condition-case e (ntake 2 (cons 1 2)) (error e)) (condition-case e (ntake 4 (cons 1 (cons 2 (cons 3 4)))) (error e)) (condition-case e (take 2 (cons 1 2)) (error e)) (ntake 1 (cons 1 2)) (condition-case e (ntake 2 5) (error e)))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((wrong-type-argument listp (1 . 2)) (wrong-type-argument listp (1 2 3 . 4)) (wrong-type-argument listp 2) (1) (wrong-type-argument listp 5))""#
        ]],
    );
}

#[test]
fn value_lt_exact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn (require 'bytecomp) (list
(list (value< most-positive-fixnum (float most-positive-fixnum)) (value< 9007199254740992.0 9007199254740993) (value< 9007199254740993 9007199254740992.0) (value< -9007199254740993 -9007199254740992.0) (value< -9007199254740992.0 -9007199254740993) (value< 1 1.5) (value< 1.5 1) (value< 1 0.0e+NaN) (value< 0.0e+NaN 1) (value< most-positive-fixnum 1.0e+INF) (value< -1.0e+INF most-negative-fixnum) (mapcar #'number-to-string (sort (list 9007199254740993 9007199254740992.0))))

(let ((sorter (byte-compile (lambda (sequence) (sort sequence)))))
  (list
    (mapcar #'number-to-string (sort (list 9007199254740993 9007199254740992.0)))
    (mapcar #'number-to-string (sort (vector 9007199254740993 9007199254740992.0)))
    (mapcar #'number-to-string (funcall sorter (list 9007199254740993 9007199254740992.0)))
    (mapcar #'number-to-string (funcall sorter (list 9007199254740993 9007199254740992.0)))
    (mapcar #'number-to-string (funcall sorter (vector 9007199254740993 9007199254740992.0)))
    (mapcar #'number-to-string (funcall sorter (vector 9007199254740993 9007199254740992.0)))))
))
"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((t t nil t nil t nil nil nil t t (\"9007199254740992.0\" \"9007199254740993\")) ((\"9007199254740992.0\" \"9007199254740993\") (\"9007199254740992.0\" \"9007199254740993\") (\"9007199254740992.0\" \"9007199254740993\") (\"9007199254740992.0\" \"9007199254740993\") (\"9007199254740992.0\" \"9007199254740993\") (\"9007199254740992.0\" \"9007199254740993\")))""#
        ]],
    );
}

#[test]
fn bounded_vector_size_rejection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(r#"(condition-case e (make-vector most-positive-fixnum 0) (error e))"#);
}

#[test]
fn bounded_string_byte_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(r#"(condition-case e (make-string most-positive-fixnum ?é) (error e))"#);
}

#[test]
fn bounded_constructor_small() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(list (make-vector 0 7) (make-vector 3 7) (make-string 3 ?é) (make-string 0 ?é) (make-bool-vector 0 nil) (length (make-bool-vector 65 t)) (aref (make-bool-vector 65 nil) 64) (aref (make-bool-vector 65 t) 64))"#,
    );
}

#[test]
fn bounded_live_memory_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let ((memory-signal-data (list 'error "owned allocation exhausted"))) (condition-case e (make-vector most-positive-fixnum 0) (error (list e (eq e memory-signal-data)))))"#,
    );
}

#[test]
fn bounded_buffer_local_memory_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(with-temp-buffer (make-local-variable 'memory-signal-data) (setq memory-signal-data (list 'error "buffer-local exhausted")) (condition-case e (make-vector most-positive-fixnum 0) (error (list e (eq e memory-signal-data)))))"#,
    );
}

#[test]
fn bounded_memory_dotted_identity_and_callbacks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let* ((memory-signal-data (cons 'error 'owned-tail)) (gdl-hook-count 0) (gdl-debugger-count 0) (internal-when-entered-debugger -1) (inhibit-debugger nil) (debug-ignored-errors nil) (debug-on-error t) (debug-on-signal t) (debugger (lambda (&rest _) (setq gdl-debugger-count (1+ gdl-debugger-count)))) (signal-hook-function (lambda (&rest _) (setq gdl-hook-count (1+ gdl-hook-count))))) (list (condition-case e (make-vector most-positive-fixnum 0) (error (list e (eq e memory-signal-data)))) gdl-hook-count gdl-debugger-count))"#,
    );
}

#[test]
fn bounded_ordinary_error_callback_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let* ((gdl-hook-count 0) (gdl-debugger-count 0) (internal-when-entered-debugger -1) (inhibit-debugger nil) (debug-ignored-errors nil) (debug-on-error t) (debug-on-signal t) (debugger (lambda (&rest _) (setq gdl-debugger-count (1+ gdl-debugger-count)))) (signal-hook-function (lambda (&rest _) (setq gdl-hook-count (1+ gdl-hook-count))))) (list (condition-case e (error "ordinary") (error e)) gdl-hook-count gdl-debugger-count))"#,
    );
}

#[test]
fn bounded_undefined_memory_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let* ((memory-signal-data (cons 'gdl-audit-undefined-condition 'owned-tail)) (gdl-hook-count 0) (gdl-debugger-count 0) (internal-when-entered-debugger -1) (inhibit-debugger nil) (debug-ignored-errors nil) (debug-on-error t) (debug-on-signal t) (debugger (lambda (&rest _) (setq gdl-debugger-count (1+ gdl-debugger-count)))) (signal-hook-function (lambda (&rest _) (setq gdl-hook-count (1+ gdl-hook-count))))) (list (condition-case e (make-vector most-positive-fixnum 0) (error (list e (eq e memory-signal-data)))) gdl-hook-count gdl-debugger-count))"#,
    );
}

#[test]
fn bounded_malformed_memory_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let ((out nil)) (dolist (datum (list nil t "oom" (cons 17 'owned-tail))) (let* ((memory-signal-data datum) (gdl-hook-count 0) (signal-hook-function (lambda (&rest _) (setq gdl-hook-count (1+ gdl-hook-count))))) (push (list (condition-case e (make-vector most-positive-fixnum 0) (error e)) gdl-hook-count) out))) (nreverse out))"#,
    );
}

#[test]
fn bounded_aggregate_string_width_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let ((width (number-to-string most-positive-fixnum))) (list (condition-case e (format (concat "a%" width "s") "a") (error e)) (condition-case e (format-message (concat "a%" width "s") "a") (error e))))"#,
    );
}

#[test]
fn bounded_aggregate_conversion_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let ((width (number-to-string most-positive-fixnum))) (list (condition-case e (format (concat "a%" width "d") 'bad) (error e)) (condition-case e (format (concat "a%" width "c") "bad") (error e)) (condition-case e (format (concat "a%" width "q") 1) (error e)) (condition-case e (format (concat "a%" width "d")) (error e))))"#,
    );
}

#[test]
fn bounded_format_saturation_small_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(list (format "%.999999999999999999999999999s" "a") (format "%.18446744073709551616s" "a") (condition-case e (format "%999999999999999999999999999$s" 1) (error e)))"#,
    );
}

#[test]
fn bounded_format_encoding_storage_and_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let* ((raw (unibyte-string 255)) (src (propertize "é" 'face 'bold))) (list (string-to-list (format "[%s]" raw)) (multibyte-string-p (format "[%s]" raw)) (string-to-list (format "[%s]" src)) (get-text-property 1 'face (format "[%s]" src)) (multibyte-string-p (format "%%" "é")) (let ((s (propertize "plain" 'face 'bold))) (eq s (format s))) (let ((s (propertize "text" 'face 'bold))) (eq s (format "%s" s)))))"#,
    );
}

#[test]
fn bounded_bool_vector_bounded_zero_and_truthy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(mapcar (lambda (n) (let ((a (make-bool-vector n nil)) (b (make-bool-vector n 'truthy))) (list (length a) (length b) (if (= n 0) nil (list (aref a 0) (aref a (1- n)) (aref b 0) (aref b (1- n))))))) '(0 1 63 64 65 127 128 129 100000))"#,
    );
}

#[test]
fn bounded_vector_bounded_initialization_and_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let* ((cell (list 'value)) (a (make-vector 3 cell)) (b (make-vector 3 nil))) (aset a 1 'replacement) (list (eq cell (aref a 0)) (eq cell (aref a 2)) (aref a 1) b))"#,
    );
}

#[test]
fn compiled_sequence_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn (require 'bytecomp) (defun gdl--take (n l) (take n l)) (defun gdl--ntake (n l) (ntake n l)) (defun gdl--value (a b) (value< a b)) (byte-compile 'gdl--take) (byte-compile 'gdl--ntake) (byte-compile 'gdl--value) (list (gdl--take (expt 2 100) '(1 2)) (gdl--ntake (- (expt 2 100)) (list 1 2)) (condition-case e (gdl--ntake 2 (cons 1 2)) (error e)) (gdl--value 9007199254740992.0 9007199254740993)))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK ((1 2) nil (wrong-type-argument listp (1 . 2)) t)""#]],
    );
}

#[test]
fn gdl_bounded_memory_binding_survives_public_loader_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let load_root = crate::common::oracle_sandbox::OracleSandbox::create_fixture_tempdir()
        .expect("memory roundtrip fixture directory");
    // The backing layout overflows before allocation. Load inside the dynamic
    // descriptor binding and condition-case, rather than preloading the file.
    std::fs::write(
        load_root.path().join("gdl-oom-roundtrip.el"),
        ";; -*- lexical-binding: nil; -*-\n(make-vector most-positive-fixnum 0)\n",
    )
    .expect("memory roundtrip payload");
    let form = r#"
(progn
  (defvar gdl-roundtrip-hook-count 0)
  (defvar gdl-roundtrip-debug-count 0)
  (let ((gdl-roundtrip-payload
         (expand-file-name "gdl-oom-roundtrip.el"
                           (getenv "NEOVM_ORACLE_LOAD_ROOT"))))
    (mapcar
     (lambda (operation)
       (let* ((memory-signal-data (cons 'error 'gdl-roundtrip-owned-tail))
              (gdl-roundtrip-hook-count 0)
              (gdl-roundtrip-debug-count 0)
              (internal-when-entered-debugger -1)
              (inhibit-debugger nil)
              (debug-ignored-errors nil)
              (debug-on-error t)
              (debug-on-signal t)
              (debugger (lambda (&rest ignored)
                          (setq gdl-roundtrip-debug-count
                                (1+ gdl-roundtrip-debug-count))))
              (signal-hook-function
               (lambda (&rest ignored)
                 (setq gdl-roundtrip-hook-count
                       (1+ gdl-roundtrip-hook-count)))))
         (let ((result
                (condition-case e
                    (cond
                     ((eq operation 'direct)
                      (make-vector most-positive-fixnum 0))
                     ((eq operation 'load)
                      (load gdl-roundtrip-payload nil t t))
                     ((eq operation 'load-file)
                      (load-file gdl-roundtrip-payload))
                     ((eq operation 'require)
                      (require 'gdl-oom-public-roundtrip-audit-20261010
                               gdl-roundtrip-payload)))
                  (error (list e (eq e memory-signal-data))))))
           (list operation result
                 gdl-roundtrip-hook-count gdl-roundtrip-debug-count))))
     '(direct load load-file require))))
"#;
    // Pure live parity uses the existing oracle_gdl harness and its sandbox.
    // No unverified snapshot expectation is installed by this proposal.
    crate::common::assert_oracle_parity_with_load_root(form, &[], load_root.path());
}
