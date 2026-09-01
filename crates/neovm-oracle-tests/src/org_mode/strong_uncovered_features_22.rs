//! Strong uncovered-features-22 oracle tests — complex multi-step workflows.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute-src-block with different languages
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_lang() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(format \"%s\" (+ 1 2))\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-execute-src-block)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute-src-block with header args
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :results value\n(+ 10 20)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-execute-src-block)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute-src-block with :results output
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :results output\n(princ \"hello\")\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-execute-src-block)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute-buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n#+BEGIN_SRC emacs-lisp\n(+ 2)\n#+END_SRC")
  (org-babel-execute-buffer)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n#+BEGIN_SRC emacs-lisp\n(+ 2)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-execute-subtree)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-expand-src-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"(let ((x '1)\\n      (y '2))\\n(+ x y)\\n)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :var x=1 y=2\n(+ x y)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-expand-src-block))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-check-src-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"No suspicious header arguments found.\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-check-src-block))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-insert-result
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#+BEGIN_SRC emacs-lisp\\n(+ 1)\\n#+END_SRC\\n\\n#+RESULTS:\\n: 42\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-insert-result "42" '("value"))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-result-to-file
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_to_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-result-to-file)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-result-to-file "test.png" "desc" '("figure"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-goto-src-block-head
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_goto() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#+BEGIN_SRC emacs-lisp\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n(+ 2)\n#+END_SRC")
  (goto-char (point-min))
  (search-forward "(+ 2)")
  (beginning-of-line)
  (org-babel-goto-src-block-head)
  (buffer-substring-no-properties (line-beginning-position) (line-end-position)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-mark-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (24 36)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n(+ 2)\n#+END_SRC")
  (goto-char (point-min))
  (search-forward "(+ 1)")
  (beginning-of-line)
  (org-babel-mark-block)
  (list (region-beginning) (region-end)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-demarcate-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_demarcate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#+BEGIN_SRC emacs-lisp\\n  (+ 1)\\n#+END_SRC\\n\\n#+BEGIN_SRC emacs-lisp\\n  (+ 2)\\n  (+ 3)\\n#+END_SRC\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n(+ 2)\n(+ 3)\n#+END_SRC")
  (goto-char (point-min))
  (search-forward "(+ 2)")
  (beginning-of-line)
  (org-babel-demarcate-block)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-switch-to-session
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_session() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK \"#+BEGIN_SRC emacs-lisp :session\\n(+ 1)\\n#+END_SRC\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :session\n(+ 1)\n#+END_SRC")
  (goto-char (point-min))
  (condition-case nil
      (org-babel-switch-to-session)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-initiate-session
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_init() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK \"#+BEGIN_SRC emacs-lisp :session\\n(+ 1)\\n#+END_SRC\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :session\n(+ 1)\n#+END_SRC")
  (goto-char (point-min))
  (condition-case nil
      (org-babel-initiate-session)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-params-from-properties
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((:results . \"value\")) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n:PROPERTIES:\n:header-args: :results value\n:END:\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-params-from-properties "emacs-lisp"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-parse-src-block-match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-parse-src-block-match)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :results value :var x=1\n(+ x)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-parse-src-block-match))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-get-src-block-info
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"emacs-lisp\" ((:colname-names) (:rowname-names) (:result-params \"value\" \"replace\") (:result-type . value) (:results . \"value replace\") (:exports . \"code\") (:lexical . \"no\") (:tangle . \"no\") (:hlines . \"no\") (:noweb . \"no\") (:cache . \"no\") (:session . \"none\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :results value\n(+ 1)\n#+END_SRC")
  (goto-char (point-min))
  (let ((info (org-babel-get-src-block-info)))
    (list (nth 0 info) (nth 2 info))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-get-src-block-lang
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_lang2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-get-src-block-lang)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n#+BEGIN_SRC python\nprint(1)\n#+END_SRC")
  (goto-char (point-min))
  (list (org-babel-get-src-block-lang)
        (progn (search-forward "python") (org-babel-get-src-block-lang))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-insert-header-arg
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_header_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp :results)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-insert-header-arg :results "value")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-merge-params
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-merge-params)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-merge-params '((:results . "value")) '((:results . "output"))) "##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-variable-assignments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (void-function org-babel-variable-assignments:emacs-lisp)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-variable-assignments:emacs-lisp '((:var . "x=1") (:var . "y=2"))) "##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-result-params
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_result_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-result-params)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :results value output\n(+ 1)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-result-params))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf22_src_elisp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "value"))) "##,
        expect,
    );
}
