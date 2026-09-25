//! Oracle parity for the `switch` opcode's jump-table lookup.
//!
//! A top-level `pcase` is interpreted, so every form here byte-compiles a
//! lambda and calls it: that is what executes GNU `Bswitch`
//! (src/bytecode.c), which looks the dispatch value up in a constant hash
//! table under the table's own test. Each corpus is dispatched well past the
//! JIT's hotness threshold, so the answer is pinned for the interpreter's
//! first dispatches, for a table dispatched repeatedly, and for compiled code.
//!
//! The mutation cases stay inside what GNU itself answers consistently: GNU
//! scans a small `eq` table linearly up to its COUNT, so a `remhash` that
//! leaves a hole there hides later keys, and `clrhash` on a jump table crashes
//! GNU 31. The mutated `eq` table therefore has more than five keys, and no
//! case clears a table.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// A runner shared by the corpus cases: dispatch every input once, then 150
/// more times, and report whether every round answered like the first.
const RUNNER: &str = r#"(run (lambda (f inputs)
              (let ((first (mapcar f inputs)) (stable t))
                (dotimes (_ 150)
                  (unless (equal (mapcar f inputs) first) (setq stable nil)))
                (list stable first))))"#;

fn with_runner(bindings: &str, body: &str) -> String {
    format!("(let* ({RUNNER}\n       {bindings})\n  {body})")
}

#[test]
fn oracle_prop_switch_eq_table_of_symbols_and_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = with_runner(
        "(f (byte-compile (lambda (x) (pcase x ('a 1) ('b 2) (:k 3) ('nil 4) ('t 5) ('c 6) (_ 0)))))",
        r#"(funcall run f (list 'a 'b :k nil t 'c 'd :other 0 "a" (list 'a) 1.0 (intern "a") (make-symbol "a")))"#,
    );
    let expect = expect_test::expect![[r#""OK (t (1 2 3 4 5 6 0 0 0 0 0 0 1 0))""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_switch_fixnum_tables_dense_sparse_and_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = with_runner(
        "(dense (byte-compile (lambda (x) (pcase x (0 'a0) (1 'a1) (2 'a2) (3 'a3) (4 'a4) (5 'a5) (6 'a6) (7 'a7) (8 'a8) (9 'a9) (_ 'no)))))
       (sparse (byte-compile (lambda (x) (cond ((eql x -4) 'neg) ((eql x ?a) 'ch) ((eql x 1000) 'k) ((eql x 2305843009213693951) 'mpf) ((eql x -2305843009213693952) 'mnf) ((eql x 0) 'zero) (t 'no)))))
       (mixed (byte-compile (lambda (x) (pcase x (1 'one) (2 'two) ('a 'sym) (?\\C-c 'ctl) (_ 'no)))))
       (inputs (list -5 -4 0 1 2 3 5 8 9 10 97 ?b 1000 2305843009213693951 -2305843009213693952 1.0 'a nil (expt 2 70)))",
        "(list (funcall run dense inputs) (funcall run sparse inputs) (funcall run mixed inputs))",
    );
    let expect = expect_test::expect![[
        r#""OK ((t (no no a0 a1 a2 a3 a5 a8 a9 no no no no no no no no no no)) (t (no neg zero no no no no no no no ch no k mpf mnf no no no no)) (t (no no no one two ctl no no no no no no no no no no sym no no)))""#
    ]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_switch_float_keys_compare_bits_under_eql_and_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = with_runner(
        "(by-eql (byte-compile (lambda (x) (cond ((eql x 0.0) 'pz) ((eql x -0.0) 'nz) ((eql x 1.0) 'one) ((eql x 1) 'int) ((eql x 0.0e+NaN) 'nan) (t 'no)))))
       (by-equal (byte-compile (lambda (x) (cond ((equal x 0.0) 'pz) ((equal x -0.0) 'nz) ((equal x 1.5) 'x15) ((equal x 1) 'int) (t 'no)))))
       (inputs (list 0.0 -0.0 1.0 1 (/ 2.0 2) 0.0e+NaN (- 0.0e+NaN) 1.5 (* 1.5 1) 2.0 'a nil (expt 2 64)))",
        "(list (funcall run by-eql inputs) (funcall run by-equal inputs))",
    );
    let expect = expect_test::expect![[
        r#""OK ((t (pz nz one int one nan no no no no no no no)) (t (pz nz no int no no no x15 x15 no no no no)))""#
    ]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_switch_equal_table_of_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = with_runner(
        r#"(f (byte-compile (lambda (x) (pcase x ("alpha" 1) ("beta" 2) ("" 3) ("\377" 4) ("é" 5) ("abc" 6) (_ 0)))))"#,
        r#"(funcall run f (list "alpha" (copy-sequence "alpha") (propertize "beta" 'face 'bold) "" (make-string 0 ?x)
                       "\377" (string-to-multibyte "\377") (string ?é) (encode-coding-string (string ?é) 'utf-8)
                       (encode-coding-string (string ?é) 'latin-1) (string-to-multibyte "abc") (string-to-unibyte "abc")
                       "ALPHA" 'alpha nil 1))"#,
    );
    let expect = expect_test::expect![[r#""OK (t (1 1 2 3 3 4 0 5 0 0 6 6 0 0 0 0))""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_switch_equal_table_of_conses() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = with_runner(
        r#"(f (byte-compile (lambda (x) (pcase x (`(a b) 1) (`(a . b) 2) (`((a) b) 3) (`(a) 4) (`(1 "s" 2.5) 5) (_ 0)))))
       (circular (let ((c (list 'a 'b))) (setcdr (cdr c) c) c))
       (deep (let ((d nil)) (dotimes (_ 300) (setq d (list d))) d))
       (shared (let ((tail (list 'b))) (list (list 'a) (car tail))))
       (dag (let ((a (list 'a))) (list a a)))"#,
        r#"(funcall run f (list (list 'a 'b) (cons 'a 'b) (list (list 'a) 'b) (list 'a) (list 'a 'b 'c) (list 'b) 'a nil
                       (list 1 "s" 2.5) (list 1 (copy-sequence "s") 2.5) (list 1 "s" 2.50001) (list 1 "s" 2.5 nil)
                       circular deep shared dag (vector 'a 'b) (list 'a (list 'b))))"#,
    );
    let expect = expect_test::expect![[r#""OK (t (1 2 3 4 0 0 0 0 5 5 0 0 0 0 3 0 0 0))""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_switch_bignum_and_vector_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = with_runner(
        "(f (byte-compile (lambda (x) (pcase x (18446744073709551616 'bn) ('[1 2] 'vec) ('(1 [2]) 'lv) ('a 'sym) (_ 'no)))))",
        "(funcall run f (list (expt 2 64) (1+ (expt 2 64)) (vector 1 2) (vector 1 3) [1 2] (list 1 (vector 2)) 'a 18446744073709551616.0 nil))",
    );
    let expect = expect_test::expect![[r#""OK (t (bn no vec no vec lv sym no no))""#]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_switch_cond_over_member_and_memql_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = with_runner(
        r#"(by-member (byte-compile (lambda (x) (cond ((member x '("a" "b")) 1) ((member x '((1 2) (3))) 2) ((equal x 'c) 3) (t 0)))))
       (by-memql (byte-compile (lambda (x) (cond ((memql x '(1.5 2.5)) 1) ((eql x 3.5) 2) ((memq x '(q r s)) 3) ((eql x 7) 4) (t 0)))))
       (inputs (list "a" (copy-sequence "b") "c" (list 1 2) (list 3) (list 3 4) 'c 'q 's 1.5 (+ 1.0 1.5) 3.5 7 7.0 nil))"#,
        "(list (funcall run by-member inputs) (funcall run by-memql inputs))",
    );
    let expect = expect_test::expect![[
        r#""OK ((t (1 1 0 2 2 0 3 0 0 0 0 0 0 0 0)) (t (0 0 0 0 0 0 0 3 3 1 1 2 4 0 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

#[test]
fn oracle_prop_switch_tables_larger_than_a_linear_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = with_runner(
        r#"(syms (byte-compile `(lambda (x) (pcase x ,@(mapcar (lambda (i) (list (list 'quote (intern (format "p05s%d" i))) i)) (number-sequence 0 39)) (_ -1)))))
       (fixes (byte-compile `(lambda (x) (pcase x ,@(mapcar (lambda (i) (list (* i 7) i)) (number-sequence 0 49)) (_ -1)))))
       (strs (byte-compile `(lambda (x) (pcase x ,@(mapcar (lambda (i) (list (format "k%d" i) i)) (number-sequence 0 24)) (_ -1)))))
       (lists (byte-compile `(lambda (x) (pcase x ,@(mapcar (lambda (i) (list (list '\` (list 'k i)) i)) (number-sequence 0 19)) (_ -1)))))"#,
        r#"(list (funcall run syms (list 'p05s0 'p05s39 'p05s17 'p05s40 7 nil))
        (funcall run fixes (list 0 7 343 344 350 -7 'a))
        (funcall run strs (list "k0" "k24" (copy-sequence "k13") "k25" 'k1))
        (funcall run lists (list (list 'k 0) (list 'k 19) (list 'k 20) (list 'k) 'k)))"#,
    );
    let expect = expect_test::expect![[
        r#""OK ((t (0 39 17 -1 -1 -1)) (t (0 1 49 -1 -1 -1 -1)) (t (0 24 13 -1 -1)) (t (0 19 -1 -1 -1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(&form, expect);
}

/// `symbols-with-pos-enabled` makes a positioned symbol `eq` its bare symbol:
/// stripped at the top level for `eq`/`eql` tables and at every level for
/// `equal` ones. Without it, a positioned symbol matches no symbol key.
#[test]
fn oracle_prop_switch_under_symbols_with_pos_enabled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((f (byte-compile (lambda (x) (pcase x ('a 1) ('b 2) (_ 0)))))
       (g (byte-compile (lambda (x) (pcase x (`(a b) 1) (`(c . d) 2) (_ 0)))))
       (h (byte-compile (lambda (x) (cond ((eql x 'a) 1) ((eql x 'b) 2) ((eql x 5) 3) (t 0)))))
       (forms (read-positioning-symbols "(a b (a b) (c . d) (x a b))"))
       (pa (nth 0 forms)) (pb (nth 1 forms)) (pab (nth 2 forms)) (pcd (nth 3 forms))
       (probe (lambda ()
                (list (mapcar f (list pa pb 'a 'b pab))
                      (mapcar g (list pab pcd (list 'a 'b) (list 'a pb) pa))
                      (mapcar h (list pa pb 'a 5 pab))))))
  (list (symbol-with-pos-p pa)
        (funcall probe)
        (let ((symbols-with-pos-enabled t)) (funcall probe))
        (let ((symbols-with-pos-enabled t)) (dotimes (_ 400) (funcall probe)) (funcall probe))
        (funcall probe)))"#;
    let expect = expect_test::expect![[
        r#""OK (t ((0 0 1 2 0) (0 0 1 0 0) (0 0 1 3 0)) ((1 2 1 2 0) (1 2 1 1 0) (1 2 1 3 0)) ((1 2 1 2 0) (1 2 1 1 0) (1 2 1 3 0)) ((0 0 1 2 0) (0 0 1 0 0) (0 0 1 3 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// A jump table is an ordinary Lisp object: a program can pull it out of the
/// constants vector and `puthash`/`remhash` it, and the next dispatch obeys
/// the new contents. Dispatched a few times first, so a table that caches
/// anything about its keys has done so.
#[test]
fn oracle_prop_switch_follows_a_mutated_jump_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((f (byte-compile (lambda (x) (pcase x ('a 1) ('b 2) ('c 3) ('d 4) ('e 5) ('f 6) ('g 7) (_ 0)))))
       (g (byte-compile (lambda (x) (pcase x (`(a b) 1) (`(c) 2) ("s" 3) (_ 0)))))
       (ftbl nil) (gtbl nil))
  (mapc (lambda (c) (when (hash-table-p c) (setq ftbl c))) (aref f 2))
  (mapc (lambda (c) (when (hash-table-p c) (setq gtbl c))) (aref g 2))
  (list (mapcar f '(a b c d e f g h z))
        (mapcar g (list (list 'a 'b) (list 'c) "s" (list 'e) 'a))
        (progn (puthash 'z (gethash 'a ftbl) ftbl) (funcall f 'z))
        (progn (remhash 'a ftbl) (list (funcall f 'a) (funcall f 'z) (funcall f 'g)))
        (progn (puthash 'b (gethash 'c ftbl) ftbl) (list (funcall f 'b) (funcall f 'c)))
        (progn (puthash (list 'e) (gethash "s" gtbl) gtbl) (funcall g (list 'e)))
        (progn (remhash (list 'a 'b) gtbl) (funcall g (list 'a 'b)))
        (progn (puthash "s" (gethash (list 'c) gtbl) gtbl) (list (funcall g "s") (funcall g (list 'c))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 0 0) (1 2 3 0 0) 1 (0 1 7) (3 3) 3 0 (2 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The same after the function has run long enough to be compiled natively.
#[test]
fn oracle_prop_switch_follows_a_jump_table_mutated_after_warmup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((f (byte-compile (lambda (x) (pcase x (`(a b) 1) (`(c) 2) ('d 3) (_ 0)))))
       (tbl nil) (warm nil))
  (mapc (lambda (c) (when (hash-table-p c) (setq tbl c))) (aref f 2))
  (dotimes (i 1200)
    (setq warm (list (funcall f (list 'a 'b)) (funcall f (list 'c)) (funcall f 'd) (funcall f i))))
  (list warm
        (progn (puthash (list 'e) (gethash (list 'c) tbl) tbl) (funcall f (list 'e)))
        (progn (puthash 'd (gethash (list 'a 'b) tbl) tbl) (funcall f 'd))
        (progn (remhash (list 'a 'b) tbl) (funcall f (list 'a 'b)))
        (let (r)
          (dotimes (_ 1200)
            (setq r (list (funcall f (list 'a 'b)) (funcall f (list 'c)) (funcall f 'd) (funcall f (list 'e)))))
          r)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 0) 2 1 0 (0 2 1 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
