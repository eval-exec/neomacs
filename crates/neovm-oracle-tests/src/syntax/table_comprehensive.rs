//! Comprehensive oracle parity tests for syntax table operations.
//!
//! Covers: make-syntax-table (with/without parent), modify-syntax-entry for
//! all syntax classes, char-syntax, syntax-table-p, copy-syntax-table,
//! set-syntax-table, with-syntax-table, string-to-syntax, syntax-class-to-char,
//! skip-syntax-forward/backward, forward-comment, and combined DSL scenarios.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-syntax-table: with parent, without parent, nested inheritance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_make_and_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((grandparent (make-syntax-table))
                         (_ (modify-syntax-entry ?@ "w" grandparent))
                         (_ (modify-syntax-entry ?# "." grandparent))
                         (_ (modify-syntax-entry ?$ "_" grandparent))
                         (parent (make-syntax-table grandparent))
                         (_ (modify-syntax-entry ?# "w" parent))   ;; override grandparent
                         (_ (modify-syntax-entry ?% "<" parent))   ;; new entry
                         (child (make-syntax-table parent))
                         (_ (modify-syntax-entry ?$ "." child))    ;; override grandparent via parent
                         (no-parent (make-syntax-table)))          ;; no parent arg
                    (with-temp-buffer
                      ;; Test grandparent
                      (set-syntax-table grandparent)
                      (let ((gp-at (char-syntax ?@))
                            (gp-hash (char-syntax ?#))
                            (gp-dollar (char-syntax ?$)))
                        ;; Test parent (inherits @, overrides #, adds %)
                        (set-syntax-table parent)
                        (let ((p-at (char-syntax ?@))
                              (p-hash (char-syntax ?#))
                              (p-dollar (char-syntax ?$))
                              (p-percent (char-syntax ?%)))
                          ;; Test child (inherits @, #=w from parent, overrides $)
                          (set-syntax-table child)
                          (let ((c-at (char-syntax ?@))
                                (c-hash (char-syntax ?#))
                                (c-dollar (char-syntax ?$))
                                (c-percent (char-syntax ?%)))
                            ;; No-parent table: @ should have standard syntax
                            (set-syntax-table no-parent)
                            (let ((np-at (char-syntax ?@)))
                              (list gp-at gp-hash gp-dollar
                                    p-at p-hash p-dollar p-percent
                                    c-at c-hash c-dollar c-percent
                                    np-at)))))))"#;
    let expect = expect_test::expect![[r#""OK (119 46 95 119 119 95 60 119 119 46 60 46)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// modify-syntax-entry: ALL syntax class descriptors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_all_syntax_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each syntax class: space(whitespace), w(word), _(symbol), .(punctuation),
    // ((open), )(close), "(string-quote), \(escape), /(char-quote),
    // $(paired-delimiter), '(expression-prefix), <(comment-start),
    // >(comment-end), !(generic-comment), |(generic-string)
    let form = r#"(let ((st (make-syntax-table)))
                    ;; Assign each class to a different character
                    (modify-syntax-entry ?A " " st)   ;; whitespace
                    (modify-syntax-entry ?B "w" st)   ;; word
                    (modify-syntax-entry ?C "_" st)   ;; symbol
                    (modify-syntax-entry ?D "." st)   ;; punctuation
                    (modify-syntax-entry ?E "(F" st)  ;; open paren, matched with F
                    (modify-syntax-entry ?F ")E" st)  ;; close paren, matched with E
                    (modify-syntax-entry ?G "\"" st)  ;; string quote
                    (modify-syntax-entry ?H "\\" st)  ;; escape
                    (modify-syntax-entry ?I "/" st)    ;; char-quote
                    (modify-syntax-entry ?J "$" st)    ;; paired delimiter
                    (modify-syntax-entry ?K "'" st)    ;; expression prefix
                    (modify-syntax-entry ?L "<" st)    ;; comment start
                    (modify-syntax-entry ?M ">" st)    ;; comment end
                    (modify-syntax-entry ?N "!" st)    ;; generic comment
                    (modify-syntax-entry ?O "|" st)    ;; generic string
                    ;; Read back all classes
                    (with-temp-buffer
                      (set-syntax-table st)
                      (list
                        (char-syntax ?A)
                        (char-syntax ?B)
                        (char-syntax ?C)
                        (char-syntax ?D)
                        (char-syntax ?E)
                        (char-syntax ?F)
                        (char-syntax ?G)
                        (char-syntax ?H)
                        (char-syntax ?I)
                        (char-syntax ?J)
                        (char-syntax ?K)
                        (char-syntax ?L)
                        (char-syntax ?M)
                        (char-syntax ?N)
                        (char-syntax ?O))))"#;
    let expect = expect_test::expect![[r#""OK (32 119 95 46 40 41 34 92 47 36 39 60 62 33 124)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// modify-syntax-entry with flags: comment-start/end style bits
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_comment_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two-char comment delimiters with style flags:
    // "< 1" = first char of comment-start, "< 2" = second char of comment-start
    // "> 1" = first char of comment-end, "> 2" = second char of comment-end
    // "b" flag for style b comments
    let form = r#"(let ((st (make-syntax-table)))
                    ;; C-style comments: // and /* */
                    ;; / is punctuation + comment-start char 1 (for //) + comment-start char 2 (for /*)
                    (modify-syntax-entry ?/ ". 124b" st)
                    ;; * is punctuation + comment-end char 2 (for */) + comment-start char 2 (for /*)
                    (modify-syntax-entry ?* ". 23" st)
                    ;; newline ends // comments (style b)
                    (modify-syntax-entry ?\n "> b" st)
                    ;; Verify the syntax entries
                    (with-temp-buffer
                      (set-syntax-table st)
                      (let ((slash-syn (syntax-after 1))  ;; won't work without text
                            (star-class (char-syntax ?*))
                            (slash-class (char-syntax ?/))
                            (nl-class (char-syntax ?\n)))
                        ;; Insert a C-style code snippet and test forward-comment
                        (insert "code /* block comment */ more // line comment\nrest")
                        (goto-char 6)  ;; at the '/' of /*
                        (let ((before-block (point)))
                          (forward-comment 1)
                          (let ((after-block (point)))
                            ;; Now at "more"
                            (skip-syntax-forward " ")
                            (skip-syntax-forward "w")
                            (skip-syntax-forward " ")
                            ;; At // line comment
                            (let ((before-line (point)))
                              (forward-comment 1)
                              (let ((after-line (point)))
                                (list star-class slash-class nl-class
                                      before-block after-block
                                      before-line after-line))))))))"#;
    let expect = expect_test::expect![[r#""OK (46 46 62 6 25 31 47)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// syntax-table-p predicate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Valid syntax tables
  (syntax-table-p (make-syntax-table))
  (syntax-table-p (copy-syntax-table (standard-syntax-table)))
  (syntax-table-p (standard-syntax-table))
  ;; Not syntax tables
  (syntax-table-p nil)
  (syntax-table-p t)
  (syntax-table-p 42)
  (syntax-table-p "hello")
  (syntax-table-p '(1 2 3))
  (syntax-table-p (make-hash-table))
  (syntax-table-p (make-vector 10 nil))
  ;; A char-table that is not a syntax table
  (syntax-table-p (make-char-table 'foo))
  ;; Current buffer's syntax table
  (with-temp-buffer
    (syntax-table-p (syntax-table))))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil nil nil nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-syntax-table: deep independence, parent chain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_copy_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((orig (make-syntax-table))
                         (_ (modify-syntax-entry ?@ "w" orig))
                         (_ (modify-syntax-entry ?# "." orig))
                         (_ (modify-syntax-entry ?$ "_" orig))
                         (copy1 (copy-syntax-table orig))
                         (copy2 (copy-syntax-table orig)))
                    ;; Modify copy1 heavily
                    (modify-syntax-entry ?@ "." copy1)
                    (modify-syntax-entry ?# "w" copy1)
                    (modify-syntax-entry ?& "<" copy1)
                    ;; Modify copy2 differently
                    (modify-syntax-entry ?@ "_" copy2)
                    (modify-syntax-entry ?$ "w" copy2)
                    ;; Verify all three are independent
                    (with-temp-buffer
                      (set-syntax-table orig)
                      (let ((o-at (char-syntax ?@))
                            (o-hash (char-syntax ?#))
                            (o-dollar (char-syntax ?$)))
                        (set-syntax-table copy1)
                        (let ((c1-at (char-syntax ?@))
                              (c1-hash (char-syntax ?#))
                              (c1-amp (char-syntax ?&)))
                          (set-syntax-table copy2)
                          (let ((c2-at (char-syntax ?@))
                                (c2-dollar (char-syntax ?$)))
                            (list o-at o-hash o-dollar
                                  c1-at c1-hash c1-amp
                                  c2-at c2-dollar
                                  ;; Verify they differ
                                  (not (= o-at c1-at))
                                  (not (= o-at c2-at))
                                  (not (= c1-at c2-at))))))))"#;
    let expect = expect_test::expect![[r#""OK (119 46 95 46 119 60 95 119 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-syntax-table and with-syntax-table scoping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_scoping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (let ((st1 (make-syntax-table))
                          (st2 (make-syntax-table))
                          (st3 (make-syntax-table)))
                      (modify-syntax-entry ?* "w" st1)
                      (modify-syntax-entry ?* "." st2)
                      (modify-syntax-entry ?* "_" st3)
                      ;; set-syntax-table changes buffer-local
                      (set-syntax-table st1)
                      (let ((r1 (char-syntax ?*)))
                        (set-syntax-table st2)
                        (let ((r2 (char-syntax ?*)))
                          ;; with-syntax-table is temporary
                          (let ((r3 (with-syntax-table st3 (char-syntax ?*)))
                                (r4 (char-syntax ?*)))  ;; back to st2
                            ;; Nested with-syntax-table
                            (let ((r5 (with-syntax-table st1
                                        (let ((inner (char-syntax ?*)))
                                          (with-syntax-table st3
                                            (list inner (char-syntax ?*)))))))
                              (list r1 r2 r3 r4 r5
                                    ;; After all with-syntax-table, still st2
                                    (char-syntax ?*))))))))"#;
    let expect = expect_test::expect![[r#""OK (119 46 95 46 (119 95) 46)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-to-syntax and syntax-class-to-char comprehensive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_string_to_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; All basic class descriptors
  (string-to-syntax " ")
  (string-to-syntax "w")
  (string-to-syntax "_")
  (string-to-syntax ".")
  (string-to-syntax "(")
  (string-to-syntax ")")
  (string-to-syntax "\"")
  (string-to-syntax "\\")
  (string-to-syntax "/")
  (string-to-syntax "$")
  (string-to-syntax "'")
  (string-to-syntax "<")
  (string-to-syntax ">")
  (string-to-syntax "!")
  (string-to-syntax "|")
  ;; With matching char
  (string-to-syntax "()")
  (string-to-syntax ")(")
  (string-to-syntax "(<")
  (string-to-syntax ">{")
  ;; With flags
  (string-to-syntax ". 1")
  (string-to-syntax ". 2")
  (string-to-syntax ". 3")
  (string-to-syntax ". 4")
  (string-to-syntax ". 1b")
  (string-to-syntax "< 2b")
  (string-to-syntax "> b")
  ;; syntax-class-to-char roundtrip for all 16 classes
  (let ((results nil))
    (dotimes (i 16)
      (let ((ch (syntax-class-to-char i)))
        (setq results (cons (list i ch) results))))
    (nreverse results))
  ;; string-to-syntax -> car gives class code
  (car (string-to-syntax "w"))
  (car (string-to-syntax " "))
  (car (string-to-syntax "."))
  (car (string-to-syntax "\"")))"#;
    let expect = expect_test::expect![[
        r#""OK ((0) (2) (3) (1) (4) (5) (7) (9) (10) (8) (6) (11) (12) (14) (15) (4 . 41) (5 . 40) (4 . 60) (12 . 123) (65537) (131073) (262145) (524289) (2162689) (2228235) (2097164) ((0 32) (1 46) (2 119) (3 95) (4 40) (5 41) (6 39) (7 34) (8 36) (9 92) (10 47) (11 60) (12 62) (13 64) (14 33) (15 124)) 2 0 1 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// skip-syntax-forward and skip-syntax-backward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_skip_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (let ((st (make-syntax-table)))
                      (modify-syntax-entry ?_ "w" st)
                      (modify-syntax-entry ?- "_" st)
                      (modify-syntax-entry ?# "<" st)
                      (modify-syntax-entry ?\n ">" st)
                      (set-syntax-table st)
                      (insert "  hello_world  --sym-- 42 # comment\nmore words here")
                      (goto-char (point-min))
                      (let ((results nil))
                        ;; Skip whitespace forward
                        (let ((skipped1 (skip-syntax-forward " ")))
                          (setq results (cons (list 'ws-fwd skipped1 (point)) results)))
                        ;; Skip word forward
                        (let ((skipped2 (skip-syntax-forward "w")))
                          (setq results (cons (list 'word-fwd skipped2 (point)) results)))
                        ;; Skip whitespace
                        (skip-syntax-forward " ")
                        ;; Skip symbol chars
                        (let ((skipped3 (skip-syntax-forward "_")))
                          (setq results (cons (list 'sym-fwd skipped3 (point)) results)))
                        ;; Skip mixed: word+symbol
                        (skip-syntax-forward " ")
                        (let ((skipped4 (skip-syntax-forward "w_")))
                          (setq results (cons (list 'mixed-fwd skipped4 (point)) results)))
                        ;; skip-syntax-forward with LIMIT
                        (goto-char (point-min))
                        (let ((skipped5 (skip-syntax-forward " " 4)))
                          (setq results (cons (list 'ws-limited skipped5 (point)) results)))
                        ;; skip-syntax-backward
                        (goto-char (point-max))
                        (let ((skipped6 (skip-syntax-backward "w")))
                          (setq results (cons (list 'word-back skipped6 (point)) results)))
                        ;; skip-syntax-backward with LIMIT
                        (goto-char (point-max))
                        (let ((skipped7 (skip-syntax-backward "w" (- (point-max) 3))))
                          (setq results (cons (list 'word-back-lim skipped7 (point)) results)))
                        ;; Complemented syntax class: skip everything except word
                        (goto-char (point-min))
                        (let ((skipped8 (skip-syntax-forward "^ w")))
                          (setq results (cons (list 'not-word-fwd skipped8 (point)) results)))
                        (nreverse results))))"#;
    let expect = expect_test::expect![[
        r#""OK ((ws-fwd 2 3) (word-fwd 11 14) (sym-fwd 2 18) (mixed-fwd 5 23) (ws-limited 2 3) (word-back -4 48) (word-back-lim -3 49) (not-word-fwd 0 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment with various comment styles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_forward_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (let ((st (make-syntax-table)))
                      ;; Set up # as line-comment start, newline as comment end
                      (modify-syntax-entry ?# "<" st)
                      (modify-syntax-entry ?\n ">" st)
                      (set-syntax-table st)
                      (insert "code # comment1\ncode2 # comment2\ncode3")
                      ;; Test forward-comment
                      (goto-char 6) ;; at '#' of first comment
                      (let ((before1 (point))
                            (_ (forward-comment 1))
                            (after1 (point)))
                        ;; Skip to next comment
                        (skip-syntax-forward " w")
                        (let ((before2 (point))
                              (_ (forward-comment 1))
                              (after2 (point)))
                          ;; Test forward-comment with count > 1
                          (goto-char 6)
                          (let ((before-multi (point))
                                (_ (forward-comment 2))
                                (after-multi (point)))
                            ;; Test backward-comment (negative count)
                            (goto-char (point-max))
                            (let ((before-back (point))
                                  (_ (forward-comment -1))
                                  (after-back (point)))
                              (list before1 after1
                                    before2 after2
                                    before-multi after-multi
                                    before-back after-back)))))))"#;
    let expect = expect_test::expect![[r#""OK (6 17 23 34 6 17 39 39)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modify syntax for ranges and standard chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_ranges_and_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (let ((st (make-syntax-table)))
                      ;; Modify a range: make digits into symbol chars
                      (modify-syntax-entry '(?0 . ?9) "_" st)
                      ;; Make lowercase letters punctuation
                      (modify-syntax-entry '(?a . ?f) "." st)
                      (set-syntax-table st)
                      ;; Read back individual chars
                      (let ((results nil))
                        (dolist (ch '(?0 ?5 ?9 ?a ?c ?f ?g ?z ?A ?Z))
                          (setq results (cons (list ch (char-syntax ch)) results)))
                        ;; Test skip-syntax with modified classes
                        (insert "012abc789ghi")
                        (goto-char (point-min))
                        ;; Skip symbol chars (digits)
                        (let ((s1 (skip-syntax-forward "_")))
                          (let ((p1 (point)))
                            ;; Skip punctuation (a-f)
                            (let ((s2 (skip-syntax-forward ".")))
                              (let ((p2 (point)))
                                ;; Skip remaining symbol chars (789)
                                (let ((s3 (skip-syntax-forward "_")))
                                  (let ((p3 (point)))
                                    (list (nreverse results)
                                          (list s1 p1 s2 p2 s3 p3))))))))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 25 77)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: custom language syntax for tokenization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_table_comprehensive_custom_language() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a syntax table for a Python-like language:
    // - # starts line comments
    // - _ is word constituent (identifiers)
    // - : is punctuation
    // - ( ) are parens
    // - ' and " are string delimiters
    // Tokenize a snippet using skip-syntax
    let form = r#"(with-temp-buffer
                    (let ((st (make-syntax-table)))
                      (modify-syntax-entry ?# "<" st)
                      (modify-syntax-entry ?\n ">" st)
                      (modify-syntax-entry ?_ "w" st)
                      (modify-syntax-entry ?: "." st)
                      (modify-syntax-entry ?\( "()" st)
                      (modify-syntax-entry ?\) ")(" st)
                      (modify-syntax-entry ?' "\"" st)
                      (modify-syntax-entry ?\" "\"" st)
                      (modify-syntax-entry '(?a . ?z) "w" st)
                      (modify-syntax-entry '(?A . ?Z) "w" st)
                      (modify-syntax-entry '(?0 . ?9) "w" st)
                      (set-syntax-table st)
                      (insert "def foo_bar(x): # comment\n  return x + 1")
                      (goto-char (point-min))
                      (let ((tokens nil))
                        ;; Token 1: "def"
                        (skip-syntax-forward " ")
                        (let ((s (point)))
                          (skip-syntax-forward "w")
                          (setq tokens (cons (buffer-substring s (point)) tokens)))
                        ;; Token 2: "foo_bar"
                        (skip-syntax-forward " ")
                        (let ((s (point)))
                          (skip-syntax-forward "w")
                          (setq tokens (cons (buffer-substring s (point)) tokens)))
                        ;; Token 3: "(" — open paren
                        (let ((s (point)))
                          (setq tokens (cons (char-syntax (char-after)) tokens))
                          (forward-char 1))
                        ;; Token 4: "x"
                        (let ((s (point)))
                          (skip-syntax-forward "w")
                          (setq tokens (cons (buffer-substring s (point)) tokens)))
                        ;; Token 5: ")" — close paren
                        (setq tokens (cons (char-syntax (char-after)) tokens))
                        (forward-char 1)
                        ;; Token 6: ":" — punctuation
                        (setq tokens (cons (char-syntax (char-after)) tokens))
                        (forward-char 1)
                        ;; Skip comment
                        (skip-syntax-forward " ")
                        (forward-comment 1)
                        ;; Token 7: "return"
                        (skip-syntax-forward " ")
                        (let ((s (point)))
                          (skip-syntax-forward "w")
                          (setq tokens (cons (buffer-substring s (point)) tokens)))
                        (nreverse tokens))))"#;
    let expect = expect_test::expect![[r#""OK (\"def\" \"foo_bar\" 40 \"x\" 41 46 \"return\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
