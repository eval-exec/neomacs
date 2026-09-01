//! Strict combo oracle probes, batch 253: display/rendering variable existence
//! sweep. boundp over standard display defcustoms (cursor/spacing/scroll/bidi/
//! fringe/boundary indicators). Any nil-in-Neomacs/t-in-GNU is a missing-
//! variable bug (same class as search-spaces-regexp void).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_display_cursor_spacing_scroll_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'line-spacing)
      (boundp 'cursor-type)
      (boundp 'x-stretch-cursor)
      (boundp 'visible-cursor)
      (boundp 'void-text-area-pointer)
      (boundp 'highlight-nonselected-windows)
      (boundp 'window-min-height)
      (boundp 'window-min-width)
      (boundp 'scroll-margin)
      (boundp 'scroll-conservatively)
      (boundp 'scroll-step)
      (boundp 'scroll-preserve-screen-position)
      (boundp 'next-screen-context-lines)
      (boundp 'recenter-redisplay)
      (boundp 'mouse-yank-at-point)
      (boundp 'make-cursor-line-fully-visible))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_bidi_fringe_boundary_indicator_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'bidi-display-reordering)
      (boundp 'bidi-paragraph-direction)
      (boundp 'bidi-paragraph-separate-re)
      (boundp 'show-trailing-whitespace)
      (boundp 'indicate-empty-lines)
      (boundp 'indicate-buffer-boundaries)
      (boundp 'fringe-mode)
      (boundp 'overflow-newline-into-fringe)
      (boundp 'fringes-outside-margins)
      (boundp 'visual-order-cursor-movement)
      (boundp 'auto-hscroll-mode)
      (boundp 'hscroll-margin)
      (boundp 'hscroll-step)
      (boundp 'tab-width)
      (boundp 'indent-tabs-mode))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_display_table_glyph_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'glyph-table)
      (boundp 'glyphless-char-display)
      (boundp 'glyphless-char-display-control)
      (boundp 'standard-display-table)
      (boundp 'current-display-table)
      (boundp 'text-scale-mode)
      (boundp 'text-scale-mode-step)
      (boundp 'display-line-numbers)
      (boundp 'display-line-numbers-width)
      (boundp 'display-line-numbers-offset)
      (boundp 'hl-line-mode)
      (boundp 'global-hl-line-mode))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
