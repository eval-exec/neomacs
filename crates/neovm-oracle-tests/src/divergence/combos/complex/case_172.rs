//! Complex combo batch 172 — `pcase` advanced: `pred`, `app`, `quote`,
//! `let`, `rx`, `map`, `and`, `or`, `not`, `guard`, with combined
//! patterns and macro patterns.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx172_pcase_pred_with_various_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:int :str :cons :vec :nil :sym)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ((pred integerp) :int)
            ((pred stringp) :str)
            ((pred consp) :cons)
            ((pred vectorp) :vec)
            ((pred null) :nil)
            ((pred symbolp) :sym)))
        '(42 "hello" (1 2) [1 2] nil alpha))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_app_with_transformation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:length 5) (:length 3) (:length 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ((app length len) (list :length len))
            ((app car-safe 'first) :first-car)))
        '("hello" (1 2 3) [a b c d]))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_quote_with_literal_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:alpha-symbol :beta-symbol :gamma-symbol :string-literal :number-literal :other)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ('alpha :alpha-symbol)
            ('beta :beta-symbol)
            ((quote gamma) :gamma-symbol)
            ("str" :string-literal)
            (42 :number-literal)
            (_ :other)))
        '(alpha beta gamma "str" 42 :unknown))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_let_pattern_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:a 1 :b 2 :c 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(pcase-let ((`(,a ,b ,c) (list 1 2 3)))
  (list :a a :b b :c c))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_let_star_multiple_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(pcase-let* ((`(,a ,b) (list 1 2))
             (`(,c ,d) (list 3 4)))
  (list a b c d))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_or_with_multiple_alternatives() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:greek :color :small-int :other)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ((or 'alpha 'beta 'gamma) :greek)
            ((or 'red 'green 'blue) :color)
            ((or 1 2 3) :small-int)
            (_ :other)))
        '(alpha red 1 delta))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_and_with_combined_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ((and (pred integerp) (pred (> _ 100))) :big-int)
            ((and (pred stringp) (app length len) (pred (> len 3))) :long-str)
            ((and (pred consp) (app car-safe 'first)) :cons-with-first)
            (_ :other)))
        '(200 "hello" (first . rest) 50 "ab"))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_not_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Unknown not pattern: (not (pred integerp))\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ((not (pred integerp)) :not-int)
            (_ :int)))
        '(42 "hello" (1 2) [a b] nil))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_guard_clause() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:other :other :other :other :other)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ((and n (guard (and (integerp n) (< 0 n 100))) :valid-range))
            (_ :other)))
        '(-5 0 50 100 200))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_map_pattern_with_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ht (make-hash-table :test 'equal)))
      (puthash :name "alpha" ht)
      (puthash :age 30 ht)
      (pcase ht
        ((map (:name name) (:age age))
         (list :parsed name age))
        (_ :no-match)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_with_destructuring_complex_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:start-pattern 1 2 3) (:mid-pattern 1 (2 3) 4) (:end-pattern (one two three)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            (`(start (,a ,b) ,c) (list :start-pattern a b c))
            (`(mid (,a . ,rest) ,c) (list :mid-pattern a rest c))
            (`(end . ,rest) (list :end-pattern rest))
            (_ :other)))
        '((start (1 2) 3)
          (mid (1 2 3) 4)
          (end one two three)))
"##,
        expect,
    );
}

#[test]
fn div_cx172_pcase_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '("alpha" 42 (1 2 3) [a b c])))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Pcase mega test: %S" data))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((pcase-result
             (mapcar (lambda (v)
                       (pcase v
                         ((pred stringp) :str)
                         ((pred integerp) :int)
                         ((pred consp) :cons)
                         ((pred vectorp) :vec)))
                     data)))
        (let ((state (list pcase-result
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
