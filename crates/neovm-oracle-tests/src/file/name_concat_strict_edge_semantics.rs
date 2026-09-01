//! Oracle parity tests for GNU `file-name-concat` semantics.
//!
//! GNU implements this in `src/fileio.c`: nil and empty components are
//! skipped, slashes are inserted only between non-final non-empty components,
//! and absolute-looking later components are concatenated syntactically rather
//! than normalized.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_name_concat_filters_and_separator_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (file-name-concat "")
 (file-name-concat nil)
 (file-name-concat "" nil "")
 (file-name-concat "a")
 (file-name-concat "a" "b")
 (file-name-concat "a/" "b")
 (file-name-concat "a" "b/")
 (file-name-concat "a/" "b/")
 (file-name-concat "" "a" nil "" "b" "")
 (file-name-concat "/tmp" "a" "b")
 (file-name-concat "/tmp/" "/absolute" "tail")
 (file-name-concat "a" "/b" "c")
 (file-name-concat "a" "." ".." "b")
 (file-name-concat "a" nil "b" nil "c")
 (condition-case err
     (file-name-concat)
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-concat "a" 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\" \"\" \"\" \"a\" \"a/b\" \"a/b\" \"a/b/\" \"a/b/\" \"a/b\" \"/tmp/a/b\" \"/tmp//absolute/tail\" \"a//b/c\" \"a/./../b\" \"a/b/c\" (wrong-number-of-arguments (file-name-concat 0)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_name_concat_unibyte_multibyte_conversion_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((raw (unibyte-string #xe9))
       (wide "\u00e9")
       (cases
        (list
         (file-name-concat raw "tail")
         (file-name-concat raw nil "tail")
         (file-name-concat raw "" "tail")
         (file-name-concat raw wide)
         (file-name-concat "head" raw)
         (file-name-concat "" raw)
         (file-name-concat nil raw))))
  (mapcar (lambda (s)
            (list (multibyte-string-p s)
                  (unibyte-string-p s)
                  (length s)
                  (string-bytes s)
                  (string-to-list s)))
          cases))
"#;

    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
