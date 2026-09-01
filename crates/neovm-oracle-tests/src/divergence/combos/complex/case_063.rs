//! Complex combo batch 63 — buffer-locals / hooks / frame-parameter matrix.
//!
//! Targets divergence surface around per-buffer local variables and their
//! default propagation, buffer-local hooks (depth + permanent-local),
//! frame-parameter read/write including `frame-parameter` lists, window
//! parameters and `window-parameter`, and the interaction of these with
//! buffer-switching.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx63_make_local_variable_with_setq_default_kill_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (100 5 5 t 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx63-lv*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx63-counter) 0))
  (with-current-buffer buf
    (setq neo-cx63-counter 100)
    (setq-default neo-cx63-counter 5))
  (let ((local (buffer-local-value 'neo-cx63-counter buf))
        (default (default-value 'neo-cx63-counter)))
    (with-current-buffer buf
      (kill-local-variable 'neo-cx63-counter))
    (let ((after-kill (buffer-local-value 'neo-cx63-counter buf)))
      (kill-buffer buf)
      (list local default after-kill
            (default-boundp 'neo-cx63-counter)
            (default-value 'neo-cx63-counter)))))
"##,
        expect,
    );
}

#[test]
fn div_cx63_buffer_local_hooks_with_depth_and_permanent_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 4) 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (with-temp-buffer
    (add-hook 'neo-cx63-hook (lambda () (push :one calls)) :depth 10 t)
    (add-hook 'neo-cx63-hook (lambda () (push :two calls)) :depth -1 t)
    (add-hook 'neo-cx63-hook (lambda () (push :three calls)) :depth 5 t)
    (add-hook 'neo-cx63-hook (lambda () (push :permanent calls)) nil t)
    (put 'neo-cx63-hook 'permanent-local t)
    (run-hooks 'neo-cx63-hook)
    (let ((first-run (nreverse calls)))
      (setq calls nil)
      (kill-all-local-variables)
      (run-hooks 'neo-cx63-hook)
      (let ((after-kill (nreverse calls)))
        (list first-run after-kill)))))
"##,
        expect,
    );
}

#[test]
fn div_cx63_setq_default_does_not_overwrite_existing_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (100 200 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(setq-default neo-cx63-shared 1)
(let ((buf-a (get-buffer-create " *neo-cx63-a*"))
      (buf-b (get-buffer-create " *neo-cx63-b*")))
  (with-current-buffer buf-a
    (set (make-local-variable 'neo-cx63-shared) 100))
  (setq-default neo-cx63-shared 7)
  (let ((a-val (buffer-local-value 'neo-cx63-shared buf-a))
        (b-val (with-current-buffer buf-b
                 (set (make-local-variable 'neo-cx63-shared) 200)
                 neo-cx63-shared))
        (default (default-value 'neo-cx63-shared)))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (list a-val b-val default)))
"##,
        expect,
    );
}

#[test]
fn div_cx63_frame_parameters_get_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"F1\" dark nil \"hello\" nil \"initial_terminal\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (let ((name (frame-parameter frame 'name))
        (background-mode (frame-parameter frame 'background-mode))
        (left (frame-parameter frame 'left)))
    (modify-frame-parameters frame '((neo-cx63-custom-param . "hello")))
    (let ((got (frame-parameter frame 'neo-cx63-custom-param)))
      (modify-frame-parameters frame '((neo-cx63-custom-param)))
      (list name background-mode left got
            (frame-parameter frame 'neo-cx63-custom-param)
            (terminal-name (frame-terminal frame))))))
"##,
        expect,
    );
}

#[test]
fn div_cx63_window_parameters_get_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:value 42 (neo-cx63-param . :value) (neo-cx63-num . 42) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (set-window-parameter win 'neo-cx63-param :value)
  (set-window-parameter win 'neo-cx63-num 42)
  (let ((got (window-parameter win 'neo-cx63-param))
        (num (window-parameter win 'neo-cx63-num))
        (all (window-parameters win)))
    (set-window-parameter win 'neo-cx63-param nil)
    (list got num (assq 'neo-cx63-param all) (assq 'neo-cx63-num all)
          (window-parameter win 'neo-cx63-param))))
"##,
        expect,
    );
}

#[test]
fn div_cx63_buffer_local_value_in_indirect_buffer_inherits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable neo-cx63-shared)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((base (get-buffer-create " *neo-cx63-base*")))
  (with-current-buffer base
    (erase-buffer)
    (insert "shared text content")
    (set (make-local-variable 'neo-cx63-shared) :base))
  (let ((ind (make-indirect-buffer base " *neo-cx63-ind*")))
    (let ((base-val (buffer-local-value 'neo-cx63-shared base))
          (ind-val (with-current-buffer ind neo-cx63-shared)))
      (with-current-buffer ind
        (set (make-local-variable 'neo-cx63-shared) :indirect))
      (let ((ind-val-2 (buffer-local-value 'neo-cx63-shared ind))
            (base-val-2 (buffer-local-value 'neo-cx63-shared base)))
        (kill-buffer ind)
        (kill-buffer base)
        (list base-val ind-val ind-val-2 base-val-2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx63_local_variable_p_and_local_variable_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf (get-buffer-create " *neo-cx63-alias*")))
      (with-current-buffer buf
        (defvar neo-cx63-orig :default)
        (make-variable-buffer-local 'neo-cx63-orig)
        (when (fboundp 'defvaralias)
          (defvaralias 'neo-cx63-alias 'neo-cx63-orig))
        (setq neo-cx63-orig :local)
        (let ((orig (local-variable-p 'neo-cx63-orig))
              (alias (if (boundp 'neo-cx63-alias)
                         (local-variable-p 'neo-cx63-alias)))
              (val (if (boundp 'neo-cx63-alias) neo-cx63-alias)))
          (prog1 (list orig alias val
                       (buffer-local-value 'neo-cx63-orig buf))
            (kill-buffer buf))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx63_kill_buffer_hook_and_buffer_list_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:kill) (#<killed buffer>) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx63-kill*"))
      (fired nil))
  (with-current-buffer buf
    (add-hook 'kill-buffer-hook (lambda () (push :kill fired)) nil t))
  (let ((in-list-before (memq buf (buffer-list))))
    (kill-buffer buf)
    (let ((in-list-after (memq buf (buffer-list))))
      (list fired in-list-before in-list-after))))
"##,
        expect,
    );
}

#[test]
fn div_cx63_buffer_local_face_remapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil ((default :height 2.0) (bold :foreground \"red\")) (bold :foreground \"red\") (default :height 2.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((before (buffer-local-value 'face-remapping-alist (current-buffer))))
    (setq-local face-remapping-alist '((default :height 2.0)
                                       (bold :foreground "red")))
    (let ((after (buffer-local-value 'face-remapping-alist (current-buffer))))
      (list before after
            (assq 'bold face-remapping-alist)
            (assq 'default face-remapping-alist)))))
"##,
        expect,
    );
}

#[test]
fn div_cx63_default_toplevel_value_and_buffer_local_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:local :default :default :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf (get-buffer-create " *neo-cx63-tl*")))
      (with-current-buffer buf
        (set (make-local-variable 'neo-cx63-tl-var) :local))
      (setq-default neo-cx63-tl-var :default)
      (let ((local (buffer-local-value 'neo-cx63-tl-var buf))
            (tl (default-toplevel-value 'neo-cx63-tl-var))
            (dv (default-value 'neo-cx63-tl-var)))
        (kill-buffer buf)
        (list local tl dv
              (default-toplevel-value 'neo-cx63-tl-var))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx63_buffer_locals_marker_overlay_undo_textprop_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (add-hook 'after-change-functions
            (lambda (&rest _) (push :change calls)) nil t)
  (with-temp-buffer
    (buffer-enable-undo)
    (set (make-local-variable 'neo-cx63-edge) 0)
    (insert "ABCDEFGHIJ")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 4))
          (ov (make-overlay 3 8)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (setq neo-cx63-edge 99)
      (narrow-to-region 2 9)
      (let ((local (buffer-local-value 'neo-cx63-edge (current-buffer))))
        (delete-region 3 5)
        (insert "XY")
        (let ((state (list local
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (buffer-string) (length calls))))
          (undo) (undo)
          (widen)
          (list state
                (marker-position m)
                (overlayp ov) (overlay-start ov) (overlay-end ov)
                (buffer-string)
                (buffer-local-value 'neo-cx63-edge (current-buffer)))))))
"##,
        expect,
    );
}
