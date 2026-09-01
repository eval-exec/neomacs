//! Oracle parity tests for GNU `file-size-human-readable` semantics.
//!
//! GNU implements this in `lisp/files.el`.  The behavior is pure Elisp:
//! nil/`iec` use a 1024 divisor, any other non-nil flavor uses 1000, and
//! formatting follows GNU `ls -lh` one-decimal rounding rules.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_size_human_readable_flavors_rounding_units_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (file-size-human-readable 0)
 (file-size-human-readable 1)
 (file-size-human-readable 999)
 (file-size-human-readable 1000)
 (file-size-human-readable 1024)
 (file-size-human-readable 1536)
 (file-size-human-readable 1048576)
 (file-size-human-readable 1536 'iec)
 (file-size-human-readable 1536 'si)
 ;; Any non-nil, non-iec flavor follows the SI divisor branch.
 (file-size-human-readable 1536 'gnu)
 (file-size-human-readable 1536 nil "" "B")
 (file-size-human-readable 1536 'iec " ")
 (file-size-human-readable-iec 1536)
 ;; Rounding boundaries from GNU's one-decimal rule.
 (file-size-human-readable 1075)
 (file-size-human-readable 1996)
 (file-size-human-readable 2000 'si)
 (file-size-human-readable -1)
 (file-size-human-readable 0 'iec " " nil)
 (file-size-human-readable 0 nil 42 "B")
 (condition-case err
     (file-size-human-readable "1536")
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-size-human-readable 1 nil nil 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-size-human-readable-iec "1536")
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"0\" \"1\" \"999\" \"1000\" \"1k\" \"1.5k\" \"1M\" \"1.5KiB\" \"1.5k\" \"1.5k\" \"1.5kB\" \"1.5 KiB\" \"1.5 KiB\" \"1k\" \"1.9k\" \"2k\" \"-1\" \"0 B\" \"042B\" (wrong-type-argument (number-or-marker-p \"1536\")) (wrong-type-argument (sequencep 42)) (wrong-type-argument (number-or-marker-p \"1536\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
