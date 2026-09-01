//! Oracle parity tests for GNU `subr.el` `lsh`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_lsh_negative_fixnum_and_bignum_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:lsh is not just a raw machine logical shift.  For negative
    // fixnums shifted right, it first maps the value through the fixnum-width
    // unsigned domain.  Negative bignums shifted right signal
    // `args-out-of-range' instead of being shifted arithmetically.
    let form = r#"
(list
 ;; Positive values match ash for both directions.
 (lsh 1 0)
 (lsh 1 5)
 (lsh 32 -1)
 (= (lsh 17 3) (ash 17 3))
 (= (lsh 1024 -4) (ash 1024 -4))

 ;; Negative fixnum right shifts are unsigned over Emacs' fixnum width.
 (lsh -1 -1)
 (lsh -2 -1)
 (lsh -3 -1)
 (lsh most-negative-fixnum -1)
 (= (lsh -1 -1) most-positive-fixnum)
 (= (lsh -2 -1) most-positive-fixnum)
 (= (lsh -3 -1) (1- most-positive-fixnum))
 (= (lsh -4 -2) (1- most-positive-fixnum))
 (not (= (lsh -1 -1) (ash -1 -1)))

 ;; Left shifts of negative values delegate to ash.
 (= (lsh -1 1) (ash -1 1))
 (= (lsh -5 3) (ash -5 3))

 ;; A negative bignum is outside the documented negative-count domain.
 (condition-case err
     (lsh (1- most-negative-fixnum) -1)
   (error (cons (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (1 32 16 t t 0 0 0 0 t t t nil t t t (args-out-of-range -2305843009213693953 -1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
