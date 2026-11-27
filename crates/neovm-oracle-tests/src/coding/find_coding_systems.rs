//! GNU Emacs 31.1 parity for coding repertoire expansion (`:charset-list
//! 'iso-2022` / `'emacs-mule`), `find-coding-systems-region-internal`
//! candidate ordering (`Vcoding_system_list`), and fixed-EOL base bucket
//! resolution.
//!
//! GNU Emacs 31.1 (`a360712c9d27`) source contract:
//! - `Fdefine_coding_system_internal` (`src/coding.c:11017-11031`):
//!   - `:charset-list 'iso-2022` expands `CODING_ATTR_CHARSET_LIST` to
//!     `Viso_2022_charset_list`; `:charset-list 'emacs-mule` expands it to
//!     `Vemacs_mule_charset_list`.
//! - `Ffind_coding_systems_region_internal` (`src/coding.c:9091-9210`):
//!   - Walks `Vcoding_system_list` (`:9134-9147`), keeping each entry `sym`
//!     not in `exclude` for which `EQ(sym, CODING_ATTR_BASE_NAME(attrs))`
//!     (canonical base coding systems, including fixed-EOL bases whose own
//!     names end in `-unix`/`-dos`/`-mac`, while skipping aliases and
//!     auto-generated `-unix`/`-dos`/`-mac` subsidiaries of undecided bases).
//!   - Filters by `char_encodable_p` (`:9064-9082`) and reverses back onto
//!     `(raw-text no-conversion)` (`:9204-9209`), preserving exact
//!     `Vcoding_system_list` order.

use crate::common::assert_oracle_parity_expect;
use expect_test::expect;

#[test]
fn coding_repertoire_expands_iso2022_and_emacs_mule_markers() {
    let form = r#"(list
  (unencodable-char-position 0 2 'emacs-mule nil (string ?a #xe9))
  (unencodable-char-position 0 2 'emacs-mule nil (string ?a #x3042))
  (unencodable-char-position 0 2 'emacs-mule nil (string ?a #x1f600))
  (unencodable-char-position 0 2 'iso-2022-7bit nil (string ?a #xe9))
  (unencodable-char-position 0 2 'iso-2022-7bit nil (string ?a #x3042))
  (unencodable-char-position 0 2 'iso-2022-7bit nil (string ?a #x1f600))
  (unencodable-char-position 0 2 'compound-text nil (string ?a #xe9))
  (unencodable-char-position 0 2 'compound-text nil (string ?a #x1f600))
  (check-coding-systems-region (string ?a #xe9 #x3042 #x1f600) nil '(emacs-mule iso-2022-7bit compound-text utf-8)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (nil nil 1 nil nil 1 nil 1 ((emacs-mule 3) (iso-2022-7bit 3) (compound-text 3)))""#
        ]],
    );
}

#[test]
fn find_coding_systems_region_internal_vcoding_system_list_order() {
    let form = r#"(list
  (find-coding-systems-region-internal (string ?a #x1f600) nil nil)
  (find-coding-systems-region-internal (string ?a #xe9) nil nil)
  (find-coding-systems-region-internal (string ?a #x3042) nil nil)
  (find-coding-systems-region-internal (string ?a #xe9) nil '(utf-8 raw-text no-conversion)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((chinese-gb18030 utf-7-imap utf-7 utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le utf-8-emacs utf-8-auto utf-8-with-signature utf-8 prefer-utf-8 raw-text no-conversion) (georgian-academy georgian-ps windows-1256 windows-1258 vietnamese-viqr vietnamese-vscii vietnamese-viscii japanese-shift-jis-2004 euc-jis-2004 iso-2022-jp-2004 eucjp-ms japanese-iso-8bit japanese-iso-7bit-1978-irv iso-2022-jp-2 cp851 iso-latin-10 hp-roman8 next mac-roman cp437 cp865 cp863 cp861 cp860 cp858 cp857 cp852 cp850 cp775 ibm297 ibm285 ibm284 ibm280 ibm278 ibm277 ibm275 ibm274 ibm273 ibm256 windows-1257 windows-1254 windows-1252 windows-1250 iso-latin-9 iso-latin-8 iso-latin-7 iso-latin-6 iso-latin-5 iso-latin-4 iso-latin-3 iso-latin-2 ibm1047 chinese-gb18030 chinese-gbk chinese-hz chinese-iso-8bit iso-2022-cn-ext iso-2022-cn utf-7-imap utf-7 compound-text-with-extensions ctext-no-compositions compound-text iso-2022-8bit-ss2 iso-2022-7bit-lock-ss2 iso-2022-7bit-lock iso-2022-7bit-ss2 iso-2022-7bit utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le utf-8-emacs utf-8-auto utf-8-with-signature utf-8 emacs-mule iso-latin-1 prefer-utf-8 raw-text no-conversion) (korean-cp949 iso-2022-kr korean-iso-8bit japanese-shift-jis-2004 euc-jis-2004 iso-2022-jp-2004 eucjp-ms japanese-iso-8bit japanese-iso-7bit-1978-irv japanese-cp932 japanese-shift-jis iso-2022-jp-2 iso-2022-jp chinese-gb18030 chinese-gbk chinese-big5-hkscs chinese-hz chinese-iso-8bit iso-2022-cn-ext iso-2022-cn utf-7-imap utf-7 compound-text-with-extensions ctext-no-compositions compound-text iso-2022-8bit-ss2 iso-2022-7bit-lock-ss2 iso-2022-7bit-lock iso-2022-7bit-ss2 iso-2022-7bit utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le utf-8-emacs utf-8-auto utf-8-with-signature utf-8 emacs-mule prefer-utf-8 raw-text no-conversion) (georgian-academy georgian-ps windows-1256 windows-1258 vietnamese-viqr vietnamese-vscii vietnamese-viscii japanese-shift-jis-2004 euc-jis-2004 iso-2022-jp-2004 eucjp-ms japanese-iso-8bit japanese-iso-7bit-1978-irv iso-2022-jp-2 cp851 iso-latin-10 hp-roman8 next mac-roman cp437 cp865 cp863 cp861 cp860 cp858 cp857 cp852 cp850 cp775 ibm297 ibm285 ibm284 ibm280 ibm278 ibm277 ibm275 ibm274 ibm273 ibm256 windows-1257 windows-1254 windows-1252 windows-1250 iso-latin-9 iso-latin-8 iso-latin-7 iso-latin-6 iso-latin-5 iso-latin-4 iso-latin-3 iso-latin-2 ibm1047 chinese-gb18030 chinese-gbk chinese-hz chinese-iso-8bit iso-2022-cn-ext iso-2022-cn utf-7-imap utf-7 compound-text-with-extensions ctext-no-compositions compound-text iso-2022-8bit-ss2 iso-2022-7bit-lock-ss2 iso-2022-7bit-lock iso-2022-7bit-ss2 iso-2022-7bit utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le utf-8-emacs utf-8-auto utf-8-with-signature emacs-mule iso-latin-1 prefer-utf-8 raw-text no-conversion))""#
        ]],
    );
}

#[test]
fn find_coding_systems_region_internal_custom_and_fixed_eol_bases() {
    let form = r#"(progn
  (define-coding-system 'fcs-fixed-unix "d"
    :coding-type 'utf-8 :charset-list '(unicode) :mnemonic ?Z :eol-type 'unix)
  (define-coding-system 'fcs-fixed "d"
    :coding-type 'utf-8 :charset-list '(unicode) :mnemonic ?Z :eol-type 'unix)
  (define-coding-system 'fcs-undec "d"
    :coding-type 'utf-8 :charset-list '(unicode) :mnemonic ?Z)
  (define-coding-system-alias 'fcs-alias 'fcs-undec)
  (find-coding-systems-region-internal (string ?a #x1f600) nil nil))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (fcs-undec fcs-fixed fcs-fixed-unix chinese-gb18030 utf-7-imap utf-7 utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le utf-8-emacs utf-8-auto utf-8-with-signature utf-8 prefer-utf-8 raw-text no-conversion)""#
        ]],
    );
}

#[test]
fn find_coding_systems_region_internal_dynamic_coding_system_list() {
    let form = r#"(list
  (let ((coding-system-list '(iso-latin-1 utf-8)))
    (find-coding-systems-region-internal (string ?a #xe9) nil nil))
  (let ((coding-system-list '(utf-8 iso-latin-1)))
    (find-coding-systems-region-internal (string ?a #xe9) nil nil))
  (let ((coding-system-list '(latin-1 utf-8-unix utf-8 iso-latin-1)))
    (find-coding-systems-region-internal (string ?a #xe9) nil nil))
  (let ((coding-system-list '(utf-8 utf-8)))
    (find-coding-systems-region-internal (string ?a #xe9) nil nil))
  (let ((coding-system-list '(raw-text no-conversion undecided utf-8)))
    (find-coding-systems-region-internal (string ?a #xe9) nil nil))
  (let ((coding-system-list nil))
    (find-coding-systems-region-internal (string ?a #xe9) nil nil)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK ((iso-latin-1 utf-8 raw-text no-conversion) (utf-8 iso-latin-1 raw-text no-conversion) (utf-8 iso-latin-1 raw-text no-conversion) (utf-8 utf-8 raw-text no-conversion) (utf-8 raw-text no-conversion) (raw-text no-conversion))""#
        ]],
    );
}

#[test]
fn find_coding_systems_region_internal_buffer_region_and_fast_paths() {
    let form = r#"(list
  (with-temp-buffer
    (insert (string ?a #x1f600))
    (equal (find-coding-systems-region-internal (point-min) (point-max) nil)
           (find-coding-systems-region-internal (buffer-string) nil nil)))
  (with-temp-buffer
    (insert (string ?a #xe9 ?b))
    (let ((m1 (copy-marker 1)) (m2 (copy-marker 4)))
      (list
        (equal (find-coding-systems-region-internal m1 m2 nil)
               (find-coding-systems-region-internal (string ?a #xe9 ?b) nil nil))
        (find-coding-systems-region-internal 1 2 nil)
        (find-coding-systems-region-internal 3 4 nil))))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string #xc3 #xa9))
    (find-coding-systems-region-internal (point-min) (point-max) nil))
  (find-coding-systems-region-internal (unibyte-string #xc3 #xa9) nil nil)
  (find-coding-systems-region-internal "ascii-only" nil nil)
  (condition-case e
      (with-temp-buffer
        (insert "ab")
        (find-coding-systems-region-internal 1 9 nil))
    (error e))
  (condition-case e
      (find-coding-systems-region-internal (string ?a #xe9) nil 42)
    (error e)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (t (t t t) t t t (args-out-of-range 1 9) (wrong-type-argument listp 42))""#
        ]],
    );
}

#[test]
fn coding_system_fixed_eol_with_eol_suffix_properties_and_repertoire() {
    let form = r#"(progn
  (define-coding-system 'fcs-lone-unix "lone"
    :coding-type 'charset :charset-list '(iso-8859-1) :mnemonic ?L :eol-type 'unix)
  (define-coding-system 'fcs-pair "pair-base"
    :coding-type 'charset :charset-list '(iso-8859-1) :mnemonic ?P :eol-type 'unix)
  (define-coding-system 'fcs-pair-unix "pair-unix"
    :coding-type 'utf-8 :charset-list '(unicode) :mnemonic ?U :eol-type 'unix)
  (define-coding-system 'fcs-undec-b "undec"
    :coding-type 'charset :charset-list '(iso-8859-1) :mnemonic ?D)
  (coding-system-put 'fcs-lone-unix :custom-tag 'lone-val)
  (coding-system-put 'fcs-pair-unix :custom-tag 'pair-unix-val)
  (coding-system-put 'fcs-undec-b-unix :custom-tag 'undec-shared)
  (list
    (list (coding-system-get 'fcs-lone-unix :charset-list)
          (coding-system-get 'fcs-lone-unix :mnemonic)
          (coding-system-get 'fcs-lone-unix :custom-tag)
          (unencodable-char-position 0 2 'fcs-lone-unix nil (string ?a #xe9))
          (unencodable-char-position 0 2 'fcs-lone-unix nil (string ?a #x1f600)))
    (list (coding-system-get 'fcs-pair :charset-list)
          (coding-system-get 'fcs-pair :custom-tag)
          (coding-system-get 'fcs-pair-unix :charset-list)
          (coding-system-get 'fcs-pair-unix :custom-tag)
          (unencodable-char-position 0 2 'fcs-pair nil (string ?a #x1f600))
          (unencodable-char-position 0 2 'fcs-pair-unix nil (string ?a #x1f600)))
    (list (coding-system-get 'fcs-undec-b :custom-tag)
          (coding-system-get 'fcs-undec-b-unix :custom-tag)
          (coding-system-get 'fcs-undec-b-dos :custom-tag))
    (let ((coding-system-list '(fcs-undec-b-unix fcs-undec-b fcs-pair-unix fcs-pair fcs-lone-unix)))
      (list
        (find-coding-systems-region-internal (string ?a #xe9) nil nil)
        (find-coding-systems-region-internal (string ?a #x1f600) nil nil)))))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (((iso-8859-1) 76 lone-val nil 1) ((iso-8859-1) nil (unicode) pair-unix-val 1 nil) (undec-shared undec-shared undec-shared) ((fcs-undec-b fcs-pair-unix fcs-pair fcs-lone-unix raw-text no-conversion) (fcs-pair-unix raw-text no-conversion)))""#
        ]],
    );
}
