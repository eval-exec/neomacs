//! GNU Emacs 31.1 parity for `:default-char` in `coding-system-put`,
//! `define-coding-system`, and all encoders (`charset`, `shift-jis`, `big5`,
//! `iso-2022`, `euc`, `emacs-mule`, `us-ascii`, `iso-latin-1`, aliases, and
//! `encode-coding-region`).
//!
//! GNU Emacs 31.1 (`a360712c9d27`) source contract:
//! - `Fcoding_system_put` (`src/coding.c:11540-11547`): `:default-char`
//!   coerces `nil` to `? ` (`32`), validates any non-nil value with
//!   `CHECK_CHARACTER` (`(wrong-type-argument characterp ...)`), stores the
//!   coerced value on `coding_attr_default_char` and the shared spec plist,
//!   and returns it. Bare symbol `default-char` is an ordinary plist key.
//! - `encode_coding_charset` (`src/coding.c:5648-5649`),
//!   `encode_coding_sjis` (`:4949-4982`), `encode_coding_big5` (`:5039-5054`),
//!   `encode_coding_iso_2022` (`:4515-4520`), `encode_coding_emacs_mule`
//!   (`:2641-2663`): substitute `coding->default_char` on unencodable chars.

use crate::common::assert_oracle_parity_expect;
use expect_test::expect;

#[test]
fn coding_system_put_default_char_validation_and_coercion() {
    let form = r#"(progn
  (define-coding-system-alias 'dc-l1-alias 'iso-latin-1)
  (let ((orig (coding-system-get 'iso-latin-1 :default-char)))
    (unwind-protect
        (list
          (coding-system-put 'dc-l1-alias :default-char nil)
          (coding-system-get 'dc-l1-alias :default-char)
          (coding-system-get 'iso-latin-1 :default-char)
          (coding-system-get 'dc-l1-alias-unix :default-char)
          (coding-system-put 'dc-l1-alias-unix :default-char ?*)
          (coding-system-get 'dc-l1-alias :default-char)
          (coding-system-get 'iso-latin-1 :default-char)
          (condition-case e (coding-system-put 'dc-l1-alias :default-char "x") (error e))
          (condition-case e (coding-system-put 'dc-l1-alias :default-char -1) (error e))
          (condition-case e (coding-system-put 'dc-l1-alias :default-char #x400000) (error e))
          (coding-system-put 'dc-l1-alias 'default-char "bare-ok"))
      (coding-system-put 'iso-latin-1 :default-char orig))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (32 32 32 32 42 42 42 (wrong-type-argument characterp \"x\") (wrong-type-argument characterp -1) (wrong-type-argument characterp 4194304) \"bare-ok\")""#
        ]],
    );
}

#[test]
fn encode_coding_string_and_region_respect_default_char_on_builtins_and_aliases() {
    let form = r#"(progn
  (define-coding-system-alias 'dc-l1-a 'iso-latin-1)
  (let ((orig-l1 (coding-system-get 'iso-latin-1 :default-char))
        (orig-as (coding-system-get 'us-ascii :default-char)))
    (unwind-protect
        (progn
          (coding-system-put 'dc-l1-a :default-char ?*)
          (coding-system-put 'us-ascii :default-char ?!)
          (list
            (string-to-list (encode-coding-string (string ?a #x3042 ?b) 'dc-l1-a))
            (string-to-list (encode-coding-string (string ?a #x3042 ?b) 'iso-latin-1))
            (string-to-list (encode-coding-string (string ?a #x3042 ?b) 'dc-l1-a-unix))
            (with-temp-buffer
              (insert (string ?a #x3042 ?b))
              (encode-coding-region (point-min) (point-max) 'dc-l1-a)
              (string-to-list (buffer-string)))
            (string-to-list (encode-coding-string (string ?a #xe9 ?b) 'us-ascii))
            (progn
              (coding-system-put 'us-ascii :default-char nil)
              (string-to-list (encode-coding-string (string ?a #xe9 ?b) 'us-ascii)))))
      (coding-system-put 'iso-latin-1 :default-char orig-l1)
      (coding-system-put 'us-ascii :default-char orig-as))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[r#""OK ((97 42 98) (97 42 98) (97 42 98) (97 42 98) (97 33 98) (97 32 98))""#]],
    );
}

#[test]
fn encode_coding_string_respects_default_char_across_all_codecs() {
    let form = r#"(progn
  (define-coding-system 'dc-c-star "d" :coding-type 'charset :charset-list '(iso-8859-1) :mnemonic ?L :default-char ?*)
  (define-coding-system 'dc-c-e9 "d" :coding-type 'charset :charset-list '(iso-8859-1) :mnemonic ?L :default-char #xe9)
  (define-coding-system 'dc-c-bad "d" :coding-type 'charset :charset-list '(iso-8859-1) :mnemonic ?L :default-char #x1f600)
  (define-coding-system 'dc-s-star "d" :coding-type 'shift-jis :charset-list '(ascii katakana-jisx0201 japanese-jisx0208) :mnemonic ?S :default-char ?*)
  (define-coding-system 'dc-s-ja "d" :coding-type 'shift-jis :charset-list '(ascii katakana-jisx0201 japanese-jisx0208) :mnemonic ?S :default-char #x3042)
  (define-coding-system 'dc-b-star "d" :coding-type 'big5 :charset-list '(ascii big5) :mnemonic ?B :default-char ?*)
  (define-coding-system 'dc-b-zh "d" :coding-type 'big5 :charset-list '(ascii big5) :mnemonic ?B :default-char #x4e00)
  (define-coding-system 'dc-i-star "d" :coding-type 'iso-2022 :charset-list '(ascii japanese-jisx0208) :designation [ascii nil nil nil] :flags '(7-bit designation ascii-eol) :mnemonic ?J :default-char ?*)
  (define-coding-system 'dc-i-ja "d" :coding-type 'iso-2022 :charset-list '(ascii japanese-jisx0208) :designation [ascii nil nil nil] :flags '(7-bit designation ascii-eol) :mnemonic ?J :default-char #x3042)
  (define-coding-system 'dc-e-star "d" :coding-type 'iso-2022 :charset-list '(ascii japanese-jisx0208) :designation [ascii japanese-jisx0208 nil nil] :flags '(8-bit-level-4) :mnemonic ?E :default-char ?*)
  (define-coding-system 'dc-e-ja "d" :coding-type 'iso-2022 :charset-list '(ascii japanese-jisx0208) :designation [ascii japanese-jisx0208 nil nil] :flags '(8-bit-level-4) :mnemonic ?E :default-char #x3042)
  (define-coding-system 'dc-m-star "d" :coding-type 'emacs-mule :charset-list 'emacs-mule :mnemonic ?M :default-char ?*)
  (list
    (string-to-list (encode-coding-string (string ?a #x3042 ?b) 'dc-c-star))
    (string-to-list (encode-coding-string (string ?a #x3042 ?b) 'dc-c-e9))
    (string-to-list (encode-coding-string (string ?a #x3042 ?b) 'dc-c-bad))
    (string-to-list (encode-coding-string (string ?a #x1f600 ?b) 'dc-s-star))
    (string-to-list (encode-coding-string (string ?a #x1f600 ?b) 'dc-s-ja))
    (string-to-list (encode-coding-string (string ?a #x1f600 ?b) 'dc-b-star))
    (string-to-list (encode-coding-string (string ?a #x1f600 ?b) 'dc-b-zh))
    (string-to-list (encode-coding-string (string ?a #x1f600 ?b) 'dc-i-star))
    (string-to-list (encode-coding-string (string ?a #x1f600 ?b) 'dc-i-ja))
    (string-to-list (encode-coding-string (string ?a #x1f600 ?b) 'dc-e-star))
    (string-to-list (encode-coding-string (string ?a #x1f600 ?b) 'dc-e-ja))
    (string-to-list (encode-coding-string (string ?a #x1f600 ?b) 'dc-m-star))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((97 42 98) (97 233 98) (97 0 98) (97 42 98) (97 130 160 98) (97 42 98) (97 164 64 98) (97 42 98) (97 27 36 66 36 34 27 40 66 98) (97 42 98) (97 164 162 98) (97 42 98))""#
        ]],
    );
}

#[test]
fn encode_coding_string_utf16_default_char_matches_gnu() {
    let form = r#"(let ((orig-be (coding-system-get 'utf-16-be :default-char))
      (orig-le (coding-system-get 'utf-16le :default-char)))
  (unwind-protect
      (progn
        (coding-system-put 'utf-16-be :default-char #x300000)
        (coding-system-put 'utf-16le :default-char #x300000)
        (let ((over-unicode
               (list
                (string-to-list (encode-coding-string (string #x200000) 'utf-16-be))
                (string-to-list (encode-coding-string (string #x3fffff) 'utf-16-be))
                (string-to-list (encode-coding-string (string #xd800) 'utf-16-be))
                (string-to-list (encode-coding-string (string #x200000) 'utf-16le)))))
          (coding-system-put 'utf-16-be :default-char ?A)
          (coding-system-put 'utf-16le :default-char ?A)
          (list
           over-unicode
           (string-to-list (encode-coding-string (string #x200000) 'utf-16-be))
           (string-to-list (encode-coding-string (string #x3fffff) 'utf-16-be))
           (string-to-list (encode-coding-string (string #x200000) 'utf-16le)))))
    (coding-system-put 'utf-16-be :default-char orig-be)
    (coding-system-put 'utf-16le :default-char orig-le)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (((254 255 227 192 220 0) (254 255 227 192 220 0) (254 255 216 0) (192 227 0 220)) (254 255 0 65) (254 255 0 65) (65 0))""#
        ]],
    );
}
