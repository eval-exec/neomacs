//! GNU 31.1 editfns.c:3840-4170 float conversion oracle regressions.
use crate::common::{
    assert_oracle_parity, assert_oracle_parity_expect,
    return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn oracle_gdl_float_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (list (length (format "%.65536f" 0.1)) (length (format "%.70000e" 0.1)) (length (format "%.70000g" 0.1)) (length (format "%.70000f" 5)) (length (format "%#.70000g" 1.0)) (length (format "%.70000f" 1.0e+INF)))
(list (let* ((maximum most-positive-fixnum)
       (width (number-to-string maximum))
       (cases (list
         (list (concat "a%" width "f") 1.0)
         (list (concat "a%." (number-to-string (- maximum 2)) "f") 1.0)
         (list (concat "a%." (number-to-string (- maximum 6)) "e") 1.0)
         (list (concat "a%#." (number-to-string (1- maximum)) "g") 1.0)
         (list (concat "a%" width "f") 1.0e+INF)
         (list (concat "a%" width "f") 'bad)))
       (out nil))
  (dolist (call '(format format-message))
    (dolist (case cases)
      (push (condition-case e (funcall call (car case) (cadr case)) (error e)) out)))
  (nreverse out)) (progn (require 'bytecomp) (let ((run (lambda () (let* ((maximum most-positive-fixnum)
       (width (number-to-string maximum))
       (cases (list
         (list (concat "a%" width "f") 1.0)
         (list (concat "a%." (number-to-string (- maximum 2)) "f") 1.0)
         (list (concat "a%." (number-to-string (- maximum 6)) "e") 1.0)
         (list (concat "a%#." (number-to-string (1- maximum)) "g") 1.0)
         (list (concat "a%" width "f") 1.0e+INF)
         (list (concat "a%" width "f") 'bad)))
       (out nil))
  (dolist (call (list (byte-compile (lambda (control value) (format control value))) (byte-compile (lambda (control value) (format-message control value)))))
    (dolist (case cases)
      (push (condition-case e (funcall call (car case) (cadr case)) (error e)) out)))
  (nreverse out))))) (list (funcall run) (funcall run))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((65538 70006 57 70002 70001 3) (((error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Format specifier doesn’t match argument type\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Format specifier doesn’t match argument type\")) (((error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Format specifier doesn’t match argument type\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Format specifier doesn’t match argument type\")) ((error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Format specifier doesn’t match argument type\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Maximum string size exceeded\") (error \"Format specifier doesn’t match argument type\")))))""#
    ]];
    assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_gdl_float_signs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (format "%+f" -0.0) (format "% g" -0.0) (format "%+e" -0.0) (format "%+f" 1.0e+INF) (format "% f" 0.0e+NaN) (format "%05f|" 1.0e+INF) (format "%+08f|" -1.0e+INF) (format "%010.3e" -0.0e+NaN) (format "%+g" 0.0e+NaN) (format "% 08.3f" 3.14159) (format "%08f" -0.0) (format "%#g" 12.0))"#;
    let expect = expect_test::expect![[
        r#""OK (\"-0.000000\" \"-0\" \"-0.000000e+00\" \"+inf\" \" nan\" \"  inf|\" \"    -inf|\" \"      -nan\" \"+nan\" \" 003.142\" \"-0.000000\" \"12.0000\")""#
    ]];
    assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_gdl_float_integers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (format "%.0f" 9007199254740993) (format "%.0f" (1- (expt 2 64))) (format "%.20e" most-positive-fixnum) (format "%f" most-positive-fixnum) (format "%.20g" (1- (expt 2 63))) (format "%.0f" (- 1 (expt 2 63))) (format "%.0f" (expt 2 64)) (format "%.0f" (1+ (expt 2 64))) (format "%.0f" (- (expt 2 63))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"9007199254740993\" \"18446744073709551615\" \"2.30584300921369395100e+18\" \"2305843009213693951.000000\" \"9223372036854775807\" \"-9223372036854775807\" \"18446744073709551616\" \"18446744073709551616\" \"-9223372036854775808\")""#
    ]];
    assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_gdl_float_byte_compiled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
      (require 'bytecomp)
      (let ((f (byte-compile (lambda (control value) (format control value)))))
        (list (funcall f "%+f" -0.0)
              (funcall f "%05f" 1.0e+INF)
              (funcall f "%.0f" 9007199254740993)
              (funcall f "%.20g" (1- (expt 2 63)))
              (funcall f "% 08.3f" 3.14159)
              (length (funcall f "%.70000e" 0.1)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"-0.000000\" \"  inf\" \"9007199254740993\" \"9223372036854775807\" \" 003.142\" 70006)""#
    ]];
    assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_gdl_ascii_char_aggregate_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let ((memory-signal-data (quote (error formatter-storage-exhausted)))) (condition-case e (format (concat "a%" (number-to-string (1- most-positive-fixnum)) "c") 10) (error e)))"#,
    );
}

#[test]
fn oracle_gdl_ascii_char_width_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(let ((char-width-table (copy-sequence char-width-table))) (aset char-width-table ?a 0) (aset char-width-table ?é 7) (list (format "%3c" ?a) (format "%3c" ?é)))"#,
    );
}

#[test]
fn oracle_gdl_ascii_char_excess_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r#"(list (length (format "%.20000c" ?a)) (substring (format "%.20000c" ?a) 0 3) (substring (format "%.20000c" ?a) -1))"#,
    );
}

#[test]
fn oracle_gdl_ascii_char_zero_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(r#"(list (format "%.0c|" ?a) (format "%.1c|" ?a) (format "%+05c|" ?a))"#);
}
