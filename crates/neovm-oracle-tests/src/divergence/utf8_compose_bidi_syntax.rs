//! UTF-8 / multibyte *composition, bidi direction & syntax modification* probes.
//!
//! Follows up on the composition divergence (#33): `compose-string`,
//! `decompose-region`, and `find-composition` against a string. Also probes
//! `current-bidi-paragraph-direction` for LTR/RTL text, `read` of multibyte
//! symbols, and `modify-syntax-entry` for non-ASCII characters.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- compose-string / decompose-region --------------------------------------

#[test]
fn div_utf8_compose_string_find_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"café\" 0 4 (composition (0 4 []))) (composition (0 4 [])) (0 4 [] t nil 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s (copy-sequence "café")))
  (compose-string s 0 (length s) "")
  (list s
        (text-properties-at 0 s)
        (find-composition 0 nil s t)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_decompose_region_after_compose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(condition-case err
    (with-temp-buffer
      (insert "abc")
      (compose-region 1 3 "")
      (decompose-region 1 3)
      (list (find-composition 1 nil nil t) (buffer-string)))
  (error (cons (car err) 'errored)))
"#,
        expect,
    );
}

// --- bidi paragraph direction ----------------------------------------------

#[test]
fn div_utf8_current_bidi_paragraph_direction_ltr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK left-to-right""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café 世界 hello")
  (current-bidi-paragraph-direction))
"#,
        expect,
    );
}

#[test]
fn div_utf8_current_bidi_paragraph_direction_rtl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK right-to-left""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "שלום עולם")
  (current-bidi-paragraph-direction))
"#,
        expect,
    );
}

// --- read of multibyte symbols ----------------------------------------------

#[test]
fn div_utf8_read_multibyte_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (café t \"λ\" t 世界)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (read "café")
      (symbolp (read "café"))
      (symbol-name (read "λ"))
      (eq (read "café") (intern "café"))
      (read " 世界"))
"#,
        expect,
    );
}

// --- modify-syntax-entry for multibyte --------------------------------------

#[test]
fn div_utf8_modify_syntax_entry_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (95 119 119)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((st (copy-syntax-table (standard-syntax-table))))
  (modify-syntax-entry ?é "_" st)
  (modify-syntax-entry ?\x3042 "w" st)
  (with-syntax-table st
    (list (char-syntax ?é)
          (char-syntax ?\x3042)
          (char-syntax ?a))))
"#,
        expect,
    );
}

// --- prin1 of structures containing multibyte strings -----------------------

#[test]
fn div_utf8_prin1_vector_alist_with_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[\\\"café\\\" \\\"世界\\\" 233]\" \"((\\\"café\\\" . \\\"世界\\\") (\\\"λ\\\" . \\\"λλ\\\"))\" 13)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (prin1-to-string ["café" "世界" ?é])
      (prin1-to-string '(("café" . "世界") ("λ" . "λλ")))
      (length (prin1-to-string ["café" "世界"])))
"#,
        expect,
    );
}

#[test]
fn div_utf8_find_composition_automatic_no_lisp_font_at() {
    // GNU `autocmp_chars` (src/composite.c:958-971) resolves the font natively
    // on graphical frames and passes `win->frame` on TTY/batch frames without
    // ever invoking Lisp `font-at`. Assert BOTH facts: `font-at` call count is
    // 0 AND `find-composition` returns the shaped terminal gstring spans.
    let expect = expect_test::expect![[
        r#""OK ((4 6 [[utf-8-unix 101 769] nil [0 1 101 101 1 0 1 1 0 nil] [0 1 769 769 0 0 0 1 0 nil] nil nil nil nil nil nil]) (4 6 [[utf-8-unix 101 769] nil [0 1 101 101 1 0 1 1 0 nil] [0 1 769 769 0 0 0 1 0 nil] nil nil nil nil nil nil]) nil (8 10 [[utf-8-unix 97 776] nil [0 1 97 97 1 0 1 1 0 nil] [0 1 776 776 0 0 0 1 0 nil] nil nil nil nil nil nil]) (14 16 [[utf-8-unix 10084 65039] nil [0 1 10084 10084 1 0 1 1 0 nil] [0 1 65039 65039 0 0 0 1 0 nil] nil nil nil nil nil nil]) (3 5 [[utf-8-unix 101 769] nil [0 1 101 101 1 0 1 1 0 nil] [0 1 769 769 0 0 0 1 0 nil] nil nil nil nil nil nil]) nil (4 6 [[utf-8-unix 101 769] nil [0 1 101 101 1 0 1 1 0 nil] [0 1 769 769 0 0 0 1 0 nil] nil nil nil nil nil nil]) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((font-at-calls 0)
      (snap (lambda (x)
              (and x (list (car x) (cadr x) (copy-tree (caddr x) t))))))
  (advice-add 'font-at :before (lambda (&rest _) (setq font-at-calls (1+ font-at-calls))))
  (with-temp-buffer
    (insert "cafe\u0301 na\u0308ive \u2764\ufe0f\n")
    (set-window-buffer (selected-window) (current-buffer))
    (goto-char (point-min))
    (list (funcall snap (find-composition 4 nil nil t))
          (funcall snap (find-composition 4 nil nil nil))
          (funcall snap (find-composition 1 nil nil t))
          (funcall snap (find-composition 9 nil nil t))
          (funcall snap (find-composition 14 nil nil t))
          (funcall snap (find-composition 3 nil "cafe\u0301" t))
          (let ((auto-composition-mode nil))
            (funcall snap (find-composition 4 nil nil t)))
          (let ((auto-composition-mode "dummy"))
            (funcall snap (find-composition 4 nil nil t)))
          font-at-calls)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_find_composition_string_nil_auto_composition_function() {
    // Issue #842: GNU `Ffind_composition_internal` (`src/composite.c:2071-2081`)
    // routes both buffer and string arms through `find_automatic_composition`
    // -> `autocmp_chars` (`src/composite.c:929-984`), which invokes
    // `safe_calln (Vauto_composition_function, ...)` (line 979). When
    // `auto-composition-function` is nil (or void / returns nil), both buffer
    // and string arms return nil; stored string compositions are unaffected.
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil nil (0 2 t))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "cafe\u0301\n")
  (set-window-buffer (selected-window) (current-buffer))
  (let ((stored (copy-sequence "ab")))
    (compose-string stored 0 2)
    (list (let ((auto-composition-function nil))
            (find-composition 4 nil nil t))
          (let ((auto-composition-function nil))
            (find-composition 3 nil "cafe\u0301" t))
          (let ((auto-composition-function nil))
            (find-composition 3 nil "cafe\u0301" nil))
          (let ((auto-composition-function nil))
            (find-composition 0 5 "cafe\u0301" t))
          (let ((auto-composition-function (lambda (&rest _) nil)))
            (find-composition 3 nil "cafe\u0301" t))
          (let ((auto-composition-function 'definitely-void-acf-842))
            (find-composition 3 nil "cafe\u0301" t))
          (progn
            ;; Warm automatic composition first, then re-query with nil.
            (find-composition 3 nil "cafe\u0301" t)
            (let ((auto-composition-function nil))
              (find-composition 3 nil "cafe\u0301" t)))
          (let ((auto-composition-function nil))
            (find-composition 0 nil stored nil)))))
"#,
        expect,
    );
}
