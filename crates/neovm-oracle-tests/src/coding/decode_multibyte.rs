//! GNU Emacs 31.1 parity for decoding multibyte buffers and strings containing
//! already-decoded non-ASCII characters (`ONE_MORE_BYTE`, `src/coding.c:633-656`).
//!
//! GNU Emacs 31.1 (`a360712c9d27`) source contract:
//! - `src/coding.c:633-656` (`ONE_MORE_BYTE`): when `coding->src_multibyte` is
//!   true, ASCII bytes (`< 0x80`) and raw eight-bit characters (`0xC0..=0xC1`)
//!   yield raw bytes `0x00..=0xFF` for the decoder, whereas already-decoded
//!   multibyte characters yield `-char_code`, terminating any partial
//!   multibyte byte sequence and passing the character through unchanged.
//! - `src/coding.c:7096-7135` (`produce_chars`, `chars_at_source == 1`): for
//!   `raw-text` / `no-conversion` on a multibyte source, `dst_multibyte == true`
//!   preserves already-decoded characters verbatim, while `dst_multibyte == false`
//!   emits `(unsigned char)(-char_code)`.
//! - `src/coding.c:8092-8168, 9455-9530` (`decode_coding_object` /
//!   `code_convert_region`): `decode-coding-region` in-place, into `t`, and
//!   into a separate multibyte or unibyte destination buffer.

use crate::common::assert_oracle_parity_expect;
use expect_test::expect;

#[test]
fn decode_coding_region_in_multibyte_buffer_preserves_non_ascii_across_codecs() {
    let form = r#"(let ((cases '(iso-latin-1 utf-8 utf-8-dos raw-text no-conversion
                 undecided japanese-shift-jis chinese-big5 utf-16le)))
  (mapcar (lambda (cs)
            (with-temp-buffer
              (insert (string #xe9))
              (let ((n (decode-coding-region (point-min) (point-max) cs)))
                (list cs n (point) (string-to-list (buffer-string))))))
          cases))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((iso-latin-1 1 2 (233)) (utf-8 1 2 (233)) (utf-8-dos 1 2 (233)) (raw-text 1 2 (233)) (no-conversion 1 2 (233)) (undecided 1 2 (233)) (japanese-shift-jis 1 2 (233)) (chinese-big5 1 2 (233)) (utf-16le 1 2 (233)))""#
        ]],
    );
}

#[test]
fn decode_multibyte_mixed_raw_bytes_and_literal_chars_across_codecs() {
    let form = r#"(list
  ;; Mixed ASCII, raw byte8, and already-decoded multibyte chars in buffer
  (with-temp-buffer
    (insert (string ?a (unibyte-char-to-multibyte #xe9) #x3042 (unibyte-char-to-multibyte #xf1)))
    (let ((n (decode-coding-region (point-min) (point-max) 'iso-latin-1)))
      (list n (string-to-list (buffer-string)))))
  (with-temp-buffer
    (insert (string ?a (unibyte-char-to-multibyte #xc3) (unibyte-char-to-multibyte #xa9) #x3042))
    (let ((n (decode-coding-region (point-min) (point-max) 'utf-8)))
      (list n (string-to-list (buffer-string)))))
  ;; Interrupted UTF-8 sequence: literal char flushes partial #xc3 as raw byte8
  (string-to-list
   (decode-coding-string
    (string (unibyte-char-to-multibyte #xc3) #x3042 (unibyte-char-to-multibyte #xa9))
    'utf-8))
  ;; UTF-16LE, Shift-JIS, ISO-2022-JP (literal char resets ISO-2022 G0 to ASCII)
  (string-to-list
   (decode-coding-string
    (string (unibyte-char-to-multibyte #x61) (unibyte-char-to-multibyte 0)
            #x3042
            (unibyte-char-to-multibyte #x62) (unibyte-char-to-multibyte 0))
    'utf-16le))
  (string-to-list
   (decode-coding-string
    (string (unibyte-char-to-multibyte #x82) (unibyte-char-to-multibyte #xa0)
            #xe9
            (unibyte-char-to-multibyte #x82) (unibyte-char-to-multibyte #xa2))
    'japanese-shift-jis))
  (string-to-list
   (decode-coding-string
    (string #x1b ?$ ?B ?$ ?\" #xe9 ?$ ?\" #x1b ?\( ?B)
    'iso-2022-jp))
  ;; UTF-8 BOM stripped at stream start, literal char preserved, following UTF-8 decoded
  (string-to-list
   (decode-coding-string
    (string (unibyte-char-to-multibyte #xef) (unibyte-char-to-multibyte #xbb)
            (unibyte-char-to-multibyte #xbf) #xe9
            (unibyte-char-to-multibyte #xc3) (unibyte-char-to-multibyte #xa9))
    'utf-8-with-signature)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((4 (97 233 12354 241)) (3 (97 233 12354)) (4194243 12354 4194217) (97 12354 98) (12354 233 12356) (12354 233 36 34) (233 233))""#
        ]],
    );
}

#[test]
fn decode_multibyte_detection_eol_hooks_and_charset_properties() {
    let form = r#"(progn
  (defun neomacs-mb-prc (len)
    (save-excursion (insert "!"))
    (1+ len))
  (define-coding-system 'neomacs-mb-prc-cs "test"
    :coding-type 'utf-8 :mnemonic ?P
    :post-read-conversion 'neomacs-mb-prc)
  (list
   ;; undecided detection skips literal multibyte chars and inspects raw bytes
   (let ((s (decode-coding-string (string #xe9) 'undecided)))
     (list (string-to-list s) last-coding-system-used))
   (let ((s (decode-coding-string (string #xe9 ?\r ?\n) 'undecided)))
     (list (string-to-list s) last-coding-system-used))
   (let ((s (decode-coding-string
             (string #x3042 (unibyte-char-to-multibyte #xc3) (unibyte-char-to-multibyte #xa9))
             'undecided)))
     (list (string-to-list s) last-coding-system-used))
   (let ((s (decode-coding-string
             (string #x3042 (unibyte-char-to-multibyte #xe9))
             'undecided)))
     (list (string-to-list s) last-coding-system-used))
   ;; DOS EOL conversion with literal multibyte char + raw byte8
   (with-temp-buffer
     (insert (string #xe9 ?\r ?\n (unibyte-char-to-multibyte #xe9)))
     (let ((n (decode-coding-region (point-min) (point-max) 'iso-latin-1-dos)))
       (list n (string-to-list (buffer-string)))))
   ;; :post-read-conversion on multibyte string
   (string-to-list
    (decode-coding-string
     (string #xe9 (unibyte-char-to-multibyte #xc3) (unibyte-char-to-multibyte #xa9))
     'neomacs-mb-prc-cs))
   ;; Charset text properties on literal-only vs mixed iso-latin-1
   (let ((s (decode-coding-string (string #xe9) 'iso-latin-1)))
     (list (string-to-list s) (get-text-property 0 'charset s)))
   (let ((s (decode-coding-string
             (string #x3042 (unibyte-char-to-multibyte #xe9) #x3044)
             'iso-latin-1)))
     (list (string-to-list s)
           (get-text-property 0 'charset s)
           (get-text-property 1 'charset s)
           (get-text-property 2 'charset s)))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (((233) utf-8) ((233 10) utf-8-dos) ((12354 233) utf-8) ((12354 233) iso-latin-1) (3 (233 10 233)) (33 233 233) ((233) nil) ((12354 233 12356) nil iso-8859-1 iso-8859-1))""#
        ]],
    );
}

#[test]
fn decode_multibyte_destinations_point_and_raw_text_unibyte_contract() {
    let form = r#"(list
  ;; decode-coding-region destination = t and separate multibyte buffer
  (with-temp-buffer
    (insert (string #xe9))
    (string-to-list (decode-coding-region (point-min) (point-max) 'iso-latin-1 t)))
  (with-temp-buffer
    (let ((dst (current-buffer)))
      (with-temp-buffer
        (insert (string #xe9))
        (let ((n (decode-coding-region (point-min) (point-max) 'iso-latin-1 dst)))
          (with-current-buffer dst
            (list n (string-to-list (buffer-string))))))))
  ;; decode-coding-region from multibyte buffer into unibyte destination buffer
  (with-temp-buffer
    (let ((dst (current-buffer)))
      (set-buffer-multibyte nil)
      (with-temp-buffer
        (insert (string #xe9 ?a))
        (let ((n (decode-coding-region (point-min) (point-max) 'iso-latin-1 dst)))
          (with-current-buffer dst
            (list n (string-to-list (buffer-string))))))))
  ;; Point restoration after in-place decode shrinking raw UTF-8 pair in multibyte buffer
  (with-temp-buffer
    (insert (string ?x #xe9 (unibyte-char-to-multibyte #xc3) (unibyte-char-to-multibyte #xa9) ?y))
    (goto-char (point-max))
    (let ((n (decode-coding-region 2 5 'utf-8)))
      (list n (point) (string-to-list (buffer-string)))))
  ;; raw-text / no-conversion in multibyte buffer vs unibyte destinations
  (with-temp-buffer
    (insert (string #xe9 ?\r ?\n))
    (let ((n (decode-coding-region (point-min) (point-max) 'raw-text)))
      (list n last-coding-system-used (string-to-list (buffer-string)))))
  (with-temp-buffer
    (insert (string #xe9 ?\r ?\n))
    (let ((n (decode-coding-region (point-min) (point-max) 'no-conversion)))
      (list n last-coding-system-used (string-to-list (buffer-string)))))
  (let ((s (decode-coding-string (string #xe9) 'no-conversion)))
    (list (multibyte-string-p s) (string-to-list s)))
  (with-temp-buffer
    (let ((dst (current-buffer)))
      (set-buffer-multibyte nil)
      (with-temp-buffer
        (insert (string #xe9 ?a))
        (let ((n (decode-coding-region (point-min) (point-max) 'raw-text dst)))
          (with-current-buffer dst
            (list n (string-to-list (buffer-string)))))))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((233) (1 (233)) (2 (195 169 97)) (2 5 (120 233 233 121)) (2 raw-text-dos (233 10)) (3 no-conversion (233 13 10)) (nil (23)) (2 (23 97)))""#
        ]],
    );
}

#[test]
fn decode_multibyte_custom_utf16_utf8_and_non_idempotent_translation() {
    let form = r#"(let ((tt1 (make-translation-table (list (cons ?a ?A) (cons #x3042 #x3044))))
      (tt-ni (make-char-table 'translation-table)))
  ;; Non-idempotent table (?a -> ?b, ?b -> ?c) detects double translation.
  (aset tt-ni ?a ?b)
  (aset tt-ni ?b ?c)
  (define-translation-table 'neomacs-mb-tt1 tt1)
  (define-translation-table 'neomacs-mb-tt-ni tt-ni)
  (defun neomacs-mb-bang (len)
    (save-excursion (insert "!"))
    (1+ len))
  (define-coding-system 'neomacs-mb-utf16le "test"
    :coding-type 'utf-16 :endian 'little :mnemonic ?U
    :decode-translation-table 'neomacs-mb-tt1)
  (define-coding-system 'neomacs-mb-utf16le-bom "test"
    :coding-type 'utf-16 :endian 'little :bom t :mnemonic ?U
    :decode-translation-table 'neomacs-mb-tt1)
  (define-coding-system 'neomacs-mb-utf8-tt "test"
    :coding-type 'utf-8 :mnemonic ?U
    :decode-translation-table 'neomacs-mb-tt1)
  (define-coding-system 'neomacs-mb-utf8-ni "test"
    :coding-type 'utf-8 :mnemonic ?U
    :decode-translation-table 'neomacs-mb-tt-ni)
  (define-coding-system 'neomacs-mb-hook-ni "test"
    :coding-type 'utf-8 :mnemonic ?H
    :decode-translation-table 'neomacs-mb-tt-ni
    :post-read-conversion 'neomacs-mb-bang)
  (define-coding-system 'neomacs-mb-cs-hook-ni "test"
    :coding-type 'charset :mnemonic ?C
    :charset-list '(ascii iso-8859-1)
    :decode-translation-table 'neomacs-mb-tt-ni
    :post-read-conversion 'neomacs-mb-bang)
  (list
   ;; Custom UTF-16LE on multibyte source (raw UTF-16 bytes + literal char)
   (string-to-list
    (decode-coding-string
     (string (unibyte-char-to-multibyte #x61) (unibyte-char-to-multibyte 0) #x3042)
     'neomacs-mb-utf16le))
   ;; Custom UTF-16LE with BOM after a literal char: BOM bytes not stripped post-start
   (string-to-list
    (decode-coding-string
     (string #x3042 (unibyte-char-to-multibyte #xff) (unibyte-char-to-multibyte #xfe)
             (unibyte-char-to-multibyte #x61) (unibyte-char-to-multibyte 0))
     'neomacs-mb-utf16le-bom))
   ;; Custom UTF-8 on multibyte source preserves incomplete bytes as eight-bit raw chars
   (string-to-list
    (decode-coding-string
     (string (unibyte-char-to-multibyte #xc3) #x3042 (unibyte-char-to-multibyte #xa9))
     'neomacs-mb-utf8-tt))
   ;; Non-idempotent table runs exactly once across non-hooked, HookCodec::Base (unibyte &
   ;; multibyte + DOS EOL), and dedicated-codec hooked multibyte decode
   (string-to-list
    (decode-coding-string
     (string ?a #xe9 (unibyte-char-to-multibyte #xc3) (unibyte-char-to-multibyte #xa9))
     'neomacs-mb-utf8-ni))
   (string-to-list
    (decode-coding-string
     (unibyte-string ?a #xc3 #xa9)
     'neomacs-mb-hook-ni))
   (let ((s (decode-coding-string (string ?a #xe9 ?\r ?\n) 'neomacs-mb-hook-ni)))
     (list (string-to-list s) last-coding-system-used))
   (string-to-list
    (decode-coding-string
     (string ?a #xe9)
     'neomacs-mb-cs-hook-ni))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((65 12356) (12356 65279 65) (4194243 12356 4194217) (98 233 233) (33 98 233) ((33 98 233 10) neomacs-mb-hook-ni-dos) (33 98 233))""#
        ]],
    );
}

#[test]
fn decode_multibyte_hook_utf8_signature_and_retained_undecided() {
    let form = r#"(let ((tt-ni (make-char-table 'translation-table)))
  (aset tt-ni ?a ?b)
  (aset tt-ni ?b ?c)
  (define-translation-table 'neomacs-mb-sig-ni tt-ni)
  (defun neomacs-mb-sig-bang (len)
    (save-excursion (insert "!"))
    (1+ len))
  (define-coding-system 'neomacs-mb-utf8-sig-hook "test"
    :coding-type 'utf-8 :bom t :mnemonic ?S
    :decode-translation-table 'neomacs-mb-sig-ni
    :post-read-conversion 'neomacs-mb-sig-bang)
  (define-coding-system 'neomacs-mb-ret-undec "test"
    :coding-type 'utf-8 :mnemonic ?R :charset-list '(unicode)
    :bom (cons 'utf-8-with-signature 'utf-8)
    :post-read-conversion 'neomacs-mb-sig-bang)
  (setcdr (coding-system-get 'neomacs-mb-ret-undec :bom) nil)
  (list
   ;; HookCodec::Utf8Signature: literal U+FEFF is NOT stripped as a raw UTF-8 BOM,
   ;; and non-idempotent translation runs once
   (string-to-list
    (decode-coding-string (string #xfeff ?a) 'neomacs-mb-utf8-sig-hook))
   ;; Raw UTF-8 BOM at stream start IS stripped, literal #xe9 preserved
   (string-to-list
    (decode-coding-string
     (string (unibyte-char-to-multibyte #xef) (unibyte-char-to-multibyte #xbb)
             (unibyte-char-to-multibyte #xbf) #xe9 ?a)
     'neomacs-mb-utf8-sig-hook))
   ;; DOS EOL + literal U+FEFF on HookCodec::Utf8Signature
   (let ((s (decode-coding-string (string #xfeff ?a ?\r ?\n) 'neomacs-mb-utf8-sig-hook)))
     (list (string-to-list s) last-coding-system-used))
   ;; HookCodec::RetainedUndecided: literal #xe9 preserved verbatim (never flattened)
   (let ((saved (coding-system-priority-list)))
     (unwind-protect
         (progn
           (set-coding-system-priority 'neomacs-mb-ret-undec)
           (list
            (let ((s (decode-coding-string (string #xe9 ?a) 'undecided)))
              (list (string-to-list s) last-coding-system-used))
            (let ((s (decode-coding-string (string #xe9 ?\r ?\n) 'undecided)))
              (list (string-to-list s) last-coding-system-used))))
       (apply #'set-coding-system-priority saved)))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((33 65279 98) (33 233 98) ((33 65279 98 10) neomacs-mb-utf8-sig-hook-dos) (((33 233 97) neomacs-mb-ret-undec) ((33 233 10) neomacs-mb-ret-undec-dos)))""#
        ]],
    );
}

#[test]
fn decode_multibyte_gb2312_set_buffer_multibyte_t_vs_to_and_charset_runs() {
    let form = r#"(list
  ;; `(set-buffer-multibyte t)` reinterprets 0xD2 0xBB (valid 2-byte UTF-8) as
  ;; literal U+04BB ("һ"); `decode-coding-region` preserves it verbatim.
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert #xd2 #xbb)
    (set-buffer-multibyte t)
    (decode-coding-region (point-min) (point-max) 'cn-gb-2312-unix)
    (list (string-to-list (buffer-string)) (get-text-property 0 'charset (buffer-string))))
  ;; `(set-buffer-multibyte 'to)` keeps 0xD2 0xBB as raw eight-bit chars;
  ;; `decode-coding-region` decodes GB2312 U+4E00 ("一") with charset property.
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert #xd2 #xbb)
    (set-buffer-multibyte 'to)
    (decode-coding-region (point-min) (point-max) 'cn-gb-2312-unix)
    (list (string-to-list (buffer-string)) (get-text-property 0 'charset (buffer-string))))
  ;; Non-UTF-8 GB2312 sequence 0xB0 0xA1 remains raw eight-bit under
  ;; `(set-buffer-multibyte t)` and decodes to U+554A ("啊") with charset property.
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert #xb0 #xa1)
    (set-buffer-multibyte t)
    (decode-coding-region (point-min) (point-max) 'cn-gb-2312-unix)
    (list (string-to-list (buffer-string)) (get-text-property 0 'charset (buffer-string))))
  ;; Round-trip encode-coding-region -> decode-coding-region on U+4E00 ("一").
  (with-temp-buffer
    (insert (string #x4e00))
    (encode-coding-region (point-min) (point-max) 'cn-gb-2312-unix)
    (decode-coding-region (point-min) (point-max) 'cn-gb-2312-unix)
    (list (string-to-list (buffer-string)) (get-text-property 0 'charset (buffer-string)))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (((1211) nil) ((19968) chinese-gb2312) ((21834) chinese-gb2312) ((19968) chinese-gb2312))""#
        ]],
    );
}
