//! Complex combo batch 373 — `advice`/`add-function`/`remove-function`
//! ultimate: before/after/around/override/filter-args/filter-return on
//! defun, subr builtin, and var place; advice-member-p/advice-mapc.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx373_advice_before_after_around_combined_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (42 ((:around-enter 21) (:before 21) (:primary 21) (:after 21) (:around-exit 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx373-target (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx373-target :before
              (lambda (x) (push (list :before x) calls)) '((name . adv-b)))
  (advice-add 'neo-cx373-target :after
              (lambda (x) (push (list :after x) calls)) '((name . adv-a)))
  (advice-add 'neo-cx373-target :around
              (lambda (fn x) (push (list :around-enter x) calls)
                (let ((r (funcall fn x)))
                  (push (list :around-exit r) calls) r)) '((name . adv-ar)))
  (prog1 (list (neo-cx373-target 21) (nreverse calls))
    (advice-remove 'neo-cx373-target 'adv-b)
    (advice-remove 'neo-cx373-target 'adv-a)
    (advice-remove 'neo-cx373-target 'adv-ar)))
"##,
        expect,
    )
}

#[test]
fn div_cx373_advice_override_completely_replaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (500 (:override) 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx373-orig (x) (push :orig calls) (* x 2))
  (advice-add 'neo-cx373-orig :override
              (lambda (x) (push :override calls) (* x 100)) '((name . adv-ov)))
  (let ((r (neo-cx373-orig 5)))
    (advice-remove 'neo-cx373-orig 'adv-ov)
    (list r (nreverse calls) (neo-cx373-orig 5))))
"##,
        expect,
    )
}

#[test]
fn div_cx373_advice_filter_args_and_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (60 200 ((:primary (10 20 30)) :primary (:filtered 100)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx373-fa (&rest args) (push (list :primary args) calls) (apply #'+ args))
  (advice-add 'neo-cx373-fa :filter-args
              (lambda (args) (mapcar (lambda (x) (* x 10)) args)) '((name . adv-fa)))
  (defun neo-cx373-fr () (push :primary calls) 100)
  (advice-add 'neo-cx373-fr :filter-return
              (lambda (r) (push (list :filtered r) calls) (* r 2)) '((name . adv-fr)))
  (let ((r-fa (neo-cx373-fa 1 2 3)))
    (advice-remove 'neo-cx373-fa 'adv-fa)
    (let ((r-fr (neo-cx373-fr)))
      (advice-remove 'neo-cx373-fr 'adv-fr)
      (list r-fa r-fr (nreverse calls)))))
"##,
        expect,
    )
}

#[test]
fn div_cx373_advice_on_subr_builtin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (calls)
      (advice-add 'car :around
                  (lambda (fn x) (push :around calls) (funcall fn x))
                  '((name . subr-adv)))
      (let ((r (car '(1 2 3))))
        (advice-remove 'car 'subr-adv)
        (list r (length calls) (car '(4 5 6)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx373_advice_member_p_and_mapc_iterate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function symbol<)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defun neo-cx373-mp () :ok)
(advice-add 'neo-cx373-mp :before (lambda () :a) '((name . my-advice)))
(advice-add 'neo-cx373-mp :after (lambda () :b) '((name . other-advice)))
(let (names)
  (advice-mapc (lambda (adv props) (push (plist-get props 'name) names)) 'neo-cx373-mp)
  (let ((sorted-names (sort names #'symbol<)))
    (advice-remove 'neo-cx373-mp 'my-advice)
    (advice-remove 'neo-cx373-mp 'other-advice)
    sorted-names))
"##,
        expect,
    )
}

#[test]
fn div_cx373_add_function_to_var_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ quote\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defvar neo-cx373-fn-var (lambda (x) (push (list :primary x) calls) x))
  (add-function :before (var 'neo-cx373-fn-var)
                (lambda (x) (push (list :before x) calls)))
  (add-function :after (var 'neo-cx373-fn-var)
                (lambda (x) (push (list :after x) calls)))
  (let ((result (funcall neo-cx373-fn-var 42)))
    (list result (nreverse calls))))
"##,
        expect,
    )
}

#[test]
fn div_cx373_define_advice_legacy_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 50""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defun neo-cx373-da (x) x)
      (define-advice neo-cx373-da (:filter-args (args))
        (mapcar (lambda (a) (* a 10)) args))
      (let ((r (neo-cx373-da 5)))
        (advice-remove 'neo-cx373-da (intern "neo-cx373-da@:filter-args"))
        r))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx373_advice_remove_restores_original() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 10 ((:before 5) (:primary 5) (:primary 5)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx373-rm (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx373-rm :before
              (lambda (x) (push (list :before x) calls)) '((name . adv-rm)))
  (let ((with-advice (neo-cx373-rm 5)))
    (advice-remove 'neo-cx373-rm 'adv-rm)
    (let ((after-remove (neo-cx373-rm 5)))
      (list with-advice after-remove (nreverse calls)))))
"##,
        expect,
    )
}

#[test]
fn div_cx373_advice_multiple_named_remove_selectively() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:r (:b2 :primary :a1) :r)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx373-sel () (push :primary calls) :r)
  (advice-add 'neo-cx373-sel :before (lambda () (push :b1 calls)) '((name . b1)))
  (advice-add 'neo-cx373-sel :before (lambda () (push :b2 calls)) '((name . b2)))
  (advice-add 'neo-cx373-sel :after (lambda () (push :a1 calls)) '((name . a1)))
  (let ((r1 (neo-cx373-sel)))
    (advice-remove 'neo-cx373-sel 'b1)
    (setq calls nil)
    (let ((r2 (neo-cx373-sel)))
      (advice-remove 'neo-cx373-sel 'b2)
      (advice-remove 'neo-cx373-sel 'a1)
      (list r1 (nreverse calls) r2))))
"##,
        expect,
    )
}

#[test]
fn div_cx373_advice_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx373-mega (x) (push (list :primary x) calls) (* x 2))
  (advice-add 'neo-cx373-mega :before
              (lambda (x) (push (list :before x) calls)) '((name . mega-adv-1)))
  (advice-add 'neo-cx373-mega :after
              (lambda (x) (push (list :after x) calls)) '((name . mega-adv-2)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Advice ultimate mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((r (neo-cx373-mega 21)))
        (let ((state (list r (nreverse calls)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen()
          (advice-remove 'neo-cx373-mega 'mega-adv-1)
          (advice-remove 'neo-cx373-mega 'mega-adv-2)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    )
}
