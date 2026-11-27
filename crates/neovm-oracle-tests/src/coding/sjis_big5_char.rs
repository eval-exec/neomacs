//! GNU Emacs 31.1 parity for `decode-sjis-char`, `encode-sjis-char`,
//! `decode-big5-char`, and `encode-big5-char`.
//!
//! GNU Emacs 31.1 (`a360712c9d27`) source contract:
//! - `Fdecode_sjis_char` (`src/coding.c:10534-10584`): `CHECK_FIXNAT`,
//!   uses the active `Vsjis_coding_system` (most recently defined
//!   `:coding-type 'shift-jis`, `japanese-shift-jis-2004` after loadup),
//!   returns ASCII `<= 0x7F` directly when ascii-compatible, decodes
//!   `0xA0 <= ch < 0xDF` via `ch - 0x80` on the 2nd charset
//!   (`katakana-jisx0201`), validates 2-byte SJIS ranges and applies
//!   `SJIS_TO_JIS` (`src/coding.h:567-580`) on the 3rd charset, signaling
//!   `(error "Invalid code: %d")` on range or decode failure.
//! - `Fencode_sjis_char` (`src/coding.c:10587-10613`): `CHECK_CHARACTER`,
//!   returns ASCII `<= 0x7F` directly when ascii-compatible, encodes via
//!   the active `Vsjis_coding_system`'s `:charset-list`, and applies
//!   `JIS_TO_SJIS` (`src/coding.h:608-620`).
//! - `Fdecode_big5_char` (`src/coding.c:10615-10657`): `CHECK_FIXNAT`,
//!   uses the active `Vbig5_coding_system` (`chinese-big5`), masks
//!   `b2 = ch & 0x7F` (`:10646`), validates `b1 in 0xA1..=0xFE` and
//!   `b2 in 0x40..=0x7E`, and decodes `ch` on the 2nd charset (`big5`).
//! - `Fencode_big5_char` (`src/coding.c:10659-10683`): `CHECK_CHARACTER`,
//!   returns ASCII `<= 0x7F` directly when ascii-compatible, and encodes
//!   via `Vbig5_coding_system`'s `:charset-list`.

use crate::common::assert_oracle_parity_expect;
use expect_test::expect;

#[test]
fn decode_sjis_char_matches_gnu() {
    let form = r#"(list
  (decode-sjis-char 0)
  (decode-sjis-char ?A)
  (decode-sjis-char #x7f)
  (condition-case e (decode-sjis-char #x80) (error e))
  (condition-case e (decode-sjis-char #xa0) (error e))
  (decode-sjis-char #xa1)
  (decode-sjis-char #xb1)
  (decode-sjis-char #xde)
  (condition-case e (decode-sjis-char #xdf) (error e))
  (decode-sjis-char #x8140)
  (decode-sjis-char #x82a0)
  (decode-sjis-char #x857e)
  (decode-sjis-char #x889f)
  (decode-sjis-char #xe040)
  (condition-case e (decode-sjis-char #x817f) (error e))
  (condition-case e (decode-sjis-char #x813f) (error e))
  (condition-case e (decode-sjis-char #x81fd) (error e))
  (condition-case e (decode-sjis-char #xf040) (error e))
  (condition-case e (decode-sjis-char -1) (error e))
  (condition-case e (decode-sjis-char "a") (error e)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (0 65 127 (error \"Invalid code: 128\") (error \"Invalid code: 160\") 65377 65393 65438 (error \"Invalid code: 223\") 12288 12354 233 20124 28478 (error \"Invalid code: 33151\") (error \"Invalid code: 33087\") (error \"Invalid code: 33277\") (error \"Invalid code: 61504\") (wrong-type-argument wholenump -1) (wrong-type-argument wholenump \"a\"))""#
        ]],
    );
}

#[test]
fn encode_sjis_char_matches_gnu() {
    let form = r#"(list
  (encode-sjis-char 0)
  (encode-sjis-char ?A)
  (encode-sjis-char #x7f)
  (encode-sjis-char #x3000)
  (encode-sjis-char #x3042)
  (encode-sjis-char #x4e00)
  (encode-sjis-char #xe9)
  (encode-sjis-char #xff71)
  (encode-sjis-char #xff9e)
  (condition-case e (encode-sjis-char -1) (error e))
  (condition-case e (encode-sjis-char #x400000) (error e))
  (condition-case e (encode-sjis-char "a") (error e)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (0 65 127 33088 33440 35050 34174 28847 28892 (wrong-type-argument characterp -1) (wrong-type-argument characterp 4194304) (wrong-type-argument characterp \"a\"))""#
        ]],
    );
}

#[test]
fn decode_big5_char_matches_gnu() {
    let form = r#"(list
  (decode-big5-char 0)
  (decode-big5-char ?A)
  (decode-big5-char #x7f)
  (condition-case e (decode-big5-char #x80) (error e))
  (decode-big5-char #xa140)
  (decode-big5-char #xa440)
  (decode-big5-char #xa442)
  (decode-big5-char #xa4c0)
  (condition-case e (decode-big5-char #xa4a1) (error e))
  (condition-case e (decode-big5-char #xa47f) (error e))
  (condition-case e (decode-big5-char #xa040) (error e))
  (condition-case e (decode-big5-char #xff40) (error e))
  (condition-case e (decode-big5-char -1) (error e))
  (condition-case e (decode-big5-char "a") (error e)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (0 65 127 (error \"Invalid code: 128\") 12288 19968 19969 20998 (error \"Invalid code: 42145\") (error \"Invalid code: 42111\") (error \"Invalid code: 41024\") (error \"Invalid code: 65344\") (wrong-type-argument wholenump -1) (wrong-type-argument wholenump \"a\"))""#
        ]],
    );
}

#[test]
fn encode_big5_char_matches_gnu() {
    let form = r#"(list
  (encode-big5-char 0)
  (encode-big5-char ?A)
  (encode-big5-char #x7f)
  (encode-big5-char #x3000)
  (encode-big5-char #x4e00)
  (encode-big5-char #x4e01)
  (encode-big5-char #x5206)
  (condition-case e (encode-big5-char -1) (error e))
  (condition-case e (encode-big5-char #x400000) (error e))
  (condition-case e (encode-big5-char "a") (error e)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (0 65 127 41280 42048 42050 42176 (wrong-type-argument characterp -1) (wrong-type-argument characterp 4194304) (wrong-type-argument characterp \"a\"))""#
        ]],
    );
}

#[test]
fn sjis_char_tracks_most_recent_shift_jis_coding_system() {
    let form = r#"(progn
  (let ((before-e9 (list (encode-sjis-char #xe9) (decode-sjis-char #x857e))))
    (define-coding-system 't-sjis-208 "208-only"
      :coding-type 'shift-jis
      :charset-list '(ascii katakana-jisx0201 japanese-jisx0208)
      :mnemonic ?S)
    (let ((after-redef (list (decode-sjis-char #x82a0)
                             (decode-sjis-char #x857e))))
      (list before-e9 after-redef))))"#;

    assert_oracle_parity_expect(form, expect![[r#""OK ((34174 233) (12354 1311534))""#]]);
}
