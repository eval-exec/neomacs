//! Strict combo oracle probes, batch 148: text-formatting engine. fill-region
//! at fill-column with adaptive fill + fill-prefix, comment-region / uncom-
//! ment-region round-trip in emacs-lisp-mode, indent-region of lisp source,
//! and justify-line / current-fill-column edge cases.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_fill_region_adaptive_fill_fill_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "This is a long paragraph of text that should be filled at the configured fill column boundary and wrapped into multiple lines.\n\nSecond paragraph here also long enough to require wrapping at the boundary into several separate filled lines.\n")
  (let ((fill-column 30))
    (fill-region (point-min) (point-max))
    (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""OK \"This is a long paragraph of\\ntext that should be filled at\\nthe configured fill column\\nboundary and wrapped into\\nmultiple lines.\\n\\nSecond paragraph here also\\nlong enough to require\\nwrapping at the boundary into\\nseveral separate filled lines.\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_fill_region_with_fill_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "> first line of quoted material that is quite long and needs wrapping\n> second quoted line also long\n")
  (let ((fill-column 35)
        (fill-prefix "> "))
    (fill-region (point-min) (point-max) nil nil t))
  (buffer-string))
"##;
    let expect = expect_test::expect![[
        r#""OK \"> first line of quoted material\\n> that is quite long and needs\\n> wrapping second quoted line also\\n> long\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_comment_region_uncomment_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "line one\nline two\nline three\n")
  (comment-region (point-min) (point-max))
  (let ((commented (buffer-string)))
    (comment-region (point-min) (point-max) -1)
    (let ((uncommented (buffer-string)))
      (comment-region (point-min) (point-max) 2)
      (let ((double (buffer-string)))
        (list commented uncommented double (list comment-start comment-end comment-add))))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\";; line one\\n;; line two\\n;; line three\\n\" \"; line one\\n; line two\\n; line three\\n\" \";; ; line one\\n;; ; line two\\n;; ; line three\\n\" (\";\" \"\" 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_indent_region_lisp_source() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun broken-indent (a b)\n(let ((x (+ a 1))\n(y (- b 2)))\n(when (> x y)\n(list x y (* x y)))))\n")
  (indent-region (point-min) (point-max))
  (buffer-string))
"##;
    let expect = expect_test::expect![[
        r#""OK \"(defun broken-indent (a b)\\n  (let ((x (+ a 1))\\n\t(y (- b 2)))\\n    (when (> x y)\\n      (list x y (* x y)))))\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_justify_line_and_fill_column_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "short\nthis line is medium length and justifies\nx\n")
  (let ((fill-column 40))
    (goto-char (point-min))
    (forward-line 1)
    (let ((line-start (point)))
      (justify-current-line 'full nil t)
      (let ((justified (buffer-substring line-start (line-end-position))))
        (goto-char (point-min))
        (let ((cfc (current-fill-column)))
          (list justified cfc (current-column)))))))
"##;
    let expect =
        expect_test::expect![[r#""OK (\"this line is medium length and justifies\" 40 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
