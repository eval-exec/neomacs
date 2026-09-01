//! Oracle parity tests for GNU `buffer-hash` primitive semantics.
//!
//! GNU implements `Fbuffer_hash` in `src/fns.c`: nil means current buffer,
//! non-nil input is resolved through `get-buffer`, and live buffer contents are
//! hashed with SHA-1 over the raw buffer bytes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_buffer_hash_live_buffer_lookup_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((b (get-buffer-create " *bh-live-oracle*")))
  (with-current-buffer b
    (erase-buffer)
    (insert "abc"))
  (list
   (with-current-buffer b (buffer-hash))
   (buffer-hash b)
   (buffer-hash " *bh-live-oracle*")
   (condition-case err
       (buffer-hash " *missing-bh-oracle*")
     (error (list (car err) (cdr err))))
   (condition-case err
       (buffer-hash 42)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"a9993e364706816aba3e25717850c26c9cd0d89d\" \"a9993e364706816aba3e25717850c26c9cd0d89d\" \"a9993e364706816aba3e25717850c26c9cd0d89d\" (error (\"No buffer named  *missing-bh-oracle*\")) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
