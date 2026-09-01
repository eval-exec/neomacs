# UTF-8 / multibyte divergences — Neomacs vs GNU Emacs

Oracle parity tests (`crates/neovm-oracle-tests/src/divergence_utf8_*.rs`) probing
string/UTF-8/multibyte handling. Each `#[test]` runs the same Elisp form in
GNU Emacs and the Neomacs binary and compares normalized output; a panic
means a divergence. Failing tests are committed deliberately — they *are*
the surfaced divergences.

## Run them

```bash
# build the neomacs binary once
cargo run -p xtask -- fresh-build --release

# run all UTF-8 divergence tests
cargo nextest run -p neovm-oracle-tests -E 'test(/div_utf8/)' --no-fail-fast
```

GNU Emacs is expected on `PATH` (or `NEOVM_FORCE_ORACLE_PATH=/path/to/emacs`);
the Neomacs binary at `target/release/neomacs` (or `NEOVM_BINARY_PATH=...`).

Scope at time of writing: **964 tests, 413 pass, 551 divergences.**

## Exhaustive per-element matrices (bulk of the count)

Beyond the hand-written theme tests above, the bulk of the 551 divergences come
from generated matrices — one focused `#[test]` per element of an enumerable
set, each surfacing its own divergence. These pin the *exact* scope of the
confirmed root-cause bugs:

- **eight-bit charset matrix** (`eightbit_charset_matrix.rs`, bytes 128–255):
  `char-charset` of every raw byte → `unicode` (NEO) vs `eight-bit` (GNU).
  ~128 divergences — exhausts Theme 1.
- **eight-bit string-bytes matrix** (`eightbit_bytes_matrix.rs`, bytes 128–255):
  `string-bytes` of decoding each invalid byte → 3 (NEO) vs 2 (GNU) bytes.
  ~128 divergences — exhausts Theme 2.
- **coding-system registry matrix** (`coding_registry_matrix.rs`, all 125 GNU
  coding systems): decode a sample per coding. ~109 unsupported → U+FFFD.
  Exhausts Theme 9 (decode side).
- **coding-system encode matrix** (`coding_encode_matrix.rs`, all 125 coding
  systems): encode `café世界` per coding. ~112 unsupported → nil. Theme 9
  (encode side).
- **charset-chars matrix** (`charset_chars_matrix.rs`, all ~202 charsets):
  `charset-chars` per charset. Only 6 diverge (mostly works) — extends Theme 10.

Root cause theme: **Neomacs uses a UTF-8-internal string model**, diverging
from GNU's eight-bit-charset model. Almost every divergence traces back to
how raw bytes (0x80–0xFF) are represented and classified.

## Themes & reproductions

### Theme 1 — Eight-bit charset misclassification (`unicode` vs `eight-bit`)
`char-charset` (and `encode-char`/`decode-char 'eight-bit`) classify raw-byte
characters as `unicode` in Neomacs; GNU classifies them `eight-bit`.
- `coding_deep::div_utf8_char_charset_eightbit_range`
- `coding_deep::div_utf8_encode_decode_char_eightbit_charset`
- `char_ops_regex::div_utf8_char_charset_classification`
- `charset_conv_deep::div_utf8_string_make_multibyte_each_byte_is_eightbit`

### Theme 2 — Eight-bit byte-width inconsistency (3 vs 2 bytes)
Two construction paths produce eight-bit chars with **different internal
byte widths**:
`decode-coding-string` recovery → **3 bytes/char**,
`string-make-multibyte` / `unibyte-char-to-multibyte` → **2 bytes/char**.
GNU is uniformly 2. The recovered chars share the codepoint but leak through
every byte-derived op (string-bytes, md5, %S/prin1, `equal`).
- `coding::div_utf8_decode_invalid_utf8_bytes` (12 vs 8)
- `coding::div_utf8_decode_truncated_utf8_sequence` (5 vs 4)
- `digest_print::div_utf8_pinned_decode_vs_make_eightbit_width` (pinned)
- `digest_print::div_utf8_md5_of_recovered_eightbit_bytes` (md5 differs)
- `digest_print::div_utf8_format_S_multibyte_and_eightbit` (raw vs escaped)
- `print_escape::div_utf8_print_escape_nonascii_eightbit`
- `string_compare_format::div_utf8_eightbit_recovered_vs_constructed_equality`
  (`string=` t but `equal` nil)

### Theme 3 — Missing BOM on `-with-signature` encode
`(encode-coding-string` / `encode-coding-region ... 'utf-8-with-signature)`
does **not** prepend the UTF-8 BOM (EF BB BF) in Neomacs. Likely root cause:
the coding-system-plist reports `:category coding-category-undecided` and
`:ascii-compatible-p t` instead of `coding-category-utf-8-sig` / `nil`, so
the signature path never triggers.
- `coding::div_utf8_encode_utf8_with_signature_bom`
- `coding_deep::div_utf8_encode_signature_byte_expansion`
- `coding_deep::div_utf8_encode_signature_multibyte_payload`
- `buffer_region_ops::div_utf8_encode_coding_region_with_signature_in_buffer`
- `more_codings::div_utf8_coding_system_category_and_type` (the plist root cause)

### Theme 4 — Unsupported / stubbed legacy coding systems
Big5, latin-1/9, UTF-8/16 work; the following fail (nil on encode, U+FFFD on
decode) and `detect-coding-string` is a stub returning `(undecided)`:
- `more_codings::div_utf8_windows1252_smart_quotes`
- `more_codings::div_utf8_iso8859_7_greek_decode` (U+FFFD)
- `more_codings::div_utf8_koi8_r_cyrillic_decode` (U+FFFD)
- `more_codings::div_utf8_gbk_cjk_roundtrip`
- `more_codings::div_utf8_shiftjis_japanese_roundtrip`
- `more_codings::div_utf8_eucjp_japanese_roundtrip`
- `more_codings::div_utf8_detect_coding_string_bom`

### Theme 5 — `charset` text property on latin-1 → unibyte file read
Reading a latin-1 file into a **unibyte** buffer attaches a
`(charset iso-8859-1)` text property in Neomacs; GNU returns a plain string.
Byte content is identical. (UTF-8 reads and latin-1→*multibyte* reads are clean.)
- `buffer_io::div_utf8_file_roundtrip_latin1`
- `buffer_charset_props::div_utf8_fileread_latin1_unibyte_buffer_charset_property`

### Theme 6 — `set-buffer-multibyte t` raw-byte corruption [DATA LOSS]
Promoting a unibyte buffer holding **≥2 chars including raw bytes** back to
multibyte **drops a trailing suffix** of the characters. A single raw byte
and normal multibyte round-trips are unaffected → off-by-one / length bug in
the raw-byte promotion path.
- `buffer_multibyte_toggle::div_utf8_toggle_unibyte_to_multibyte_trailing_ascii_dropped`
- `buffer_multibyte_toggle::div_utf8_toggle_unibyte_to_multibyte_raw_bytes_only`
- `buffer_multibyte_toggle::div_utf8_toggle_unibyte_to_multibyte_leading_ascii`
- `buffer_multibyte_toggle::div_utf8_toggle_unibyte_to_multibyte_interleaved`
- `buffer_multibyte_toggle::div_utf8_toggle_unibyte_to_multibyte_preserves_point_max`

### Theme 7 — Display composition not registered / incompatible format
`(compose-region ...)` / `(compose-string ...)` followed by
`(find-composition ...)` returns `nil` in Neomacs; GNU returns the recorded
composition info `(FROM TO COMPONENTS ...)`. Worse, the composition text
property itself is stored in an incompatible format: Neomacs uses
`((4 . ""))` (alist), GNU uses `(0 4 [])`.
- `bidi_compose_misc::div_utf8_find_composition_explicit_compose`
- `compose_bidi_syntax::div_utf8_compose_string_find_composition`

### Theme 8 — Bidi RTL paragraph direction auto-detection broken
`(current-bidi-paragraph-direction)` with `bidi-paragraph-direction` left at
its default (nil = auto) returns `left-to-right` for Arabic AND Hebrew text in
Neomacs; GNU returns `right-to-left`. RTL auto-detection is broken for all
RTL scripts; LTR text, CJK, digits, and empty buffers are detected correctly.
Note: an EXPLICIT `bidi-paragraph-direction` value (e.g. `'right-to-left`) IS
honored — only auto-detection is broken.
- `compose_bidi_syntax::div_utf8_current_bidi_paragraph_direction_rtl`
- `bidi_deep::div_utf8_bidi_direction_across_scripts`

### Theme 9 — Legacy coding-system matrix (30 unsupported codings)
Comprehensive expansion of Theme 4. Neomacs supports only **utf-8, utf-16,
latin-1, latin-9 (iso-8859-15), big5, iso-8859-9, and gb2312** of the tested
codings. The following 30 are **unsupported**: single-byte decode yields
`U+FFFD (65533)` for every byte; multibyte encode yields `nil`.
- ISO-8859 family (10): iso-8859-2, -3, -4, -5, -6, -8, -10, -13, -14, -16
  (`legacy_codings::div_utf8_decode_iso8859_*`,
   `div_utf8_decode_coding_string_cyrillic_iso8859_5`)
- Windows codepages (7): windows-1250, -1251, -1253, -1254, -1255, -1256,
  -1257 (`div_utf8_decode_windows_*`)
- Other single-byte (4): koi8-u, mac-roman, viscii, tis-620
- CJK / multibyte / misc (9): euc-kr, gb18030, iso-2022-jp, iso-2022-cn,
  iso-2022-kr, utf-7, emacs-mule
  (`div_utf8_*_roundtrip`), plus `div_utf8_find_auto_coding_expressions`.
Supported (pass): iso-8859-9, gb2312 (besides utf-8/16, latin-1/9, big5).

### Theme 10 — Charset registry metadata incomplete / broken
- `charset-chars` ERRORS in Neomacs (`wrong-type-argument arrayp nil`)
  instead of returning char counts (`charset_coding_infra::div_utf8_charset_chars_counts`).
- `charset-plist` returns an INCOMPLETE plist — only `:dimension :docstring
  :short-name :long-name`; missing `:name :code-space :iso-final-char
  :emacs-mule-id :ascii-compatible-p :code-offset`
  (`charset_coding_infra::div_utf8_charset_plist_builtins`).
  (`define-charset`, `make-coding-system`, `charset-code-space`,
  `coding-system-aliases`/`type`/`mnemonic` all work.)

## What already works (coverage, not divergences)
UTF-8/UTF-16 encode-decode, Unicode property tables (general-category,
bidi-class, char-script, char-to-name, case, decomposition), char-width,
ucs-normalize (NFC/NFD/NFKC incl. Hangul), legacy `make-char` iso-2022
charsets, char-table ranges, category tables, char-fold, regex over
multibyte (classes/alternation/groups/word-boundary), string-width and
`format` display-width padding, buffer substring/insert/delete/transpose,
`string-make-multibyte` (correctly does NOT decode UTF-8), md5/sha/base64
of normal multibyte.

## Files
`divergence_utf8_{bidi_compose_misc, bidi_deep, buffer_charset_props,
buffer_io, buffer_multibyte_toggle, buffer_region_ops, char_ops_regex,
char_properties, char_tables, charset_chars_matrix, charset_coding_infra,
charset_conv_deep, coding, coding_deep, coding_encode_matrix,
coding_registry_matrix, compose_bidi_syntax, digest_print,
eightbit_bytes_matrix, eightbit_charset_matrix, fill_case_category,
legacy_codings, more_codings, print_escape, string_compare_format,
string_primitives, syntax_display, unicode_property_matrix}.rs`
