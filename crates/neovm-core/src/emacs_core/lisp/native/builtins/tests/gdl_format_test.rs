//! GNU 31.1 format regressions from retained successful oracle receipts.

fn assert_gnu(_name: &str, form: &str, frozen: &str) {
    crate::test_utils::init_test_tracing();
    let expected = frozen.trim_end_matches('\n').to_owned();
    // This fixture starts from a preload snapshot, not CLI normal-top-level.
    // Supply GNU lisp/startup.el:1728-1730's command-line memory-message setup here;
    // substitute-command-keys supplies the real key binding and text properties.
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
        format!("OK {expected}")
    );
}

#[test]
fn gdl_nonfinite_radix() {
    assert_gnu(
        "gdl_nonfinite_radix",
        r#"(let (out) (dolist (x '(1.0e+INF -1.0e+INF 0.0e+NaN -0.0e+NaN)) (dolist (fmt '("%x" "%o" "%X" "%b" "%B")) (push (condition-case e (format fmt x) (error e)) out))) (list (nreverse out) (condition-case e (format-message "%x" 1.0e+INF) (error e)) (format "%x" 1e30)))"#,
        include_str!("gdl_format/gdl_nonfinite_radix.expect"),
    );
}

#[test]
fn gdl_width_bound() {
    assert_gnu(
        "gdl_width_bound",
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
        include_str!("gdl_format/gdl_width_bound.expect"),
    );
}

#[test]
fn gdl_saturating_counts() {
    assert_gnu(
        "gdl_saturating_counts",
        r#"(list (condition-case e (format "%99999999999999999999$s" 1) (error e)) (condition-case e (format "%18446744073709551616$s" 1) (error e)) (condition-case e (format "%99999999999999999999$s %s" 1 2) (error e)) (format "%.99999999999999999999s" "a") (format "%.18446744073709551616s" "a"))"#,
        include_str!("gdl_format/gdl_saturating_counts.expect"),
    );
}

#[test]
fn gdl_bignum_precision() {
    assert_gnu(
        "gdl_bignum_precision",
        r#"(let ((b (- (expt 2 70)))) (list (format "%.30d" b) (format "%.23d" b) (format "%.30x" b) (format "%#.30x" b) (format "%.5d" -12) (format "%.30d" (- b)) (format "%030d" b)))"#,
        include_str!("gdl_format/gdl_bignum_precision.expect"),
    );
}

#[test]
fn gdl_decimal_float() {
    assert_gnu(
        "gdl_decimal_float",
        r#"(list (list (format "%.0d" 0.5) (format "%.0d" -0.0) (format "%5.0d|" 0.3) (format "%+.0d" 0.3) (format "%.0i" 0.9) (format "%.0d" 0) (format "%+d" 1.0e+INF) (format "%05d" -1.0e+INF) (format "%d" -0.0e+NaN) (format "%.3d" 1.0e+INF) (format "%.4d" -0.0e+NaN) (format "%.5d" -12.9) (format "%08d" 0.5))
(let ((out nil) (width (number-to-string most-positive-fixnum)))
  (dolist (call '(format format-message))
    (dolist (case (list (list (concat "a%" width "d") 1.0)
                       (list (concat "a%." width "d") 1.0)
                       (list (concat "a%" width "i") -1.0e+INF)
                       (list (concat "a%" width "x") 1.0e+INF)))
      (push (condition-case e (funcall call (car case) (cadr case)) (error e)) out)))
  (nreverse out)))"#,
        include_str!("gdl_format/gdl_decimal_float.expect"),
    );
}

#[test]
fn gdl_zero_string_precision() {
    assert_gnu(
        "gdl_zero_string_precision",
        r#"(list (format "%.0s|" "​") (format "%.0s|" "\n") (format "%.0s|" "abc") (format "%3.0s|" (propertize "​" 'face 'bold)) (format "%.1s|" "​a") (format "%.0s|" (unibyte-string 10 255)) (text-properties-at 0 (format "%.0s|" (propertize "​" 'face 'bold))))"#,
        include_str!("gdl_format/gdl_zero_string_precision.expect"),
    );
}

#[cfg(test)]
#[path = "gdl_format_revival_test.rs"]
mod revived_fixtures;
