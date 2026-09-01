//! Oracle parity tests for GNU mapping primitive edge semantics.
//!
//! GNU implements `mapconcat`, `mapcar`, `mapc`, and `mapcan` in `src/fns.c`
//! through `mapcar1`.  `mapcar1` computes sequence length up front, but for
//! lists it stops early if the list is shortened as a side effect.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_mapcar_mapc_sequence_types_and_char_table_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((bv (make-bool-vector 3 nil)))
  (aset bv 1 t)
  (list
   (mapcar #'identity '(a b c))
   (mapcar #'identity [a b c])
   (mapcar #'identity "aé")
   (mapcar #'identity bv)
   (let ((v [a b c]))
     (list (eq v (mapc #'ignore v)) v))
   (condition-case err
       (mapcar #'identity (make-char-table 'test nil))
     (error (list (car err) (cdr err))))
   (condition-case err
       (mapc #'ignore 42)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((a b c) (a b c) (97 233) (nil t nil) (t [a b c]) (wrong-type-argument (listp #^[nil nil test nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil])) (wrong-type-argument (sequencep 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_map_functions_accept_byte_code_sequence_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:mapcar1 accepts CLOSUREP sequences, which includes
    // byte-code functions.  The byte-code object is traversed as its vector of
    // slots, while char-tables still take the explicit `listp' error path.
    let form = r#"
(let ((bc #[257 "\300\207" [42] 1]))
  (list
   (type-of bc)
   (length bc)
   (mapcar #'type-of bc)
   (let ((seen nil))
     (list (eq (mapc (lambda (x) (push (type-of x) seen)) bc) bc)
           (nreverse seen)))
   (mapcan (lambda (x) (list (type-of x))) bc)
   (mapconcat (lambda (x) (symbol-name (type-of x))) bc ",")))
"#;

    let expect = expect_test::expect![[
        r#""OK (byte-code-function 4 (integer string vector integer) (t (integer string vector integer)) (integer string vector integer) \"integer,string,vector,integer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapcar_stops_when_list_shortened_by_callback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((seq (list 1 2 3 4))
      (seen nil))
  (list
   (mapcar (lambda (x)
             (push x seen)
             (when (= x 1)
               (setcdr seq nil))
             (* x 10))
           seq)
   (nreverse seen)
   seq))
"#;

    let expect = expect_test::expect![[r#""OK ((10) (1) (1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapcar_follows_callback_rewritten_cdr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:mapcar1 reads XCDR after FUNCTION returns.  This control
    // case pins that traversal rule for mapcar, not only mapc/mapcan.
    let form = r#"
(let ((seq (list 1 2))
      (replacement (list 99))
      (seen nil))
  (list
   (mapcar (lambda (x)
             (push x seen)
             (when (= x 1)
               (setcdr seq replacement))
             (* x 10))
           seq)
   (nreverse seen)
   seq
   replacement))
"#;

    let expect = expect_test::expect![[r#""OK ((10 990) (1 99) (1 99) (99))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapc_stops_when_list_shortened_by_callback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:mapcar1 reads XCDR after calling FUNCTION, so destructive
    // shortening by FUNCTION stops `mapc` at the new end of the list.
    let form = r#"
(let ((seq (list 1 2 3 4))
      (seen nil))
  (list
   (eq (mapc (lambda (x)
               (push x seen)
               (when (= x 1)
                 (setcdr seq nil)))
             seq)
       seq)
   (nreverse seen)
   seq))
"#;

    let expect = expect_test::expect![[r#""OK (t (1) (1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapc_follows_callback_rewritten_cdr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:mapcar1 reads XCDR after FUNCTION returns, so replacing
    // the current cons cdr changes which element the next iteration sees.
    let form = r#"
(let ((seq (list 1 2))
      (replacement (list 99))
      (seen nil))
  (list
   (eq (mapc (lambda (x)
               (push x seen)
               (when (= x 1)
                 (setcdr seq replacement)))
             seq)
       seq)
   (nreverse seen)
   seq
   replacement))
"#;

    let expect = expect_test::expect![[r#""OK (t (1 99) (1 99) (99))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapconcat_separator_and_return_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (mapconcat #'identity '("a" nil "c") nil)
 (mapconcat #'identity '("a" "b" "c") [?|])
 (mapconcat (lambda (x) (vector (+ ?0 x))) '(1 2 3) '(?-))
 (mapconcat #'identity [] ",")
 (condition-case err
     (mapconcat #'identity '("a" bad "c") ",")
   (error (list (car err) (cdr err))))
 (condition-case err
     (mapconcat (lambda (_x) 42) '(a) ",")
   (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 40)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapconcat_follows_callback_rewritten_cdr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fmapconcat maps through mapcar1.  mapcar1 reads XCDR
    // after FUNCTION returns, so rewiring the current cons cdr changes the
    // next mapped element before concat assembles the string.
    let form = r#"
(let ((seq (list "a" "b"))
      (replacement (list "z"))
      (seen nil))
  (list
   (mapconcat (lambda (x)
                (push x seen)
                (when (equal x "a")
                  (setcdr seq replacement))
                x)
              seq
              ",")
   (nreverse seen)
   seq
   replacement))
"#;

    let expect = expect_test::expect![[r#""OK (\"a,z\" (\"a\" \"z\") (\"a\" \"z\") (\"z\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapcan_destructive_nconc_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((first (list 'a))
       (second (list 'b))
       (third (list 'c))
       (input (list first nil second third))
       (result (mapcan #'identity input)))
  (list
   result
   (eq result first)
   (cdr first)
   (eq (cdr first) second)
   (eq (cdr second) third)
   input
   (condition-case err
       (mapcan (lambda (_x) 42) '(a b))
     (error (list (car err) (cdr err)))))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 16 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapcan_stops_when_list_shortened_by_callback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fmapcan maps with mapcar1 before calling nconc, so a
    // callback that shortens the input list should limit the mapped lists too.
    let form = r#"
(let ((seq (list 'a 'b 'c))
      (seen nil))
  (list
   (mapcan (lambda (x)
             (push x seen)
             (when (eq x 'a)
               (setcdr seq nil))
             (list x x))
           seq)
   (nreverse seen)
   seq))
"#;

    let expect = expect_test::expect![[r#""OK ((a a) (a) (a))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapcan_follows_callback_rewritten_cdr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fmapcan shares mapcar1 traversal, then nconcs the mapped
    // result lists.  Rewiring the current cons cdr should change the next
    // mapped element before nconc runs.
    let form = r#"
(let ((seq (list 'a 'b))
      (replacement (list 'z))
      (seen nil))
  (list
   (mapcan (lambda (x)
             (push x seen)
             (when (eq x 'a)
               (setcdr seq replacement))
             (list x))
           seq)
   (nreverse seen)
   seq
   replacement))
"#;

    let expect = expect_test::expect![[r#""OK ((a z) (a z) (a z) (z))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mapping_dotted_and_circular_input_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((cycle (list 'a 'b))
       (_ (setcdr (last cycle) cycle)))
  (list
   (condition-case err
       (mapcar #'identity '(a b . c))
     (error (list (car err) (cdr err))))
   (condition-case err
       (mapconcat #'identity '("a" "b" . c) ",")
     (error (list (car err) (cdr err))))
   (condition-case err
       (mapcan #'list '(a b . c))
     (error (list (car err) (cdr err))))
   (condition-case err
       (mapcar #'identity cycle)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp c)) (wrong-type-argument (listp c)) (wrong-type-argument (listp c)) (circular-list ((a b a b . #2))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
