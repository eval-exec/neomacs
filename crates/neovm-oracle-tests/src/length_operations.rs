//! Oracle parity tests for `length`, `safe-length`, `proper-list-p`,
//! `string-bytes`, `string-width`, and length comparison operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// length on various types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_length_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (length nil)
                        (length '(a))
                        (length '(a b c))
                        (length '(1 2 3 4 5 6 7 8 9 10)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 3 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_length_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (length "")
                        (length "hello")
                        (length "café")
                        (length "日本語"))"#;
    let expect = expect_test::expect![[r#""OK (0 5 4 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_length_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (length [])
                        (length [1 2 3])
                        (length (make-vector 100 0)))"#;
    let expect = expect_test::expect![[r#""OK (0 3 100)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_length_and_sequencep_vectorlike_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:Flength accepts char-tables, bool-vectors, closures, and
    // records.  GNU data.c:Fsequencep is narrower: lists or arrays only, so
    // records and closures are not sequences even though `length` accepts them.
    let form = r#"
(let ((table (make-char-table 'generic 65))
      (rec (record 'tag 1 2))
      (fun (lambda (x) x))
      (bv (make-bool-vector 3 t)))
  (list
   (sequencep table)
   (length table)
   (sequencep rec)
   (length rec)
   (sequencep fun)
   (length fun)
   (sequencep bv)
   (length bv)))
"#;
    let expect = expect_test::expect![[r#""OK (t 4194304 nil 3 nil 3 t 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_length_comparison_vectorlike_and_error_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:length<, length>, and length= validate LENGTH as a fixnum,
    // use a list-specific bounded traversal for conses, and otherwise compare
    // against Flength.  This preserves the same record/closure/char-table
    // acceptance as `length`, plus dotted-list short-circuit behavior.
    let form = r#"
(list
 (length< (record 'tag 1 2) 4)
 (length= (lambda (x) x) 3)
 (length> (make-char-table 'generic 65) 1000)
 (condition-case err
     (length< 42 1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (length= '(a b . c) 2)
   (error (list (car err) (cdr err))))
 (condition-case err
     (length< '(a b . c) 3)
   (error (list (car err) (cdr err))))
 (condition-case err
     (length> '(a b . c) 1)
   (error (list (car err) (cdr err)))))
"#;
    let expect =
        expect_test::expect![[r#""OK (t t t (wrong-type-argument (sequencep 42)) t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// safe-length (handles circular/dotted lists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_safe_length_normal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (safe-length nil)
                        (safe-length '(a b c))
                        (safe-length '(1 2 3 4 5))
                        (safe-length "not a list")
                        (safe-length 42))"#;
    let expect = expect_test::expect![[r#""OK (0 3 5 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_safe_length_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dotted list: (a b . c) has safe-length 2
    let form = r#"(list (safe-length '(a . b))
                        (safe-length '(a b . c))
                        (safe-length '(1 2 3 . 4)))"#;
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// proper-list-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proper_list_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (proper-list-p nil)
                        (proper-list-p '(a b c))
                        (proper-list-p '(1))
                        (proper-list-p '(a . b))
                        (proper-list-p '(a b . c))
                        (proper-list-p 42)
                        (proper-list-p "string")
                        (proper-list-p [vector]))"#;
    let expect = expect_test::expect![[r#""OK (0 3 1 nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-bytes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (string-bytes "")
                        (string-bytes "hello")
                        (string-bytes "café")
                        (string-bytes "日本語")
                        (string-bytes "\x00\x01\x02"))"#;
    let expect = expect_test::expect![[r#""OK (0 5 5 9 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_bytes_vs_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-bytes >= length; equality only for ASCII
    let form = r#"(let ((ascii "hello world")
                        (multi "héllo wörld")
                        (cjk "日本語テスト"))
                    (list (= (string-bytes ascii) (length ascii))
                          (> (string-bytes multi) (length multi))
                          (> (string-bytes cjk) (length cjk))
                          (- (string-bytes multi) (length multi))
                          (- (string-bytes cjk) (length cjk))))"#;
    let expect = expect_test::expect![[r#""OK (t t t 2 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (string-width "")
                        (string-width "hello")
                        (string-width "café"))"#;
    let expect = expect_test::expect![[r#""OK (0 5 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_width_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CJK characters are typically double-width
    let form = r#"(list (string-width "日本語")
                        (string-width "Abc")
                        (string-width "A日B本C語"))"#;
    let expect = expect_test::expect![[r#""OK (6 3 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: string statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_length_string_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute various statistics about a list of strings
    let form = r#"(let ((strings '("hello" "world" "café" "日本語" "" "a")))
                    (let ((total-chars 0)
                          (total-bytes 0)
                          (total-width 0)
                          (max-len 0)
                          (min-len most-positive-fixnum)
                          (remaining strings))
                      (while remaining
                        (let* ((s (car remaining))
                               (len (length s)))
                          (setq total-chars (+ total-chars len)
                                total-bytes (+ total-bytes (string-bytes s))
                                total-width (+ total-width (string-width s))
                                max-len (max max-len len)
                                min-len (min min-len len)
                                remaining (cdr remaining))))
                      (list total-chars total-bytes total-width
                            max-len min-len
                            (length strings))))"#;
    let expect = expect_test::expect![[r#""OK (18 25 21 5 0 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: group-by-length
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_length_group_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Group a list of strings by their length
    let form = r#"(let ((words '("a" "bb" "cc" "ddd" "ee" "f" "ggg" "hh")))
                    (let ((groups nil))
                      (dolist (w words)
                        (let* ((len (length w))
                               (existing (assq len groups)))
                          (if existing
                              (setcdr existing
                                      (append (cdr existing) (list w)))
                            (setq groups
                                  (cons (list len w) groups)))))
                      ;; Sort by length
                      (sort groups
                            (lambda (a b) (< (car a) (car b))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 \"a\" \"f\") (2 \"bb\" \"cc\" \"ee\" \"hh\") (3 \"ddd\" \"ggg\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: proper-list validation pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proper_list_filter_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Filter and classify a heterogeneous collection
    let form = r#"(let ((items (list nil
                                    '(a b c)
                                    '(x . y)
                                    42
                                    "str"
                                    '(1 2 3 4)
                                    '(p q . r)
                                    [vec]
                                    '(single))))
                    (let ((proper nil)
                          (improper nil)
                          (non-list nil))
                      (dolist (item items)
                        (cond
                         ((proper-list-p item)
                          (setq proper
                                (cons (list item (safe-length item))
                                      proper)))
                         ((consp item)
                          (setq improper
                                (cons (list item (safe-length item))
                                      improper)))
                         (t
                          (setq non-list (cons item non-list)))))
                      (list (nreverse proper)
                            (nreverse improper)
                            (nreverse non-list))))"#;
    let expect = expect_test::expect![[
        r#""OK (((nil 0) ((a b c) 3) ((1 2 3 4) 4) ((single) 1)) (((x . y) 1) ((p q . r) 2)) (42 \"str\" [vec]))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: padded column formatting with string-width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_width_column_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Format entries to align in columns based on display width
    let form = r#"(let ((entries '(("Name" "Age" "City")
                                    ("Alice" "30" "Boston")
                                    ("Bob" "25" "NYC"))))
                    ;; Compute max width per column
                    (let ((ncols (length (car entries)))
                          (col-widths nil))
                      (let ((i 0))
                        (while (< i ncols)
                          (let ((max-w 0))
                            (dolist (row entries)
                              (let ((w (string-width (nth i row))))
                                (when (> w max-w) (setq max-w w))))
                            (setq col-widths (append col-widths (list max-w))))
                          (setq i (1+ i))))
                      ;; Format each row
                      (mapcar
                       (lambda (row)
                         (let ((parts nil) (i 0))
                           (while (< i ncols)
                             (let* ((cell (nth i row))
                                    (pad (- (nth i col-widths)
                                            (string-width cell))))
                               (setq parts
                                     (cons (concat cell
                                                   (make-string
                                                    (max 0 pad) ?\ ))
                                           parts)))
                             (setq i (1+ i)))
                           (mapconcat #'identity (nreverse parts) " | ")))
                       entries)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Name  | Age | City  \" \"Alice | 30  | Boston\" \"Bob   | 25  | NYC   \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
