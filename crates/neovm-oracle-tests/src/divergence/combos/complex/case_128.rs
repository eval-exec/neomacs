//! Complex combo batch 128 — `setq-default` / `setq-local` / `let` scope
//! interactions, dynamic vs lexical binding scoping corner cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx128_setq_default_does_not_affect_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:local-a :local-b :new-default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx128-shared :default)
(let ((buf-a (get-buffer-create " *neo-cx128-a*"))
      (buf-b (get-buffer-create " *neo-cx128-b*")))
  (with-current-buffer buf-a
    (set (make-local-variable 'neo-cx128-shared) :local-a))
  (with-current-buffer buf-b
    (set (make-local-variable 'neo-cx128-shared) :local-b))
  (setq-default neo-cx128-shared :new-default)
  (let ((a-val (buffer-local-value 'neo-cx128-shared buf-a))
        (b-val (buffer-local-value 'neo-cx128-shared buf-b))
        (def (default-value 'neo-cx128-shared)))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (list a-val b-val def)))
"##,
        expect,
    );
}

#[test]
fn div_cx128_setq_local_creates_buffer_local_in_current_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:local-a :global :global)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx128-loc :global)
(let ((buf-a (get-buffer-create " *neo-cx128-loc-a*"))
      (buf-b (get-buffer-create " *neo-cx128-loc-b*")))
  (with-current-buffer buf-a
    (setq-local neo-cx128-loc :local-a))
  (let ((a-val (buffer-local-value 'neo-cx128-loc buf-a))
        (b-val (buffer-local-value 'neo-cx128-loc buf-b))
        (default (default-value 'neo-cx128-loc))
        (a-local-p (buffer-local-value 'neo-cx128-loc buf-a)))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (list a-val b-val default)))
"##,
        expect,
    );
}

#[test]
fn div_cx128_let_shadowing_does_not_change_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :buffer-local""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx128-shad :default)
(let ((buf (get-buffer-create " *neo-cx128-shad*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx128-shad) :buffer-local))
  (with-current-buffer buf
    (let ((neo-cx128-shad :let-shadowed))
      (list neo-cx128-shad
            (local-variable-p 'neo-cx128-shad))))
  (let ((after-let (buffer-local-value 'neo-cx128-shad buf)))
    (kill-buffer buf)
    after-let))
"##,
        expect,
    );
}

#[test]
fn div_cx128_dynamic_binding_let_visible_in_called_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :let-bound""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defvar neo-cx128-dyn :unset)
(defun neo-cx128-read-dyn () neo-cx128-dyn)
(let ((neo-cx128-dyn :let-bound))
  (neo-cx128-read-dyn))
"##,
        expect,
    );
}

#[test]
fn div_cx128_lexical_binding_let_invisible_in_called_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:let-bound :let-bound)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (defvar neo-cx128-lex :global)
  (defun neo-cx128-read-lex () neo-cx128-lex)
  (let ((neo-cx128-lex :let-bound))
    (list (neo-cx128-read-lex)
          neo-cx128-lex)))
"##,
        expect,
    );
}

#[test]
fn div_cx128_let_with_buffer_local_does_not_affect_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:default :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx128-bl :default)
(let ((buf (get-buffer-create " *neo-cx128-bl*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx128-bl) :buffer-local))
  (let ((default-before (default-value 'neo-cx128-bl)))
    (with-current-buffer buf
      (let ((neo-cx128-bl :let-in-buf))
        (list default-before
              (default-value 'neo-cx128-bl)
              neo-cx128-bl)))
    (let ((default-after (default-value 'neo-cx128-bl)))
      (kill-buffer buf)
      (list default-before default-after))))
"##,
        expect,
    );
}

#[test]
fn div_cx128_setq_default_in_let_does_not_persist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:outer :in-let :outer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx128-persist :outer)
(let ((saved neo-cx128-persist))
  (setq-default neo-cx128-persist :in-let)
  (let ((in-let (default-value 'neo-cx128-persist)))
    (setq-default neo-cx128-persist saved)
    (list saved in-let (default-value 'neo-cx128-persist))))
"##,
        expect,
    );
}

#[test]
fn div_cx128_buffer_local_persistence_after_kill_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:in-buf :survivor :survivor)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx128-kill :survivor)
(let ((buf (get-buffer-create " *neo-cx128-kill*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx128-kill) :in-buf))
  (let ((local-p (buffer-local-value 'neo-cx128-kill buf)))
    (kill-buffer buf)
    (list local-p
          (default-value 'neo-cx128-kill)
          (condition-case e (buffer-local-value 'neo-cx128-kill buf)
            (error (car e))))))
"##,
        expect,
    );
}

#[test]
fn div_cx128_with_temp_buffer_default_value_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:inherited :inherited nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx128-temp :inherited)
(with-temp-buffer
  (list neo-cx128-temp
        (default-value 'neo-cx128-temp)
        (local-variable-p 'neo-cx128-temp)))
"##,
        expect,
    );
}

#[test]
fn div_cx128_let_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :default""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx128-mega :default)
(let ((buf (get-buffer-create " *neo-cx128-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (set (make-local-variable 'neo-cx128-mega) :buffer-local)
    (insert "Setq/let mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((in-let (let ((neo-cx128-mega :let-shadow))
                      (list neo-cx128-mega
                            (default-value 'neo-cx128-mega)))))
        (let ((state (list in-let
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (kill-buffer buf)
          (list state (default-value 'neo-cx128-mega))))))
"##,
        expect,
    );
}
