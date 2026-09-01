//! Complex combo batch 126 — `add-function`/`remove-function` with
//! `:before`/`:after`/`:around`/`:override`/`:filter-args`/`:filter-return`,
//! `advice--p` introspection, `advice-member-p` queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx126_add_function_to_place_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ quote\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defvar neo-cx126-fn-var (lambda (x) (push (list :primary x) calls) x))
  (add-function :before (var 'neo-cx126-fn-var)
                (lambda (x) (push (list :before x) calls)))
  (add-function :after (var 'neo-cx126-fn-var)
                (lambda (x) (push (list :after x) calls)))
  (let ((result (funcall neo-cx126-fn-var 42)))
    (list result (nreverse calls))))
"##,
        expect,
    );
}

#[test]
fn div_cx126_add_function_override_completely() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ quote\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defvar neo-cx126-orig (lambda (x) (push :orig calls) (* x 2))))
  (add-function :override (var 'neo-cx126-orig)
                (lambda (x) (push :override calls) (* x 100)))
  (let ((r (funcall neo-cx126-orig 5)))
    (list r (nreverse calls))))
"##,
        expect,
    );
}

#[test]
fn div_cx126_add_function_filter_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ quote\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defvar neo-cx126-sum-fn (lambda (&rest args)
                              (push (list :primary args) calls)
                              (apply #'+ args))))
  (add-function :filter-args (var 'neo-cx126-sum-fn)
                (lambda (args) (mapcar (lambda (x) (* x 10)) args)))
  (let ((r (funcall neo-cx126-sum-fn 1 2 3)))
    (list r (nreverse calls))))
"##,
        expect,
    );
}

#[test]
fn div_cx126_add_function_filter_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ quote\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defvar neo-cx126-base (lambda () (push :primary calls) 100)))
  (add-function :filter-return (var 'neo-cx126-base)
                (lambda (r) (push (list :filtered r) calls) (* r 2)))
  (let ((r (funcall neo-cx126-base)))
    (list r (nreverse calls))))
"##,
        expect,
    );
}

#[test]
fn div_cx126_add_function_around_with_call_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ quote\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defvar neo-cx126-target (lambda (x) (push (list :primary x) calls) (* x 3))))
  (add-function :around (var 'neo-cx126-target)
                (lambda (fn x)
                  (push (list :around-enter x) calls)
                  (let ((r (funcall fn x)))
                    (push (list :around-exit r) calls)
                    r)))
  (let ((r (funcall neo-cx126-target 7)))
    (list r (nreverse calls))))
"##,
        expect,
    );
}

#[test]
fn div_cx126_add_function_before_after_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ quote\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defvar neo-cx126-multi (lambda (x) (push (list :primary x) calls) x))
  (add-function :before (var 'neo-cx126-multi)
                (lambda (x) (push (list :before-1 x) calls)))
  (add-function :before (var 'neo-cx126-multi)
                (lambda (x) (push (list :before-2 x) calls)))
  (add-function :after (var 'neo-cx126-multi)
                (lambda (x) (push (list :after-1 x) calls)))
  (add-function :after (var 'neo-cx126-multi)
                (lambda (x) (push (list :after-2 x) calls)))
  (let ((r (funcall neo-cx126-multi 99)))
    (list r (nreverse calls))))
"##,
        expect,
    );
}

#[test]
fn div_cx126_advice_member_p_named_advices() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#[128 \"��\u{2}\\\"���\u{2}\\\"�\" [#[nil (:a) (t)] #[nil (:primary) (t)] :before ((name . my-adv-1)) apply] 4 advice] #[128 \"��\u{2}\\\"��\u{3}\\\"��\" [#[nil (:b) (t)] #[128 \"��\u{2}\\\"���\u{2}\\\"�\" [#[nil (:a) (t)] #[nil (:primary) (t)] :before ((name . my-adv-1)) apply] 4 advice] :after ((name . my-adv-2)) apply] 5 advice] nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defun neo-cx126-target () :primary)
(advice-add 'neo-cx126-target :before (lambda () :a) '((name . my-adv-1)))
(advice-add 'neo-cx126-target :after  (lambda () :b) '((name . my-adv-2)))
(list (advice-member-p 'my-adv-1 'neo-cx126-target)
      (advice-member-p 'my-adv-2 'neo-cx126-target)
      (advice-member-p 'missing 'neo-cx126-target))
"##,
        expect,
    );
}

#[test]
fn div_cx126_advice_mapc_iterate_advices() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((nil) (nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defun neo-cx126-target () :primary)
(advice-add 'neo-cx126-target :before (lambda () :a) '((name . adv-a)))
(advice-add 'neo-cx126-target :after  (lambda () :b) '((name . adv-b)))
(let (collected)
  (advice-mapc (lambda (adv props)
                 (push (list (plist-get props 'name)) collected))
               'neo-cx126-target)
  (nreverse collected))
"##,
        expect,
    );
}

#[test]
fn div_cx126_place_var_vs_function_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ function\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx126-fn (x) (push (list :primary x) calls) x)
  (add-function :before (function 'neo-cx126-fn)
                (lambda (x) (push (list :before x) calls)))
  (let ((r (neo-cx126-fn 42)))
    (list r (nreverse calls))))
"##,
        expect,
    );
}

#[test]
fn div_cx126_remove_function_by_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ quote\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defvar neo-cx126-rf (lambda (x) (push (list :primary x) calls) x))
  (let ((my-adv (lambda (x) (push (list :before x) calls))))
    (add-function :before (var 'neo-cx126-rf) my-adv)
    (let ((with-advice (length calls)))
      (funcall neo-cx126-rf 1)
      (let ((count-1 (length calls)))
        (remove-function (var 'neo-cx126-rf) my-adv)
        (funcall neo-cx126-rf 2)
        (let ((count-2 (length calls)))
          (list with-advice count-1 count-2))))))
"##,
        expect,
    );
}

#[test]
fn div_cx126_add_function_to_named_var_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ quote\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defvar neo-cx126-mega-var (lambda (x) (push (list :primary x) calls) (* x 2)))
  (add-function :before (var 'neo-cx126-mega-var)
                (lambda (x) (push (list :before x) calls))
                '((name . mega-adv-1)))
  (add-function :after (var 'neo-cx126-mega-var)
                (lambda (x) (push (list :after x) calls))
                '((name . mega-adv-2)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "add-function mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((r (funcall neo-cx126-mega-var 21)))
        (let ((state (list r (nreverse calls)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (remove-function (var 'neo-cx126-mega-var) 'mega-adv-1)
          (remove-function (var 'neo-cx126-mega-var) 'mega-adv-2)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
