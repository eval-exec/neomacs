//! GNU Emacs 31.1 parity for `compound-text-with-extensions` Extended Segments
//! (`ESC % G ... ESC % @` UTF-8 segments and `ESC % / [0-4] M L ...` non-standard
//! charset segments) and ISO-2022 raw extended-segment preservation.
//!
//! GNU Emacs 31.1 (`a360712c9d27`) source contract:
//! - `src/coding.c:3500-3555, 3798-3845` (`decode_coding_iso_2022`): `ESC % G`
//!   sets `CODING_ISO_EMBEDDED_UTF_8` and preserves all bytes as raw ASCII /
//!   eight-bit characters until `ESC % @`; `ESC % / [0-4] M L` sets
//!   `CODING_ISO_EXTSEGMENT_LEN = (M - 128) * 128 + (L - 128)` and preserves
//!   that many following bytes as raw ASCII / eight-bit characters.
//! - `lisp/international/mule.el:1599-1639` (`ctext-post-read-conversion`) and
//!   `lisp/international/mule.el:1688-1755` (`ctext-pre-write-conversion`):
//!   post-read / pre-write conversion hooks on `compound-text-with-extensions`
//!   (`lisp/international/mule-conf.el:1565-1587`).

use crate::common::assert_oracle_parity_expect;
use expect_test::expect;

#[test]
fn compound_text_with_extensions_decodes_utf8_and_nonstd_extended_segments() {
    let form = r#"(list
  ;; UTF-8 extended segment ESC % G ... ESC % @
  (string-to-list
   (decode-coding-string
    (unibyte-string #x1b ?% ?G #xc3 #xa9 #x1b ?% ?@)
    'compound-text-with-extensions))
  ;; Mixed ASCII + UTF-8 extended segment + ASCII
  (string-to-list
   (decode-coding-string
    (unibyte-string ?a #x1b ?% ?G #xe2 #x82 #xac #x1b ?% ?@ ?b)
    'compound-text-with-extensions))
  ;; Mixed ISO-2022 JISX0208 + GR Latin-1 + UTF-8 extended segment + ASCII
  (string-to-list
   (decode-coding-string
    (unibyte-string #x1b ?$ ?\( ?B ?$ ?\" #x1b ?\( ?B #xe9
                    #x1b ?% ?G #xf0 #x9f #x98 #x80 #x1b ?% ?@ ?z)
    'compound-text-with-extensions))
  ;; Non-standard charset extended segment ESC % / 1 M L iso8859-15 \002 #xa4 -> €
  (string-to-list
   (decode-coding-string
    (unibyte-string #x1b ?% ?/ ?1 #x80 #x8c ?i ?s ?o ?8 ?8 ?5 ?9 ?- ?1 ?5 2 #xa4)
    'compound-text-with-extensions))
  ;; decode-coding-region in-place on multibyte buffer containing raw extended segment
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string ?a #x1b ?% ?G #xc3 #xa9 #x1b ?% ?@ ?b))
    (set-buffer-multibyte t)
    (let ((n (decode-coding-region (point-min) (point-max) 'compound-text-with-extensions)))
      (list n (string-to-list (buffer-string))))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[r#""OK ((233) (97 8364 98) (12354 233 128512 122) (8364) (3 (97 233 98)))""#]],
    );
}

#[test]
fn compound_text_with_extensions_encodes_and_roundtrips_extended_segments() {
    let form = r#"(list
  ;; Encode non-CTEXT Unicode emoji via ESC % G ... ESC % @
  (string-to-list
   (encode-coding-string (string ?a #x1f600 ?b) 'compound-text-with-extensions))
  ;; Encode Euro sign via compound-text-with-extensions
  (string-to-list
   (encode-coding-string (string ?a #x20ac ?b) 'compound-text-with-extensions))
  ;; Round-trip mixed ASCII + Latin-1 + Euro + Japanese + Emoji via base and alias
  (let ((orig (string ?a #xe9 #x20ac #x3042 #x1f600 ?b)))
    (list
     (equal orig
            (decode-coding-string
             (encode-coding-string orig 'compound-text-with-extensions)
             'compound-text-with-extensions))
     (equal orig
            (decode-coding-string
             (encode-coding-string orig 'ctext-with-extensions)
             'ctext-with-extensions)))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((97 27 37 71 240 159 152 128 27 37 64 98) (97 27 45 70 164 98 27 45 65) (t t))""#
        ]],
    );
}

#[test]
fn compound_text_plain_preserves_extended_segment_raw_bytes() {
    let form = r#"(list
  ;; Plain compound-text (no post-read hook) keeps ESC % G ... ESC % @ payload as raw byte8
  (string-to-list
   (decode-coding-string
    (unibyte-string #x1b ?% ?G #xc3 #xa9 #x1b ?% ?@)
    'compound-text))
  ;; Plain compound-text keeps ESC % / [0-4] M L payload as raw byte8
  (string-to-list
   (decode-coding-string
    (unibyte-string #x1b ?% ?/ ?1 #x80 #x8c ?i ?s ?o ?8 ?8 ?5 ?9 ?- ?1 ?5 2 #xa4)
    'compound-text)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((27 37 71 4194243 4194217 27 37 64) (27 37 47 49 4194176 4194188 105 115 111 56 56 53 57 45 49 53 2 4194212))""#
        ]],
    );
}

#[test]
fn compound_text_extended_segment_preserves_state_across_literal_multibyte() {
    let form = r#"(list
  ;; GNU `decode_coding_iso_2022` (`src/coding.c:3509-3518, 3946-3961`): a literal
  ;; multibyte char inside an extended segment jumps to `invalid_code` without
  ;; clearing `extsegment_len` or `embedded_utf8` or G1.
  (string-to-list
   (decode-coding-string
    (string #x1b ?% ?/ ?1 (unibyte-char-to-multibyte #x80) (unibyte-char-to-multibyte #x82)
            #xe9 (unibyte-char-to-multibyte #xe9))
    'compound-text))
  (string-to-list
   (decode-coding-string
    (string #x1b ?% ?G #xe9 (unibyte-char-to-multibyte #xe9)
            #x1b ?% ?@ (unibyte-char-to-multibyte #xe9))
    'compound-text)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((27 37 47 49 4194176 4194178 233 4194281) (27 37 71 233 4194281 27 37 64 233))""#
        ]],
    );
}
