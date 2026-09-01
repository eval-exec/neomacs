//! Oracle parity tests for GNU `subr.el` posn object accessors.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_posn_object_accessors_are_list_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el implements these as fixed-field accessors.  posn-object
    // prefers an image object over a string object, while object geometry comes
    // directly from fields 8 and 9.
    let form = r#"
(let ((image-pos (list nil nil '(10 . 20) 99 '("display" . 2)
                       42 '(3 . 4) '(image :type xpm)
                       '(1 . 2) '(30 . 40)))
      (string-pos (list nil nil '(10 . 20) 99 '("display" . 2)
                        42 '(3 . 4) nil
                        '(5 . 6) '(70 . 80)))
      (plain-pos (list nil nil '(10 . 20) 99 'handle
                       42 nil nil nil nil)))
  (list
   (posn-object image-pos)
   (posn-object string-pos)
   (posn-object plain-pos)
   (posn-object-x-y image-pos)
   (posn-object-width-height image-pos)
   (posn-object-x-y string-pos)
   (posn-object-width-height string-pos)
   (posn-string plain-pos)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((image :type xpm) (\"display\" . 2) nil (1 . 2) (30 . 40) (5 . 6) (70 . 80) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
