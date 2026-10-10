//! GNU editfns.c:3840-4170 float-format regressions. Fixtures come from GNU only.

const CASES: &[(&str, &str, &str)] = &[
    (
        "precision",
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
        include_str!("format_float_gnu/precision.expect"),
    ),
    (
        "signs",
        r#"(list (format "%+f" -0.0) (format "% g" -0.0) (format "%+e" -0.0) (format "%+f" 1.0e+INF) (format "% f" 0.0e+NaN) (format "%05f|" 1.0e+INF) (format "%+08f|" -1.0e+INF) (format "%010.3e" -0.0e+NaN) (format "%+g" 0.0e+NaN) (format "% 08.3f" 3.14159) (format "%08f" -0.0) (format "%#g" 12.0))"#,
        include_str!("format_float_gnu/signs.expect"),
    ),
    (
        "integers",
        r#"(list (format "%.0f" 9007199254740993) (format "%.0f" (1- (expt 2 64))) (format "%.20e" most-positive-fixnum) (format "%f" most-positive-fixnum) (format "%.20g" (1- (expt 2 63))) (format "%.0f" (- 1 (expt 2 63))) (format "%.0f" (expt 2 64)) (format "%.0f" (1+ (expt 2 64))) (format "%.0f" (- (expt 2 63))))"#,
        include_str!("format_float_gnu/integers.expect"),
    ),
];

fn oracle(_name: &str, _program: &str, frozen: &str) -> String {
    frozen.trim_end().to_owned()
}

fn assert_case(index: usize) {
    crate::test_utils::init_test_tracing();
    let (name, form, frozen) = CASES[index];
    let expected = oracle(name, form, frozen);
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(form),
        format!("OK {expected}")
    );
}

#[test]
fn float_format_precision_above_u16_matches_gnu() {
    assert_case(0);
}
#[test]
fn float_format_signs_and_padding_match_gnu() {
    assert_case(1);
}
#[test]
fn float_format_integer_precision_matches_gnu() {
    assert_case(2);
}
