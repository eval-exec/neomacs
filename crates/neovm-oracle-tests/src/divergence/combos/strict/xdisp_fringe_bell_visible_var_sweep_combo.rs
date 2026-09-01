//! Strict combo oracle probes, batch 285: xdisp / fringe / bell / visible CORE
//! variable sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_fringe_indicator_cursor_bell_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'fringe-mode)
      (boundp 'default-fringe-indicators)
      (boundp 'fringe-cursor)
      (boundp 'overflow-newline-into-fringe)
      (boundp 'fringes-outside-margins)
      (boundp 'visible-bell)
      (boundp 'ring-bell-function)
      (boundp 'visible-cursor)
      (boundp 'cursor-in-non-selected-windows)
      (boundp 'void-text-area-pointer)
      (boundp 'make-cursor-line-fully-visible)
      (boundp 'cursor-type))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_indicator_whitespace_line_wrap_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'show-trailing-whitespace)
      (boundp 'indicate-empty-lines)
      (boundp 'indicate-buffer-boundaries)
      (boundp 'word-wrap)
      (boundp 'truncate-lines)
      (boundp 'truncate-partial-width-windows)
      (boundp 'line-move-visual)
      (boundp 'line-scan-limit)
      (boundp 'auto-hscroll-mode)
      (boundp 'hscroll-margin)
      (boundp 'hscroll-step)
      (boundp 'recenter-redisplay))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_glyph_table_face_remapping_inverse_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'glyph-table)
      (boundp 'glyphless-char-display)
      (boundp 'glyphless-char-display-control)
      (boundp 'standard-display-table)
      (boundp 'current-display-table)
      (boundp 'face-remapping-alist)
      (boundp 'inverse-video)
      (boundp 'mode-line-in-non-selected-windows)
      (boundp 'highlight-nonselected-windows)
      (boundp 'no-redraw-on-reenter)
      (boundp 'baud-rate)
      (boundp 'redisplay-dont-pause))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t nil t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
