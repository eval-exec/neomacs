//! Oracle parity tests for GNU `secure-hash` primitive semantics.
//!
//! GNU implements `Fsecure_hash` in `src/fns.c`.  It accepts a symbol
//! algorithm, string or buffer object, optional character-position bounds, and
//! an optional BINARY flag that returns a unibyte digest string.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_secure_hash_ranges_binary_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (secure-hash 'sha256 "abcdef" 1 4)
 (secure-hash 'sha256 "abcdef" -5 -2)
 (length (secure-hash 'sha256 "abc" nil nil t))
 (multibyte-string-p (secure-hash 'sha256 "abc" nil nil t))
 (secure-hash 'sha256 "é")
 (condition-case err
     (secure-hash 42 "abc")
   (error (list (car err) (cdr err))))
 (condition-case err
     (secure-hash 'bad "abc")
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"a6b0f90d2ac2b8d1f250c687301aef132049e9016df936680e81fa7bc7d81d70\" \"a6b0f90d2ac2b8d1f250c687301aef132049e9016df936680e81fa7bc7d81d70\" 32 nil \"4a99557e4033c3539de2eb65472017cad5f9557f7a0625a09f1c3f6e2ba69c4c\" (wrong-type-argument (symbolp 42)) (error (\"Invalid algorithm arg: bad\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_secure_hash_encodes_literal_raw_bytes_from_a_multibyte_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  ;; Inserting a unibyte string into a multibyte buffer stores the high bytes
  ;; as Emacs raw-byte characters, exactly as `insert-file-contents-literally'
  ;; does.  GNU's `secure-hash' hashes the external encoded representation,
  ;; not the buffer gap's internal multibyte bytes.
  (insert (unibyte-string 65 195 169 90))
  (let ((coding-system-for-write 'utf-8-unix))
    (list
     (buffer-size)
     (append (buffer-string) nil)
     (secure-hash 'sha256 (current-buffer)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (4 (65 4194243 4194217 90) \"6a1917777ebb7105da25b045353aeda24a7a9863e4d1ab0d72e1dc5f7d482257\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
