//! Complex combo batch 88 — buffer-local variables & frame parameters &
//! display engine: buffer-local-value indirection, default-toplevel-value,
//! frame parameters persistence, display-pixel-* queries, and `set-buffer`
//! interactions across `with-current-buffer`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx88_default_toplevel_value_with_buffer_local_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:local :default :default :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf (get-buffer-create " *neo-cx88-tl*")))
      (setq-default neo-cx88-shared :default)
      (with-current-buffer buf
        (set (make-local-variable 'neo-cx88-shared) :local))
      (let ((local (buffer-local-value 'neo-cx88-shared buf))
            (tl (default-toplevel-value 'neo-cx88-shared))
            (dv (default-value 'neo-cx88-shared)))
        (kill-buffer buf)
        (list local tl dv
              (default-toplevel-value 'neo-cx88-shared))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx88_kill_local_variable_in_indirect_buffer_does_not_affect_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((base (get-buffer-create " *neo-cx88-base*")))
  (with-current-buffer base
    (set (make-local-variable 'neo-cx88-ind) :base)
    :base)
  (let ((ind (make-indirect-buffer base " *neo-cx88-ind*")))
    (let ((base-val-before (buffer-local-value 'neo-cx88-ind base))
          (ind-val-before (buffer-local-value 'neo-cx88-ind ind)))
      (with-current-buffer ind
        (setq neo-cx88-ind :indirect))
      (let ((base-val-after (buffer-local-value 'neo-cx88-ind base))
            (ind-val-after (buffer-local-value 'neo-cx88-ind ind)))
        (kill-buffer ind)
        (kill-buffer base)
        (list base-val-before ind-val-before
              base-val-after ind-val-after))))
"##,
        expect,
    );
}

#[test]
fn div_cx88_buffer_local_value_with_non_local_returns_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 42 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx88-glob 42)
(let ((buf (get-buffer-create " *neo-cx88-glob*")))
  (let ((val (buffer-local-value 'neo-cx88-glob buf))
        (loc (buffer-local-value 'neo-cx88-glob (current-buffer))))
    (kill-buffer buf)
    (list val loc
          (local-variable-p 'neo-cx88-glob buf)
          (local-variable-p 'neo-cx88-glob (current-buffer)))))
"##,
        expect,
    );
}

#[test]
fn div_cx88_frame_parameter_round_trip_for_custom_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (let ((before (frame-parameter frame 'neo-cx88-custom)))
    (modify-frame-parameters frame '((neo-cx88-custom . "value-1")))
    (let ((v1 (frame-parameter frame 'neo-cx88-custom)))
      (modify-frame-parameters frame '((neo-cx88-custom . "value-2")))
      (let ((v2 (frame-parameter frame 'neo-cx88-custom)))
        (modify-frame-parameters frame '((neo-cx88-custom)))  ; remove
        (list before v1 v2 (frame-parameter frame 'neo-cx88-custom)))))
"##,
        expect,
    );
}

#[test]
fn div_cx88_display_pixel_dimensions_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function display-pixel-dimensions)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (integerp (display-pixel-width))
      (integerp (display-pixel-height))
      (consp (display-pixel-dimensions))
      (integerp (display-mm-width))
      (integerp (display-mm-height))
      (integerp (display-color-cells))
      (integerp (display-planar-pixels (selected-frame)))
      (display-graphic-p))
"##,
        expect,
    );
}

#[test]
fn div_cx88_with_current_buffer_chain_does_not_leak() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"in a\" \"in b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((origin (current-buffer))
      (buf-a (get-buffer-create " *neo-cx88-a*"))
      (buf-b (get-buffer-create " *neo-cx88-b*")))
  (with-current-buffer buf-a
    (insert "in a")
    (with-current-buffer buf-b
      (insert "in b")))
  (let ((in-origin (eq (current-buffer) origin))
        (in-a (with-current-buffer buf-a (buffer-string)))
        (in-b (with-current-buffer buf-b (buffer-string))))
    (prog1 (list in-origin in-a in-b)
      (kill-buffer buf-a)
      (kill-buffer buf-b))))
"##,
        expect,
    );
}

#[test]
fn div_cx88_buffer_local_face_remapping_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil ((default :foreground \"blue\") (bold :height 2.0)) (default :foreground \"blue\") (bold :height 2.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((before (buffer-local-value 'face-remapping-alist (current-buffer))))
    (setq-local face-remapping-alist '((default :foreground "blue")
                                       (bold :height 2.0)))
    (let ((after (buffer-local-value 'face-remapping-alist (current-buffer))))
      (list before after
            (assq 'default face-remapping-alist)
            (assq 'bold face-remapping-alist)))))
"##,
        expect,
    );
}

#[test]
fn div_cx88_set_buffer_then_with_temp_buffer_does_not_propagate_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx88-se-a*"))
      (origin (current-buffer)))
  (with-current-buffer buf-a
    (insert "AAAA")
    (goto-char 3))
  (let ((p-in-a (with-current-buffer buf-a (point))))
    (with-temp-buffer
      (insert "TEMP")
      (goto-char 5))
    (let ((p-after-temp-in-a (with-current-buffer buf-a (point)))
          (back-in-origin (eq (current-buffer) origin)))
      (kill-buffer buf-a)
      (list p-in-a p-after-temp-in-a back-in-origin))))
"##,
        expect,
    );
}

#[test]
fn div_cx88_buffer_locals_with_let_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:let-shadowed t :global) :buffer-local)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx88-sh :global)
(let ((buf (get-buffer-create " *neo-cx88-sh*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx88-sh) :buffer-local))
  (let ((result-in-buf
         (with-current-buffer buf
           (let ((neo-cx88-sh :let-shadowed))
             (list neo-cx88-sh
                   (local-variable-p 'neo-cx88-sh)
                   (default-value 'neo-cx88-sh))))))
    (let ((after-let (with-current-buffer buf neo-cx88-sh)))
      (kill-buffer buf)
      (list result-in-buf after-let))))
"##,
        expect,
    );
}

#[test]
fn div_cx88_buffer_local_undo_boundary_with_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx88-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (setq-local neo-cx88-mega-counter 0)
    (insert "Buffer content for mega test")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 3 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 20)
      (undo-boundary)
      (cl-incf neo-cx88-mega-counter)
      (delete-region 5 9)
      (insert "X")
      (cl-incf neo-cx88-mega-counter)
      (let ((state (list neo-cx88-mega-counter
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state
              neo-cx88-mega-counter
              (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))
  (kill-buffer buf))
"##,
        expect,
    );
}

#[test]
fn div_cx88_buffer_local_default_value_after_setq_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:a 99 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx88-dv 1)
(let ((buf-a (get-buffer-create " *neo-cx88-dv-a*"))
      (buf-b (get-buffer-create " *neo-cx88-dv-b*")))
  (with-current-buffer buf-a
    (set (make-local-variable 'neo-cx88-dv) :a))
  (setq-default neo-cx88-dv 99)
  (let ((a-val (buffer-local-value 'neo-cx88-dv buf-a))
        (b-val (buffer-local-value 'neo-cx88-dv buf-b))
        (default (default-value 'neo-cx88-dv)))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (list a-val b-val default)))
"##,
        expect,
    );
}
