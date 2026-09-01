# Source-audit divergences — GNU src/*.c vs neovm-core

Found by direct source-level comparison (not blind runtime probing). These are
higher-confidence than the blind scans because each traces to a specific
algorithmic difference in the Rust implementation.

## Run them
```bash
cargo nextest run -p neovm-oracle-tests -E \
  'test(/div_ac_/)|test(/div_ae_/)|test(/div_af_/)|test(/div_ar_/)|test(/div_aq_/)|test(/div_acf_/)|test(/div_cf_cp_/)' --no-fail-fast
```

## Theme A — case-fold-search missing the upper UTF-8 lead-byte range (~28)
**Root cause**: Neomacs case-fold-search equivalence only covers 2-byte UTF-8
chars whose lead byte is the *lower* of the pair (CE for Greek, D0 for
Cyrillic). Chars whose lead byte is the *upper* half (**CF**, **D1**) are
NOT folded. So lowercase never case-fold-matches its uppercase for those.

- Greek π–ω (CF range, 9 chars): πρστυφχψω → ΠΡΣΤΥΦΧΨΩ. Greek α–ο fold OK.
- Cyrillic р–я (D1 range, 16 chars): рстуфхцчшщъыьэюя → РСТ...Я. Cyrillic а–п fold OK.
- `div_cf_cp_*` matrix (56 tests, 25 fail), `div_acf_sigma_lower_to_upper`,
  `div_acf_omega_lower_to_upper`, `div_acf_cyrillic_lower_to_upper`.
- Reverse (Σ→σ, Α→α) works; only the lowercase→uppercase direction breaks.

## Theme B — case-symbols-as-words / special casing (2)
- `(let ((case-symbols-as-words t)) (capitalize "foo_bar baz"))` → Neomacs
  `"Foo_Bar Baz"` vs GNU `"Foo_bar baz"`. Neomacs uses is_alphanumeric() not
  the syntax table. (`div_ac_case_symbols_as_words`)
- Greek final sigma: `(downcase "ΑΣ")` → `"ασ"` (Neomacs) vs `"ας"` (GNU);
  misses the trailing-Σ→ς rule. (`div_ac_greek_final_sigma_downcase`)

## Theme C — current-column ignores `display` text property (1)
- `(with-temp-buffer (insert "x") (put-text-property 1 2 'display "abc") (current-column))`
  → 1 (Neomacs) vs 3 (GNU). Column accounting ignores the display glyph.
  (`div_ae_current_column_ignores_display`)

## Theme D — error-message quote style (curly vs straight) (~3)
- GNU renders quoted error text with curly quotes ‘…’ (via `format-message`
  backtick→‘ conversion) in some C-level error paths; Neomacs emits straight
  quotes/ASCII. `format-message` itself IS implemented correctly — only the
  internal error-signal path differs.
  (`div_aq_error_message_quote_style`, `div_ar_replace_match_*`)

## Coverage confirmed faithful by the audit (over-predicted)
The audits flagged many candidates that turned out to work: case-table IS
honored, char-width-table mutation honored, `with-case-table`/`filter-buffer-
substring`/`format-spec` all present, format `%d`-on-float/`%05.2d`/`%4c`/`%.0g`
correct, `selective-display` count-lines, `position-bytes` narrowing, field
stickiness, mark-marker relocation, `expand-file-name` (../.., //, trailing),
`directory-files` dot handling, marker relocation over multibyte, regex
backrefs/shy-groups/intervals/non-greedy/word-boundaries/`\ca`/`\&`/`\1`.

## Theme E — display/indent column accounting ignores display constructs (6)
- `current-column` / `move-to-column` ignore the `display` text property
  (glyph width), `display` slice/multi-glyph, `buffer-display-table` glyph
  replacement, and composition (multi-char → one glyph). Root: neovm-core
  indent.rs scan_for_column vs GNU indent.c current_column_1.
  `div_adisp_*` tests (12 tests, 6 fail). Invisible text, `display (height)`,
  and tabs are handled correctly (controls pass).

## Files
`divergence_audit_{casefiddle,editfns,fileio,regex,quotes_casefold}.rs`,
`divergence_casefold_matrix.rs`.
