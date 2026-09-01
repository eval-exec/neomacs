//! Divergence tests: edit simulation stress — realistic editing session combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_simulate_code_edit_session() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"(defurenamed-n my-func (arg)\\n  \\\"(* X.\\\"\\n  (let ((x 1))\\n    (+ x arg)))\" 0 4 (type keyword) 13 19 (type function) 20 22 (type args) 29 32 (type doc) 38 41 (type keyword) 42 44 (type var) 52 53 (type var) 54 57 (type args)) #(\"(defun my-func (arg)\\n  \\\"Docstring.\\\"\\n  (let ((x 1))\\n    (+ x arg)))\" 0 4 (type keyword) 5 11 (type function) 12 14 (type args) 21 24 (type doc) 24 32 (type doc) 35 38 (type keyword) 39 41 (type var) 49 50 (type var) 51 54 (type args)) t 1 t 22 t font-lock-doc-face t font-lock-string-face t keyword t function t args t doc t keyword t var t var t args t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(defun my-func (arg)\n  \"Docstring.\"\n  (let ((x 1))\n    (+ x arg)))")
  (let ((ov-doc (make-overlay 22 33))
        (ov-body (make-overlay 36 56))
        (m-start (copy-marker 1 t))
        (m-end (copy-marker (point-max)))
        (m-doc (copy-marker 22 t)))
    (overlay-put ov-doc 'face 'font-lock-doc-face)
    (overlay-put ov-body 'face 'font-lock-string-face)
    (put-text-property 1 5 'type 'keyword)
    (put-text-property 6 12 'type 'function)
    (put-text-property 13 15 'type 'args)
    (put-text-property 22 33 'type 'doc)
    (put-text-property 36 39 'type 'keyword)
    (put-text-property 40 42 'type 'var)
    (put-text-property 50 51 'type 'var)
    (put-text-property 52 55 'type 'args)
    (undo-boundary)
    (goto-char 6)
    (insert "renamed-")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "Docstring" nil t)
    (replace-match "Updated documentation")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "(+ x" nil t)
    (replace-match "(* x")
    (let ((s (buffer-string)))
      (primitive-undo 3 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "(defun my-func (arg)\n  \"Docstring.\"\n  (let ((x 1))\n    (+ x arg)))")
            (marker-position m-start) (= (marker-position m-start) 1)
            (marker-position m-doc) (= (marker-position m-doc) 22)
            (overlay-get ov-doc 'face) (eq (overlay-get ov-doc 'face) 'font-lock-doc-face)
            (overlay-get ov-body 'face) (eq (overlay-get ov-body 'face) 'font-lock-string-face)
            (get-text-property 1 'type) (eq (get-text-property 1 'type) 'keyword)
            (get-text-property 6 'type) (eq (get-text-property 6 'type) 'function)
            (get-text-property 13 'type) (eq (get-text-property 13 'type) 'args)
            (get-text-property 22 'type) (eq (get-text-property 22 'type) 'doc)
            (get-text-property 36 'type) (eq (get-text-property 36 'type) 'keyword)
            (get-text-property 40 'type) (eq (get-text-property 40 'type) 'var)
            (get-text-property 50 'type) (eq (get-text-property 50 'type) 'var)
            (get-text-property 52 'type) (eq (get-text-property 52 'type) 'args))))) "#,
        expect,
    );
}

#[test]
fn divergence_simulate_text_reformatting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable s)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq fill-column 15)
  (insert "This is a very long paragraph that needs to be reformatted properly for display.")
  (put-text-property 1 4 'word 'w1)
  (put-text-property 6 7 'word 'w2)
  (put-text-property 9 10 'word 'w3)
  (put-text-property 12 16 'word 'w4)
  (put-text-property 18 22 'word 'w5)
  (let ((ov (make-overlay 1 (1+ (buffer-size)))))
    (overlay-put ov 'paragraph 'first)
    (let ((m1 (copy-marker 1 t))
          (m2 (copy-marker 18 t)))
      (undo-boundary)
      (goto-char 1)
      (fill-paragraph nil)
      (let ((s (buffer-string))
            (line-count (length (split-string s "\n"))))
        (primitive-undo 1 buffer-undo-list)
        (list s line-count (> line-count 1)
              (buffer-string)
              (= (length (split-string (buffer-string) "\n")) 1)
              (marker-position m1) (= (marker-position m1) 1)
              (overlay-get ov 'paragraph) (eq (overlay-get ov 'paragraph) 'first)
              (get-text-property 1 'word) (eq (get-text-property 1 'word) 'w1)
              (get-text-property 6 'word) (eq (get-text-property 6 'word) 'w2)))))) "#,
        expect,
    );
}

#[test]
fn divergence_simulate_comment_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r#"(progn
  (insert "line1\nline2\nline3\nline4\nline5")
  (put-text-property 1 5 'line-num 1)
  (put-text-property 7 11 'line-num 2)
  (put-text-property 13 17 'line-num 3)
  (put-text-property 19 23 'line-num 4)
  (put-text-property 25 29 'line-num 5)
  (let ((ov (make-overlay 7 17)))
    (overlay-put ov 'selection 'active)
    (undo-boundary)
    (condition-case err
        (comment-region 7 17)
      (error nil))
    (let ((s (buffer-string)))
      (condition-case err
          (uncomment-region 7 17)
        (error nil))
      (list s
            (buffer-string)
            (= (length (split-string (buffer-string) "\n")) 5)
            (overlay-get ov 'selection) (eq (overlay-get ov 'selection) 'active)
            (get-text-property 1 'line-num) (= (get-text-property 1 'line-num) 1)
            (get-text-property 7 'line-num) (= (get-text-property 7 'line-num) 2)
            (get-text-property 13 'line-num) (= (get-text-property 13 'line-num) 3))))) "#,
    );
}

#[test]
fn divergence_simulate_find_replace_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"HELLO-bar-HELLO-baz-HELLO-qux-HELLO\" 6 8 (token bar) 16 18 (token baz) 26 28 (token qux)) nil nil #(\"foo-bar-foo-baz-foo-qux-foo\" 0 2 (token foo) 4 6 (token bar) 8 10 (token foo) 12 14 (token baz) 16 18 (token foo) 20 22 (token qux) 24 26 (token foo)) t 4 nil 12 nil 20 nil 28 nil replace t foo t bar t foo t baz t foo t qux t foo t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "foo-bar-foo-baz-foo-qux-foo")
  (put-text-property 1 3 'token 'foo)
  (put-text-property 5 7 'token 'bar)
  (put-text-property 9 11 'token 'foo)
  (put-text-property 13 15 'token 'baz)
  (put-text-property 17 19 'token 'foo)
  (put-text-property 21 23 'token 'qux)
  (put-text-property 25 27 'token 'foo)
  (let ((ov (make-overlay 1 27))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 9 t))
        (m3 (copy-marker 17 t))
        (m4 (copy-marker 25 t)))
    (overlay-put ov 'scope 'replace)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "foo" nil t)
      (replace-match "HELLO"))
    (let ((s (buffer-string))
          (p1 (get-text-property 1 'token))
          (p2 (get-text-property 5 'token)))
      (primitive-undo 1 buffer-undo-list)
      (list s p1 p2
            (buffer-string)
            (string= (buffer-string) "foo-bar-foo-baz-foo-qux-foo")
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 9)
            (marker-position m3) (= (marker-position m3) 17)
            (marker-position m4) (= (marker-position m4) 25)
            (overlay-get ov 'scope) (eq (overlay-get ov 'scope) 'replace)
            (get-text-property 1 'token) (eq (get-text-property 1 'token) 'foo)
            (get-text-property 5 'token) (eq (get-text-property 5 'token) 'bar)
            (get-text-property 9 'token) (eq (get-text-property 9 'token) 'foo)
            (get-text-property 13 'token) (eq (get-text-property 13 'token) 'baz)
            (get-text-property 17 'token) (eq (get-text-property 17 'token) 'foo)
            (get-text-property 21 'token) (eq (get-text-property 21 'token) 'qux)
            (get-text-property 25 'token) (eq (get-text-property 25 'token) 'foo))))) "#,
        expect,
    );
}

#[test]
fn divergence_simulate_indent_region_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"line1\\n    line2\\n    line3\\nline4\" 0 4 (line 1) 10 14 (line 2) 20 24 (line 3) 26 30 (line 4)) 11 #(\"line1\\nline2\\nline3\\nline4\" 0 4 (line 1) 6 10 (line 2) 12 16 (line 3) 18 22 (line 4)) t 7 t t t 1 t 2 t 3 t 4 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1\nline2\nline3\nline4")
  (put-text-property 1 5 'line 1)
  (put-text-property 7 11 'line 2)
  (put-text-property 13 17 'line 3)
  (put-text-property 19 23 'line 4)
  (let ((ov (make-overlay 7 17))
        (m (copy-marker 7 t)))
    (overlay-put ov 'indent-target t)
    (undo-boundary)
    (indent-rigidly 7 17 4)
    (let ((s (buffer-string))
          (m-pos (marker-position m)))
      (primitive-undo 1 buffer-undo-list)
      (list s m-pos
            (buffer-string)
            (string= (buffer-string) "line1\nline2\nline3\nline4")
            (marker-position m) (= (marker-position m) 7)
            (overlay-get ov 'indent-target) (eq (overlay-get ov 'indent-target) t)
            (get-text-property 1 'line) (= (get-text-property 1 'line) 1)
            (get-text-property 7 'line) (= (get-text-property 7 'line) 2)
            (get-text-property 13 'line) (= (get-text-property 13 'line) 3)
            (get-text-property 19 'line) (= (get-text-property 19 'line) 4))))) "#,
        expect,
    );
}

#[test]
fn divergence_simulate_delete_to_kill_ring_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"KEEP1-1-KEEP2-2-KEEP3\" 0 4 (zone keep1) 8 12 (zone keep2) 16 20 (zone keep3)) #(\"DELETE\" 0 6 (zone del2)) #(\"KEEP1-DELETE1-KEEP2-DELETE2-KEEP3\" 0 4 (zone keep1) 6 12 (zone del1) 14 18 (zone keep2) 20 26 (zone del2) 28 32 (zone keep3)) t 1 t 15 t 29 t main t keep1 t del1 t keep2 t del2 t keep3 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "KEEP1-DELETE1-KEEP2-DELETE2-KEEP3")
  (put-text-property 1 5 'zone 'keep1)
  (put-text-property 7 13 'zone 'del1)
  (put-text-property 15 19 'zone 'keep2)
  (put-text-property 21 27 'zone 'del2)
  (put-text-property 29 33 'zone 'keep3)
  (let ((ov (make-overlay 1 33))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 15 t))
        (m3 (copy-marker 29 t)))
    (overlay-put ov 'buffer 'main)
    (undo-boundary)
    (kill-region 7 13)
    (undo-boundary)
    (kill-region 15 21)
    (let ((s (buffer-string))
          (kr (current-kill 0)))
      (primitive-undo 2 buffer-undo-list)
      (list s kr
            (buffer-string)
            (string= (buffer-string) "KEEP1-DELETE1-KEEP2-DELETE2-KEEP3")
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 15)
            (marker-position m3) (= (marker-position m3) 29)
            (overlay-get ov 'buffer) (eq (overlay-get ov 'buffer) 'main)
            (get-text-property 1 'zone) (eq (get-text-property 1 'zone) 'keep1)
            (get-text-property 7 'zone) (eq (get-text-property 7 'zone) 'del1)
            (get-text-property 15 'zone) (eq (get-text-property 15 'zone) 'keep2)
            (get-text-property 21 'zone) (eq (get-text-property 21 'zone) 'del2)
            (get-text-property 29 'zone) (eq (get-text-property 29 'zone) 'keep3))))) "#,
        expect,
    );
}

#[test]
fn divergence_simulate_multi_step_refactor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 27 88)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(let ((old-var 1))\n  (+ old-var 2))")
  (put-text-property 1 4 'type 'special)
  (put-text-property 6 6 'type 'var)
  (put-text-property 10 16 'type 'varname)
  (put-text-property 25 28 'type 'func)
  (put-text-property 30 36 'type 'varref)
  (let ((ov (make-overlay 1 (1+ (buffer-size))))
        (m (copy-marker 10 t)))
    (overlay-put ov 'refactor 'active)
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "old-var" nil t)
    (replace-match "new-var")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "old-var" nil t)
    (replace-match "new-var")
    (let ((s (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "(let ((old-var 1))\n  (+ old-var 2))")
            (marker-position m) (= (marker-position m) 10)
            (overlay-get ov 'refactor) (eq (overlay-get ov 'refactor) 'active)
            (get-text-property 1 'type) (eq (get-text-property 1 'type) 'special)
            (get-text-property 10 'type) (eq (get-text-property 10 'type) 'varname)))))) "#,
        expect,
    );
}

#[test]
fn divergence_simulate_whitespace_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"hello world\\n foo bar\\nbaz \" 0 4 (word w1) 6 9 (word w2) 19 21 (word w5)) #(\"hello   world\\n   foo    bar\\nbaz   \" 0 4 (word w1) 7 8 (word w2) 8 11 (word w2) 15 17 (word w3) 22 24 (word w4) 26 28 (word w5)) t 9 nil t t w1 t w2 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello   world\n   foo    bar\nbaz   ")
  (put-text-property 1 5 'word 'w1)
  (put-text-property 8 12 'word 'w2)
  (put-text-property 16 18 'word 'w3)
  (put-text-property 23 25 'word 'w4)
  (put-text-property 27 29 'word 'w5)
  (let ((ov (make-overlay 1 (1+ (buffer-size))))
        (m (copy-marker 8 t)))
    (overlay-put ov 'cleanup t)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "  +" nil t)
      (replace-match " "))
    (let ((s (buffer-string)))
      (primitive-undo 1 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "hello   world\n   foo    bar\nbaz   ")
            (marker-position m) (= (marker-position m) 8)
            (overlay-get ov 'cleanup) (eq (overlay-get ov 'cleanup) t)
            (get-text-property 1 'word) (eq (get-text-property 1 'word) 'w1)
            (get-text-property 8 'word) (eq (get-text-property 8 'word) 'w2))))) "#,
        expect,
    );
}

#[test]
fn divergence_simulate_duplicate_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"LINE1\\nLINE\\nLINE2\\nLINE3\" 0 4 (lnum 1) 6 10 (lnum 2) 11 15 (lnum 2) 17 21 (lnum 3)) 12 #(\"LINE1\\nLINE2\\nLINE3\" 0 4 (lnum 1) 6 10 (lnum 2) 12 16 (lnum 3)) t 7 t 1 t 2 t 3 t 1 t 2 t 3 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "LINE1\nLINE2\nLINE3")
  (put-text-property 1 5 'lnum 1)
  (put-text-property 7 11 'lnum 2)
  (put-text-property 13 17 'lnum 3)
  (let ((ov1 (make-overlay 1 5))
        (ov2 (make-overlay 7 11))
        (ov3 (make-overlay 13 17))
        (m (copy-marker 7 t)))
    (overlay-put ov1 'line 1)
    (overlay-put ov2 'line 2)
    (overlay-put ov3 'line 3)
    (undo-boundary)
    (goto-char 7)
    (let ((line (buffer-substring 7 11)))
      (insert line "\n"))
    (let ((s (buffer-string))
          (m-pos (marker-position m)))
      (primitive-undo 1 buffer-undo-list)
      (list s m-pos
            (buffer-string)
            (string= (buffer-string) "LINE1\nLINE2\nLINE3")
            (marker-position m) (= (marker-position m) 7)
            (overlay-get ov1 'line) (= (overlay-get ov1 'line) 1)
            (overlay-get ov2 'line) (= (overlay-get ov2 'line) 2)
            (overlay-get ov3 'line) (= (overlay-get ov3 'line) 3)
            (get-text-property 1 'lnum) (= (get-text-property 1 'lnum) 1)
            (get-text-property 7 'lnum) (= (get-text-property 7 'lnum) 2)
            (get-text-property 13 'lnum) (= (get-text-property 13 'lnum) 3))))) "#,
        expect,
    );
}

#[test]
fn divergence_simulate_transpose_words() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Don’t have two things to transpose\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "alpha beta gamma delta")
  (put-text-property 1 5 'pos 1)
  (put-text-property 7 10 'pos 2)
  (put-text-property 12 16 'pos 3)
  (put-text-property 18 22 'pos 4)
  (let ((ov (make-overlay 1 22))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 7 t))
        (m3 (copy-marker 12 t))
        (m4 (copy-marker 18 t)))
    (overlay-put ov 'sentence t)
    (undo-boundary)
    (transpose-words 1)
    (undo-boundary)
    (transpose-words -1)
    (undo-boundary)
    (goto-char 12)
    (transpose-words 1)
    (primitive-undo 3 buffer-undo-list)
    (list (buffer-string)
          (string= (buffer-string) "alpha beta gamma delta")
          (marker-position m1) (= (marker-position m1) 1)
          (marker-position m2) (= (marker-position m2) 7)
          (marker-position m3) (= (marker-position m3) 12)
          (marker-position m4) (= (marker-position m4) 18)
          (overlay-get ov 'sentence) (eq (overlay-get ov 'sentence) t)
          (get-text-property 1 'pos) (= (get-text-property 1 'pos) 1)
          (get-text-property 7 'pos) (= (get-text-property 7 'pos) 2)
          (get-text-property 12 'pos) (= (get-text-property 12 'pos) 3)
          (get-text-property 18 'pos) (= (get-text-property 18 'pos) 4))))) "#,
        expect,
    );
}
