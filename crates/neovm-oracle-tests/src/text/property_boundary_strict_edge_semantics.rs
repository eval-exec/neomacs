//! Oracle parity tests for GNU text-property boundary search semantics.
//!
//! GNU implements these functions in `src/textprop.c`.  Plain strings with no
//! interval tree still have observable nil-property search results, and
//! `next-property-change` has a special LIMIT=t mode that the single-property
//! variants do not share.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_text_property_plain_string_boundaries_and_limit_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((plain "abcdef")
       (s (copy-sequence plain)))
  (put-text-property 1 3 'face 'bold s)
  (put-text-property 3 5 'face 'bold s)
  (put-text-property 5 6 'face 'italic s)
  (list
   (text-property-any 0 6 'missing nil plain)
   (text-property-any 0 0 'missing nil plain)
   (text-property-any 0 6 'missing 'value plain)
   (text-property-not-all 0 6 'missing nil plain)
   (text-property-not-all 0 0 'missing 'value plain)
   (text-property-not-all 0 6 'missing 'value plain)
   ;; LIMIT=t is special only for `next-property-change`.
   (next-property-change 0 s t)
   (next-property-change 1 s t)
   (next-property-change 3 s t)
   ;; The ordinary path coalesces adjacent intervals with equal plists.
   (next-property-change 1 s)
   (condition-case err
       (next-single-property-change 1 'face s t)
     (error (list (car err) (cdr err))))
   (condition-case err
       (previous-single-property-change 5 'face s t)
     (error (list (car err) (cdr err))))
   (previous-property-change 6 s)
   (previous-property-change 6 s 4)))"#;

    let expect = expect_test::expect![[
        r#""OK (0 nil nil nil nil 0 1 3 5 5 (wrong-type-argument (integer-or-marker-p t)) (wrong-type-argument (integer-or-marker-p t)) 5 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
