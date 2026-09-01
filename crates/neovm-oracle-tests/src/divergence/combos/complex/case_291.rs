//! Complex combo batch 291 — `buffer-local` variables deep:
//! `default-boundp`/`default-value`/`setq-default`,
//! `local-variable-p`/`buffer-local-value`/`kill-local-variable`,
//! `make-variable-buffer-local` vs `make-local-variable`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx291_make_variable_buffer_local_permanent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:a :b :global t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx291-perm :global)
(make-variable-buffer-local 'neo-cx291-perm)
(let ((buf-a (get-buffer-create " *neo-cx291-a*"))
      (buf-b (get-buffer-create " *neo-cx291-b*")))
  (with-current-buffer buf-a (setq neo-cx291-perm :a))
  (with-current-buffer buf-b (setq neo-cx291-perm :b))
  (list (buffer-local-value 'neo-cx291-perm buf-a)
        (buffer-local-value 'neo-cx291-perm buf-b)
        (default-value 'neo-cx291-perm)
        (local-variable-p 'neo-cx291-perm buf-a)
        (local-variable-p 'neo-cx291-perm buf-b)))
"##,
        expect,
    )
}

#[test]
fn div_cx291_kill_local_variable_restores_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:local :global nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx291-kill :global)
(let ((buf (get-buffer-create " *neo-cx291-kill*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx291-kill) :local))
  (let ((local-before (buffer-local-value 'neo-cx291-kill buf)))
    (with-current-buffer buf (kill-local-variable 'neo-cx291-kill))
    (let ((after-kill (buffer-local-value 'neo-cx291-kill buf)))
      (kill-buffer buf)
      (list local-before after-kill (local-variable-p 'neo-cx291-kill buf)))))
"##,
        expect,
    )
}

#[test]
fn div_cx291_kill_all_local_variables_except_permanent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable neo-cx291-normal)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx291-all*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx291-normal) :normal)
    (set (make-local-variable 'neo-cx291-perm) :perm)
    (put 'neo-cx291-perm 'permanent-local t))
  (let ((before-normal (buffer-local-value 'neo-cx291-normal buf))
        (before-perm (buffer-local-value 'neo-cx291-perm buf)))
    (with-current-buffer buf (kill-all-local-variables))
    (let ((after-normal (buffer-local-value 'neo-cx291-normal buf))
          (after-perm (buffer-local-value 'neo-cx291-perm buf)))
      (kill-buffer buf)
      (list before-normal before-perm after-normal after-perm))))
"##,
        expect,
    )
}

#[test]
fn div_cx291_default_toplevel_value_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:local :default :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (setq-default neo-cx291-tl :default)
      (let ((buf (get-buffer-create " *neo-cx291-tl*")))
        (with-current-buffer buf
          (set (make-local-variable 'neo-cx291-tl) :local))
        (let ((local (buffer-local-value 'neo-cx291-tl buf))
              (tl (default-toplevel-value 'neo-cx291-tl))
              (dv (default-value 'neo-cx291-tl)))
          (kill-buffer buf)
          (list local tl dv))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx291_local_variable_if_set_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx291-ifset*")))
  (with-current-buffer buf
    (make-variable-buffer-local 'neo-cx291-ifset)
    (setq neo-cx291-ifset :local))
  (list (local-variable-if-set-p 'neo-cx291-ifset buf)
        (local-variable-p 'neo-cx291-ifset buf)))
"##,
        expect,
    )
}

#[test]
fn div_cx291_setq_default_does_not_overwrite_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:a 99 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx291-override 1)
(let ((buf-a (get-buffer-create " *neo-cx291-ov-a*"))
      (buf-b (get-buffer-create " *neo-cx291-ov-b*")))
  (with-current-buffer buf-a (set (make-local-variable 'neo-cx291-override) :a))
  (setq-default neo-cx291-override 99)
  (let ((a-val (buffer-local-value 'neo-cx291-override buf-a))
        (b-val (buffer-local-value 'neo-cx291-override buf-b))
        (default (default-value 'neo-cx291-override)))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (list a-val b-val default)))
"##,
        expect,
    )
}

#[test]
fn div_cx291_buffer_local_let_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :buffer-local""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx291-shadow :global)
(let ((buf (get-buffer-create " *neo-cx291-shadow*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx291-shadow) :buffer-local))
  (with-current-buffer buf
    (let ((neo-cx291-shadow :let-shadowed))
      (list neo-cx291-shadow
            (local-variable-p 'neo-cx291-shadow)
            (default-value 'neo-cx291-shadow))))
  (let ((after-let (buffer-local-value 'neo-cx291-shadow buf)))
    (kill-buffer buf)
    after-let))
"##,
        expect,
    )
}

#[test]
fn div_cx291_indirect_buffer_inherits_locals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable neo-cx291-ind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((base (get-buffer-create " *neo-cx291-ind-base*")))
  (with-current-buffer base
    (set (make-local-variable 'neo-cx291-ind) :base))
  (let ((ind (make-indirect-buffer base " *neo-cx291-ind*")))
    (let ((base-val (buffer-local-value 'neo-cx291-ind base))
          (ind-val (with-current-buffer ind neo-cx291-ind)))
      (with-current-buffer ind
        (set (make-local-variable 'neo-cx291-ind) :indirect))
      (let ((after-set-base (buffer-local-value 'neo-cx291-ind base))
            (after-set-ind (with-current-buffer ind neo-cx291-ind)))
        (kill-buffer ind)
        (kill-buffer base)
        (list base-val ind-val after-set-base after-set-ind)))))
"##,
        expect,
    )
}

#[test]
fn div_cx291_default_boundp_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil :val t :val)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (default-boundp 'neo-cx291-never-set)
      (setq-default neo-cx291-now-set :val)
      (default-boundp 'neo-cx291-now-set)
      (default-value 'neo-cx291-now-set))
"##,
        expect,
    )
}

#[test]
fn div_cx291_buflocal_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx291-mega :default)
(let ((buf (get-buffer-create " *neo-cx291-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (set (make-local-variable 'neo-cx291-mega) :buffer-local)
    (insert "Buffer-local mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list neo-cx291-mega
                         (default-value 'neo-cx291-mega)
                         (local-variable-p 'neo-cx291-mega)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (kill-buffer buf)
        (list state (default-value 'neo-cx291-mega))))))
"##,
        expect,
    )
}
