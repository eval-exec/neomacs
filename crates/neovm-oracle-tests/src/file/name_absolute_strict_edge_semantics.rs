//! Oracle parity tests for GNU `file-name-absolute-p` tilde semantics.
//!
//! GNU implements this in `src/fileio.c`: on Unix, `~` and `~/...` are
//! absolute, but `~USER` is absolute only when USER names an existing login.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_name_absolute_p_tilde_user_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((unlikely-user "~neomacs-oracle-user-that-should-not-exist-9f4e4f54b7"))
  (list
   (file-name-absolute-p "/")
   (file-name-absolute-p "/tmp")
   (file-name-absolute-p "relative")
   (file-name-absolute-p "")
   (file-name-absolute-p "~")
   (file-name-absolute-p "~/")
   (file-name-absolute-p "~/x")
   (file-name-absolute-p unlikely-user)
   (file-name-absolute-p (concat unlikely-user "/x"))
   (condition-case err
       (file-name-absolute-p)
     (error (list (car err) (cdr err))))
   (condition-case err
       (file-name-absolute-p 42)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t nil nil t t t nil nil (wrong-number-of-arguments (file-name-absolute-p 0)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
