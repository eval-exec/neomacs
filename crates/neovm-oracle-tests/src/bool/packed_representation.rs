//! Oracle parity for GNU `PVEC_BOOL_VECTOR` semantics (P3.2 L0): the type
//! predicates, every `bool-vector-*` operation across word boundaries
//! (lengths 0, 1, 63, 64, 65, 1000) with GNU's destination and
//! `wrong-length-argument` rules, the `#&N"..."` print/read round trip,
//! the sequence functions, and `memory-use-counts` vector cells.
//!
//! GNU src/data.c:3709-4016, src/alloc.c:2126-2200, src/print.c:2144,
//! src/fns.c (`internal_equal`, `Fcopy_sequence`, `concat_to_vector`).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// A Lisp helper defining `bvo-bits`: a bool-vector of N bits from a
/// deterministic pattern seeded with SEED (no `random`, so both engines
/// build the same vectors).
const BITS_HELPER: &str = r##"(fset 'bvo-bits
  (lambda (n seed)
    (let ((v (make-bool-vector n nil)) (x seed) (i 0))
      (while (< i n)
        (setq x (% (+ (* x 1103515245) 12345) 2147483648))
        (when (> (% (/ x 65536) 7) 2) (aset v i t))
        (setq i (1+ i)))
      v)))"##;

#[test]
fn oracle_bool_vector_type_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t bool-vector bool-vector nil t nil 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((b (make-bool-vector 5 t)))
  (list (bool-vector-p b) (vectorp b) (arrayp b) (sequencep b)
        (type-of b) (cl-type-of b) (vector-or-char-table-p b)
        (atom b) (listp b) (length b)))"##,
        expect,
    );
}

#[test]
fn oracle_bool_vector_set_operations_across_word_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 0 0 0 0 0 0 t t t 0 0 0 0) (1 1 1 1 0 0 0 t t t 1 1 0 0) (63 37 54 22 32 15 26 t nil t 3 0 2 0) (64 37 55 22 33 15 27 t nil t 3 0 1 0) (65 37 55 22 33 15 28 t nil t 3 0 1 0) (1000 577 818 357 461 220 423 t nil t 3 4 0 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        &format!(
            r##"(progn {BITS_HELPER}
  (mapcar
   (lambda (n)
     (let ((a (bvo-bits n 1)) (b (bvo-bits n 2)))
       (list n
             (bool-vector-count-population a)
             (bool-vector-count-population (bool-vector-union a b))
             (bool-vector-count-population (bool-vector-intersection a b))
             (bool-vector-count-population (bool-vector-exclusive-or a b))
             (bool-vector-count-population (bool-vector-set-difference a b))
             (bool-vector-count-population (bool-vector-not a))
             (bool-vector-subsetp (bool-vector-intersection a b) a)
             (bool-vector-subsetp a (bool-vector-intersection a b))
             (equal (bool-vector-not (bool-vector-not a)) a)
             (if (> n 0) (bool-vector-count-consecutive a (aref a 0) 0) 0)
             (bool-vector-count-consecutive a t (/ n 2))
             (bool-vector-count-consecutive a nil (/ n 2))
             (bool-vector-count-consecutive a t n))))
   '(0 1 63 64 65 1000)))"##
        ),
        expect,
    );
}

#[test]
fn oracle_bool_vector_destination_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 nil t nil t t nil nil) (1 t nil nil t t nil nil) (64 t nil nil t t nil t) (65 t nil nil t t nil t) (1000 t nil nil t t nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        &format!(
            r##"(progn {BITS_HELPER}
  (mapcar
   (lambda (n)
     (let* ((a (bvo-bits n 3)) (b (bvo-bits n 4)) (d (make-bool-vector n nil))
            (r1 (bool-vector-union a b d))
            (r2 (bool-vector-union a b d))
            (e (make-bool-vector n nil))
            (r3 (bool-vector-not a e)))
       (list n (eq r1 d) (and (null r1) (= 0 (bool-vector-count-population d)))
             r2 (eq r3 e) (equal d (bool-vector-union a b))
             (bool-vector-intersection a a a)
             (eq (bool-vector-exclusive-or a a a) a))))
   '(0 1 64 65 1000)))"##
        ),
        expect,
    );
}

#[test]
fn oracle_bool_vector_wrong_length_argument_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((wrong-length-argument 3 4) (wrong-length-argument 3 4 5) (wrong-length-argument 3 3 5) (wrong-length-argument 3 4 4) (wrong-length-argument 3 4) (wrong-type-argument bool-vector-p [1 2 3]) (args-out-of-range #&3\"\u{7}\" 4) (wrong-type-argument wholenump -1) (wrong-type-argument wholenump -1) (args-out-of-range #&3\"\u{7}\" 3) (args-out-of-range #&3\"\u{7}\" -1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (make-bool-vector 3 t)) (b (make-bool-vector 4 t))
      (c (make-bool-vector 5 nil)))
  (list (condition-case e (bool-vector-union a b) (error e))
        (condition-case e (bool-vector-union a b c) (error e))
        (condition-case e (bool-vector-intersection a (make-bool-vector 3 nil) c) (error e))
        (condition-case e (bool-vector-subsetp a b) (error e))
        (condition-case e (bool-vector-not a b) (error e))
        (condition-case e (bool-vector-union a [1 2 3]) (error e))
        (condition-case e (bool-vector-count-consecutive a t 4) (error e))
        (condition-case e (bool-vector-count-consecutive a t -1) (error e))
        (condition-case e (make-bool-vector -1 nil) (error e))
        (condition-case e (aref a 3) (error e))
        (condition-case e (aset a -1 t) (error e))))"##,
        expect,
    );
}

#[test]
fn oracle_bool_vector_print_read_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (((0 \"#&0\\\"\\\"\" t t) (7 \"#&7\\\"x\\\"\" t t) (14 \"#&14\\\"\\\\346\u{3}\\\"\" t t) (21 \"#&21\\\"_\\\\333\u{1f}\\\"\" t t) (28 \"#&28\\\"\\\\343\\\\345\\\\262\u{4}\\\"\" t t) (35 \"#&35\\\"\\\\373?u\\\\375\u{2}\\\"\" t t) (42 \"#&42\\\"\u{e}{J\\\\341\u{b}\\0\\\"\" t t) (49 \"#&49\\\"4\\\\307\\\\307\\\\275\\\\237\\\\254\\0\\\"\" t t) (56 \"#&56\\\"\\\\251\\\\216\\\\210\\\\335\\\\326\\\\356\\\\263\\\"\" t t) (63 \"#&63\\\"\\\\231\\\\222\\\\315\\\\231\\\\340\\\\262\\\\\\\"k\\\"\" t t) (70 \"#&70\\\"\\\\365fr\\\\335-\\\\363\\\\233\\\\355\u{1f}\\\"\" t t)) #&3\"\u{7}\" (t t t t t t t t t t) \"#&40\\\"\\\\377\\\\377 ...\\\"\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        &format!(
            r##"(progn {BITS_HELPER}
  (let ((out nil) (n 0))
    (while (<= n 70)
      (let* ((b (bvo-bits n (+ n 11))) (s (prin1-to-string b))
             (back (car (read-from-string s))))
        (push (list n s (equal back b) (bool-vector-p back)) out))
      (setq n (+ n 7)))
    (list (nreverse out)
          (car (read-from-string "#&3\"\\377\""))
          (append (car (read-from-string "#&10\"\\377\\377\"")) nil)
          (let ((print-length 2)) (prin1-to-string (make-bool-vector 40 t))))))"##
        ),
        expect,
    );
}

#[test]
fn oracle_bool_vector_sequence_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t nil [t nil nil t t] [x t nil nil t t nil] (t nil nil t t) (t nil nil t t z) (nil t t nil nil) 3 #&5\"\u{13}\" (#&5\"\u{13}\" #&5\"\u{13}\") (#&5\"\u{1f}\" #&5\"\u{1f}\") t t t nil (wrong-type-argument sequencep #&5\"\u{19}\") (wrong-type-argument list-or-vector-p #&5\"\u{19}\") t t nil t (big small nil) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((b (bool-vector t nil nil t t)))
  (list (equal b (copy-sequence b)) (eq b (copy-sequence b))
        (vconcat b) (vconcat [x] b (bool-vector nil))
        (append b nil) (append b '(z))
        (mapcar #'not b) (let ((n 0)) (mapc (lambda (x) (when x (setq n (1+ n)))) b) n)
        (reverse b) (let ((c (copy-sequence b))) (list (nreverse c) c))
        (let ((c (copy-sequence b))) (list (fillarray c nil) (fillarray c 'x)))
        (elt b 3) (length< b 6) (length= b 5) (length> b 5)
        (condition-case e (concat b) (error e))
        (condition-case e (sort (copy-sequence b) #'<) (error e))
        (value< (bool-vector nil t) (bool-vector t nil))
        (value< (bool-vector t) (bool-vector t nil))
        (equal (bool-vector t nil) (bool-vector t nil nil))
        (equal (make-bool-vector 130 t) (make-bool-vector 130 t))
        (let ((h (make-hash-table :test 'equal)))
          (puthash (make-bool-vector 200 t) 'big h)
          (puthash (bool-vector t nil) 'small h)
          (list (gethash (make-bool-vector 200 t) h) (gethash (bool-vector t nil) h)
                (gethash (bool-vector nil t) h)))
        (= (sxhash-equal (make-bool-vector 70 t)) (sxhash-equal (make-bool-vector 70 t)))))"##,
        expect,
    );
}

#[test]
fn oracle_bool_vector_memory_use_counts_vector_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 2 3 17)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(mapcar (lambda (n)
          (let* ((before (nth 2 (memory-use-counts)))
                 (b (make-bool-vector n t)))
            (and b (- (nth 2 (memory-use-counts)) before))))
        '(0 1 64 65 1000))"##,
        expect,
    );
}

/// A vector whose slot 0 happens to be `--bool-vector--` or
/// `--char-table--` is a plain vector, as in GNU (no in-band tags).
#[test]
fn oracle_tag_symbols_in_slot_zero_make_a_plain_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil t 5 --bool-vector-- 1 [--bool-vector-- 3 1 0 1] (--bool-vector-- 3 1 0 1) nil t 5 nil (args-out-of-range [x nil nil nil 0] 5) [x nil nil nil 0] \"[--bool-vector-- 3 1 0 1]\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((fake (vector (intern "--bool-vector--") 3 1 0 1))
      (fake-ct (vector (intern "--char-table--") nil nil nil 0)))
  (list (bool-vector-p fake) (vectorp fake) (length fake) (aref fake 0)
        (aref fake 2) (vconcat fake) (append fake nil)
        (char-table-p fake-ct) (vectorp fake-ct) (length fake-ct) (aref fake-ct 3)
        (condition-case e (aref fake-ct 5) (error e))
        (progn (aset fake-ct 0 'x) fake-ct)
        (prin1-to-string fake) (sequencep fake-ct)))"##,
        expect,
    );
}
