//! Lane GDL: GNU editfns.c numeric conversion and format parsing regressions.
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn gdl_nonfinite_radix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn (require 'bytecomp) (list
(let (out) (dolist (x '(1.0e+INF -1.0e+INF 0.0e+NaN -0.0e+NaN)) (dolist (fmt '("%x" "%o" "%X" "%b" "%B")) (push (condition-case e (format fmt x) (error e)) out))) (list (nreverse out) (condition-case e (format-message "%x" 1.0e+INF) (error e)) (format "%x" 1e30)))
(let ((f (byte-compile (lambda (control value) (format control value))))
                              (m (byte-compile (lambda (control value) (format-message control value))))
                              (out nil))
                         (dolist (value '(1.0e+INF -1.0e+INF 0.0e+NaN -0.0e+NaN))
                           (dolist (control '("%x" "%o" "%X" "%b" "%B"))
                             (push (condition-case e (funcall f control value) (error e)) out)))
                         (list (nreverse out)
                               (condition-case e (funcall m "%x" 1.0e+INF) (error e))
                               (condition-case e (funcall m "%x" 1.0e+INF) (error e))
                               (funcall f "%x" 1e30) (funcall f "%x" 1e30)))))
"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK (((overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error)) (overflow-error) \"c9f2c9cd04675000000000000\")""#
        ]],
    );
}

#[test]
fn gdl_width_bound() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn (require 'bytecomp) (list
(list (condition-case e (format "%2305843009213693952s" "a") (error e)) (condition-case e (format "%9223372036854775807d" 12) (error e)) (condition-case e (format "%99999999999999999999s" "a") (error e)))
(let ((f (byte-compile (lambda (control value) (format control value)))))
                         (list (condition-case e (funcall f "%2305843009213693952s" "a") (error e))
                               (condition-case e (funcall f "%2305843009213693952s" "a") (error e))
                               (condition-case e (funcall f "%9223372036854775807d" 12) (error e))))))
"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\"))""#
        ]],
    );
}

#[test]
fn gdl_saturating_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn (require 'bytecomp) (list
(list (condition-case e (format "%99999999999999999999$s" 1) (error e)) (condition-case e (format "%18446744073709551616$s" 1) (error e)) (condition-case e (format "%99999999999999999999$s %s" 1 2) (error e)) (format "%.99999999999999999999s" "a") (format "%.18446744073709551616s" "a"))
(let ((f (byte-compile (lambda (control value) (format control value)))))
                          (list (condition-case e (funcall f "%99999999999999999999$s" 1) (error e))
                                (condition-case e (funcall f "%18446744073709551616$s" 1) (error e))
                                (funcall f "%.99999999999999999999s" "a")
                                (funcall f "%.18446744073709551616s" "a")))))
"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((error \"Not enough arguments for format string\") (error \"Not enough arguments for format string\") (error \"Not enough arguments for format string\") \"a\" \"a\")""#
        ]],
    );
}

#[test]
fn gdl_bignum_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn (require 'bytecomp) (list
(let ((b (- (expt 2 70)))) (list (format "%.30d" b) (format "%.23d" b) (format "%.30x" b) (format "%#.30x" b) (format "%.5d" -12) (format "%.30d" (- b)) (format "%030d" b)))
(let ((f (byte-compile (lambda (control value) (format control value))))
                             (b (- (expt 2 70))))
                          (list (funcall f "%.30d" b) (funcall f "%.23d" b)
                                (funcall f "%.30x" b) (funcall f "%#.30x" b)
                                (funcall f "%.5d" -12) (funcall f "%.30d" (- b))
                                (funcall f "%030d" b)))))
"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK (\"-00000001180591620717411303424\" \"-1180591620717411303424\" \"-00000000000400000000000000000\" \"-0x00000000000400000000000000000\" \"-00012\" \"000000001180591620717411303424\" \"-00000001180591620717411303424\")""#
        ]],
    );
}

#[test]
fn gdl_decimal_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn (require 'bytecomp) (list
(list (format "%.0d" 0.5) (format "%.0d" -0.0) (format "%5.0d|" 0.3) (format "%+.0d" 0.3) (format "%.0i" 0.9) (format "%.0d" 0) (format "%+d" 1.0e+INF) (format "%05d" -1.0e+INF) (format "%d" -0.0e+NaN) (format "%.3d" 1.0e+INF) (format "%.4d" -0.0e+NaN) (format "%.5d" -12.9) (format "%08d" 0.5))
(let ((f (byte-compile (lambda (control value) (format control value)))))
                         (list (funcall f "%.0d" 0.5) (funcall f "%.0d" -0.0)
                               (funcall f "%5.0d|" 0.3) (funcall f "%+.0i" 0.9)
                               (funcall f "%+d" 1.0e+INF) (funcall f "%05d" -1.0e+INF)
                               (funcall f "%d" -0.0e+NaN) (funcall f "%.3d" 1.0e+INF)
                               (funcall f "%.4d" -0.0e+NaN) (funcall f "%08d" 0.5)))))
"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK (\"0\" \"0\" \"    0|\" \"+0\" \"0\" \"\" \"+inf\" \" -inf\" \"-nan\" \"0inf\" \"-0nan\" \"-00012\" \"00000000\")""#
        ]],
    );
}

#[test]
fn gdl_zero_string_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn (require 'bytecomp) (list
(list (format "%.0s|" "​") (format "%.0s|" "\n") (format "%.0s|" "abc") (format "%3.0s|" (propertize "​" 'face 'bold)) (format "%.1s|" "​a") (format "%.0s|" (unibyte-string 10 255)) (text-properties-at 0 (format "%.0s|" (propertize "​" 'face 'bold))))
(let ((f (byte-compile (lambda (control value) (format control value)))))
                          (list (funcall f "%.0s|" "​") (funcall f "%.0s|" "\n")
                                (funcall f "%.0s|" (unibyte-string 10 255))
                                (funcall f "%3.0s|" (propertize "​" 'face 'bold))
                                (funcall f "%.1s|" "​a")
                                (funcall f (propertize "é%.0s界" 'face 'italic)
                                           (propertize "​" 'face 'bold))
                                (text-properties-at 0 (funcall f "%.0s|" (propertize "​" 'face 'bold)))))))
"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK (\"|\" \"|\" \"|\" \"   |\" \"\u{200b}a|\" \"|\" nil)""#]],
    );
}
