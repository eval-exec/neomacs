//! GNU Emacs 31.1 parity for `:decode-translation-table`,
//! `:encode-translation-table`, `standard-translation-table-for-decode`,
//! `standard-translation-table-for-encode`, and `enable-character-translation`.
//!
//! GNU Emacs 31.1 (`a360712c9d27`) source contract:
//! - `src/coding.c:6862-6991` (`get_translation_table`,
//!   `LOOKUP_TRANSLATION_TABLE`, `get_translation`)
//! - `src/coding.c:7003-7082` (`produce_chars` — decode translation runs after
//!   decoding and before `decode_eol` / `:post-read-conversion`, skipped when
//!   `chars_at_source` is set by `raw-text`/`no-conversion`)
//! - `src/coding.c:7607-7734, 7769-7773` (`consume_chars` / `encode_coding` —
//!   encode translation runs after `:pre-write-conversion` and EOL expansion
//!   except DOS's literal `\r`, skipped for `encode_coding_raw_text`)
//! - `src/coding.c:7934` (`decode_coding_gap` disables ASCII optimization when
//!   a decode translation table is active)
//! - `src/coding.c:9065-9082, 9143-9144, 9243, 9300, 9396-9397`
//!   (`char_encodable_p`, `find-coding-systems-region-internal`,
//!   `unencodable-char-position`, `check-coding-systems-region` apply
//!   `translate_char` from `src/character.c:151-167`)
//! - `src/coding.c:11071-11081, 11548-11559` (`define-coding-system-internal`
//!   and `coding-system-put` store typed translation table attributes)

use crate::common::assert_oracle_parity_expect;
use expect_test::expect;

#[test]
fn decode_and_encode_translation_tables_basic_chained_and_symbols() {
    let form = r#"(let* ((dec-tbl (make-translation-table-from-alist
                     (list (cons ?a ?A) (cons ?b [?B ?1]) (cons #xe9 ?E))))
       (enc-tbl (make-translation-table-from-alist
                 (list (cons ?A ?a) (cons #xe9 ?e))))
       (dec-tbl2 (make-translation-table-from-alist
                  (list (cons ?A ?Z) (cons ?E ?W))))
       (enc-tbl2 (make-translation-table-from-alist
                  (list (cons ?a ?x) (cons ?e ?y))))
       (_ (define-translation-table 'neomacs-tt-sym-dec
            (list (cons ?a ?A) (cons #xe9 ?E))))
       (_ (define-translation-table 'neomacs-tt-sym-enc
            (list (cons ?A ?a) (cons #xe9 ?e))))
       (_ (define-coding-system 'neomacs-tt-utf8 "tt utf8"
            :coding-type 'utf-8 :mnemonic ?T
            :decode-translation-table dec-tbl
            :encode-translation-table enc-tbl))
       (_ (define-coding-system 'neomacs-tt-chain "tt chain"
            :coding-type 'utf-8 :mnemonic ?T
            :decode-translation-table (list 'neomacs-tt-sym-dec dec-tbl2)
            :encode-translation-table (list 'neomacs-tt-sym-enc enc-tbl2)))
       (_ (define-coding-system 'neomacs-tt-raw "tt raw"
            :coding-type 'raw-text :mnemonic ?R
            :decode-translation-table dec-tbl
            :encode-translation-table enc-tbl))
       (_ (define-coding-system-internal 'neomacs-tt-internal ?I 'utf-8
            (list 'unicode) t dec-tbl enc-tbl nil nil nil nil nil nil nil))
       (_ (define-coding-system-alias 'neomacs-tt-alias 'neomacs-tt-internal))
       (internal-before
        (list (decode-coding-string (unibyte-string ?a #xc3 #xa9) 'neomacs-tt-internal)
              (encode-coding-string (string ?A #xe9) 'neomacs-tt-internal)
              (not (null (coding-system-get 'neomacs-tt-internal :decode-translation-table)))
              (not (null (coding-system-get 'neomacs-tt-internal :encode-translation-table)))))
       (_ (coding-system-put 'neomacs-tt-alias :decode-translation-table
                             (make-translation-table-from-alist
                              (list (cons ?a ?Z) (cons #xe9 ?Q))))))
  (list
   ;; Basic decode (including 1-to-many vector) and encode
   (decode-coding-string (unibyte-string ?a ?b #xc3 #xa9) 'neomacs-tt-utf8)
   (encode-coding-string (string ?A #xe9 ?c) 'neomacs-tt-utf8)
   ;; Symbol-named + chained translation table list
   (decode-coding-string (unibyte-string ?a #xc3 #xa9) 'neomacs-tt-chain)
   (encode-coding-string (string ?A #xe9) 'neomacs-tt-chain)
   ;; enable-character-translation nil disables both directions
   (let ((enable-character-translation nil))
     (list (decode-coding-string (unibyte-string ?a ?b #xc3 #xa9) 'neomacs-tt-utf8)
           (string-to-list (encode-coding-string (string ?A #xe9) 'neomacs-tt-utf8))))
   ;; standard-translation-table-for-decode / -encode chain after coding table
   (let ((standard-translation-table-for-decode dec-tbl2)
         (standard-translation-table-for-encode enc-tbl2))
     (list (decode-coding-string (unibyte-string ?a #xc3 #xa9) 'neomacs-tt-utf8)
           (encode-coding-string (string ?A #xe9) 'neomacs-tt-utf8)))
   ;; standard-translation-table-for-* on built-in utf-8 and undecided
   (let ((standard-translation-table-for-decode dec-tbl)
         (standard-translation-table-for-encode enc-tbl))
     (list (decode-coding-string (unibyte-string ?a #xc3 #xa9) 'utf-8)
           (encode-coding-string (string ?A #xe9) 'utf-8)
           (decode-coding-string (unibyte-string ?a #xc3 #xa9) 'undecided)))
   ;; raw-text and no-conversion skip translation in both directions
   (let ((standard-translation-table-for-decode dec-tbl)
         (standard-translation-table-for-encode enc-tbl))
     (list (string-to-list (decode-coding-string (unibyte-string ?a #xe9) 'neomacs-tt-raw))
           (string-to-list (encode-coding-string (string ?A #xe9) 'neomacs-tt-raw))
           (string-to-list (decode-coding-string (unibyte-string ?a #xe9) 'no-conversion))
           (string-to-list (encode-coding-string (string ?A #xe9) 'no-conversion))))
   ;; define-coding-system-internal with nil plist + coding-system-put via alias
   internal-before
   (decode-coding-string (unibyte-string ?a #xc3 #xa9) 'neomacs-tt-internal)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (\"AB1E\" \"aec\" \"ZW\" \"xy\" (\"abé\" (65 195 169)) (\"ZW\" \"xy\") (\"AE\" \"ae\" \"AE\") ((97 4194281) (65 195 169) (97 233) (65 195 169)) (\"AE\" \"ae\" nil nil) \"ZQ\")""#
        ]],
    );
}

#[test]
fn translation_tables_multi_char_sequences_and_unmatched_prefixes() {
    let form = r#"(let* ((multi-dec (make-translation-table-from-alist
                       (list (cons [?x ?y] [?X ?Y]) (cons [?x ?z] ?Q))))
       (multi-enc (make-translation-table-from-alist
                   (list (cons [?X ?Y] [?x ?y]) (cons [?X ?Z] ?q))))
       (_ (define-coding-system 'neomacs-tt-multi "tt multi"
            :coding-type 'utf-8 :mnemonic ?M
            :decode-translation-table multi-dec
            :encode-translation-table multi-enc)))
  (list
   (decode-coding-string (unibyte-string ?x ?y ?  ?x ?z ?  ?x ?a #xc3 #xa9) 'neomacs-tt-multi)
   (decode-coding-string (unibyte-string #xc3 #xa9 ?x) 'neomacs-tt-multi)
   (string-to-list
    (encode-coding-string (concat "XY XZ Xa" (string #xe9)) 'neomacs-tt-multi))
   (string-to-list
    (encode-coding-string (concat (string #xe9) "X") 'neomacs-tt-multi))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[r#""OK (\"XY Q xaé\" \"éx\" (120 121 32 113 32 97 195 169) (195 169))""#]],
    );
}

#[test]
fn translation_tables_ascii_fast_path_regions_buffers_and_files() {
    let form = r#"(let* ((dec-tbl (make-translation-table-from-alist
                     (list (cons ?a ?A) (cons ?b [?B ?1]) (cons #xe9 ?E))))
       (enc-tbl (make-translation-table-from-alist
                 (list (cons ?A ?a) (cons #xe9 ?e))))
       (_ (define-coding-system 'neomacs-tt-fast-utf8 "tt fast utf8"
            :coding-type 'utf-8 :mnemonic ?F
            :decode-translation-table dec-tbl
            :encode-translation-table enc-tbl))
       (_ (define-coding-system 'neomacs-tt-fast-utf16 "tt fast utf16"
            :coding-type 'utf-16 :mnemonic ?U
            :endian 'little
            :decode-translation-table dec-tbl
            :encode-translation-table enc-tbl)))
  (list
   ;; Pure-ASCII code_convert_string fast path on ASCII-compatible coding
   (decode-coding-string "ab c" 'neomacs-tt-fast-utf8)
   (encode-coding-string "A" 'neomacs-tt-fast-utf8)
   ;; Non-ASCII-compatible coding (utf-16le) does NOT take the ASCII fast path
   (decode-coding-string (unibyte-string ?a 0) 'neomacs-tt-fast-utf16)
   (string-to-list (encode-coding-string "A" 'neomacs-tt-fast-utf16))
   ;; Buffer destinations bypass code_convert_string ASCII fast path
   (with-temp-buffer
     (decode-coding-string "ab" 'neomacs-tt-fast-utf8 nil (current-buffer))
     (buffer-string))
   (with-temp-buffer
     (encode-coding-string "A" 'neomacs-tt-fast-utf8 nil (current-buffer))
     (buffer-string))
   ;; decode-coding-region / encode-coding-region always translate pure-ASCII text
   (with-temp-buffer
     (insert "ab")
     (decode-coding-region (point-min) (point-max) 'neomacs-tt-fast-utf8)
     (buffer-string))
   (with-temp-buffer
     (insert "A")
     (encode-coding-region (point-min) (point-max) 'neomacs-tt-fast-utf8)
     (buffer-string))
   (with-temp-buffer
     (set-buffer-multibyte nil)
     (insert "A")
     (encode-coding-region (point-min) (point-max) 'neomacs-tt-fast-utf8)
     (buffer-string))
   ;; insert-file-contents (decode_coding_gap) skips ASCII opt when table is active
   (let* ((temporary-file-directory "/tmp")
          (f (make-temp-file "neomacs-tt-gap-")))
     (unwind-protect
         (progn
           (with-temp-file f
             (set-buffer-multibyte nil)
             (insert "ab"))
           (with-temp-buffer
             (let ((coding-system-for-read 'neomacs-tt-fast-utf8))
               (insert-file-contents f))
             (buffer-string)))
       (when (file-exists-p f) (delete-file f))))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (\"ab c\" \"A\" \"A\" (97 0) \"AB1\" \"a\" \"AB1\" \"a\" \"a\" \"AB1\")""#
        ]],
    );
}

#[test]
fn translation_tables_eol_and_conversion_hook_ordering() {
    let form = r#"(progn
  (defun neomacs-tt-post-read (len)
    (save-excursion (insert "!") (1+ len)))
  (defun neomacs-tt-pre-write (_from to)
    (save-excursion (goto-char to) (insert "A")))
  (let* ((eol-dec (make-translation-table-from-alist
                   (list (cons ?\r ?R) (cons ?\n ?N) (cons ?a ?A))))
         (eol-enc (make-translation-table-from-alist
                   (list (cons ?\r ?R) (cons ?\n ?N) (cons ?A ?a))))
         (hook-dec (make-translation-table-from-alist
                    (list (cons ?a ?A) (cons #xe9 ?E))))
         (hook-enc (make-translation-table-from-alist
                    (list (cons ?A ?a) (cons #xe9 ?e))))
         (_ (define-coding-system 'neomacs-tt-dos "tt dos"
              :coding-type 'utf-8 :mnemonic ?D :eol-type 'dos
              :decode-translation-table eol-dec
              :encode-translation-table eol-enc))
         (_ (define-coding-system 'neomacs-tt-mac "tt mac"
              :coding-type 'utf-8 :mnemonic ?M :eol-type 'mac
              :decode-translation-table eol-dec
              :encode-translation-table eol-enc))
         (_ (define-coding-system 'neomacs-tt-hook "tt hook"
              :coding-type 'utf-8 :mnemonic ?H
              :decode-translation-table hook-dec
              :encode-translation-table hook-enc
              :post-read-conversion 'neomacs-tt-post-read
              :pre-write-conversion 'neomacs-tt-pre-write)))
    (list
     ;; Decode translation runs before decode_eol
     (decode-coding-string "a\r\n" 'neomacs-tt-dos)
     (decode-coding-string "a\r" 'neomacs-tt-mac)
     ;; Encode: DOS emits untranslated \r then translates \n; Mac maps \n->\r then translates \r
     (string-to-list (encode-coding-string "A\n" 'neomacs-tt-dos))
     (encode-coding-string "A\n" 'neomacs-tt-mac)
     ;; Hook ordering: decode -> translate -> post-read; pre-write -> translate -> encode
     (decode-coding-string (unibyte-string ?a #xc3 #xa9) 'neomacs-tt-hook)
     (encode-coding-string (string #xe9) 'neomacs-tt-hook))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[r#""OK (\"ARN\" \"AR\" (97 13 78) \"aR\" \"!AE\" \"ea\")""#]],
    );
}

#[test]
fn translation_tables_encodability_queries() {
    let form = r#"(let* ((enc-tbl (make-translation-table-from-alist
                     (list (cons #xe9 ?e))))
       (enc-step1 (make-translation-table-from-alist
                   (list (cons #xe9 #xe8))))
       (enc-step2 (make-translation-table-from-alist
                   (list (cons #xe8 ?e))))
       (enc-vec (make-translation-table-from-alist
                 (list (cons #xe9 [?e ?1]))))
       (_ (define-translation-table 'neomacs-tt-enc-s1
            (list (cons #xe9 #xe8))))
       (_ (define-coding-system 'neomacs-tt-ascii "tt ascii"
            :coding-type 'charset :mnemonic ?A
            :charset-list '(ascii)
            :encode-translation-table enc-tbl))
       (_ (define-coding-system 'neomacs-tt-ascii-chain "tt ascii chain"
            :coding-type 'charset :mnemonic ?C
            :charset-list '(ascii)
            :encode-translation-table (list 'neomacs-tt-enc-s1 enc-step2)))
       (_ (define-coding-system 'neomacs-tt-ascii-plain "tt ascii plain"
            :coding-type 'charset :mnemonic ?P
            :charset-list '(ascii)))
       (_ (define-coding-system 'neomacs-tt-ascii-vec "tt ascii vec"
            :coding-type 'charset :mnemonic ?V
            :charset-list '(ascii)
            :encode-translation-table enc-vec))
       (sample (string ?a #xe9 ?b)))
  (list
   ;; unencodable-char-position
   (unencodable-char-position 0 3 'neomacs-tt-ascii nil sample)
   (unencodable-char-position 0 3 'neomacs-tt-ascii-chain nil sample)
   (let ((enable-character-translation nil))
     (unencodable-char-position 0 3 'neomacs-tt-ascii nil sample))
   (let ((standard-translation-table-for-encode enc-tbl))
     (unencodable-char-position 0 3 'neomacs-tt-ascii-plain nil sample))
   ;; Vector translation is ignored by translate_char, but encode still expands it
   (unencodable-char-position 0 3 'neomacs-tt-ascii-vec nil sample)
   (encode-coding-string sample 'neomacs-tt-ascii-vec)
   ;; check-coding-systems-region (string & buffer)
   (check-coding-systems-region sample nil '(neomacs-tt-ascii neomacs-tt-ascii-chain neomacs-tt-ascii-plain))
   (let ((enable-character-translation nil))
     (check-coding-systems-region sample nil '(neomacs-tt-ascii neomacs-tt-ascii-plain)))
   (with-temp-buffer
     (insert sample)
     (check-coding-systems-region (point-min) (point-max) '(neomacs-tt-ascii neomacs-tt-ascii-plain)))
   ;; find-coding-systems-region-internal
   (not (null (memq 'neomacs-tt-ascii (find-coding-systems-region-internal sample nil nil))))
   (not (null (memq 'neomacs-tt-ascii-chain (find-coding-systems-region-internal sample nil nil))))
   (let ((enable-character-translation nil))
     (not (null (memq 'neomacs-tt-ascii (find-coding-systems-region-internal sample nil nil)))))
   (let ((standard-translation-table-for-encode enc-tbl))
     (not (null (memq 'neomacs-tt-ascii-plain (find-coding-systems-region-internal sample nil nil)))))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (nil nil 1 nil 1 \"ae1b\" ((neomacs-tt-ascii-plain 1)) ((neomacs-tt-ascii 1) (neomacs-tt-ascii-plain 1)) ((neomacs-tt-ascii-plain 2)) t t nil t)""#
        ]],
    );
}
