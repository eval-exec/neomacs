// Frozen GNU 31.1 exit-0 fixtures; provenance is in the gdL scratch report.
// No oracle updater or external process is executed by these tests.
fn assert_case(form: &str, frozen: &str) {
    super::assert_gnu("revived-fixture", form, frozen.trim_end_matches('\n'));
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_format__gdl_nonfinite_radix() {
    assert_case(
        r#"(let (out) (dolist (x '(1.0e+INF -1.0e+INF 0.0e+NaN -0.0e+NaN)) (dolist (fmt '("%x" "%o" "%X" "%b" "%B")) (push (condition-case e (format fmt x) (error e)) out))) (list (nreverse out) (condition-case e (format-message "%x" 1.0e+INF) (error e)) (format "%x" 1e30)))"#,
        include_str!("gdl_format_revival/gdl_format--gdl_nonfinite_radix.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_format__gdl_width_bound() {
    assert_case(
        r#"(list (list (condition-case e (format "%2305843009213693952s" "a") (error e)) (condition-case e (format "%9223372036854775807d" 12) (error e)) (condition-case e (format "%99999999999999999999s" "a") (error e)))
(let* ((maximum most-positive-fixnum)
       (width (number-to-string maximum))
       (lower (number-to-string (1- maximum)))
       (big (expt 2 100))
       (cases (list
         (list (concat "a%" width "s") "a")
         (list (concat "a%" width "s") (unibyte-string 255))
         (list (concat "%" width "s") "é")
         (list (concat "a%" width "s") "")
         (list (concat "a%-" width "s") "a")
         (list (concat "a%" width "S") "a")
         (list (concat "a%" width "c") ?a)
         (list (concat "a%" width "d") 1)
         (list (concat "a%" width "i") -1)
         (list (concat "a%." width "d") 1)
         (list (concat "a%." width "d") big)
         (list (concat "a%#." width "x") big)
         (list (concat "é%" lower "s") "a")
         (list (string-as-unibyte (concat (unibyte-string 255) "%" lower "s")) "é")
         (list (string-as-unibyte (concat (unibyte-string 255) "%" lower ".1s")) (list "é"))
         (list (string-as-unibyte (concat (unibyte-string 255) "%" lower ".1S")) (list "é"))
         (list (propertize (concat "a%" width "s") 'face 'bold) "a")
         (list (concat "a%" width "d") 'bad)
         (list (concat "a%" width "c") big)
         (list (concat "a%" width "q") 1)))
       (out nil))
  (dolist (call '(format format-message))
    (dolist (case cases)
      (push (condition-case e (funcall call (car case) (cadr case)) (error e)) out))
    (push (condition-case e (funcall call (concat "%s%" lower "s") "aa" "a") (error e)) out)
    (push (condition-case e (funcall call (concat "a%" width "d")) (error e)) out))
  (list (nreverse out)
        (let ((memory-signal-data '(error formatter-storage-exhausted)))
          (list
            (let (errors)
              (dolist (call '(format format-message))
                (push (condition-case e (funcall call (concat "%" width "s") (unibyte-string 255)) (error e)) errors))
              (nreverse errors))
            (condition-case e (format (concat "%" width "s") (string-to-multibyte (unibyte-string 255))) (error e))))
        (condition-case e (format (concat "%" width "sa") "a") (error e))
        (condition-case e (format (string-as-unibyte (concat (unibyte-string 255) "%" lower "s")) "a") (error e)))))"#,
        include_str!("gdl_format_revival/gdl_format--gdl_width_bound.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_format__gdl_saturating_counts() {
    assert_case(
        r#"(list (condition-case e (format "%99999999999999999999$s" 1) (error e)) (condition-case e (format "%18446744073709551616$s" 1) (error e)) (condition-case e (format "%99999999999999999999$s %s" 1 2) (error e)) (format "%.99999999999999999999s" "a") (format "%.18446744073709551616s" "a"))"#,
        include_str!("gdl_format_revival/gdl_format--gdl_saturating_counts.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_format__gdl_bignum_precision() {
    assert_case(
        r#"(let ((b (- (expt 2 70)))) (list (format "%.30d" b) (format "%.23d" b) (format "%.30x" b) (format "%#.30x" b) (format "%.5d" -12) (format "%.30d" (- b)) (format "%030d" b)))"#,
        include_str!("gdl_format_revival/gdl_format--gdl_bignum_precision.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_format__gdl_decimal_float() {
    assert_case(
        r#"(list (list (format "%.0d" 0.5) (format "%.0d" -0.0) (format "%5.0d|" 0.3) (format "%+.0d" 0.3) (format "%.0i" 0.9) (format "%.0d" 0) (format "%+d" 1.0e+INF) (format "%05d" -1.0e+INF) (format "%d" -0.0e+NaN) (format "%.3d" 1.0e+INF) (format "%.4d" -0.0e+NaN) (format "%.5d" -12.9) (format "%08d" 0.5))
(let ((out nil) (width (number-to-string most-positive-fixnum)))
  (dolist (call '(format format-message))
    (dolist (case (list (list (concat "a%" width "d") 1.0)
                       (list (concat "a%." width "d") 1.0)
                       (list (concat "a%" width "i") -1.0e+INF)
                       (list (concat "a%" width "x") 1.0e+INF)))
      (push (condition-case e (funcall call (car case) (cadr case)) (error e)) out)))
  (nreverse out)))"#,
        include_str!("gdl_format_revival/gdl_format--gdl_decimal_float.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_format__gdl_zero_string_precision() {
    assert_case(
        r#"(list (format "%.0s|" "​") (format "%.0s|" "\n") (format "%.0s|" "abc") (format "%3.0s|" (propertize "​" 'face 'bold)) (format "%.1s|" "​a") (format "%.0s|" (unibyte-string 10 255)) (text-properties-at 0 (format "%.0s|" (propertize "​" 'face 'bold))))"#,
        include_str!("gdl_format_revival/gdl_format--gdl_zero_string_precision.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn format_float_gnu__precision() {
    assert_case(
        r#"(list (list (length (format "%.65536f" 0.1)) (length (format "%.70000e" 0.1)) (length (format "%.70000g" 0.1)) (length (format "%.70000f" 5)) (length (format "%#.70000g" 1.0)) (length (format "%.70000f" 1.0e+INF)))
(let* ((maximum most-positive-fixnum)
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
  (nreverse out)))"#,
        include_str!("gdl_format_revival/format_float_gnu--precision.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn format_float_gnu__signs() {
    assert_case(
        r#"(list (format "%+f" -0.0) (format "% g" -0.0) (format "%+e" -0.0) (format "%+f" 1.0e+INF) (format "% f" 0.0e+NaN) (format "%05f|" 1.0e+INF) (format "%+08f|" -1.0e+INF) (format "%010.3e" -0.0e+NaN) (format "%+g" 0.0e+NaN) (format "% 08.3f" 3.14159) (format "%08f" -0.0) (format "%#g" 12.0))"#,
        include_str!("gdl_format_revival/format_float_gnu--signs.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn format_float_gnu__integers() {
    assert_case(
        r#"(list (format "%.0f" 9007199254740993) (format "%.0f" (1- (expt 2 64))) (format "%.20e" most-positive-fixnum) (format "%f" most-positive-fixnum) (format "%.20g" (1- (expt 2 63))) (format "%.0f" (- 1 (expt 2 63))) (format "%.0f" (expt 2 64)) (format "%.0f" (1+ (expt 2 64))) (format "%.0f" (- (expt 2 63))))"#,
        include_str!("gdl_format_revival/format_float_gnu--integers.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_integer_oracle__gdl_nonfinite_radix() {
    assert_case(
        r#"
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
"#,
        include_str!("gdl_format_revival/gdl_integer_oracle--gdl_nonfinite_radix.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_integer_oracle__gdl_width_bound() {
    assert_case(
        r#"(list
(progn (require 'bytecomp) (list
(list (condition-case e (format "%2305843009213693952s" "a") (error e)) (condition-case e (format "%9223372036854775807d" 12) (error e)) (condition-case e (format "%99999999999999999999s" "a") (error e)))
(let ((f (byte-compile (lambda (control value) (format control value)))))
                         (list (condition-case e (funcall f "%2305843009213693952s" "a") (error e))
                               (condition-case e (funcall f "%2305843009213693952s" "a") (error e))
                               (condition-case e (funcall f "%9223372036854775807d" 12) (error e))))))

(list (let* ((maximum most-positive-fixnum)
       (width (number-to-string maximum))
       (lower (number-to-string (1- maximum)))
       (big (expt 2 100))
       (cases (list
         (list (concat "a%" width "s") "a")
         (list (concat "a%" width "s") (unibyte-string 255))
         (list (concat "%" width "s") "é")
         (list (concat "a%" width "s") "")
         (list (concat "a%-" width "s") "a")
         (list (concat "a%" width "S") "a")
         (list (concat "a%" width "c") ?a)
         (list (concat "a%" width "d") 1)
         (list (concat "a%" width "i") -1)
         (list (concat "a%." width "d") 1)
         (list (concat "a%." width "d") big)
         (list (concat "a%#." width "x") big)
         (list (concat "é%" lower "s") "a")
         (list (string-as-unibyte (concat (unibyte-string 255) "%" lower "s")) "é")
         (list (string-as-unibyte (concat (unibyte-string 255) "%" lower ".1s")) (list "é"))
         (list (string-as-unibyte (concat (unibyte-string 255) "%" lower ".1S")) (list "é"))
         (list (propertize (concat "a%" width "s") 'face 'bold) "a")
         (list (concat "a%" width "d") 'bad)
         (list (concat "a%" width "c") big)
         (list (concat "a%" width "q") 1)))
       (out nil))
  (dolist (call '(format format-message))
    (dolist (case cases)
      (push (condition-case e (funcall call (car case) (cadr case)) (error e)) out))
    (push (condition-case e (funcall call (concat "%s%" lower "s") "aa" "a") (error e)) out)
    (push (condition-case e (funcall call (concat "a%" width "d")) (error e)) out))
  (list (nreverse out)
        (let ((memory-signal-data '(error formatter-storage-exhausted)))
          (list
            (let (errors)
              (dolist (call '(format format-message))
                (push (condition-case e (funcall call (concat "%" width "s") (unibyte-string 255)) (error e)) errors))
              (nreverse errors))
            (condition-case e (format (concat "%" width "s") (string-to-multibyte (unibyte-string 255))) (error e))))
        (condition-case e (format (concat "%" width "sa") "a") (error e))
        (condition-case e (format (string-as-unibyte (concat (unibyte-string 255) "%" lower "s")) "a") (error e)))) (progn (require 'bytecomp) (let ((run (lambda () (let* ((maximum most-positive-fixnum)
       (width (number-to-string maximum))
       (lower (number-to-string (1- maximum)))
       (big (expt 2 100))
       (cases (list
         (list (concat "a%" width "s") "a")
         (list (concat "a%" width "s") (unibyte-string 255))
         (list (concat "%" width "s") "é")
         (list (concat "a%" width "s") "")
         (list (concat "a%-" width "s") "a")
         (list (concat "a%" width "S") "a")
         (list (concat "a%" width "c") ?a)
         (list (concat "a%" width "d") 1)
         (list (concat "a%" width "i") -1)
         (list (concat "a%." width "d") 1)
         (list (concat "a%." width "d") big)
         (list (concat "a%#." width "x") big)
         (list (concat "é%" lower "s") "a")
         (list (string-as-unibyte (concat (unibyte-string 255) "%" lower "s")) "é")
         (list (string-as-unibyte (concat (unibyte-string 255) "%" lower ".1s")) (list "é"))
         (list (string-as-unibyte (concat (unibyte-string 255) "%" lower ".1S")) (list "é"))
         (list (propertize (concat "a%" width "s") 'face 'bold) "a")
         (list (concat "a%" width "d") 'bad)
         (list (concat "a%" width "c") big)
         (list (concat "a%" width "q") 1)))
       (out nil))
  (dolist (call (list (byte-compile (lambda (control &rest values) (apply #'format control values))) (byte-compile (lambda (control &rest values) (apply #'format-message control values)))))
    (dolist (case cases)
      (push (condition-case e (funcall call (car case) (cadr case)) (error e)) out))
    (push (condition-case e (funcall call (concat "%s%" lower "s") "aa" "a") (error e)) out)
    (push (condition-case e (funcall call (concat "a%" width "d")) (error e)) out))
  (list (nreverse out)
        (let ((memory-signal-data '(error formatter-storage-exhausted)))
          (list
            (let (errors)
              (dolist (call (list (byte-compile (lambda (control &rest values) (apply #'format control values))) (byte-compile (lambda (control &rest values) (apply #'format-message control values)))))
                (push (condition-case e (funcall call (concat "%" width "s") (unibyte-string 255)) (error e)) errors))
              (nreverse errors))
            (condition-case e (format (concat "%" width "s") (string-to-multibyte (unibyte-string 255))) (error e))))
        (condition-case e (format (concat "%" width "sa") "a") (error e))
        (condition-case e (format (string-as-unibyte (concat (unibyte-string 255) "%" lower "s")) "a") (error e))))))) (list (funcall run) (funcall run))))))"#,
        include_str!("gdl_format_revival/gdl_integer_oracle--gdl_width_bound.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_integer_oracle__gdl_saturating_counts() {
    assert_case(
        r#"
(progn (require 'bytecomp) (list
(list (condition-case e (format "%99999999999999999999$s" 1) (error e)) (condition-case e (format "%18446744073709551616$s" 1) (error e)) (condition-case e (format "%99999999999999999999$s %s" 1 2) (error e)) (format "%.99999999999999999999s" "a") (format "%.18446744073709551616s" "a"))
(let ((f (byte-compile (lambda (control value) (format control value)))))
                          (list (condition-case e (funcall f "%99999999999999999999$s" 1) (error e))
                                (condition-case e (funcall f "%18446744073709551616$s" 1) (error e))
                                (funcall f "%.99999999999999999999s" "a")
                                (funcall f "%.18446744073709551616s" "a")))))
"#,
        include_str!("gdl_format_revival/gdl_integer_oracle--gdl_saturating_counts.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_integer_oracle__gdl_bignum_precision() {
    assert_case(
        r#"
(progn (require 'bytecomp) (list
(let ((b (- (expt 2 70)))) (list (format "%.30d" b) (format "%.23d" b) (format "%.30x" b) (format "%#.30x" b) (format "%.5d" -12) (format "%.30d" (- b)) (format "%030d" b)))
(let ((f (byte-compile (lambda (control value) (format control value))))
                             (b (- (expt 2 70))))
                          (list (funcall f "%.30d" b) (funcall f "%.23d" b)
                                (funcall f "%.30x" b) (funcall f "%#.30x" b)
                                (funcall f "%.5d" -12) (funcall f "%.30d" (- b))
                                (funcall f "%030d" b)))))
"#,
        include_str!("gdl_format_revival/gdl_integer_oracle--gdl_bignum_precision.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_integer_oracle__gdl_decimal_float() {
    assert_case(
        r#"(list
(progn (require 'bytecomp) (list
(list (format "%.0d" 0.5) (format "%.0d" -0.0) (format "%5.0d|" 0.3) (format "%+.0d" 0.3) (format "%.0i" 0.9) (format "%.0d" 0) (format "%+d" 1.0e+INF) (format "%05d" -1.0e+INF) (format "%d" -0.0e+NaN) (format "%.3d" 1.0e+INF) (format "%.4d" -0.0e+NaN) (format "%.5d" -12.9) (format "%08d" 0.5))
(let ((f (byte-compile (lambda (control value) (format control value)))))
                         (list (funcall f "%.0d" 0.5) (funcall f "%.0d" -0.0)
                               (funcall f "%5.0d|" 0.3) (funcall f "%+.0i" 0.9)
                               (funcall f "%+d" 1.0e+INF) (funcall f "%05d" -1.0e+INF)
                               (funcall f "%d" -0.0e+NaN) (funcall f "%.3d" 1.0e+INF)
                               (funcall f "%.4d" -0.0e+NaN) (funcall f "%08d" 0.5)))))

(list (let ((out nil) (width (number-to-string most-positive-fixnum)))
  (dolist (call '(format format-message))
    (dolist (case (list (list (concat "a%" width "d") 1.0)
                       (list (concat "a%." width "d") 1.0)
                       (list (concat "a%" width "i") -1.0e+INF)
                       (list (concat "a%" width "x") 1.0e+INF)))
      (push (condition-case e (funcall call (car case) (cadr case)) (error e)) out)))
  (nreverse out)) (progn (require 'bytecomp) (let ((out nil) (width (number-to-string most-positive-fixnum)))
  (dolist (call (list (byte-compile (lambda (control value) (format control value))) (byte-compile (lambda (control value) (format-message control value)))))
    (dolist (case (list (list (concat "a%" width "d") 1.0)
                       (list (concat "a%." width "d") 1.0)
                       (list (concat "a%" width "i") -1.0e+INF)
                       (list (concat "a%" width "x") 1.0e+INF)))
      (push (condition-case e (funcall call (car case) (cadr case)) (error e)) out)))
  (nreverse out)))))"#,
        include_str!("gdl_format_revival/gdl_integer_oracle--gdl_decimal_float.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_integer_oracle__gdl_zero_string_precision() {
    assert_case(
        r#"
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
"#,
        include_str!("gdl_format_revival/gdl_integer_oracle--gdl_zero_string_precision.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_float_oracle__oracle_gdl_float_precision() {
    assert_case(
        r#"(list (list (length (format "%.65536f" 0.1)) (length (format "%.70000e" 0.1)) (length (format "%.70000g" 0.1)) (length (format "%.70000f" 5)) (length (format "%#.70000g" 1.0)) (length (format "%.70000f" 1.0e+INF)))
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
  (nreverse out))))) (list (funcall run) (funcall run))))))"#,
        include_str!("gdl_format_revival/gdl_float_oracle--oracle_gdl_float_precision.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_float_oracle__oracle_gdl_float_signs() {
    assert_case(
        r#"(list (format "%+f" -0.0) (format "% g" -0.0) (format "%+e" -0.0) (format "%+f" 1.0e+INF) (format "% f" 0.0e+NaN) (format "%05f|" 1.0e+INF) (format "%+08f|" -1.0e+INF) (format "%010.3e" -0.0e+NaN) (format "%+g" 0.0e+NaN) (format "% 08.3f" 3.14159) (format "%08f" -0.0) (format "%#g" 12.0))"#,
        include_str!("gdl_format_revival/gdl_float_oracle--oracle_gdl_float_signs.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_float_oracle__oracle_gdl_float_integers() {
    assert_case(
        r#"(list (format "%.0f" 9007199254740993) (format "%.0f" (1- (expt 2 64))) (format "%.20e" most-positive-fixnum) (format "%f" most-positive-fixnum) (format "%.20g" (1- (expt 2 63))) (format "%.0f" (- 1 (expt 2 63))) (format "%.0f" (expt 2 64)) (format "%.0f" (1+ (expt 2 64))) (format "%.0f" (- (expt 2 63))))"#,
        include_str!("gdl_format_revival/gdl_float_oracle--oracle_gdl_float_integers.expect"),
    );
}

// Existing successful GNU oracle fixture; see successful-fixtures.json
#[test]
fn gdl_float_oracle__oracle_gdl_float_byte_compiled() {
    assert_case(
        r#"(progn
      (require 'bytecomp)
      (let ((f (byte-compile (lambda (control value) (format control value)))))
        (list (funcall f "%+f" -0.0)
              (funcall f "%05f" 1.0e+INF)
              (funcall f "%.0f" 9007199254740993)
              (funcall f "%.20g" (1- (expt 2 63)))
              (funcall f "% 08.3f" 3.14159)
              (length (funcall f "%.70000e" 0.1)))))"#,
        include_str!("gdl_format_revival/gdl_float_oracle--oracle_gdl_float_byte_compiled.expect"),
    );
}

// GNU bignum.c:78-86; editfns.c:4016-4029
#[test]
fn minimal__nonfinite_radix() {
    assert_case(
        r#"(condition-case e (format "%x" 1.0e+INF) (error e))"#,
        include_str!("gdl_format_revival/minimal--nonfinite-radix.expect"),
    );
}

// GNU editfns.c:3332-3340,3599-3606,3656-3658
#[test]
fn minimal__saturating_field() {
    assert_case(
        r#"(condition-case e (format "%18446744073709551616$s" 1) (error e))"#,
        include_str!("gdl_format_revival/minimal--saturating-field.expect"),
    );
}

// GNU editfns.c:3332-3340,3637-3640,3732-3760
#[test]
fn minimal__saturating_string_precision() {
    assert_case(
        r#"(format "%.18446744073709551616s" "a")"#,
        include_str!("gdl_format_revival/minimal--saturating-string-precision.expect"),
    );
}

// GNU editfns.c:3632-3634; lisp.h:1621-1622
#[test]
fn minimal__width_limit() {
    assert_case(
        r#"(condition-case e (format "%2305843009213693952s" "a") (error e))"#,
        include_str!("gdl_format_revival/minimal--width-limit.expect"),
    );
}

// GNU editfns.c:3767-3769,4245-4252
#[test]
fn minimal__aggregate_string_budget() {
    assert_case(
        r#"(condition-case e (format (concat "a%" (number-to-string most-positive-fixnum) "s") "a") (error e))"#,
        include_str!("gdl_format_revival/minimal--aggregate-string-budget.expect"),
    );
}

// GNU editfns.c:4062-4092,4245-4252
#[test]
fn minimal__aggregate_numeric_budget() {
    assert_case(
        r#"(condition-case e (format (concat "a%." (number-to-string most-positive-fixnum) "d") 1) (error e))"#,
        include_str!("gdl_format_revival/minimal--aggregate-numeric-budget.expect"),
    );
}

// GNU editfns.c:3817-3832,3834-3840,4083-4092
#[test]
fn minimal__type_before_budget() {
    assert_case(
        r#"(condition-case e (format (concat "a%" (number-to-string most-positive-fixnum) "d") (quote bad)) (error e))"#,
        include_str!("gdl_format_revival/minimal--type-before-budget.expect"),
    );
}

// GNU editfns.c:4258-4263; alloc.c:4140-4142
#[test]
fn minimal__storage_live_signal() {
    assert_case(
        r#"(let ((memory-signal-data (quote (error formatter-storage-exhausted)))) (condition-case e (format (concat "%" (number-to-string most-positive-fixnum) "s") (unibyte-string 255)) (error e)))"#,
        include_str!("gdl_format_revival/minimal--storage-live-signal.expect"),
    );
}

// GNU editfns.c:3954-3982,4062-4080
#[test]
fn minimal__negative_bignum_precision() {
    assert_case(
        r#"(format "%.23d" (- (expt 2 70)))"#,
        include_str!("gdl_format_revival/minimal--negative-bignum-precision.expect"),
    );
}

// GNU editfns.c:3985-4003
#[test]
fn minimal__decimal_float_zero() {
    assert_case(
        r#"(list (format "%.0d" 0.5) (format "%.0d" -0.0) (format "%.0d" 0))"#,
        include_str!("gdl_format_revival/minimal--decimal-float-zero.expect"),
    );
}

// GNU editfns.c:3992-4003,4062-4092,4108-4113
#[test]
fn minimal__decimal_nonfinite_padding() {
    assert_case(
        r#"(list (format "%+d" 1.0e+INF) (format "%05d" -1.0e+INF) (format "%.4d" -0.0e+NaN))"#,
        include_str!("gdl_format_revival/minimal--decimal-nonfinite-padding.expect"),
    );
}

// GNU editfns.c:3735-3747
#[test]
fn minimal__zero_string_precision() {
    assert_case(
        r#"(format "%.0s|" (string #x200b))"#,
        include_str!("gdl_format_revival/minimal--zero-string-precision.expect"),
    );
}

// GNU editfns.c:3442-3454,3867-3868,4069-4078
#[test]
fn minimal__float_large_precision() {
    assert_case(
        r#"(length (format "%.65536f" 0.1))"#,
        include_str!("gdl_format_revival/minimal--float-large-precision.expect"),
    );
}

// GNU editfns.c:3889-3946,4108-4113
#[test]
fn minimal__float_signs() {
    assert_case(
        r#"(list (format "%+f" -0.0) (format "%+f" 1.0e+INF) (format "%05f|" 1.0e+INF))"#,
        include_str!("gdl_format_revival/minimal--float-signs.expect"),
    );
}

// GNU editfns.c:3892-3946
#[test]
fn minimal__integer_long_double() {
    assert_case(
        r#"(format "%.0f" 9007199254740993)"#,
        include_str!("gdl_format_revival/minimal--integer-long-double.expect"),
    );
}

// GNU editfns.c:3947-3953 ASCII c single byte,4069-4092,4245-4263
#[test]
fn minimal__ascii_char_aggregate_order() {
    assert_case(
        r#"(let ((memory-signal-data (quote (error formatter-storage-exhausted)))) (condition-case e (format (concat "a%" (number-to-string (1- most-positive-fixnum)) "c") 10) (error e)))"#,
        include_str!("gdl_format_revival/minimal--ascii-char-aggregate-order.expect"),
    );
}

// GNU editfns.c:3688-3705,3947-3953
#[test]
fn minimal__ascii_char_width_policy() {
    assert_case(
        r#"(let ((char-width-table (copy-sequence char-width-table))) (aset char-width-table ?a 0) (aset char-width-table ?é 7) (list (format "%3c" ?a) (format "%3c" ?é)))"#,
        include_str!("gdl_format_revival/minimal--ascii-char-width-policy.expect"),
    );
}

// GNU editfns.c:3867-3868,3947-3953,4069-4080
#[test]
fn minimal__ascii_char_excess_precision() {
    assert_case(
        r#"(list (length (format "%.20000c" ?a)) (substring (format "%.20000c" ?a) 0 3) (substring (format "%.20000c" ?a) -1))"#,
        include_str!("gdl_format_revival/minimal--ascii-char-excess-precision.expect"),
    );
}

// GNU editfns.c:3704,3947-3953,4108-4113
#[test]
fn minimal__ascii_char_zero_precision() {
    assert_case(
        r#"(list (format "%.0c|" ?a) (format "%.1c|" ?a) (format "%+05c|" ?a))"#,
        include_str!("gdl_format_revival/minimal--ascii-char-zero-precision.expect"),
    );
}
