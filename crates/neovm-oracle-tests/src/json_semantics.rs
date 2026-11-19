//! Oracle parity tests for GNU JSON primitive option semantics.
//!
//! GNU implements `json-serialize` and `json-parse-string` in `src/json.c`.
//! Its `json_parse_args` walks keyword pairs from right to left so duplicate
//! keyword values that appear first take precedence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_json_duplicate_keyword_options_use_first_value_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (json-serialize :a :null-object :a :null-object :b)
   (error (list (car err) (cdr err))))
 (condition-case err
     (json-serialize :b :null-object :a :null-object :b)
   (error (list (car err) (cdr err))))
 (condition-case err
     (json-parse-string
      "[null,false]"
      :array-type 'list
      :null-object 'first-null
      :null-object 'second-null
      :false-object 'first-false
      :false-object 'second-false)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"null\" (wrong-type-argument (json-value-p :b)) (first-null first-false))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_json_serialize_default_null_and_false_sentinels_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (json-serialize nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (json-serialize :null)
   (error (list (car err) (cdr err))))
 (condition-case err
     (json-serialize :false)
   (error (list (car err) (cdr err))))
 (condition-case err
     (json-serialize :json-false)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"{}\" \"null\" \"false\" (wrong-type-argument (json-value-p :json-false)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// GNU 31.1 has no `json-serialize-error`: serializer failures use plain
/// `error` or `wrong-type-argument` conditions. The depth limit is
/// `json_out_nest`'s `maxdepth = 50` (the 51st container errors), and the
/// symbol is not interned at boot.
#[test]
fn oracle_json_serialize_error_conditions_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((nested (lambda (n) (let ((v 1)) (dotimes (_ n) (setq v (vector v))) v))))
  (list
   (condition-case e (length (json-serialize (funcall nested 50)))
     (error (list (car e) (cdr e))))
   (condition-case e (json-serialize (funcall nested 51))
     (error (list (car e) (cdr e))))
   (condition-case e (json-serialize 0.0e+NaN)
     (error (list (car e) (cdr e))))
   (condition-case e (json-serialize 1.0e+INF)
     (error (list (car e) (cdr e))))
   (condition-case e (json-serialize '(1 2))
     (error (list (car e) (cdr e))))
   (condition-case e (json-serialize '(a . 1))
     (error (list (car e) (cdr e))))
   (condition-case e (json-serialize '((a . 1) . 2))
     (error (list (car e) (cdr e))))
   (condition-case e (json-serialize (make-marker))
     (error (list (car e) (cdr e))))
   (intern-soft "json-serialize-error")
   (and (intern-soft "json-error") t)
   (condition-case e (with-temp-buffer (json-insert (funcall nested 51)))
     (error (list (car e) (cdr e))))
   (condition-case e (with-temp-buffer (json-insert 0.0e+NaN))
     (error (list (car e) (cdr e))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (101 (error (\"Maximum JSON serialization depth exceeded\")) (error (\"JSON does not allow Inf or NaN\" 0.0e+NaN)) (error (\"JSON does not allow Inf or NaN\" 1.0e+INF)) (wrong-type-argument (symbolp 1)) (wrong-type-argument (consp 1)) (wrong-type-argument (listp ((a . 1) . 2))) (wrong-type-argument (json-value-p #<marker in no buffer>)) nil t (error (\"Maximum JSON serialization depth exceeded\")) (error (\"JSON does not allow Inf or NaN\" 0.0e+NaN)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
