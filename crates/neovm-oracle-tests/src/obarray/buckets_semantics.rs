//! Oracle parity tests for GNU `internal--obarray-buckets` semantics.
//!
//! GNU implements this in `src/lread.c`: after `check_obarray`, it returns one
//! list per bucket in obarray order, preserving each bucket's symbol chain.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_internal_obarray_buckets_shape_contents_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((ob (make-vector 3 0)))
  (intern "alpha" ob)
  (intern "beta" ob)
  (list
   (length (internal--obarray-buckets ob))
   (mapcar (lambda (bucket)
             (and bucket
                  (mapcar (lambda (symbol) (symbol-name symbol)) bucket)))
           (internal--obarray-buckets ob))
   (condition-case err
       (internal--obarray-buckets [not-obarray])
     (error (cons (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (2 ((\"alpha\" \"beta\") nil) (wrong-type-argument obarrayp [not-obarray]))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
