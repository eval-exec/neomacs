//! Oracle parity tests for GNU `parse-colon-path` semantics.
//!
//! GNU implements this in `lisp/files.el`.  It splits on `path-separator`,
//! substitutes environment variables with `substitute-env-vars`, maps empty
//! path elements to nil, converts non-empty elements to directory syntax, and
//! preserves historical slash-collapsing behavior for leading `//+`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_parse_colon_path_empty_env_slash_and_type_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((process-environment (copy-sequence process-environment)))
  (setenv "NEOMACS_ORACLE_PATH_ROOT" "/tmp/neomacs-root")
  (setenv "NEOMACS_ORACLE_PATH_EMPTY" "")
  (list
   (parse-colon-path nil)
   (parse-colon-path "")
   (parse-colon-path ":")
   (parse-colon-path "a:b::c:")
   (parse-colon-path
    "$NEOMACS_ORACLE_PATH_ROOT/bin:${NEOMACS_ORACLE_PATH_ROOT}/lib:$UNDEF/x:$NEOMACS_ORACLE_PATH_EMPTY/end")
   (parse-colon-path "//server/path:////many")
   (condition-case err
       (parse-colon-path 42)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil (nil) (nil nil) (\"a/\" \"b/\" nil \"c/\" nil) (\"/tmp/neomacs-root/bin/\" \"/tmp/neomacs-root/lib/\" \"/x/\" \"/end/\") (\"/server/path/\" \"/many/\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
