//! GNU Emacs 31.1 parity for decoding into a unibyte destination buffer (#848).
//!
//! GNU Emacs `src/coding.c:10488-10505` (`Fdecode_coding_string`),
//! `src/coding.c:9578-9640` (`code_convert_string`), `src/coding.c:8131-8146`
//! (`decode_coding_object`), and `src/coding.c:7056-7067` (`produce_chars`):
//! when decoding into a unibyte buffer (`enable-multibyte-characters` is nil),
//! normal decoded characters (`!CHAR_BYTE8_P(c)`) emit the individual bytes of
//! Emacs's internal UTF-8 representation (`CHAR_STRING_ADVANCE_NO_UNIFY`),
//! raw eight-bit characters (`CHAR_BYTE8_P(c)`) emit their single raw byte
//! (`CHAR_TO_BYTE8`), point in the destination buffer does not move, and the
//! return value is `coding.produced_char` (decoded character count, not the
//! unibyte byte length). The same rule holds for `decode-coding-region`
//! (`src/coding.c:9505-9531`, with in-place point-after-region restoration
//! shifting by the byte delta `coding->produced - bytes` at `:8232`).

use crate::common::assert_oracle_parity_expect;
use expect_test::expect;

#[test]
fn decode_coding_string_into_unibyte_buffer_writes_internal_utf8_bytes_across_codings() {
    let form = r#"(progn
  (define-coding-system-alias 'neomacs-848-utf8-dos-alias 'utf-8-dos)
  (let ((run-case
         (lambda (input coding)
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (let ((n (decode-coding-string input coding nil (current-buffer))))
               (list n last-coding-system-used (string-to-list (buffer-string))))))))
    (list
     (funcall run-case (unibyte-string #xc3 #xa9 ?\r ?\n) 'utf-8-dos)
     (funcall run-case (unibyte-string #xc3 #xa9 ?\r) 'utf-8-mac)
     (funcall run-case (unibyte-string #xc3 #xa9 ?\n) 'utf-8-unix)
     (funcall run-case (unibyte-string #xc3 #xa9 ?\r ?\n) 'undecided-dos)
     (funcall run-case (unibyte-string #xc3 #xa9 ?\r) 'undecided-mac)
     (funcall run-case (unibyte-string #xc3 #xa9 ?\n) 'undecided-unix)
     (funcall run-case (unibyte-string #xc3 #xa9 ?\r ?\n) 'dos)
     (funcall run-case (unibyte-string #xc3 #xa9 ?\r) 'mac)
     (funcall run-case (unibyte-string #xc3 #xa9 ?\n) 'unix)
     (funcall run-case (unibyte-string #xe9 ?\r ?\n) 'iso-latin-1-dos)
     (funcall run-case (unibyte-string #xe9 ?\n) 'iso-latin-1-unix)
     (funcall run-case (unibyte-string #xfe #xff #x61 #x62 #x00 #x0a) 'utf-16be-with-signature-unix)
     (funcall run-case (unibyte-string #xc3 #xa9 ?\r ?\n) 'neomacs-848-utf8-dos-alias))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((2 utf-8-dos (195 169 10)) (2 utf-8-mac (195 169 10)) (2 utf-8-unix (195 169 10)) (2 utf-8-dos (195 169 10)) (2 utf-8-mac (195 169 10)) (2 utf-8-unix (195 169 10)) (2 utf-8-dos (195 169 10)) (2 utf-8-mac (195 169 10)) (2 utf-8-unix (195 169 10)) (2 iso-latin-1-dos (195 169 10)) (2 iso-latin-1-unix (195 169 10)) (2 utf-16be-with-signature-unix (230 133 162 10)) (2 neomacs-848-utf8-dos-alias (195 169 10)))""#
        ]],
    );
}

#[test]
fn decode_coding_string_into_unibyte_buffer_preserves_raw_bytes_point_and_props() {
    let form = r#"(list
  ;; Invalid raw byte #xff (CHAR_TO_BYTE8 -> 255) followed by valid UTF-8 é (-> 195 169)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (let ((n (decode-coding-string (unibyte-string #xff #xc3 #xa9) 'utf-8-unix nil (current-buffer))))
      (list n (point) (string-to-list (buffer-string)))))
  ;; raw-text-dos keeps raw byte #xff intact while converting CRLF
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (let ((n (decode-coding-string (unibyte-string #xff ?\r ?\n) 'raw-text-dos nil (current-buffer))))
      (list n (point) (string-to-list (buffer-string)))))
  ;; Mid-buffer insertion inserts internal UTF-8 bytes after point without moving point
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert "XY")
    (goto-char 2)
    (let ((n (decode-coding-string (unibyte-string #xc3 #xa9) 'utf-8-unix nil (current-buffer))))
      (list n (point) (string-to-list (buffer-string)))))
  ;; Charset text property from iso-latin-1 spans both internal UTF-8 bytes of é
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (let ((n (decode-coding-string (unibyte-string #xe9 ?a) 'iso-latin-1-unix nil (current-buffer))))
      (list n
            (string-to-list (buffer-string))
            (text-properties-at 1)
            (text-properties-at 2)
            (text-properties-at 3)))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((2 1 (255 195 169)) (2 1 (255 10)) (1 2 (88 195 169 89)) (2 (195 169 97) (charset iso-8859-1) (charset iso-8859-1) nil))""#
        ]],
    );
}

#[test]
fn decode_coding_string_post_read_conversion_into_unibyte_buffer() {
    let form = r#"(progn
  (defun neomacs-848-prc-fn (len)
    (save-excursion (insert "P"))
    (1+ len))
  (define-coding-system 'neomacs-848-prc "test"
    :coding-type 'utf-8 :mnemonic ?T
    :post-read-conversion 'neomacs-848-prc-fn)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (let ((n (decode-coding-string (unibyte-string #xc3 #xa9) 'neomacs-848-prc nil (current-buffer))))
      (list n (point) (string-to-list (buffer-string))))))"#;

    assert_oracle_parity_expect(form, expect![[r#""OK (2 1 (80 195 169))""#]]);
}

#[test]
fn decode_coding_region_into_unibyte_buffer_returns_decoded_char_count() {
    let form = r#"(list
  ;; In-place decode in unibyte buffer: returns decoded char count (2), point advances to byte end (4)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string #xe9 ?\r ?\n))
    (let ((n (decode-coding-region (point-min) (point-max) 'iso-latin-1-dos)))
      (list n (point) (string-to-list (buffer-string)))))
  ;; In-place decode with point after region: returns decoded char count (2), point shifts by byte delta
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string ?A #xe9 ?\r ?\n ?Z))
    (goto-char (point-max))
    (let ((n (decode-coding-region 2 5 'iso-latin-1-dos)))
      (list n (point) (string-to-list (buffer-string)))))
  ;; Separate unibyte destination buffer: returns decoded char count (2), point stays at 1
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (let ((dst (current-buffer)))
      (with-temp-buffer
        (set-buffer-multibyte nil)
        (insert (unibyte-string #xe9 ?\r ?\n))
        (let ((n (decode-coding-region (point-min) (point-max) 'iso-latin-1-dos dst)))
          (with-current-buffer dst
            (list n (point) (string-to-list (buffer-string))))))))
  ;; Regression guard: encode-coding-string into multibyte buffer produces eight-bit chars
  (with-temp-buffer
    (let ((n (encode-coding-string
              (decode-coding-string (unibyte-string #xc3 #xa9 ?\n) 'utf-8)
              'utf-8-dos nil (current-buffer))))
      (list n (point) (string-to-list (buffer-string))))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((2 4 (195 169 10)) (2 6 (65 195 169 10 90)) (2 1 (195 169 10)) (4 1 (4194243 4194217 13 10)))""#
        ]],
    );
}

/// Symmetry guard: `insert_coding_result` and `builtin_coding_region` are shared
/// across decode/encode and multibyte/unibyte destinations. Verify all four
/// combinations (`decode`/`encode` x `string`/`region`) into both multibyte and
/// unibyte destination buffers match GNU Emacs 31.1.
#[test]
fn coding_string_and_region_symmetric_encode_and_multibyte_destinations_match_gnu() {
    let form = r#"(let ((mb-str (decode-coding-string (unibyte-string #xc3 #xa9 ?\n) 'utf-8))
      (ub-str (unibyte-string #xc3 #xa9 ?\r ?\n)))
  (list
   ;; decode-coding-string -> multibyte buffer
   (with-temp-buffer
     (let ((n (decode-coding-string ub-str 'utf-8-dos nil (current-buffer))))
       (list n (point) (string-to-list (buffer-string)))))
   ;; encode-coding-string -> unibyte buffer
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (let ((n (encode-coding-string mb-str 'utf-8-dos nil (current-buffer))))
       (list n (point) (string-to-list (buffer-string)))))
   ;; encode-coding-string -> multibyte buffer
   (with-temp-buffer
     (let ((n (encode-coding-string mb-str 'utf-8-dos nil (current-buffer))))
       (list n (point) (string-to-list (buffer-string)))))
   ;; decode-coding-region -> multibyte buffer (in-place and separate)
   (with-temp-buffer
     (insert (string-as-multibyte (unibyte-string #xc3 #xa9 ?\r ?\n)))
     (let ((n (decode-coding-region (point-min) (point-max) 'utf-8-dos)))
       (list n (point) (string-to-list (buffer-string)))))
   (with-temp-buffer
     (let ((dst (current-buffer)))
       (with-temp-buffer
         (set-buffer-multibyte nil)
         (insert ub-str)
         (let ((n (decode-coding-region (point-min) (point-max) 'utf-8-dos dst)))
           (with-current-buffer dst
             (list n (point) (string-to-list (buffer-string))))))))
   ;; encode-coding-region -> multibyte buffer (in-place and separate)
   (with-temp-buffer
     (insert mb-str)
     (let ((n (encode-coding-region (point-min) (point-max) 'utf-8-dos)))
       (list n (point) (string-to-list (buffer-string)))))
   (with-temp-buffer
     (let ((dst (current-buffer)))
       (with-temp-buffer
         (insert mb-str)
         (let ((n (encode-coding-region (point-min) (point-max) 'utf-8-dos dst)))
           (with-current-buffer dst
             (list n (point) (string-to-list (buffer-string))))))))
   ;; encode-coding-region -> separate unibyte buffer
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (let ((dst (current-buffer)))
       (with-temp-buffer
         (insert mb-str)
         (let ((n (encode-coding-region (point-min) (point-max) 'utf-8-dos dst)))
           (with-current-buffer dst
             (list n (point) (string-to-list (buffer-string))))))))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((2 1 (233 10)) (4 1 (195 169 13 10)) (4 1 (4194243 4194217 13 10)) (2 3 (233 10)) (2 1 (233 10)) (4 5 (4194243 4194217 13 10)) (4 1 (4194243 4194217 13 10)) (4 1 (195 169 13 10)))""#
        ]],
    );
}

/// GNU `decode_coding_object` (`src/coding.c:8152-8197`): when decoding into a
/// unibyte destination buffer with `:post-read-conversion`, the decoder first
/// writes internal UTF-8 bytes into the unibyte buffer and then runs the hook
/// IN that unibyte buffer. Characters inserted by the hook (e.g. U+1EC7 from
/// `viqr-post-read-conversion`) truncate to their low byte (199 = 0xC7), while
/// decoder-produced non-ASCII chars remain internal UTF-8 bytes (`195 169`).
#[test]
fn decode_coding_string_vietnamese_viqr_non_ascii_buffer_destinations() {
    let form = r#"(progn
  (defun neomacs-848-cm-run-viqr (nocopy dst)
    (let ((orig (propertize (unibyte-string ?V ?i ?e ?^ ?. ?t #xc3 #xa9) 'foo 'bar))
          (last-coding-system-used 'sentinel))
      (pcase dst
        ((or 'nil 't)
         (let ((res (decode-coding-string orig 'vietnamese-viqr nocopy dst)))
           (list 'ok :eq (eq res orig) :mb (multibyte-string-p res)
                 :bytes (string-to-list res) :last last-coding-system-used)))
        ((or 'buf-mb 'buf-uni)
         (with-temp-buffer
           (when (eq dst 'buf-uni) (set-buffer-multibyte nil))
           (let* ((pt-before (point))
                  (res (decode-coding-string orig 'vietnamese-viqr nocopy (current-buffer)))
                  (pt-after (point)))
             (list 'ok :ret res :pt-same (= pt-before pt-after)
                   :mb enable-multibyte-characters
                   :bytes (string-to-list (buffer-string))
                   :last last-coding-system-used)))))))
  (list (neomacs-848-cm-run-viqr nil nil)
        (neomacs-848-cm-run-viqr nil t)
        (neomacs-848-cm-run-viqr nil 'buf-mb)
        (neomacs-848-cm-run-viqr nil 'buf-uni)
        (neomacs-848-cm-run-viqr t 'buf-uni)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((ok :eq nil :mb t :bytes (86 105 7879 116 233) :last vietnamese-viqr) (ok :eq nil :mb t :bytes (86 105 7879 116 233) :last vietnamese-viqr) (ok :ret 5 :pt-same t :mb t :bytes (86 105 7879 116 233) :last vietnamese-viqr) (ok :ret 5 :pt-same t :mb nil :bytes (86 105 199 116 195 169) :last vietnamese-viqr) (ok :ret 5 :pt-same t :mb nil :bytes (86 105 199 116 195 169) :last vietnamese-viqr))""#
        ]],
    );
}

/// Mixed decoder output (raw byte `#xff` + UTF-8 `é`) plus hook-inserted
/// multibyte char (`U+1EC7`): GNU passes `coding->produced_char` (`2`, not
/// unibyte byte length `3`) as `LEN`, runs the hook in a unibyte buffer so
/// `U+1EC7` truncates to `199`, and updates `coding->produced_char += Z - prev_Z`
/// (`2 + (4 - 3) = 3`). Also pins `vietnamese-viqr` narrowing to `LEN = 4`
/// (decoded chars, not 5 unibyte bytes) on `\xc3\xa9e^.`.
#[test]
fn decode_coding_string_post_read_hook_truncates_in_unibyte_buffer_and_tracks_produced_char() {
    let form = r#"(progn
  (defvar neomacs-848-hook-obs nil)
  (defun neomacs-848-mixed-prc (len)
    (setq neomacs-848-hook-obs
          (list len (- (point-max) (point-min)) enable-multibyte-characters
                (string-to-list (buffer-string))))
    (save-excursion
      (goto-char (point-max))
      (insert (string #x1ec7)))
    len)
  (define-coding-system 'neomacs-848-mixed-prc-cs "test"
    :coding-type 'utf-8 :mnemonic ?M
    :post-read-conversion 'neomacs-848-mixed-prc)
  (list
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (let ((n (decode-coding-string (unibyte-string #xff #xc3 #xa9)
                                    'neomacs-848-mixed-prc-cs nil (current-buffer))))
       (list n (point) neomacs-848-hook-obs (string-to-list (buffer-string)))))
   (with-temp-buffer
     (let ((n (decode-coding-string (unibyte-string #xff #xc3 #xa9)
                                    'neomacs-848-mixed-prc-cs nil (current-buffer))))
       (list n (point) neomacs-848-hook-obs (string-to-list (buffer-string)))))
   ;; LEN = coding->produced_char (4), NOT unibyte byte count (5): viqr narrows
   ;; to bytes 1..5 and converts "e^" -> 234 leaving "." (46) untouched.
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (let ((n (decode-coding-string (unibyte-string #xc3 #xa9 ?e ?^ ?.)
                                    'vietnamese-viqr nil (current-buffer))))
       (list n (point) (string-to-list (buffer-string)))))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((3 1 (2 3 nil (255 195 169)) (255 195 169 199)) (3 1 (2 2 t (4194303 233)) (4194303 233 7879)) (3 1 (195 169 234 46)))""#
        ]],
    );
}

/// `decode-coding-region` with `:post-read-conversion` into unibyte buffers
/// (both in-place and separate buffer) runs the hook in unibyte mode, truncates
/// hook-inserted multibyte chars, restores in-place point by byte length
/// (`coding->produced`), and returns `coding->produced_char`.
#[test]
fn decode_coding_region_post_read_hook_into_unibyte_buffer() {
    let form = r#"(progn
  (defvar neomacs-848-reg-obs nil)
  (defun neomacs-848-reg-prc (len)
    (setq neomacs-848-reg-obs
          (list len (- (point-max) (point-min)) enable-multibyte-characters))
    (save-excursion
      (goto-char (point-max))
      (insert (string #x1ec7)))
    len)
  (define-coding-system 'neomacs-848-reg-prc-cs "test"
    :coding-type 'utf-8 :mnemonic ?R
    :post-read-conversion 'neomacs-848-reg-prc)
  (list
   ;; Pure-ASCII VIQR in-place in unibyte buffer
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (insert "Vie^.t")
     (let ((n (decode-coding-region (point-min) (point-max) 'vietnamese-viqr)))
       (list n (point) (string-to-list (buffer-string)))))
   ;; Pure-ASCII VIQR in-place in multibyte buffer
   (with-temp-buffer
     (insert "Vie^.t")
     (let ((n (decode-coding-region (point-min) (point-max) 'vietnamese-viqr)))
       (list n (point) (string-to-list (buffer-string)))))
   ;; Pure-ASCII VIQR into separate unibyte buffer
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (let ((dst (current-buffer)))
       (with-temp-buffer
         (set-buffer-multibyte nil)
         (insert "Vie^.t")
         (let ((n (decode-coding-region (point-min) (point-max) 'vietnamese-viqr dst)))
           (with-current-buffer dst
             (list n (point) (string-to-list (buffer-string))))))))
   ;; Non-ASCII VIQR in-place in unibyte buffer: ret=5, pt=7, bytes=(86 105 199 116 195 169)
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (insert (unibyte-string ?V ?i ?e ?^ ?. ?t #xc3 #xa9))
     (let ((n (decode-coding-region (point-min) (point-max) 'vietnamese-viqr)))
       (list n (point) (string-to-list (buffer-string)))))
   ;; Non-ASCII VIQR into separate unibyte buffer
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (let ((dst (current-buffer)))
       (with-temp-buffer
         (set-buffer-multibyte nil)
         (insert (unibyte-string ?V ?i ?e ?^ ?. ?t #xc3 #xa9))
         (let ((n (decode-coding-region (point-min) (point-max) 'vietnamese-viqr dst)))
           (with-current-buffer dst
             (list n (point) (string-to-list (buffer-string))))))))
   ;; Custom UTF-8 + hook-inserted U+1EC7 into separate unibyte buffer
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (let ((dst (current-buffer)))
       (with-temp-buffer
         (set-buffer-multibyte nil)
         (insert (unibyte-string #xc3 #xa9))
         (let ((n (decode-coding-region (point-min) (point-max) 'neomacs-848-reg-prc-cs dst)))
           (with-current-buffer dst
             (list n (point) neomacs-848-reg-obs (string-to-list (buffer-string))))))))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((4 5 (86 105 199 116)) (4 5 (86 105 7879 116)) (4 1 (86 105 199 116)) (5 7 (86 105 199 116 195 169)) (5 1 (86 105 199 116 195 169)) (2 1 (1 2 nil) (195 169 199)))""#
        ]],
    );
}
