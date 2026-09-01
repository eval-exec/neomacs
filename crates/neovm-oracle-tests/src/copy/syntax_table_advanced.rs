//! Advanced oracle parity tests for `copy-syntax-table` and `make-syntax-table`.
//!
//! Covers: independent copy semantics, modifying copy without affecting original,
//! `make-syntax-table` with parent table inheritance, `syntax-table-p` predicate,
//! `set-syntax-table` switching between tables, and building a custom mini-language
//! syntax table with `char-syntax` verification.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// copy-syntax-table creates a fully independent deep copy
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_syntax_table_adv_independent_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a syntax table, set several entries, copy it,
    // verify the copy has identical entries, then mutate the copy
    // and verify the original is untouched.
    let form = r#"(with-temp-buffer
  (let ((orig (make-syntax-table)))
    ;; Set up a non-trivial configuration in the original
    (modify-syntax-entry ?@ "w" orig)
    (modify-syntax-entry ?# "<" orig)
    (modify-syntax-entry ?\n ">" orig)
    (modify-syntax-entry ?| "|" orig)
    (modify-syntax-entry ?_ "_" orig)
    (modify-syntax-entry ?~ "'" orig)
    ;; Copy it
    (let ((copy (copy-syntax-table orig)))
      ;; Verify copy matches original before mutation
      (set-syntax-table orig)
      (let ((orig-at (char-syntax ?@))
            (orig-hash (char-syntax ?#))
            (orig-nl (char-syntax ?\n))
            (orig-pipe (char-syntax ?|))
            (orig-under (char-syntax ?_))
            (orig-tilde (char-syntax ?~)))
        (set-syntax-table copy)
        (let ((copy-at (char-syntax ?@))
              (copy-hash (char-syntax ?#))
              (copy-nl (char-syntax ?\n))
              (copy-pipe (char-syntax ?|))
              (copy-under (char-syntax ?_))
              (copy-tilde (char-syntax ?~)))
          (list
           ;; All entries match before mutation
           (= orig-at copy-at)
           (= orig-hash copy-hash)
           (= orig-nl copy-nl)
           (= orig-pipe copy-pipe)
           (= orig-under copy-under)
           (= orig-tilde copy-tilde)
           ;; They are distinct objects
           (not (eq orig copy))))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modifying copy does not affect the original
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_syntax_table_adv_modify_copy_no_effect_on_original() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Aggressively mutate the copy: change classes, add paired delimiters,
    // add comment flags. The original must remain identical.
    let form = r#"(with-temp-buffer
  (let ((orig (make-syntax-table)))
    (modify-syntax-entry ?@ "w" orig)
    (modify-syntax-entry ?$ "_" orig)
    (modify-syntax-entry ?< "(>" orig)
    (modify-syntax-entry ?> ")<" orig)
    ;; Snapshot original values
    (set-syntax-table orig)
    (let ((snap-at (char-syntax ?@))
          (snap-dollar (char-syntax ?$))
          (snap-lt (char-syntax ?<))
          (snap-gt (char-syntax ?>))
          (snap-a (char-syntax ?a)))
      ;; Copy and heavily mutate
      (let ((copy (copy-syntax-table orig)))
        (modify-syntax-entry ?@ "." copy)
        (modify-syntax-entry ?$ "w" copy)
        (modify-syntax-entry ?< "w" copy)
        (modify-syntax-entry ?> "_" copy)
        (modify-syntax-entry ?a "." copy)
        (modify-syntax-entry ?z "|" copy)
        (modify-syntax-entry ?0 "<" copy)
        ;; Verify original is untouched
        (set-syntax-table orig)
        (list
         (= (char-syntax ?@) snap-at)
         (= (char-syntax ?$) snap-dollar)
         (= (char-syntax ?<) snap-lt)
         (= (char-syntax ?>) snap-gt)
         (= (char-syntax ?a) snap-a)
         ;; Verify copy has the new values
         (progn
           (set-syntax-table copy)
           (list
            (char-syntax ?@)
            (char-syntax ?$)
            (char-syntax ?<)
            (char-syntax ?>)
            (char-syntax ?a)
            (char-syntax ?z)
            (char-syntax ?0))))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t (46 119 119 95 46 124 60))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-syntax-table with parent: child inherits, parent unaffected
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_syntax_table_adv_make_with_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a parent with custom entries. Create a child via make-syntax-table
    // with that parent. Child should inherit entries. Overriding in child
    // must not affect parent. Test multi-level: grandparent -> parent -> child.
    let form = r#"(with-temp-buffer
  (let ((gp (make-syntax-table)))
    ;; Grandparent: set unique entries
    (modify-syntax-entry ?! "w" gp)
    (modify-syntax-entry ?% "_" gp)
    (modify-syntax-entry ?^ "." gp)
    (let ((parent (make-syntax-table gp)))
      ;; Parent overrides some, adds new
      (modify-syntax-entry ?! "." parent)
      (modify-syntax-entry ?& "w" parent)
      (let ((child (make-syntax-table parent)))
        ;; Child overrides parent's &
        (modify-syntax-entry ?& "_" child)
        (modify-syntax-entry ?* "<" child)
        ;; Now test all three levels
        (set-syntax-table gp)
        (let ((gp-bang (char-syntax ?!))
              (gp-pct (char-syntax ?%))
              (gp-caret (char-syntax ?^))
              (gp-amp (char-syntax ?&)))
          (set-syntax-table parent)
          (let ((p-bang (char-syntax ?!))
                (p-pct (char-syntax ?%))
                (p-caret (char-syntax ?^))
                (p-amp (char-syntax ?&)))
            (set-syntax-table child)
            (let ((c-bang (char-syntax ?!))
                  (c-pct (char-syntax ?%))
                  (c-caret (char-syntax ?^))
                  (c-amp (char-syntax ?&))
                  (c-star (char-syntax ?*)))
              (list
               ;; Grandparent values
               gp-bang gp-pct gp-caret gp-amp
               ;; Parent: ! overridden, % and ^ inherited from gp, & new
               p-bang p-pct p-caret p-amp
               ;; Child: ! inherited through parent (.), % inherited (gp _),
               ;; ^ inherited (gp .), & overridden to _, * new <
               c-bang c-pct c-caret c-amp c-star
               ;; Verify grandparent unaffected by descendants
               (progn (set-syntax-table gp)
                      (= (char-syntax ?!) gp-bang))))))))))"#;
    let expect = expect_test::expect![[r#""OK (119 95 46 95 46 95 46 119 46 95 46 95 60 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// syntax-table-p predicate on various objects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_syntax_table_adv_syntax_table_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // syntax-table-p should return t for syntax tables (which are char-tables
    // with subtype 'syntax-table), nil for everything else.
    let form = r#"(list
  ;; Positive cases
  (syntax-table-p (make-syntax-table))
  (syntax-table-p (copy-syntax-table (standard-syntax-table)))
  (syntax-table-p (make-syntax-table (standard-syntax-table)))
  (syntax-table-p (standard-syntax-table))
  ;; Negative cases
  (syntax-table-p nil)
  (syntax-table-p t)
  (syntax-table-p 42)
  (syntax-table-p "hello")
  (syntax-table-p '(1 2 3))
  (syntax-table-p [1 2 3])
  (syntax-table-p (make-char-table 'generic))
  (syntax-table-p (make-hash-table))
  ;; A char-table with wrong subtype is NOT a syntax table
  (syntax-table-p (make-char-table 'foo))
  ;; Verify syntax-table returns a syntax table
  (with-temp-buffer
    (syntax-table-p (syntax-table))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil nil nil nil nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-syntax-table switching between tables in multiple buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_syntax_table_adv_set_syntax_table_switching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create three syntax tables with distinct configurations.
    // Switch between them in a single buffer and across buffers.
    // Verify char-syntax always reflects the currently active table.
    let form = r#"(let ((st-lisp (make-syntax-table))
      (st-c (make-syntax-table))
      (st-sql (make-syntax-table)))
  ;; Lisp: ; is comment-start, - is symbol constituent
  (modify-syntax-entry ?\; "<" st-lisp)
  (modify-syntax-entry ?- "_" st-lisp)
  (modify-syntax-entry ?' "'" st-lisp)
  ;; C: / is punctuation (for // comments we'd need two-char),
  ;;    _ is word constituent, ; is punctuation
  (modify-syntax-entry ?/ "." st-c)
  (modify-syntax-entry ?_ "w" st-c)
  (modify-syntax-entry ?\; "." st-c)
  ;; SQL: - is punctuation, _ is word, ; is punctuation
  (modify-syntax-entry ?- "." st-sql)
  (modify-syntax-entry ?_ "w" st-sql)
  (modify-syntax-entry ?\; "." st-sql)
  (modify-syntax-entry ?# "<" st-sql)
  (with-temp-buffer
    ;; Start with Lisp
    (set-syntax-table st-lisp)
    (let ((lisp-semi (char-syntax ?\;))
          (lisp-dash (char-syntax ?-))
          (lisp-quote (char-syntax ?')))
      ;; Switch to C
      (set-syntax-table st-c)
      (let ((c-semi (char-syntax ?\;))
            (c-under (char-syntax ?_))
            (c-slash (char-syntax ?/)))
        ;; Switch to SQL
        (set-syntax-table st-sql)
        (let ((sql-dash (char-syntax ?-))
              (sql-under (char-syntax ?_))
              (sql-hash (char-syntax ?#)))
          ;; Switch back to Lisp to prove non-destructive
          (set-syntax-table st-lisp)
          (let ((lisp-semi2 (char-syntax ?\;))
                (lisp-dash2 (char-syntax ?-)))
            (list
             lisp-semi lisp-dash lisp-quote
             c-semi c-under c-slash
             sql-dash sql-under sql-hash
             ;; Round-trip: values match original
             (= lisp-semi lisp-semi2)
             (= lisp-dash lisp-dash2)
             ;; Cross-table differences
             (not (= lisp-semi c-semi))
             (not (= lisp-dash sql-dash)))))))))"#;
    let expect = expect_test::expect![[r#""OK (60 95 39 46 119 46 46 119 60 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: custom syntax table for a mini-language with full tokenization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_syntax_table_adv_mini_language() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a syntax table for a mini-language "MiniML":
    //   - Identifiers: letters, digits, underscores (word constituents)
    //   - Operators: + - * / = < > (punctuation)
    //   - Delimiters: ( ) [ ] (paired)
    //   - Strings: " (string quote)
    //   - Comments: (* ... *) style (two-char comment delimiters)
    //   - Symbol: ' (expression prefix for quoting)
    // Copy the table, modify the copy for a variant dialect,
    // then tokenize the same input with both and compare results.
    let form = r#"(progn
  (fset 'neovm--cst-tokenize
    (lambda (st text)
      "Tokenize TEXT using syntax table ST, returning list of (class . string) tokens."
      (with-temp-buffer
        (set-syntax-table st)
        (insert text)
        (goto-char (point-min))
        (let ((tokens nil))
          (while (< (point) (point-max))
            (let ((ch (char-after (point)))
                  (start (point)))
              (let ((syn (char-syntax ch)))
                (cond
                 ;; Whitespace: skip
                 ((= syn ?\s)
                  (skip-syntax-forward " ")
                  nil)
                 ;; Word: accumulate word chars
                 ((= syn ?w)
                  (skip-syntax-forward "w")
                  (setq tokens (cons (cons 'word (buffer-substring start (point))) tokens)))
                 ;; Symbol constituent
                 ((= syn ?_)
                  (skip-syntax-forward "_")
                  (setq tokens (cons (cons 'symbol (buffer-substring start (point))) tokens)))
                 ;; Punctuation
                 ((= syn ?.)
                  (forward-char 1)
                  (setq tokens (cons (cons 'punct (buffer-substring start (point))) tokens)))
                 ;; Open paren
                 ((= syn ?\()
                  (forward-char 1)
                  (setq tokens (cons (cons 'open (buffer-substring start (point))) tokens)))
                 ;; Close paren
                 ((= syn ?\))
                  (forward-char 1)
                  (setq tokens (cons (cons 'close (buffer-substring start (point))) tokens)))
                 ;; String delimiter
                 ((= syn ?\")
                  (forward-sexp 1)
                  (setq tokens (cons (cons 'string (buffer-substring start (point))) tokens)))
                 ;; Expression prefix
                 ((= syn ?')
                  (forward-char 1)
                  (setq tokens (cons (cons 'prefix (buffer-substring start (point))) tokens)))
                 ;; Anything else: advance
                 (t (forward-char 1)
                    (setq tokens (cons (cons 'other (buffer-substring start (point))) tokens)))))))
          (nreverse tokens)))))

  (unwind-protect
      (let ((st-miniml (make-syntax-table)))
        ;; Configure MiniML syntax
        (modify-syntax-entry '(?a . ?z) "w" st-miniml)
        (modify-syntax-entry '(?A . ?Z) "w" st-miniml)
        (modify-syntax-entry '(?0 . ?9) "w" st-miniml)
        (modify-syntax-entry ?_ "w" st-miniml)
        (modify-syntax-entry ?+ "." st-miniml)
        (modify-syntax-entry ?- "." st-miniml)
        (modify-syntax-entry ?* "." st-miniml)
        (modify-syntax-entry ?/ "." st-miniml)
        (modify-syntax-entry ?= "." st-miniml)
        (modify-syntax-entry ?< "." st-miniml)
        (modify-syntax-entry ?> "." st-miniml)
        (modify-syntax-entry ?\( "()" st-miniml)
        (modify-syntax-entry ?\) ")(" st-miniml)
        (modify-syntax-entry ?\[ "(]" st-miniml)
        (modify-syntax-entry ?\] ")[" st-miniml)
        (modify-syntax-entry ?\" "\"" st-miniml)
        (modify-syntax-entry ?' "'" st-miniml)
        ;; Create a variant: underscores are symbol-constituent, not word
        (let ((st-variant (copy-syntax-table st-miniml)))
          (modify-syntax-entry ?_ "_" st-variant)
          ;; In variant, | is string fence
          (modify-syntax-entry ?| "|" st-variant)
          (let* ((input "let foo_bar = (x + 42) 'q")
                 (tokens-ml (funcall 'neovm--cst-tokenize st-miniml input))
                 (tokens-var (funcall 'neovm--cst-tokenize st-variant input)))
            (list
             tokens-ml
             tokens-var
             ;; They should differ: _ handling changes tokenization
             (not (equal tokens-ml tokens-var))
             ;; Verify both syntax tables still valid
             (syntax-table-p st-miniml)
             (syntax-table-p st-variant)
             ;; Token counts
             (length tokens-ml)
             (length tokens-var)))))
    (fmakunbound 'neovm--cst-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK (((word . \"let\") (word . \"foo_bar\") (punct . \"=\") (open . \"(\") (word . \"x\") (punct . \"+\") (word . \"42\") (close . \")\") (prefix . \"'\") (word . \"q\")) ((word . \"let\") (word . \"foo\") (symbol . \"_\") (word . \"bar\") (punct . \"=\") (open . \"(\") (word . \"x\") (punct . \"+\") (word . \"42\") (close . \")\") (prefix . \"'\") (word . \"q\")) t t t 10 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-syntax-table with nil argument copies the standard syntax table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_syntax_table_adv_nil_copies_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // copy-syntax-table with no argument (or nil) should copy the
    // current buffer's syntax table. Verify by comparing entries
    // against the standard syntax table.
    let form = r#"(with-temp-buffer
  (let ((std-copy (copy-syntax-table)))
    ;; Verify it is a syntax table
    (let ((is-st (syntax-table-p std-copy)))
      ;; Check some standard entries match
      (set-syntax-table std-copy)
      (let ((copy-a (char-syntax ?a))
            (copy-space (char-syntax ?\s))
            (copy-paren (char-syntax ?\())
            (copy-close (char-syntax ?\)))
            (copy-quote (char-syntax ?\")))
        ;; Now modify the copy
        (modify-syntax-entry ?a "." std-copy)
        (let ((modified-a (char-syntax ?a)))
          ;; Switch back to standard to verify it is unmodified
          (set-syntax-table (standard-syntax-table))
          (let ((std-a (char-syntax ?a)))
            (list
             is-st
             copy-a copy-space copy-paren copy-close copy-quote
             ;; Copy was modified
             modified-a
             ;; Standard is unchanged
             std-a
             ;; Values before modification matched standard
             (= copy-a std-a)
             ;; After modification they differ
             (not (= modified-a std-a)))))))))"#;
    let expect = expect_test::expect![[r#""OK (t 119 32 40 41 34 46 119 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
