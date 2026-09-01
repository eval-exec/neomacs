//! Oracle parity tests for GNU `subr.el` string conversion helpers.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_string_to_list_vector_byte_and_property_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:string-to-list is `(append string nil)` and
    // string-to-vector is `(vconcat string)`.  Multibyte strings produce
    // characters, unibyte strings produce bytes, NUL bytes are preserved, and
    // text properties do not alter the produced numeric elements.
    let form = r#"
(let* ((multi (propertize "éa" 'face 'bold))
       (uni (string-as-unibyte "é"))
       (nul "a\0b"))
  (list
   (string-to-list multi)
   (mapcar (lambda (i) (get-text-property i 'face multi))
           (number-sequence 0 (1- (length multi))))
   (string-to-vector multi)
   (multibyte-string-p uni)
   (string-to-list uni)
   (string-to-vector uni)
   (string-to-list nul)
   (string-to-vector nul)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((233 97) (bold bold) [233 97] nil (195 169) [195 169] (97 0 98) [97 0 98])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_string_make_multibyte_unibyte_identity_copy_and_low_byte_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:string-make-multibyte returns the original object for
    // multibyte strings and all-ASCII unibyte strings, but copies non-ASCII
    // unibyte storage into multibyte raw-byte characters with no properties.
    // string-make-unibyte always requires a string and converts each multibyte
    // character to its low 8 bits.
    let form = r#"
(let* ((ascii-uni (unibyte-string 65 66 67))
       (raw-uni (unibyte-string 65 200 66))
       (ascii-prop (propertize "ABC" 'face 'bold))
       (multi-prop (propertize "éĀ" 'face 'bold)))
  (list
   (let ((r (string-make-multibyte ascii-uni)))
     (list (eq r ascii-uni)
           (multibyte-string-p r)
           (string-to-list r)))
   (let ((r (string-make-multibyte raw-uni)))
     (list (eq r raw-uni)
           (multibyte-string-p r)
           (length r)
           (string-bytes r)
           (string-to-list r)
           (text-properties-at 0 r)))
   (let ((r (string-make-multibyte ascii-prop)))
     (list (eq r ascii-prop)
           (multibyte-string-p r)
           (mapcar (lambda (i) (get-text-property i 'face r))
                   (number-sequence 0 (1- (length r))))))
   (let ((r (string-make-unibyte multi-prop)))
     (list (eq r multi-prop)
           (unibyte-string-p r)
           (length r)
           (string-bytes r)
           (string-to-list r)
           (text-properties-at 0 r)
           (text-properties-at 1 r)))
   (condition-case err
       (string-make-multibyte 42)
     (error err))
   (condition-case err
       (string-make-unibyte nil)
     (error err))))
"#;
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_byte_to_string_unibyte_boundaries_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/editfns.c:byte-to-string accepts only fixnums in 0..255,
    // signals a plain "Invalid byte" error outside that range, and always
    // returns a one-byte unibyte string.
    let form = r#"
(list
 (let ((s (byte-to-string 0)))
   (list s
         (length s)
         (string-bytes s)
         (multibyte-string-p s)
         (aref s 0)))
 (let ((s (byte-to-string 65)))
   (list s
         (length s)
         (string-bytes s)
         (multibyte-string-p s)
         (aref s 0)))
 (let ((s (byte-to-string 255)))
   (list (length s)
         (string-bytes s)
         (multibyte-string-p s)
         (aref s 0)
         (string-make-multibyte s)))
 (condition-case err
     (byte-to-string -1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (byte-to-string 256)
   (error (list (car err) (cdr err))))
 (condition-case err
     (byte-to-string 1.0)
   (error (list (car err) (cdr err))))
 (condition-case err
     (byte-to-string nil)
   (error (list (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\\0\" 1 1 nil 0) (\"A\" 1 1 nil 65) (1 1 nil 255 \"\\377\") (error (\"Invalid byte\")) (error (\"Invalid byte\")) (wrong-type-argument (fixnump 1.0)) (wrong-type-argument (fixnump nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_string_as_vs_to_multibyte_utf8_byte_sequence_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fstring_as_multibyte parses valid UTF-8 byte sequences
    // from a unibyte string into characters, while Fstring_to_multibyte maps
    // every non-ASCII byte to an eight-bit character.  Converted copies have
    // no text properties; already-correct representations are returned by
    // identity.
    let form = r#"
(let* ((utf8-e (unibyte-string 195 169))
       (invalid (unibyte-string 195 40))
       (raw (unibyte-string 128 255))
       (prop-utf8 (propertize utf8-e 'face 'bold)))
  (list
   (let ((r (string-as-multibyte utf8-e)))
     (list (eq r utf8-e)
           (multibyte-string-p r)
           (length r)
           (string-bytes r)
           (string-to-list r)))
   (let ((r (string-to-multibyte utf8-e)))
     (list (eq r utf8-e)
           (multibyte-string-p r)
           (length r)
           (string-bytes r)
           (string-to-list r)))
   (let ((r (string-as-multibyte invalid)))
     (list (multibyte-string-p r)
           (length r)
           (string-bytes r)
           (string-to-list r)))
   (let ((r (string-as-multibyte raw)))
     (list (multibyte-string-p r)
           (length r)
           (string-bytes r)
           (string-to-list r)))
   (let ((r (string-as-multibyte prop-utf8)))
     (list (text-properties-at 0 r)
           (text-properties-at 1 r)))
   (let ((s "é"))
     (eq s (string-as-multibyte s)))
   (let ((s (unibyte-string 65 66)))
     (eq s (string-as-unibyte s)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((nil t 1 2 (233)) (nil t 2 4 (4194243 4194217)) (t 2 3 (4194243 40)) (t 2 4 (4194176 4194303)) (nil nil) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
