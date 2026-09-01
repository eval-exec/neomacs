//! Strict core subsystem combo oracle probes.
//!
//! These tests intentionally combine several GNU Emacs subsystems in each
//! form: buffer lifecycle and local hooks, abnormal hooks, advice ordering,
//! window/frame state, markers, overlays, text properties, and undo.  Most
//! tests are parity locks.  The final `divergence_surface_*` tests are normal
//! oracle parity assertions that are expected to fail until the recorded
//! GNU/Neomacs differences are fixed.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_core_buffer_kill_query_and_kill_hook_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t ((kill \" *probe-kill*\") q2 q1) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (let ((b (generate-new-buffer " *probe-kill*")))
    (with-current-buffer b
      (setq-local kill-buffer-query-functions
                  (list (lambda () (push 'q1 log) t)
                        (lambda () (push 'q2 log) t)))
      (setq-local kill-buffer-hook
                  (list (lambda () (push (list 'kill (buffer-name)) log)))))
    (list (kill-buffer b) log (buffer-live-p b))))
"##,
        expect,
    );
}

#[test]
fn div_core_buffer_change_hooks_and_modified_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"ac\" t ((before 1 1 nil) (after 1 4 0 t) (before 2 3 t) (after 2 2 1 t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (with-temp-buffer
    (add-hook 'before-change-functions
              (lambda (b e) (push (list 'before b e (buffer-modified-p)) log))
              nil t)
    (add-hook 'after-change-functions
              (lambda (b e l) (push (list 'after b e l (buffer-modified-p)) log))
              nil t)
    (insert "abc")
    (goto-char 2)
    (delete-char 1)
    (list (buffer-string) (buffer-modified-p) (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_normal_and_abnormal_hook_order_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((global local) done ((b 42) global local))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (defvar probe--normal-hook nil)
  (defvar probe--abnormal-hook nil)
  (setq probe--normal-hook nil
        probe--abnormal-hook nil)
  (add-hook 'probe--normal-hook (lambda () (push 'global log)))
  (add-hook 'probe--abnormal-hook (lambda (x) (push (list 'a x) log) nil))
  (add-hook 'probe--abnormal-hook (lambda (x) (push (list 'b x) log) 'done))
  (list
   (with-temp-buffer
     (add-hook 'probe--normal-hook (lambda () (push 'local log)) nil t)
     (run-hooks 'probe--normal-hook)
     log)
   (run-hook-with-args-until-success 'probe--abnormal-hook 42)
   log))
"##,
        expect,
    );
}

#[test]
fn div_core_advice_depth_filter_return_and_member_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (57 (around2-in (before 8) around1-in (orig 9) around1-out (filter-ret 19) around2-out) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (defun probe--adv-depth (x) (push (list 'orig x) log) (+ x 10))
  (let ((before (lambda (x) (push (list 'before x) log)))
        (around1 (lambda (fn x)
                   (push 'around1-in log)
                   (prog1 (funcall fn (+ x 1))
                     (push 'around1-out log))))
        (around2 (lambda (fn x)
                   (push 'around2-in log)
                   (prog1 (funcall fn (* x 2))
                     (push 'around2-out log))))
        (filter-ret (lambda (r) (push (list 'filter-ret r) log) (* r 3))))
    (advice-add 'probe--adv-depth :before before)
    (advice-add 'probe--adv-depth :around around1 '((depth . 10)))
    (advice-add 'probe--adv-depth :around around2 '((depth . -10)))
    (advice-add 'probe--adv-depth :filter-return filter-ret)
    (unwind-protect
        (list (probe--adv-depth 4)
              (nreverse log)
              (not (null (advice-member-p around1 'probe--adv-depth))))
      (advice-remove 'probe--adv-depth before)
      (advice-remove 'probe--adv-depth around1)
      (advice-remove 'probe--adv-depth around2)
      (advice-remove 'probe--adv-depth filter-ret)
      (fmakunbound 'probe--adv-depth))))
"##,
        expect,
    );
}

#[test]
fn div_core_advice_preserves_command_interactive_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (interactive \"p\") (x))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defun probe--adv-command (x) (interactive "p") x)
  (let ((a (lambda (&rest _) nil)))
    (advice-add 'probe--adv-command :before a)
    (unwind-protect
        (list (commandp 'probe--adv-command)
              (interactive-form 'probe--adv-command)
              (help-function-arglist 'probe--adv-command))
      (advice-remove 'probe--adv-command a)
      (fmakunbound 'probe--adv-command))))
"##,
        expect,
    );
}

#[test]
fn div_core_window_buffer_configuration_and_parameters_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (side 42 1 \" *probe-win-a*\" 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create " *probe-win-a*"))
      (b2 (get-buffer-create " *probe-win-b*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (set-window-parameter (selected-window) 'probe 42)
        (set-window-dedicated-p (selected-window) 'side)
        (erase-buffer)
        (insert "abcdef")
        (goto-char 3)
        (let ((dedicated (window-dedicated-p))
              (param (window-parameter nil 'probe))
              (cfg (current-window-configuration))
              (w2 (split-window nil nil 'right)))
          (set-window-buffer w2 b2)
          (select-window w2)
          (erase-buffer)
          (insert "12345")
          (goto-char 4)
          (set-window-configuration cfg)
          (list dedicated param
                (count-windows)
                (buffer-name (window-buffer (selected-window)))
                (point)
                (window-point (selected-window)))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))))
"##,
        expect,
    );
}

#[test]
fn div_core_display_buffer_window_parameters_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 2 yes \" *probe-display*\" 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-display*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b (erase-buffer) (insert "x"))
        (let* ((display-buffer-alist
                '(("\\*probe-display\\*" display-buffer-below-selected
                   (window-height . 4)
                   (window-parameters . ((probe . yes))))))
               (w (display-buffer b)))
          (list (window-live-p w)
                (count-windows)
                (window-parameter w 'probe)
                (buffer-name (window-buffer w))
                (window-total-height w))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_core_buffer_swap_text_moves_markers_and_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"BETA\" \"alpha\" 3 nil 2 nil (ob) (oa))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (generate-new-buffer " *probe-swap-a*"))
      (b (generate-new-buffer " *probe-swap-b*")))
  (unwind-protect
      (let (ma mb oa ob)
        (with-current-buffer a
          (insert "alpha")
          (setq ma (copy-marker 3))
          (setq oa (make-overlay 2 5))
          (overlay-put oa 'tag 'oa))
        (with-current-buffer b
          (insert "BETA")
          (setq mb (copy-marker 2))
          (setq ob (make-overlay 1 4))
          (overlay-put ob 'tag 'ob))
        (with-current-buffer a (buffer-swap-text b))
        (list (with-current-buffer a (buffer-string))
              (with-current-buffer b (buffer-string))
              (marker-position ma) (eq (marker-buffer ma) a)
              (marker-position mb) (eq (marker-buffer mb) b)
              (with-current-buffer a
                (mapcar (lambda (o) (overlay-get o 'tag))
                        (overlays-in (point-min) (point-max))))
              (with-current-buffer b
                (mapcar (lambda (o) (overlay-get o 'tag))
                        (overlays-in (point-min) (point-max))))))
    (kill-buffer a)
    (kill-buffer b)))
"##,
        expect,
    );
}

#[test]
fn div_core_indirect_buffers_narrowing_and_local_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (t \"bcd\" 2 5 2 t ((kill \" *probe-indirect*\" t)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (with-temp-buffer
    (insert "abcdef")
    (let ((ind (make-indirect-buffer (current-buffer) " *probe-indirect*" t)))
      (unwind-protect
          (with-current-buffer ind
            (narrow-to-region 2 5)
            (add-hook 'kill-buffer-hook
                      (lambda ()
                        (push (list 'kill (buffer-name) (buffer-base-buffer)) log))
                      nil t)
            (list (bufferp (buffer-base-buffer))
                  (buffer-string)
                  (point-min) (point-max)
                  (length (buffer-local-value 'kill-buffer-hook ind))
                  (kill-buffer ind)
                  (mapcar (lambda (x)
                            (list (car x) (nth 1 x) (buffer-live-p (nth 2 x))))
                          log)))
        (when (buffer-live-p ind) (kill-buffer ind))))))
"##,
        expect,
    );
}

#[test]
fn div_core_marker_insertion_and_retarget_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 t 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (generate-new-buffer " *probe-marker-a*"))
      (b2 (generate-new-buffer " *probe-marker-b*")))
  (unwind-protect
      (let ((m (make-marker)))
        (with-current-buffer b1
          (insert "abcd")
          (set-marker m 3)
          (set-marker-insertion-type m t)
          (goto-char 3)
          (insert "X"))
        (let ((p1 (marker-position m))
              (it (marker-insertion-type m)))
          (set-marker m 1 b2)
          (list p1 it (marker-position m) (eq (marker-buffer m) b2))))
    (kill-buffer b1)
    (kill-buffer b2)))
"##,
        expect,
    );
}

#[test]
fn div_core_textprop_overlay_delete_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"abf\" 1 2 (rear-nonsticky t category probe-cat face bold)) bold nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (add-text-properties 2 5 '(face bold category probe-cat rear-nonsticky t))
  (put 'probe-cat 'face 'italic)
  (let ((ov (make-overlay 3 6)))
    (overlay-put ov 'priority 10)
    (overlay-put ov 'before-string "<")
    (overlay-put ov 'evaporate t)
    (delete-region 3 6)
    (list (buffer-string)
          (get-text-property 2 'face)
          (overlays-in (point-min) (point-max))
          (overlay-buffer ov)
          (overlay-start ov)
          (overlay-end ov))))
"##,
        expect,
    );
}

#[test]
fn div_core_text_property_stickiness_insert_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"aXb\" 0 1 (rear-nonsticky (face) face bold) 2 3 (front-sticky (face) face italic)) (rear-nonsticky (face) face bold) nil (front-sticky (face) face italic))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ab")
  (add-text-properties 1 2 '(face bold rear-nonsticky (face)))
  (add-text-properties 2 3 '(face italic front-sticky (face)))
  (goto-char 2)
  (insert "X")
  (list (buffer-string)
        (text-properties-at 1)
        (text-properties-at 2)
        (text-properties-at 3)))
"##,
        expect,
    );
}

#[test]
fn div_core_overlay_front_rear_advance_insert_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"aLbRcd\" (2 4) (3 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcd")
  (let ((o1 (make-overlay 2 3 nil nil nil))
        (o2 (make-overlay 2 3 nil t t)))
    (goto-char 2) (insert "L")
    (goto-char 4) (insert "R")
    (list (buffer-string)
          (list (overlay-start o1) (overlay-end o1))
          (list (overlay-start o2) (overlay-end o2)))))
"##,
        expect,
    );
}

#[test]
fn div_core_undo_boundaries_and_change_hooks_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\" t ((before 1 1) (before 4 4) (before 4 7) (before 1 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (with-temp-buffer
    (buffer-enable-undo)
    (add-hook 'before-change-functions
              (lambda (&rest args) (push (cons 'before args) log))
              nil t)
    (insert "abc")
    (undo-boundary)
    (insert "def")
    (let ((u1 buffer-undo-list))
      (undo 1)
      (list (buffer-string) (consp u1) (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_core_window_delete_restores_selected_window_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 t \" *probe-fsw-a*\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create " *probe-fsw-a*"))
      (b2 (get-buffer-create " *probe-fsw-b*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (let ((w2 (split-window nil nil 'right)))
          (set-window-buffer w2 b2)
          (select-window w2)
          (delete-window w2)
          (list (count-windows)
                (eq (selected-window) (frame-selected-window))
                (buffer-name (window-buffer (selected-window)))
                (window-live-p w2))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))))
"##,
        expect,
    );
}

#[test]
fn div_core_minibuffer_window_frame_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t \" *Minibuf-0*\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((mw (minibuffer-window)))
  (list (window-live-p mw)
        (window-minibuffer-p mw)
        (eq (window-frame mw) (selected-frame))
        (buffer-name (window-buffer mw))
        (active-minibuffer-window)))
"##,
        expect,
    );
}

#[test]
fn div_core_batch_frame_font_and_display_metrics_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"tty\" nil 80 25 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (frame-parameter nil 'font)
      (frame-parameter nil 'font-backend)
      (display-pixel-width)
      (display-pixel-height)
      (display-mm-width)
      (display-mm-height))
"##,
        expect,
    );
}

#[test]
fn div_core_frame_fullscreen_alpha_sequence_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((fullboth 80 70) nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(unwind-protect
    (progn
      (modify-frame-parameters
       nil '((fullscreen . fullboth) (alpha . 80) (alpha-background . 70)))
      (let ((a (list (frame-parameter nil 'fullscreen)
                     (frame-parameter nil 'alpha)
                     (frame-parameter nil 'alpha-background))))
        (modify-frame-parameters
         nil '((fullscreen . nil) (alpha . nil) (alpha-background . nil)))
        (list a
              (frame-parameter nil 'fullscreen)
              (frame-parameter nil 'alpha)
              (frame-parameter nil 'alpha-background))))
  (modify-frame-parameters
   nil '((fullscreen . nil) (alpha . nil) (alpha-background . nil))))
"##,
        expect,
    );
}

#[test]
fn div_core_inhibit_modification_hooks_boundary_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abcd\" ((before 4 4) (after 4 5 0)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (with-temp-buffer
    (add-hook 'before-change-functions
              (lambda (&rest args) (push (cons 'before args) log))
              nil t)
    (add-hook 'after-change-functions
              (lambda (&rest args) (push (cons 'after args) log))
              nil t)
    (let ((inhibit-modification-hooks t))
      (insert "abc"))
    (insert "d")
    (list (buffer-string) (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_overlay_modification_hooks_order_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 23 59)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (with-temp-buffer
    (insert "abcd")
    (let ((ov (make-overlay 2 4)))
      (overlay-put
       ov 'modification-hooks
       (list (lambda (o after beg end &optional len)
               (push (list 'mod after beg end len) log))))
      (overlay-put
       ov 'insert-in-front-hooks
       (list (lambda (o after beg end &optional len)
               (push (list 'front after beg end len) log))))
      (overlay-put
       ov 'insert-behind-hooks
       (list (lambda (o after beg end &optional len)
               (push (list 'behind after beg end len) log))))
      (goto-char 2) (insert "X")
      (goto-char (overlay-end ov)) (insert "Y")
      (delete-region 3 5)
      (list (buffer-string)
            (nreverse log)
            (list (overlay-start ov) (overlay-end ov)))))))
"##,
        expect,
    );
}

#[test]
fn div_core_temp_buffer_hooks_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#<buffer *Probe Help*> ((setup \"*Probe Help*\") (show \"*Probe Help*\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (let ((temp-buffer-setup-hook
         (list (lambda () (push (list 'setup (buffer-name)) log))))
        (temp-buffer-show-hook
         (list (lambda () (push (list 'show (buffer-name)) log)))))
    (with-output-to-temp-buffer "*Probe Help*"
      (princ "hello"))
    (list (get-buffer "*Probe Help*") (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_save_window_excursion_restores_buffer_point_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\" *probe-swe-a*\" 4 1) \" *probe-swe-a*\" 4 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create " *probe-swe-a*"))
      (b2 (get-buffer-create " *probe-swe-b*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (erase-buffer) (insert "abcdef") (goto-char 4)
        (let ((before (list (buffer-name) (point) (count-windows))))
          (save-window-excursion
            (split-window nil nil 'right)
            (switch-to-buffer b2)
            (erase-buffer) (insert "123") (goto-char 2))
          (list before (buffer-name) (point) (count-windows))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))))
"##,
        expect,
    );
}

#[test]
fn div_core_save_current_buffer_kill_current_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil \" *probe-scb-a*\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create " *probe-scb-a*"))
      (b2 (get-buffer-create " *probe-scb-b*")))
  (unwind-protect
      (progn
        (switch-to-buffer b1)
        (let ((before (current-buffer)))
          (condition-case err
              (save-current-buffer
                (set-buffer b2)
                (kill-buffer b2)
                (buffer-name (current-buffer)))
            (error (list 'err (car err))))
          (list (eq (current-buffer) before)
                (buffer-live-p b2)
                (buffer-name))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))))
"##,
        expect,
    );
}

#[test]
fn div_core_advice_removed_by_fset_redefinition_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((old 1) (new 2) ((around 1)) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (defun probe--redef (x) (list 'old x))
  (let ((a (lambda (fn x) (push (list 'around x) log) (funcall fn x))))
    (advice-add 'probe--redef :around a)
    (let ((before (probe--redef 1)))
      (fset 'probe--redef (lambda (x) (list 'new x)))
      (unwind-protect
          (list before (probe--redef 2) log (advice-member-p a 'probe--redef))
        (advice-remove 'probe--redef a)
        (fmakunbound 'probe--redef)))))
"##,
        expect,
    );
}

#[test]
fn div_core_frame_parameter_delete_default_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 (probe-x . 1)) nil (probe-x))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(unwind-protect
    (progn
      (modify-frame-parameters nil '((probe-x . 1)))
      (let ((a (list (frame-parameter nil 'probe-x)
                     (assq 'probe-x (frame-parameters)))))
        (modify-frame-parameters nil '((probe-x . nil)))
        (list a
              (frame-parameter nil 'probe-x)
              (assq 'probe-x (frame-parameters)))))
  (modify-frame-parameters nil '((probe-x . nil))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_frame_unsplittable_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (unsplittable))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (nil (unsplittable))
    // Neomacs:   OK (t (unsplittable . t))
    crate::common::assert_oracle_parity_expect(
        r##"
(unwind-protect
    (progn
      (modify-frame-parameters nil '((unsplittable . t)))
      (list (frame-parameter nil 'unsplittable)
            (assq 'unsplittable (frame-parameters))))
  (modify-frame-parameters nil '((unsplittable . nil))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_frame_visibility_nil_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (visibility . t))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (t (visibility . t))
    // Neomacs:   OK (nil (visibility))
    crate::common::assert_oracle_parity_expect(
        r##"
(unwind-protect
    (progn
      (modify-frame-parameters nil '((visibility . nil)))
      (list (frame-parameter nil 'visibility)
            (assq 'visibility (frame-parameters))))
  (modify-frame-parameters nil '((visibility . t))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_batch_frame_size_and_position_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 25 nil nil)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (80 25 nil nil)
    // Neomacs:   OK (81 27 10 20)
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (set-frame-size nil 81 26)
  (set-frame-position nil 10 20)
  (list (frame-width) (frame-height)
        (frame-parameter nil 'left)
        (frame-parameter nil 'top)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_frame_width_height_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 25 80 25 7 8)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (80 25 80 25 7 8)
    // Neomacs:   OK (90 30 90 30 7 8)
    crate::common::assert_oracle_parity_expect(
        r##"
(unwind-protect
    (progn
      (modify-frame-parameters
       nil '((width . 90) (height . 30) (left . 7) (top . 8)))
      (list (frame-width) (frame-height)
            (frame-parameter nil 'width)
            (frame-parameter nil 'height)
            (frame-parameter nil 'left)
            (frame-parameter nil 'top)))
  (modify-frame-parameters
   nil '((width . nil) (height . nil) (left . nil) (top . nil))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_window_resize_split_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((10 14 (0 1 80 11) (0 11 80 25)) 8 16 (0 1 80 9) (0 9 80 25) 2)""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((10 14 (0 1 80 11) (0 11 80 25)) 8 16 (0 1 80 9) (0 9 80 25) 2)
    // Neomacs:   OK ((10 14 (0 0 80 10) (0 10 80 24)) 8 16 (0 0 80 8) (0 8 80 24) 2)
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (delete-other-windows)
  (let* ((root (selected-window))
         (w2 (split-window root 10 'below)))
    (let ((before (list (window-total-height root)
                        (window-total-height w2)
                        (window-edges root)
                        (window-edges w2))))
      (condition-case err
          (window-resize w2 2 nil nil nil)
        (error (push (cons 'resize-error (cons (car err) (cdr err)))
                     before)))
      (prog1 (list before
                   (window-total-height root)
                   (window-total-height w2)
                   (window-edges root)
                   (window-edges w2)
                   (count-windows))
        (delete-other-windows)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_window_start_end_scroll_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((15 30 561) 22 30 561)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((15 30 561) 22 30 561)
    // Neomacs:   OK ((15 30 176) 50 50 211)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (dotimes (i 80) (insert (format "line%02d\n" i)))
  (switch-to-buffer (current-buffer))
  (let ((w (selected-window)))
    (set-window-start w 15)
    (set-window-point w 30)
    (let ((before (list (window-start w) (window-point w) (window-end w t))))
      (condition-case err
          (scroll-up 3)
        (error
         (setq before
               (cons (cons 'scroll-error (cons (car err) (cdr err)))
                     before))))
      (list before (window-start w) (window-point w) (window-end w t)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_window_margins_body_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 (2 . 3) (0 0 nil nil) 75)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (5 (2 . 3) (0 0 nil nil) 75)
    // Neomacs:   OK (5 (2 . 3) (0 0 nil nil) 80)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  (switch-to-buffer (current-buffer))
  (let ((w (selected-window)))
    (set-window-hscroll w 5)
    (set-window-margins w 2 3)
    (set-window-fringes w 4 5 nil)
    (list (window-hscroll w)
          (window-margins w)
          (window-fringes w)
          (window-body-width w))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_switch_buffer_update_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\" *probe-switch-b*\" ((update \"*scratch*\") (update \" *probe-switch-a*\") (update \" *probe-switch-b*\")))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (" *probe-switch-b*" ((update "*scratch*") (update " *probe-switch-a*") (update " *probe-switch-b*")))
    // Neomacs:   OK (" *probe-switch-b*" nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (b1 (get-buffer-create " *probe-switch-a*"))
      (b2 (get-buffer-create " *probe-switch-b*")))
  (unwind-protect
      (let ((buffer-list-update-hook
             (list (lambda () (push (list 'update (buffer-name)) log)))))
        (switch-to-buffer b1)
        (switch-to-buffer b2)
        (list (buffer-name) (nreverse log)))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_window_parameter_configuration_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (after (probe . after))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (after (probe . after))
    // Neomacs:   OK (before (probe . before))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-wparam*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (set-window-parameter nil 'probe 'before)
        (let ((cfg (current-window-configuration)))
          (set-window-parameter nil 'probe 'after)
          (set-window-configuration cfg)
          (list (window-parameter nil 'probe)
                (assq 'probe (window-parameters)))))
    (when (buffer-live-p b) (kill-buffer b))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_kill_all_local_variables_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (local t 70 nil ((change local 33)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (local t 70 nil ((change local 33)))
    // Neomacs:   OK (local t 70 nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (defvar probe--perm-local 'global)
  (put 'probe--perm-local 'permanent-local t)
  (with-temp-buffer
    (setq-local probe--perm-local 'local)
    (setq-local fill-column 33)
    (add-hook 'change-major-mode-hook
              (lambda ()
                (push (list 'change probe--perm-local fill-column) log))
              nil t)
    (kill-all-local-variables)
    (list probe--perm-local
          (local-variable-p 'probe--perm-local)
          fill-column
          (local-variable-p 'fill-column)
          log)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_derived_mode_change_hook_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (probe-child-mode (change parent-body child-body parent-hook child-hook) probe-parent-mode nil)""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (probe-child-mode (change parent-body child-body parent-hook child-hook) probe-parent-mode nil)
    // Neomacs:   OK (probe-child-mode (parent-body child-body parent-hook child-hook) probe-parent-mode nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (define-derived-mode probe-parent-mode fundamental-mode "ProbeParent"
    (push 'parent-body log))
  (define-derived-mode probe-child-mode probe-parent-mode "ProbeChild"
    (push 'child-body log))
  (add-hook 'change-major-mode-hook (lambda () (push 'change log)))
  (add-hook 'probe-parent-mode-hook (lambda () (push 'parent-hook log)))
  (add-hook 'probe-child-mode-hook (lambda () (push 'child-hook log)))
  (with-temp-buffer
    (probe-child-mode)
    (list major-mode
          (nreverse log)
          (derived-mode-p 'probe-parent-mode)
          (derived-mode-p 'fundamental-mode))))
"##,
        expect,
    );
}

#[test]
fn div_core_after_change_major_mode_hook_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (text-mode ((after text-mode)))""#]];
    // Parity lock: after-change-major-mode-hook runs with the new major mode
    // already installed.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (add-hook 'after-change-major-mode-hook
            (lambda () (push (list 'after major-mode) log)))
  (with-temp-buffer
    (text-mode)
    (list major-mode (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_field_boundary_motion_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 6 #(\"bb\" 0 2 (field mid)) 6 4)""#]];
    // Parity lock: field-beginning/field-end/field-string and constrain-to-field
    // across three text-property fields.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aa bb cc")
  (put-text-property 1 3 'field 'left)
  (put-text-property 4 6 'field 'mid)
  (put-text-property 7 9 'field 'right)
  (list (field-beginning 5) (field-end 5) (field-string 5)
        (constrain-to-field 8 5)
        (constrain-to-field 2 5)))
"##,
        expect,
    );
}

#[test]
fn div_core_default_value_buffer_local_symbol_plist_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (new-global a b symbolp)""#]];
    // Parity lock: set-default after per-buffer setq-local, plus symbol plist.
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar probe--bufvar 'global)
  (put 'probe--bufvar 'safe-local-variable #'symbolp)
  (let ((b1 (generate-new-buffer " *probe-var-a*"))
        (b2 (generate-new-buffer " *probe-var-b*")))
    (unwind-protect
        (progn
          (with-current-buffer b1 (setq-local probe--bufvar 'a))
          (with-current-buffer b2 (setq-local probe--bufvar 'b))
          (set-default 'probe--bufvar 'new-global)
          (list (default-value 'probe--bufvar)
                (buffer-local-value 'probe--bufvar b1)
                (buffer-local-value 'probe--bufvar b2)
                (get 'probe--bufvar 'safe-local-variable)))
      (kill-buffer b1)
      (kill-buffer b2))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_text_mode_change_major_mode_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (text-mode (change (after text-mode)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (text-mode (change (after text-mode)))
    // Neomacs:   OK (text-mode ((after text-mode)))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (add-hook 'change-major-mode-hook (lambda () (push 'change log)))
  (add-hook 'after-change-major-mode-hook
            (lambda () (push (list 'after major-mode) log)))
  (with-temp-buffer
    (text-mode)
    (list major-mode (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_set_auto_mode_change_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (text-mode (change text-hook))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (text-mode (change text-hook))
    // Neomacs:   OK (text-mode (text-hook))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (auto-mode-alist '(("\\.probe\\'" . text-mode))))
  (add-hook 'change-major-mode-hook (lambda () (push 'change log)))
  (add-hook 'text-mode-hook (lambda () (push 'text-hook log)))
  (with-temp-buffer
    (setq buffer-file-name "x.probe")
    (set-auto-mode)
    (list major-mode (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_read_only_before_change_hook_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (buffer-read-only ok \"abcY\" ((before 4 4)))""#]];
    // Divergence surfaced 2026-06-24: a rejected read-only insertion still
    // double-fires before-change-functions in Neomacs.
    // GNU Emacs: OK (buffer-read-only ok "abcY" ((before 4 4)))
    // Neomacs:   OK (buffer-read-only ok "abcY" ((before 4 4) (before 4 4)))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (with-temp-buffer
    (insert "abc")
    (setq buffer-read-only t)
    (add-hook 'before-change-functions
              (lambda (&rest args) (push (cons 'before args) log))
              nil t)
    (let ((err1 (condition-case err (progn (insert "X") 'ok) (error (car err))))
          (err2 (let ((inhibit-read-only t))
                  (goto-char (point-max))
                  (insert "Y")
                  'ok)))
      (list err1 err2 (buffer-string) (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_core_file_temp_attributes_and_insert_file_contents_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t 5 \"a.txt\" (0 7 \"abc\\ndef\" nil t))""#]];
    // Parity lock: temp file creation, file attributes, and insert-file-contents
    // without retaining volatile absolute temp names in the asserted result.
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-probe" t))
       (f1 (expand-file-name "a.txt" dir))
       (f2 (make-temp-file "neo-probe" nil ".txt" "abc\ndef")))
  (unwind-protect
      (progn
        (write-region "hello" nil f1 nil 'silent)
        (list
         (file-exists-p f1)
         (file-readable-p f1)
         (file-directory-p dir)
         (nth 7 (file-attributes f1 'integer))
         (file-name-nondirectory (file-truename f1))
         (with-temp-buffer
           (let ((ret (insert-file-contents f2)))
             (list (string-match-p "\\`neo-probe.*\\.txt\\'" (file-name-nondirectory (car ret)))
                   (cadr ret)
                   (buffer-string)
                   buffer-file-name
                   (buffer-modified-p))))))
    (delete-directory dir t)
    (delete-file f2)))
"##,
        expect,
    );
}

#[test]
fn div_core_call_process_environment_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 \"a\\nb\") (0 \"xyz\" \"xyz\"))""#]];
    // Parity lock: call-process, shell-command-switch, process-environment, and
    // getenv binding all agree in batch mode.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((process-environment (cons "NEO_PROBE_VAR=xyz" process-environment)))
  (list
   (with-temp-buffer
     (let ((status (call-process shell-file-name nil t nil
                                 shell-command-switch "printf 'a\\nb'")))
       (list status (buffer-string))))
   (with-temp-buffer
     (let ((status (call-process shell-file-name nil t nil
                                 shell-command-switch "printf $NEO_PROBE_VAR")))
       (list status (buffer-string) (getenv "NEO_PROBE_VAR"))))))
"##,
        expect,
    );
}

#[test]
fn div_core_register_point_text_and_rectangle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((t 4 \"bcd\") ((\"\" \"\" \"\") \"\"))""#]];
    // Parity lock: point registers, text registers, and rectangle registers.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (insert "abcdef")
   (goto-char 4)
   (point-to-register ?a)
   (copy-to-register ?b 2 5)
   (list (markerp (get-register ?a))
         (marker-position (get-register ?a))
         (get-register ?b)))
 (with-temp-buffer
   (insert "aa11\nbb22\ncc33\n")
   (copy-rectangle-to-register ?r 3 13)
   (erase-buffer)
   (insert-register ?r)
   (list (get-register ?r) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_core_timer_absolute_and_idle_shape_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((t (0 1 2 345000) nil) (t ([nil 0 1000 0 nil (closure (t) nil 'never) nil idle 0 nil]) idle))""#
    ]];
    // Parity lock: absolute timer and idle timer shape are stable when the
    // scheduled time is explicit.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((tm (run-at-time '(0 1 2 345000) nil (lambda () 'never))))
   (unwind-protect
       (list (timerp tm) (timer--time tm) (timer--repeat-delay tm))
     (cancel-timer tm)))
 (let ((tm (run-with-idle-timer 1000 nil (lambda () 'never))))
   (unwind-protect
       (list (timerp tm) (memq tm timer-idle-list) (timer--idle-delay tm))
     (cancel-timer tm))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_relative_timer_microseconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t nil)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: relative run-at-time retains a nonzero microsecond component.
    // Neomacs:   relative run-at-time reports zero microseconds.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tm (run-at-time 1000 nil (lambda () 'never))))
  (unwind-protect
      (let ((tt (timer--time tm)))
        (list (timerp tm)
              (integerp (nth 0 tt))
              (integerp (nth 1 tt))
              (integerp (nth 2 tt))
              (not (zerop (nth 3 tt)))
              (timer--repeat-delay tm)))
    (cancel-timer tm)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_repeating_timer_microseconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t 5)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: repeating run-at-time keeps a nonzero microsecond component.
    // Neomacs:   repeating run-at-time reports zero microseconds.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tm (run-at-time 10 5 (lambda () 'never))))
  (unwind-protect
      (let ((tt (timer--time tm)))
        (list (timerp tm)
              (integerp (nth 0 tt))
              (integerp (nth 1 tt))
              (integerp (nth 2 tt))
              (not (zerop (nth 3 tt)))
              (timer--repeat-delay tm)))
    (cancel-timer tm)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_message_repetition_coalescing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"same [3 times]\\n\")""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (nil "same [3 times]\n")
    // Neomacs:   OK (nil "same\nsame\nsame\n")
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((message-log-max t))
  (with-current-buffer (get-buffer-create "*Messages*")
    (let ((inhibit-read-only t))
      (erase-buffer)))
  (message "same")
  (message "same")
  (message "same")
  (list (current-message)
        (with-current-buffer "*Messages*" (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_core_variable_watcher_set_and_buffer_local_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((3 3 ((probe--watch 1 set nil) (probe--watch 2 set nil) (probe--watch 3 set nil))) (0 0 ((probe--watch-local 10 set \" *probe-watch-local*\") (probe--watch-local 11 set \" *probe-watch-local*\") (probe--watch-local nil makunbound \" *probe-watch-local*\"))))""#
    ]];
    // Parity lock: variable watchers receive global set/setq-default events,
    // buffer-local set events, and makunbound from kill-local-variable.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((log nil))
   (defvar probe--watch 0)
   (let ((watcher (lambda (sym new op where)
                    (push (list sym new op (bufferp where)) log))))
     (add-variable-watcher 'probe--watch watcher)
     (unwind-protect
         (progn
           (setq probe--watch 1)
           (set 'probe--watch 2)
           (setq-default probe--watch 3)
           (list probe--watch (default-value 'probe--watch) (nreverse log)))
       (remove-variable-watcher 'probe--watch watcher))))
 (let ((log nil))
   (defvar probe--watch-local 0)
   (let ((watcher (lambda (sym new op where)
                    (push (list sym new op
                                (and (bufferp where) (buffer-name where)))
                          log))))
     (add-variable-watcher 'probe--watch-local watcher)
     (unwind-protect
         (with-temp-buffer
           (rename-buffer " *probe-watch-local*" t)
           (setq-local probe--watch-local 10)
           (setq probe--watch-local 11)
           (kill-local-variable 'probe--watch-local)
           (list probe--watch-local
                 (default-value 'probe--watch-local)
                 (nreverse log)))
       (remove-variable-watcher 'probe--watch-local watcher)))))
"##,
        expect,
    );
}

#[test]
fn div_core_make_variable_buffer_local_default_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (new-default a b t t)""#]];
    // Parity lock: automatically buffer-local variables keep per-buffer values
    // after a default value mutation.
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar probe--auto-local 'global)
  (make-variable-buffer-local 'probe--auto-local)
  (let ((b1 (generate-new-buffer " *probe-auto-a*"))
        (b2 (generate-new-buffer " *probe-auto-b*")))
    (unwind-protect
        (progn
          (with-current-buffer b1 (setq probe--auto-local 'a))
          (with-current-buffer b2 (setq probe--auto-local 'b))
          (setq-default probe--auto-local 'new-default)
          (list (default-value 'probe--auto-local)
                (buffer-local-value 'probe--auto-local b1)
                (buffer-local-value 'probe--auto-local b2)
                (local-variable-if-set-p 'probe--auto-local b1)
                (local-variable-if-set-p 'probe--auto-local b2)))
      (kill-buffer b1)
      (kill-buffer b2))))
"##,
        expect,
    );
}

#[test]
fn div_core_window_state_get_put_keymap_and_face_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((2 (\" *probe-state-a*\" \" *probe-state-b*\") (nil nil)) (new-cmd [3 110] new-cmd) (\"<C-a>\" \"C-<S-a>\" \"C-a\" [3 f5] (control shift) a) ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] \"unspecified-fg\" \"unspecified-bg\" bold ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))))""#
    ]];
    // Parity lock: window-state-get/put, remapping keymaps, event descriptions,
    // and batch face attributes in one broader basic-subsystem probe.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((b1 (get-buffer-create " *probe-state-a*"))
       (b2 (get-buffer-create " *probe-state-b*")))
   (unwind-protect
       (progn
         (delete-other-windows)
         (switch-to-buffer b1)
         (let ((w2 (split-window nil nil 'right)))
           (set-window-buffer w2 b2)
           (set-window-parameter w2 'probe 'yes)
           (let ((state (window-state-get nil t)))
             (delete-other-windows)
             (window-state-put state nil 'safe)
             (list (count-windows)
                   (mapcar (lambda (w) (buffer-name (window-buffer w)))
                           (window-list nil 'nomini))
                   (mapcar (lambda (w) (window-parameter w 'probe))
                           (window-list nil 'nomini))))))
     (when (buffer-live-p b1) (kill-buffer b1))
     (when (buffer-live-p b2) (kill-buffer b2))))
 (let ((map (make-sparse-keymap)))
   (define-key map [remap old-cmd] 'new-cmd)
   (define-key map (kbd "C-c n") 'new-cmd)
   (list (command-remapping 'old-cmd nil (list map))
         (where-is-internal 'new-cmd map t)
         (lookup-key map [remap old-cmd])))
 (list (key-description [C-a])
       (key-description [C-S-a])
       (single-key-description ?\C-a)
       (kbd "C-c <f5>")
       (event-modifiers 'C-S-a)
       (event-basic-type 'C-S-a))
 (list (facep 'default)
       (face-attribute 'default :foreground nil 'default)
       (face-attribute 'default :background nil 'default)
       (face-attribute 'bold :weight nil 'default)
       (face-all-attributes 'bold nil)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_permanent_local_mode_change_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (local t nil nil ((change local probe)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (local t nil nil ((change local probe)))
    // Neomacs:   OK (local t nil nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (defvar probe--perm2 'global)
  (put 'probe--perm2 'permanent-local t)
  (with-temp-buffer
    (setq-local probe--perm2 'local)
    (setq-local transient-mark-mode 'probe)
    (add-hook 'change-major-mode-hook
              (lambda () (push (list 'change probe--perm2 transient-mark-mode)
                               log))
              nil t)
    (fundamental-mode)
    (text-mode)
    (list probe--perm2
          (local-variable-p 'probe--perm2)
          transient-mark-mode
          (local-variable-p 'transient-mark-mode)
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_window_prev_buffers_after_previous_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"*scratch*\" nil (\" *probe-prev-c*\"))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("*scratch*" nil (" *probe-prev-c*"))
    // Neomacs:   OK ("*scratch*" (("*scratch*" 1 1)) (" *probe-prev-c*"))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create " *probe-prev-a*"))
      (b2 (get-buffer-create " *probe-prev-b*"))
      (b3 (get-buffer-create " *probe-prev-c*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (switch-to-buffer b2)
        (switch-to-buffer b3)
        (previous-buffer)
        (let ((w (selected-window)))
          (list (buffer-name (window-buffer w))
                (mapcar (lambda (e)
                          (list (buffer-name (nth 0 e))
                                (marker-position (nth 1 e))
                                (marker-position (nth 2 e))))
                        (window-prev-buffers w))
                (mapcar #'buffer-name (window-next-buffers w)))))
    (mapc (lambda (b) (when (buffer-live-p b) (kill-buffer b)))
          (list b1 b2 b3))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_buffer_rename_update_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"probe-rename-new\" (\"*scratch*\" \"probe-rename-new\"))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("probe-rename-new" ("*scratch*" "probe-rename-new"))
    // Neomacs:   OK ("probe-rename-new" ("*scratch*"))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (buffer-list-update-hook nil))
  (add-hook 'buffer-list-update-hook
            (lambda () (push (buffer-name) log)))
  (let ((b (generate-new-buffer "probe-rename")))
    (unwind-protect
        (with-current-buffer b
          (rename-buffer "probe-rename-new" t)
          (list (buffer-name) (nreverse log)))
      (when (buffer-live-p b) (kill-buffer b)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_set_buffer_major_mode_change_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (fundamental-mode \"Fundamental\" (text-mode))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (fundamental-mode "Fundamental" (text-mode))
    // Neomacs:   OK (fundamental-mode "Fundamental" nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (with-temp-buffer
    (setq major-mode 'text-mode
          mode-name "Text")
    (add-hook 'change-major-mode-hook
              (lambda () (push major-mode log))
              nil t)
    (set-buffer-major-mode (current-buffer))
    (list major-mode mode-name (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_global_major_mode_hook_order_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (fundamental-mode ((change fundamental-mode) (text-hook text-mode) (after text-mode) (change text-mode) (after fundamental-mode)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (fundamental-mode ((change fundamental-mode) (text-hook text-mode) (after text-mode) (change text-mode) (after fundamental-mode)))
    // Neomacs:   OK (fundamental-mode ((text-hook text-mode) (after text-mode) (after fundamental-mode)))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (change-major-mode-hook nil)
      (after-change-major-mode-hook nil)
      (text-mode-hook nil))
  (add-hook 'change-major-mode-hook
            (lambda () (push (list 'change major-mode) log)))
  (add-hook 'after-change-major-mode-hook
            (lambda () (push (list 'after major-mode) log)))
  (add-hook 'text-mode-hook
            (lambda () (push (list 'text-hook major-mode) log)))
  (with-temp-buffer
    (text-mode)
    (fundamental-mode)
    (list major-mode (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_overlay_category_evaporate_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"aef\" nil nil nil #(\"<\" 0 1 (face bold)) \">\")""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("aef" nil nil nil #("<" 0 1 (face bold)) ">")
    // Neomacs:   OK ("aef" #<killed buffer> 2 2 #("<" 0 1 (face bold)) ">")
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 2 5)))
    (overlay-put o 'before-string (propertize "<" 'face 'bold))
    (overlay-put o 'after-string ">")
    (overlay-put o 'category 'probe-cat)
    (put 'probe-cat 'evaporate t)
    (delete-region 2 5)
    (list (buffer-string)
          (overlay-buffer o)
          (overlay-start o)
          (overlay-end o)
          (overlay-get o 'before-string)
          (overlay-get o 'after-string))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_bury_buffer_update_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\" *probe-bury*\" (\"*scratch*\" \" *probe-bury*\" \" *probe-bury*\"))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (" *probe-bury*" ("*scratch*" " *probe-bury*" " *probe-bury*"))
    // Neomacs:   OK (" *probe-bury*" ("*scratch*" " *probe-bury*"))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (buffer-list-update-hook nil)
      (b (get-buffer-create " *probe-bury*")))
  (unwind-protect
      (progn
        (add-hook 'buffer-list-update-hook
                  (lambda () (push (buffer-name) log)))
        (switch-to-buffer b)
        (bury-buffer b)
        (list (buffer-name (current-buffer)) (nreverse log)))
    (when (buffer-live-p b) (kill-buffer b))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_window_configuration_hook_batch_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 nil)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (1 nil)
    // Neomacs:   OK (1 ((config 2)))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (b1 (get-buffer-create " *probe-wh3-a*"))
      (b2 (get-buffer-create " *probe-wh3-b*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (setq window-configuration-change-hook nil
              window-buffer-change-functions nil
              window-size-change-functions nil
              window-selection-change-functions nil)
        (add-hook 'window-configuration-change-hook
                  (lambda () (push (list 'config (count-windows)) log)))
        (add-hook 'window-buffer-change-functions
                  (lambda (w)
                    (push (list 'buf (buffer-name (window-buffer w))) log)))
        (add-hook 'window-size-change-functions
                  (lambda (f) (push (list 'size (framep f) (count-windows)) log)))
        (add-hook 'window-selection-change-functions
                  (lambda (f) (push (list 'select (framep f) (buffer-name)) log)))
        (switch-to-buffer b1)
        (let ((w2 (split-window nil nil 'right)))
          (set-window-buffer w2 b2)
          (select-window w2)
          (delete-window w2))
        (list (count-windows) (nreverse log)))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_overlay_category_modification_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"abXdef\" 2 5 ((cat nil 3 3 nil 2 5) (cat t 3 5 0 2 7) (cat nil 4 6 nil 2 7) (cat t 4 4 2 2 5)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("abXdef" 2 5 ((cat nil 3 3 nil 2 5) (cat t 3 5 0 2 7) (cat nil 4 6 nil 2 7) (cat t 4 4 2 2 5)))
    // Neomacs:   OK ("abXdef" 2 5 nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((log nil))
    (put 'probe-cat3 'modification-hooks
         (list (lambda (o after beg end &optional len)
                 (push (list 'cat after beg end len
                             (overlay-start o) (overlay-end o))
                       log))))
    (let ((o (make-overlay 2 5)))
      (overlay-put o 'category 'probe-cat3)
      (goto-char 3)
      (insert "XX")
      (delete-region 4 6)
      (list (buffer-string)
            (overlay-start o)
            (overlay-end o)
            (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_overlay_category_insert_in_front_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"abXXcdef\" 3 3 ((front nil 3 3 nil 3 3) (front t 3 5 0 3 3)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("abXXcdef" 3 3 ((front nil 3 3 nil 3 3) (front t 3 5 0 3 3)))
    // Neomacs:   OK ("abXXcdef" 3 3 nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((log nil))
    (put 'probe-cat4 'insert-in-front-hooks
         (list (lambda (o after beg end &optional len)
                 (push (list 'front after beg end len
                             (overlay-start o) (overlay-end o))
                       log))))
    (let ((o (make-overlay 3 3 nil t nil)))
      (overlay-put o 'category 'probe-cat4)
      (goto-char 3)
      (insert "XX")
      (list (buffer-string)
            (overlay-start o)
            (overlay-end o)
            (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_overlay_category_insert_behind_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"abXXcdef\" 3 5 ((behind nil 3 3 nil 3 3) (behind t 3 5 0 3 5)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("abXXcdef" 3 5 ((behind nil 3 3 nil 3 3) (behind t 3 5 0 3 5)))
    // Neomacs:   OK ("abXXcdef" 3 5 nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((log nil))
    (put 'probe-cat5 'insert-behind-hooks
         (list (lambda (o after beg end &optional len)
                 (push (list 'behind after beg end len
                             (overlay-start o) (overlay-end o))
                       log))))
    (let ((o (make-overlay 3 3 nil nil t)))
      (overlay-put o 'category 'probe-cat5)
      (goto-char 3)
      (insert "XX")
      (list (buffer-string)
            (overlay-start o)
            (overlay-end o)
            (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_frame_parameters_buffer_list_modeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((no-accept-focus) (modeline . t) nil (\"*scratch*\") nil)""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((no-accept-focus) (modeline . t) nil ("*scratch*") nil)
    // Neomacs:   OK (nil nil (font-parameter) nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((params (frame-parameters (selected-frame))))
  (list (assq 'no-accept-focus params)
        (assq 'modeline params)
        (assq 'font-parameter params)
        (mapcar (lambda (b) (buffer-name b))
                (cdr (assq 'buffer-list params)))
        (cdr (assq 'buried-buffer-list params))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_text_category_modification_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"abXdef\" 1 2 (category probe-text-cat3) 3 4 (category probe-text-cat3)) ((4 6)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (#("abXdef" 1 2 (category probe-text-cat3) 3 4 (category probe-text-cat3)) ((4 6)))
    // Neomacs:   OK (#("abXdef" 1 2 (category probe-text-cat3) 3 4 (category probe-text-cat3)) nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((log nil))
    (put 'probe-text-cat3 'modification-hooks
         (list (lambda (&rest args) (push args log))))
    (put-text-property 2 5 'category 'probe-text-cat3)
    (goto-char 3)
    (insert "XX")
    (delete-region 4 6)
    (list (buffer-string) (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_window_scroll_error_and_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((err beginning-of-buffer 13 13 201 50) (ok 201 145 201))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((err beginning-of-buffer 13 13 201 50) (ok 201 145 201))
    // Neomacs:   OK ((err end-of-buffer 1 1 93 50) (ok 189 189 201))
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((b (get-buffer-create " *probe-win-scroll*")))
   (unwind-protect
       (progn
         (with-current-buffer b
           (erase-buffer)
           (dotimes (i 50) (insert (format "l%02d\n" i))))
         (delete-other-windows)
         (switch-to-buffer b)
         (goto-char (point-min))
         (condition-case err
             (progn
               (scroll-up 3)
               (let ((s1 (window-start))
                     (p1 (point)))
                 (scroll-down 1)
                 (list 'ok s1 p1 (window-start) (point))))
           (error (list 'err
                        (car err)
                        (point)
                        (window-start)
                        (window-end nil t)
                        (count-lines (point-min) (point-max))))))
     (when (buffer-live-p b) (kill-buffer b))))
 (let ((b (get-buffer-create " *probe-win-scroll2*")))
   (unwind-protect
       (progn
         (with-current-buffer b
           (erase-buffer)
           (dotimes (i 50) (insert (format "l%02d\n" i))))
         (delete-other-windows)
         (switch-to-buffer b)
         (goto-char (point-max))
         (condition-case err
             (progn
               (scroll-down 3)
               (list 'ok (point) (window-start) (window-end nil t)))
           (error (list 'err (car err) (point) (window-start) (window-end nil t)))))
     (when (buffer-live-p b) (kill-buffer b)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_recenter_window_end_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 57 201)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (80 57 201)
    // Neomacs:   OK (80 57 149)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-recenter*")))
  (unwind-protect
      (progn
        (with-current-buffer b
          (erase-buffer)
          (dotimes (i 50) (insert (format "l%02d\n" i))))
        (delete-other-windows)
        (switch-to-buffer b)
        (goto-char 80)
        (recenter 5)
        (list (point) (window-start) (window-end nil t)))
    (when (buffer-live-p b) (kill-buffer b))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_set_window_configuration_killed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 (\" *probe-ks-a*\" \"*scratch*\"))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (2 (" *probe-ks-a*" "*scratch*"))
    // Neomacs:   OK (2 (" *probe-ks-a*" nil))   ; leaves a window whose buffer is nil
    //            and emits a "Selecting deleted buffer" redisplay error.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (get-buffer-create " *probe-ks-a*"))
      (b (get-buffer-create " *probe-ks-b*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer a)
        (let ((w2 (split-window-below)))
          (set-window-buffer w2 b)
          (let ((cfg (current-window-configuration)))
            (delete-other-windows)
            (kill-buffer b)
            (set-window-configuration cfg)
            (list (count-windows)
                  (mapcar (lambda (w) (buffer-name (window-buffer w)))
                          (window-list nil 'nomini))))))
    (when (buffer-live-p a) (kill-buffer a))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_compare_window_configurations_split_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK nil   ; split + delete leaves a configuration GNU treats as different
    // Neomacs:   OK t     ; Neomacs treats it as identical to the pre-split configuration
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (get-buffer-create " *probe-cfg-cmp2-a*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer a)
        (let ((cfg1 (current-window-configuration))
              (w2 (split-window-below)))
          (delete-window w2)
          (compare-window-configurations cfg1 (current-window-configuration))))
    (when (buffer-live-p a) (kill-buffer a))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_window_configuration_register_killed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 (\" *probe-winreg-a*\" \"*scratch*\"))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (2 (" *probe-winreg-a*" "*scratch*"))
    // Neomacs:   OK (2 (" *probe-winreg-a*" nil))   ; register restore leaves a nil-buffer window
    //            and emits a "Selecting deleted buffer" redisplay error.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (get-buffer-create " *probe-winreg-a*"))
      (b (get-buffer-create " *probe-winreg-b*"))
      (register-alist nil))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer a)
        (let ((w2 (split-window-below)))
          (set-window-buffer w2 b)
          (window-configuration-to-register ?w)
          (delete-other-windows)
          (kill-buffer b)
          (jump-to-register ?w)
          (list (count-windows)
                (mapcar (lambda (w) (buffer-name (window-buffer w)))
                        (window-list nil 'nomini)))))
    (when (buffer-live-p a) (kill-buffer a))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_frame_buffer_list_after_bury() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\" *probe-frame-buf-a*\" \"*scratch*\") (\" *probe-frame-buf-a*\" \"*scratch*\") (\" *probe-frame-buf-b*\"))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((" *probe-frame-buf-a*" "*scratch*") (" *probe-frame-buf-a*" "*scratch*") (" *probe-frame-buf-b*"))
    // Neomacs:   OK ((" *probe-frame-buf-a*") (" *probe-frame-buf-a*") (" *probe-frame-buf-b*"))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (get-buffer-create " *probe-frame-buf-a*"))
      (b (get-buffer-create " *probe-frame-buf-b*")))
  (unwind-protect
      (progn
        (switch-to-buffer a)
        (switch-to-buffer b)
        (bury-buffer b)
        (let ((params (frame-parameters)))
          (list (mapcar #'buffer-name (frame-parameter nil 'buffer-list))
                (mapcar #'buffer-name (cdr (assq 'buffer-list params)))
                (mapcar #'buffer-name (cdr (assq 'buried-buffer-list params))))))
    (mapc (lambda (x) (when (buffer-live-p x) (kill-buffer x)))
          (list a b))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_next_previous_buffer_after_bury() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"*Messages*\" \"*scratch*\" (\"*Messages*\") (\"*Messages*\"))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("*Messages*" "*scratch*" ("*Messages*") ("*Messages*"))
    // Neomacs:   OK ("*Messages*" "*scratch*" ("*Messages*" "*scratch*") ("*Messages*"))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (get-buffer-create " *probe-nextbuf-a*"))
      (b (get-buffer-create " *probe-nextbuf-b*"))
      (c (get-buffer-create " *probe-nextbuf-c*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer a)
        (switch-to-buffer b)
        (switch-to-buffer c)
        (bury-buffer b)
        (next-buffer)
        (let ((after-next (buffer-name)))
          (previous-buffer)
          (list after-next
                (buffer-name)
                (mapcar (lambda (e) (buffer-name (car e)))
                        (window-prev-buffers))
                (mapcar #'buffer-name (window-next-buffers)))))
    (mapc (lambda (x) (when (buffer-live-p x) (kill-buffer x)))
          (list a b c))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_kill_buffer_live_process_hangup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (signal nil #<killed buffer> nil ((\"hangup\\n\" signal nil)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (signal nil #<killed buffer> nil (("hangup\n" signal nil)))
    // Neomacs:   OK (run (run open listen connect stop) #<killed buffer> nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *probe-proc-killbuf2*"))
      (log nil))
  (let ((proc (make-process
               :name "probe-proc-killbuf2"
               :buffer buf
               :command '("/bin/sh" "-c" "read line")
               :connection-type 'pipe
               :sentinel (lambda (p e)
                           (push (list e
                                       (process-status p)
                                       (buffer-live-p (process-buffer p)))
                                 log)))))
    (set-process-query-on-exit-flag proc nil)
    (let (result)
      (unwind-protect
          (progn
            (kill-buffer buf)
            (let ((i 0))
              (while (and (process-live-p proc) (< i 20))
                (accept-process-output proc 0.05)
                (setq i (1+ i))))
            (setq result
                  (list (process-status proc)
                        (process-live-p proc)
                        (process-buffer proc)
                        (marker-buffer (process-mark proc))
                        (nreverse log))))
        (when (process-live-p proc) (delete-process proc)))
      result)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_delete_process_missing_sentinel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (signal nil ((\"killed\\n\" signal)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (signal nil (("killed\n" signal)))
    // Neomacs:   OK (signal nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (let ((proc (make-process
               :name "probe-del-sent-clean"
               :command '("/bin/sh" "-c" "read line")
               :connection-type 'pipe
               :sentinel (lambda (p e)
                           (push (list e (process-status p)) log)))))
    (set-process-query-on-exit-flag proc nil)
    (delete-process proc)
    (let ((i 0))
      (while (and (null log) (< i 20))
        (accept-process-output nil 0.05)
        (setq i (1+ i))))
    (list (process-status proc)
          (process-live-p proc)
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_interrupt_process_missing_sentinel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (signal 2 ((\"interrupt\\n\" signal 2)))""#]];
    // Use a direct long-running child so SIGINT cannot race a shell installing
    // its trap. The probe locks the final status and sentinel delivery.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (let ((proc (make-process
               :name "probe-interrupt-sent"
               :command '("sleep" "30")
               :connection-type 'pipe
               :sentinel (lambda (p e)
                           (push (list e
                                       (process-status p)
                                       (process-exit-status p))
                                 log)))))
    (set-process-query-on-exit-flag proc nil)
    (interrupt-process proc)
    (let ((i 0))
      (while (and (process-live-p proc) (< i 20))
        (accept-process-output proc 0.05)
        (setq i (1+ i))))
    (let ((j 0))
      (while (and (null log) (< j 20))
        (accept-process-output proc 0.05)
        (setq j (1+ j))))
    (prog1 (list (process-status proc)
                 (process-exit-status proc)
                 (nreverse log))
      (when (process-live-p proc) (delete-process proc)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_kill_process_missing_sentinel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (signal 9 ((\"killed\\n\" signal 9)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (signal 9 (("killed\n" signal 9)))
    // Neomacs:   OK (signal 9 nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (let ((proc (make-process
               :name "probe-kill-sent"
               :command '("/bin/sh" "-c" "read line")
               :connection-type 'pipe
               :sentinel (lambda (p e)
                           (push (list e
                                       (process-status p)
                                       (process-exit-status p))
                                 log)))))
    (set-process-query-on-exit-flag proc nil)
    (kill-process proc)
    (let ((i 0))
      (while (and (process-live-p proc) (< i 20))
        (accept-process-output proc 0.05)
        (setq i (1+ i))))
    (let ((j 0))
      (while (and (null log) (< j 20))
        (accept-process-output proc 0.05)
        (setq j (1+ j))))
    (prog1 (list (process-status proc)
                 (process-exit-status proc)
                 (nreverse log))
      (when (process-live-p proc) (delete-process proc)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_quit_process_sentinel_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (signal 3 ((\"quit (core dumped)\\n\" signal 3)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (signal 3 (("quit (core dumped)\n" signal 3)))
    // Neomacs:   OK (signal 3 (("quit\n" signal 3)))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (let ((proc (make-process
               :name "probe-quit-sent"
               :command '("/bin/sh" "-c" "read line")
               :connection-type 'pipe
               :sentinel (lambda (p e)
                           (push (list e
                                       (process-status p)
                                       (process-exit-status p))
                                 log)))))
    (set-process-query-on-exit-flag proc nil)
    (quit-process proc)
    (let ((i 0))
      (while (and (process-live-p proc) (< i 20))
        (accept-process-output proc 0.05)
        (setq i (1+ i))))
    (let ((j 0))
      (while (and (null log) (< j 20))
        (accept-process-output proc 0.05)
        (setq j (1+ j))))
    (prog1 (list (process-status proc)
                 (process-exit-status proc)
                 (nreverse log))
      (when (process-live-p proc) (delete-process proc)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_stop_continue_delete_process_sentinels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (signal 9 ((\"run\" run 0) (\"killed\\n\" signal 9)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (signal 9 (("run" run 0) ("killed\n" signal 9)))
    // Neomacs:   OK (signal 9 nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (let ((proc (make-process
               :name "probe-stop-cont-sent2"
               :command '("/bin/sh" "-c" "read line")
               :connection-type 'pipe
               :sentinel (lambda (p e)
                           (push (list e
                                       (process-status p)
                                       (process-exit-status p))
                                 log)))))
    (set-process-query-on-exit-flag proc nil)
    (stop-process proc)
    (let ((i 0))
      (while (and (< i 10) (< (length log) 1))
        (accept-process-output proc 0.05)
        (setq i (1+ i))))
    (continue-process proc)
    (let ((i 0))
      (while (and (< i 10) (< (length log) 2))
        (accept-process-output proc 0.05)
        (setq i (1+ i))))
    (delete-process proc)
    (let ((i 0))
      (while (and (< i 20) (< (length log) 3))
        (accept-process-output proc 0.05)
        (setq i (1+ i))))
    (list (process-status proc)
          (process-exit-status proc)
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_execute_kbd_macro_command_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abc\" nil nil \"\" [] [])""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("abc" nil nil "" [] [])
    // Neomacs:   OK ("abc" nil nil "c" [99] [97 98 99])
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((executing-kbd-macro nil)
        (last-kbd-macro nil))
    (execute-kbd-macro (kbd "a b c"))
    (list (buffer-string)
          last-kbd-macro
          executing-kbd-macro
          (this-command-keys)
          (this-command-keys-vector)
          (recent-keys))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_call_last_kbd_macro_from_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"xy\" \"xy\" nil \"\")""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("xy" "xy" nil "")
    // Neomacs:   ERR (error "No keyboard macro has been defined")
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((executing-kbd-macro nil)
        (last-kbd-macro nil))
    (setq last-kbd-macro (kbd "x y"))
    (call-last-kbd-macro nil)
    (list (buffer-string)
          last-kbd-macro
          executing-kbd-macro
          (this-command-keys))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_help_window_return_message_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Type C-x 1 to delete the help window, C-M-v to scroll help.\\n\"""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK "Type C-x 1 to delete the help window, C-M-v to scroll help.\n"
    // Neomacs:   OK #("Type C-x 1 to delete the help window, ESC C-v to scroll help.\n" ...)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((message-log-max t)
      (help-window-select nil))
  (with-current-buffer (get-buffer-create "*Messages*")
    (let ((inhibit-read-only t))
      (erase-buffer)))
  (with-help-window "*probe-help-msg*"
    (princ "help"))
  (prog1 (with-current-buffer "*Messages*" (buffer-string))
    (when (get-buffer "*probe-help-msg*")
      (kill-buffer "*probe-help-msg*"))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_help_window_selected_message_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Type q to delete help window, C-v to scroll help.\\n\"""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK "Type q to delete help window, C-v to scroll help.\n"
    // Neomacs:   OK #("Type q to delete help window, C-v to scroll help.\n" ...)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((message-log-max t)
      (help-window-select t))
  (with-current-buffer (get-buffer-create "*Messages*")
    (let ((inhibit-read-only t))
      (erase-buffer)))
  (with-help-window "*probe-help-msg2*"
    (princ "help"))
  (prog1 (with-current-buffer "*Messages*" (buffer-string))
    (when (get-buffer "*probe-help-msg2*")
      (kill-buffer "*probe-help-msg2*"))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_substitute_command_keys_meta_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"C-g\" 0 3 (font-lock-face help-key-binding face help-key-binding)) #(\"Press ‘C-h’ then ‘C-M-v’\" 7 10 (font-lock-face help-key-binding face help-key-binding) 18 23 (font-lock-face help-key-binding face help-key-binding)) [134217750] \"C-M-v\")""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (... #("Press ‘C-h’ then ‘C-M-v’" ...) [134217750] "C-M-v")
    // Neomacs:   OK (... #("Press ‘C-h’ then ‘ESC C-v’" ...) [27 22] "ESC C-v")
    crate::common::assert_oracle_parity_expect(
        r##"
(list (substitute-command-keys "\\[keyboard-quit]")
      (substitute-command-keys "Press `\\[help-command]' then `\\[scroll-other-window]'")
      (where-is-internal 'scroll-other-window nil t)
      (key-description (where-is-internal 'scroll-other-window nil t)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_help_key_description_escape_meta() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"C-M-v\" 0 5 (font-lock-face help-key-binding face help-key-binding)) #(\"C-M-v\" 0 5 (font-lock-face help-key-binding face help-key-binding)) #(\"C-x 1\" 0 5 (font-lock-face help-key-binding face help-key-binding)) #(\"q\" 0 1 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (#("C-M-v" ...) #("C-M-v" ...) #("C-x 1" ...) #("q" ...))
    // Neomacs:   OK (#("C-M-v" ...) #("ESC C-v" ...) #("C-x 1" ...) #("q" ...))
    crate::common::assert_oracle_parity_expect(
        r##"
(list (help-key-description (kbd "C-M-v") nil)
      (help-key-description (kbd "ESC C-v") nil)
      (help-key-description (kbd "C-x 1") nil)
      (help-key-description (kbd "q") nil))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_escape_meta_key_description_canonicalization() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"M-a\" \"M-a\" \"C-M-a\" \"C-M-a\" \"M-a\" \"C-M-a\" (meta) 97)""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("M-a" "M-a" "C-M-a" "C-M-a" "M-a" "C-M-a" (meta) 97)
    // Neomacs:   OK ("M-a" "ESC a" "C-M-a" "ESC C-a" "M-a" "C-M-a" (meta) 97)
    crate::common::assert_oracle_parity_expect(
        r##"
(list (key-description (kbd "M-a"))
      (key-description (kbd "ESC a"))
      (key-description (kbd "C-M-a"))
      (key-description (kbd "ESC C-a"))
      (single-key-description ?\M-a)
      (single-key-description ?\C-\M-a)
      (event-modifiers ?\M-a)
      (event-basic-type ?\M-a))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_meta_command_lookup_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([134217825] [27 97] [134217729] [27 1] [134217848] \"M-x\")""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ([134217825] [27 97] [134217729] [27 1] [134217848] "M-x")
    // Neomacs:   OK ([134217825] [27 97] [134217729] [27 1] [27 120] "ESC x")
    crate::common::assert_oracle_parity_expect(
        r##"
(list (read-kbd-macro "M-a" nil)
      (read-kbd-macro "ESC a" nil)
      (read-kbd-macro "C-M-a" nil)
      (read-kbd-macro "ESC C-a" nil)
      (where-is-internal 'execute-extended-command nil t)
      (key-description (where-is-internal 'execute-extended-command nil t)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_manual_escape_vector_key_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"M-x\" \"M-x\" \"ESC ESC\" \"M-ESC\" \"M-[ A\" \"M-x\" (meta) 120)""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("M-x" "M-x" "ESC ESC" "M-ESC" "M-[ A" "M-x" (meta) 120)
    // Neomacs:   OK ("ESC x" "M-x" "ESC ESC" "M-ESC" "ESC [ A" "M-x" (meta) 120)
    crate::common::assert_oracle_parity_expect(
        r##"
(list (key-description [27 120])
      (key-description [134217848])
      (key-description [27 27])
      (key-description [134217755])
      (key-description [27 91 65])
      (single-key-description 134217848)
      (event-modifiers 134217848)
      (event-basic-type 134217848))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_sparse_keymap_meta_where_is() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (mx mx [134217848] \"M-x\" [134217849] \"M-y\")""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (mx mx [134217848] "M-x" [134217849] "M-y")
    // Neomacs:   OK (mx mx [27 120] "ESC x" [27 121] "ESC y")
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map (kbd "M-x") 'mx)
  (define-key map (kbd "ESC y") 'escy)
  (list (lookup-key map (kbd "M-x"))
        (lookup-key map (kbd "ESC x"))
        (where-is-internal 'mx map t)
        (key-description (where-is-internal 'mx map t))
        (where-is-internal 'escy map t)
        (key-description (where-is-internal 'escy map t))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_substitute_command_keys_escape_meta() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"M-x\" 0 3 (font-lock-face help-key-binding face help-key-binding)) #(\"M-ESC ESC\" 0 9 (font-lock-face help-key-binding face help-key-binding)) [134217755 27] \"M-ESC ESC\")""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (#("M-x" ...) #("M-ESC ESC" ...) [134217755 27] "M-ESC ESC")
    // Neomacs:   OK (#("ESC x" ...) #("ESC ESC ESC" ...) [27 27 27] "ESC ESC ESC")
    crate::common::assert_oracle_parity_expect(
        r##"
(list (substitute-command-keys "\\[execute-extended-command]")
      (substitute-command-keys "\\[keyboard-escape-quit]")
      (where-is-internal 'keyboard-escape-quit nil t)
      (key-description (where-is-internal 'keyboard-escape-quit nil t)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_file_name_handler_operation_args_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (exists \"HANDLED\" nil (\".\" \"..\" \"alpha\") ((expand-file-name (\"/probe:/x\" nil)) (file-exists-p (\"/probe:/x\")) (expand-file-name (\"/probe:/x\" nil)) (insert-file-contents (\"/probe:/x\" nil nil nil nil)) (expand-file-name (\"/probe:/out\" nil)) (write-region (1 3 \"/probe:/out\" nil silent \"/probe:/out\" nil)) (write-data 1 3 \"/probe:/out\") (expand-file-name (\"/probe:/dir\" nil)) (directory-files (\"/probe:/dir\" nil \"a\" t nil))))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: logs expand-file-name before handled insert-file-contents,
    // write-region, and directory-files, and passes full GNU operation args.
    // Neomacs:   skips several expand-file-name handler dispatches and passes
    // truncated insert-file-contents/write-region/directory-files arg lists.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (file-name-handler-alist nil))
  (letrec ((handler
            (lambda (op &rest args)
              (push (list op args) log)
              (cond
               ((eq op 'file-exists-p) 'exists)
               ((eq op 'insert-file-contents)
                (insert "HANDLED")
                (list (car args) 7))
               ((eq op 'write-region)
                (push (list 'write-data (nth 0 args) (nth 1 args) (nth 2 args))
                      log)
                nil)
               ((eq op 'directory-files)
                '("." ".." "alpha"))
               (t
                (let ((inhibit-file-name-handlers
                       (cons handler inhibit-file-name-handlers))
                      (inhibit-file-name-operation op))
                  (apply op args)))))))
    (setq file-name-handler-alist `(("\\`/probe:" . ,handler)))
    (list (file-exists-p "/probe:/x")
          (with-temp-buffer
            (insert-file-contents "/probe:/x")
            (buffer-string))
          (with-temp-buffer
            (insert "abc")
            (write-region 1 3 "/probe:/out" nil 'silent))
          (directory-files "/probe:/dir" nil "a" t)
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_thread_join_error_delivery() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil (arith-error \"bad\"))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (nil nil (arith-error "bad"))
    // Neomacs:   OK ((join-error arith-error ("bad")) nil (arith-error "bad"))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((th (make-thread
           (lambda () (signal 'arith-error '("bad")))
           "probe-thread-error")))
  (let ((join-result
         (condition-case err
             (thread-join th)
           (error (list 'join-error (car err) (cdr err))))))
    (list join-result
          (thread-live-p th)
          (thread-last-error th))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_network_client_open_delete_sentinels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (listen closed ((server-sentinel \"open from 127.0.0.1\\n\" open) (client-sentinel \"deleted\\n\" closed)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (listen closed ((server-sentinel "open from 127.0.0.1\n" open)
    //                               (client-sentinel "deleted\n" closed)))
    // Neomacs:   OK (listen closed ((client-sentinel "open\n" open)
    //                               (server-sentinel "open from 127.0.0.1\n" open)))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      server
      client)
  (unwind-protect
      (progn
        (setq server
              (make-network-process
               :name "probe-net-sentinel-server"
               :server t
               :host 'local
               :service t
               :noquery t
               :sentinel (lambda (p e)
                           (push (list 'server-sentinel
                                       e
                                       (process-status p))
                                 log))))
        (setq client
              (make-network-process
               :name "probe-net-sentinel-client"
               :host 'local
               :service (process-contact server :service)
               :noquery t
               :sentinel (lambda (p e)
                           (push (list 'client-sentinel
                                       e
                                       (process-status p))
                                 log))))
        (let ((i 0))
          (while (and (< i 10) (null log))
            (accept-process-output nil 0.05)
            (setq i (1+ i))))
        (delete-process client)
        (let ((i 0))
          (while (and (< i 20) (< (length log) 2))
            (accept-process-output nil 0.05)
            (setq i (1+ i))))
        (list (process-status server)
              (process-status client)
              (nreverse log)))
    (when (processp client) (delete-process client))
    (when (processp server) (delete-process server))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_load_history_defvar_recording() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t 42 9 ((provide . probe-load-history-feature) (defun . probe-load-history-fn) probe-load-history-var) (probe-load-history-var))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: loaded file entry records (provide . feature),
    //            (defun . function), and the defvar symbol.
    // Neomacs:   loaded file entry records provide/defun but omits defvar.
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((contents
        ";;; -*- lexical-binding: t -*-
(provide 'probe-load-history-feature)
(defun probe-load-history-fn () 42)
(defvar probe-load-history-var 9)
")
       (file (make-temp-file "neo-load-history" nil ".el" contents))
       (load-history nil))
  (unwind-protect
      (progn
        (load file nil t nil t)
        (let ((entry (cdr (assoc file load-history))))
          (list (featurep 'probe-load-history-feature)
                (probe-load-history-fn)
                probe-load-history-var
                entry
                (memq 'probe-load-history-var entry))))
    (when (get-file-buffer file)
      (kill-buffer (get-file-buffer file)))
    (delete-file file)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_case_table_search_and_conversion_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (120 121 \"xAx\" \"xay\" 0 0 2 121 121)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (120 121 "xAx" "xay" 0 0 2 121 121)
    // Neomacs:   OK (88 121 "XAY" "xay" 0 0 4 121 121)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((table (copy-case-table (standard-case-table))))
  (set-case-syntax-pair ?x ?y table)
  (with-temp-buffer
    (set-case-table table)
    (insert "x y X Y")
    (let ((case-fold-search t))
      (list (upcase ?x)
            (downcase ?y)
            (upcase "xay")
            (downcase "XAY")
            (string-match-p "x" "y")
            (string-match-p "y" "x")
            (progn
              (goto-char 1)
              (search-forward "y" nil t))
            (aref (current-case-table) ?x)
            (aref (current-case-table) ?y)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_visited_file_modtime_set_clear_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((1 2 3 4000) nil) 0 t nil)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (((1 2 3 4000) nil) 0 t nil)
    // Neomacs:   OK (((0 1 0 0) nil) (0 1 0 0) nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((file (make-temp-file "neo-visit-explicit" nil ".txt" "abc")))
  (unwind-protect
      (let ((buf (find-file-noselect file)))
        (with-current-buffer buf
          (set-visited-file-modtime '(1 2 3 4000))
          (let ((explicit (list (visited-file-modtime)
                                (verify-visited-file-modtime buf))))
            (clear-visited-file-modtime)
            (list explicit
                  (visited-file-modtime)
                  (verify-visited-file-modtime buf)
                  (buffer-modified-p)))))
    (when (get-file-buffer file)
      (kill-buffer (get-file-buffer file)))
    (delete-file file)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_set_visited_file_name_update_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 nil ((update 0 0)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (0 0 nil ((update 0 0))) ; normalized temp-name matches
    // Neomacs:   OK (0 0 nil nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (file-a (make-temp-file "neo-set-vfn-a" nil ".txt" "a"))
      (file-b (make-temp-file "neo-set-vfn-b" nil ".txt" "b")))
  (unwind-protect
      (let ((buf (find-file-noselect file-a)))
        (with-current-buffer buf
          (let ((buffer-list-update-hook
                 (list (lambda ()
                         (push (list 'update
                                     (file-name-nondirectory
                                      (or buffer-file-name ""))
                                     (buffer-name))
                               log)))))
            (set-visited-file-name file-b nil t)
            (list (string-match-p
                   "\\`neo-set-vfn-b.*\\.txt\\'"
                   (file-name-nondirectory buffer-file-name))
                  (string-match-p "\\`neo-set-vfn-b.*\\.txt\\'" (buffer-name))
                  (buffer-modified-p)
                  (mapcar (lambda (entry)
                            (list (car entry)
                                  (string-match-p
                                   "\\`neo-set-vfn-b.*\\.txt\\'"
                                   (cadr entry))
                                  (string-match-p
                                   "\\`neo-set-vfn-b.*\\.txt\\'"
                                   (caddr entry))))
                          (nreverse log))))))
    (when (get-file-buffer file-a)
      (kill-buffer (get-file-buffer file-a)))
    (when (get-file-buffer file-b)
      (kill-buffer (get-file-buffer file-b)))
    (delete-file file-a)
    (delete-file file-b)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_keymap_parent_accessible_keymaps_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (parent-cmd [3 112] ([] [3] [3]))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (parent-cmd [3 112] ([] [3] [3]))
    // Neomacs:   OK (parent-cmd [3 112] ([] [3]))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((parent (make-sparse-keymap))
      (child (make-sparse-keymap)))
  (define-key parent (kbd "C-c p") 'parent-cmd)
  (define-key child (kbd "C-c c") 'child-cmd)
  (set-keymap-parent child parent)
  (list (lookup-key child (kbd "C-c p"))
        (where-is-internal 'parent-cmd child t)
        (mapcar #'car (accessible-keymaps child))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_keymap_remap_where_is_parent_child_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (child-new nil nil child-new)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (child-new nil nil child-new)
    // Neomacs:   OK (child-new [remap old] [remap old] child-new)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((parent (make-sparse-keymap))
      (child (make-sparse-keymap)))
  (define-key parent [remap old] 'parent-new)
  (define-key child [remap old] 'child-new)
  (set-keymap-parent child parent)
  (list (command-remapping 'old nil (list child))
        (where-is-internal 'parent-new child t)
        (where-is-internal 'child-new child t)
        (lookup-key child [remap old])))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_mutex_lock_blocks_other_thread() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((t (start)) done nil (got start) \"probe-mutex-block\")""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((t (start)) done nil (got start) "probe-mutex-block")
    // Neomacs:   OK ((nil (got start)) done nil (got start) "probe-mutex-block")
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-mutex "probe-mutex-block"))
      (log nil))
  (mutex-lock m)
  (let ((th (make-thread
             (lambda ()
               (push 'start log)
               (mutex-lock m)
               (push 'got log)
               'done)
             "mutex-wait")))
    (let ((i 0))
      (while (and (< i 20) (null log))
        (sleep-for 0.01)
        (setq i (1+ i))))
    (let ((before (list (thread-live-p th) log)))
      (mutex-unlock m)
      (let ((res (thread-join th)))
        (list before
              res
              (thread-live-p th)
              log
              (mutex-name m))))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_thread_dynamic_binding_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (global \"*scratch*\" local)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (global "*scratch*" local)
    // Neomacs:   OK (main "*scratch*" local)
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar probe-thread-dyn 'global)
  (let ((buf (get-buffer-create " *probe-thread-buf*"))
        (probe-thread-dyn 'main))
    (unwind-protect
        (progn
          (with-current-buffer buf
            (setq-local probe-thread-dyn 'local))
          (let ((th
                 (make-thread
                  (lambda ()
                    (list probe-thread-dyn
                          (buffer-name)
                          (with-current-buffer buf probe-thread-dyn)))
                  "dyn-thread")))
            (thread-join th)))
      (when (buffer-live-p buf)
        (kill-buffer buf)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_all_threads_includes_live_worker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((nil \"probe-list-thread\") (nil) (run))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((nil "probe-list-thread") (nil) (run))
    // Neomacs:   OK ((nil) (nil) (run))
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (let ((th (make-thread
             (lambda ()
               (push 'run log)
               (sleep-for 0.1)
               'done)
             "probe-list-thread")))
    (let ((names-while-live (mapcar #'thread-name (all-threads))))
      (thread-join th)
      (list names-while-live
            (mapcar #'thread-name (all-threads))
            log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_load_history_defcustom_recording() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((require defface probe-lh-custom defun provide) (probe-lh-custom (defun . probe-lh-alias) (provide . probe-lh)) (defface . probe-lh-face) ((require . custom) (defface . probe-lh-face) probe-lh-custom (defun . probe-lh-alias) (provide . probe-lh)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((require defface probe-lh-custom defun provide) (probe-lh-custom ...))
    // Neomacs:   OK ((require defface defun provide) nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((contents
        ";;; -*- lexical-binding: t -*-
(require 'custom)
(defgroup probe-lh nil \"\" :group 'emacs)
(defface probe-lh-face '((t (:weight bold))) \"\")
(defcustom probe-lh-custom 3 \"\" :type 'integer :group 'probe-lh)
(defalias 'probe-lh-alias 'ignore)
(provide 'probe-lh)
")
       (file (make-temp-file "neo-loadhist-custom" nil ".el" contents))
       (load-history nil))
  (unwind-protect
      (progn
        (load file nil t nil t)
        (let ((entry (cdr (assoc file load-history))))
          (list (mapcar (lambda (x)
                          (if (consp x) (car x) x))
                        entry)
                (memq 'probe-lh-custom entry)
                (assq 'defface entry)
                entry)))
    (delete-file file)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_case_table_word_casing_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[ello World\" \"[abc Def\" \"[abc\" \"[hello]\" \"{foo} Bar\" \"{foo Bar}\" t)""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("[ello World" "[abc Def" "[abc" "[hello]" "{foo} Bar" "{foo Bar}" t)
    // Neomacs:   OK ("[Ello World" "[Abc Def" "]Abc" "[Hello]" "{Foo} Bar" "{Foo Bar}" nil)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((bracket-table (copy-case-table (standard-case-table)))
      (brace-table (copy-case-table (standard-case-table))))
  (set-case-syntax-pair ?\[ ?\] bracket-table)
  (set-case-syntax-pair ?\{ ?\} brace-table)
  (list
   (with-temp-buffer
     (set-case-table bracket-table)
     (capitalize "[ello world"))
   (with-temp-buffer
     (set-case-table bracket-table)
     (upcase-initials "[abc def"))
   (with-temp-buffer
     (set-case-table bracket-table)
     (capitalize "]abc"))
   (with-temp-buffer
     (set-case-table bracket-table)
     (insert "[hello]")
     (capitalize-region (point-min) (point-max))
     (buffer-string))
   (with-temp-buffer
     (set-case-table brace-table)
     (insert "{foo} bar")
     (goto-char (point-min))
     (capitalize-word 2)
     (buffer-string))
   (with-temp-buffer
     (set-case-table brace-table)
     (insert "{foo bar}")
     (upcase-initials-region (point-min) (point-max))
     (buffer-string))
   (with-temp-buffer
     (set-case-table brace-table)
     (char-equal ?\{ ?\}))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_process_attributes_running_child_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"/bin/sh -c sleep\\\\ 0.2\" \"\")""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (t "/bin/sh -c sleep\\ 0.2" "")
    // Neomacs:   OK (nil "/bin/sh -c sleep 0.2" "pipe:[...]")
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process
             :name "probe-attrs-child"
             :command '("/bin/sh" "-c" "sleep 0.2")
             :connection-type 'pipe)))
  (unwind-protect
      (let* ((attrs (process-attributes (process-id proc)))
             (args (cdr (assq 'args attrs)))
             (ttname (cdr (assq 'ttname attrs))))
        (list (process-running-child-p proc)
              args
              (if (and (stringp ttname)
                       (string-match-p "\\`pipe:" ttname))
                  "pipe:[...]"
                ttname)))
    (when (process-live-p proc)
      (delete-process proc))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_unibyte_search_raw_byte_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 4 (3 3 3 1 169))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (nil nil 4 (3 3 3 1 169)) ; raw-byte search-forward finds
    //            the 0xA9 byte at position 2 (point 3) inside the é sequence.
    // Neomacs:   OK (nil nil 4 (5 3 3 1 169)) ; search-forward skips the byte
    //            embedded in é and only matches the standalone trailing byte.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 195 169 65 169))
  (let ((pat (unibyte-string 169)))
    (list enable-multibyte-characters
          (multibyte-string-p (buffer-string))
          (string-bytes (buffer-string))
          (list (progn
                  (goto-char (point-min))
                  (search-forward pat nil t))
                (progn
                  (goto-char (point-min))
                  (re-search-forward pat nil t))
                (progn
                  (goto-char (point-min))
                  (skip-chars-forward (unibyte-string 195 169))
                  (point))
                (string-match pat (buffer-string))
                (char-after 4)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_unibyte_multibyte_search_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 nil nil nil)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (3 nil nil nil) ; a multibyte char never matches in a
    //            unibyte buffer, so search-forward of (string ?é) returns nil.
    // Neomacs:   OK (3 3 nil nil)   ; search-forward incorrectly matches the
    //            multibyte char against the raw é byte sequence.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 195 169 65 169))
  (let ((raw-pat (unibyte-string 195 169)))
    (list (progn
            (goto-char (point-min))
            (search-forward raw-pat nil t))
          (progn
            (goto-char (point-min))
            (search-forward (string ?é) nil t))
          (progn
            (goto-char (point-min))
            (re-search-forward (string ?é) nil t))
          (string-match (string ?é) (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_signal_process_signal_name_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 signal 15 ((\"terminated\\n\" signal 15)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (0 signal 15 (("terminated\n" signal 15)))
    // Neomacs:   OK ((err "Undefined signal name TERM") run 0 nil)
    // GNU accepts signal-name symbols (TERM) for signal-process; Neomacs only
    // accepts integer signal numbers and errors on the symbol, leaving the
    // process running with no sentinel event.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process
             :name "probe-signal-name"
             :command '("/bin/sh" "-c" "sleep 5")
             :connection-type 'pipe)))
  (set-process-query-on-exit-flag proc nil)
  (let ((log nil))
    (set-process-sentinel
     proc
     (lambda (p e)
       (push (list e (process-status p) (process-exit-status p)) log)))
    (let ((ret (condition-case err
                   (signal-process proc 'TERM)
                 (error (list 'err (cadr err))))))
      (let ((i 0))
        (while (and (< i 40) (process-live-p proc))
          (accept-process-output proc 0.05)
          (setq i (1+ i))))
      (let ((j 0))
        (while (and (< j 20) (null log))
          (accept-process-output proc 0.05)
          (setq j (1+ j))))
      (prog1 (list ret
                   (process-status proc)
                   (process-exit-status proc)
                   (nreverse log))
        (when (process-live-p proc)
          (delete-process proc))))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_insert_and_inherit_full_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"AXB\" 0 3 (face bold rear-nonsticky nil)) (face bold rear-nonsticky nil) (face bold rear-nonsticky nil))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (#("AXB" 0 3 (face bold rear-nonsticky nil))
    //                (face bold rear-nonsticky nil) (face bold rear-nonsticky nil))
    // Neomacs:   OK (#("AXB" 0 1 (face bold rear-nonsticky nil) 1 2 (face bold)
    //                2 3 (face bold rear-nonsticky nil)) (face bold)
    //                (face bold rear-nonsticky nil))
    // GNU inherits the full property plist (including rear-nonsticky nil) for
    // the inserted char and coalesces the spans; Neomacs inherits only `face`
    // and leaves a fragmented interval with a partial plist.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert (propertize "AB" 'face 'bold 'rear-nonsticky nil))
  (goto-char 2)
  (insert-and-inherit "X")
  (list (buffer-string)
        (text-properties-at 2)
        (text-properties-at 3)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_self_insert_command_inherits_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"abcXdef\" 0 4 (face bold) 4 7 (face italic)) (face bold))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (#("abcXdef" 0 4 (face bold) 4 7 (face italic)) (face bold))
    // Neomacs:   OK (#("abcXdef" 0 3 (face bold) 4 7 (face italic)) nil)
    // self-insert-command inherits text properties from the preceding char in
    // GNU (X gets `face bold` and the span coalesces); Neomacs inserts X with
    // no inherited properties, leaving an unpropertized gap.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert (propertize "abc" 'face 'bold))
  (insert (propertize "def" 'face 'italic))
  (goto-char 4)
  (self-insert-command 1 ?X)
  (list (buffer-string)
        (text-properties-at 4)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_encode_coding_string_dos_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"a\\r\\nb\" 4 (97 13 10 98) (120 13 10 121 13 10 122))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("a\r\nb" 4 (97 13 10 98) (120 13 10 121 13 10 122))
    // Neomacs:   OK ("" 0 nil (120 13 10 121 13 10 122))
    // encode-coding-string with the bare `dos` coding system produces empty
    // output in Neomacs, while GNU correctly applies CRLF EOL conversion.
    // The explicit utf-8-dos variant works in both, isolating the `dos`
    // (undecided base + dos EOL) alias as the divergent path.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dos-encoded (encode-coding-string "a\nb" 'dos))
      (utf8-dos-encoded (encode-coding-string "x\ny\nz" 'utf-8-dos)))
  (list dos-encoded
        (string-bytes dos-encoded)
        (append dos-encoded nil)
        (append utf8-dos-encoded nil)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_encode_coding_string_mac_unix_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((97 13 98) (97 10 98) (104 105) (97 10 98) (97 13 98))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((97 13 98) (97 10 98) (104 105) (97 10 98) (97 13 98))
    // Neomacs:   OK (nil nil nil (97 10 98) (97 13 98))
    // encode-coding-string with the bare mac/unix EOL aliases returns empty
    // output in Neomacs (even for a newline-free string like "hi"), while GNU
    // applies the correct EOL conversion. The fully-qualified latin-1-unix and
    // utf-8-mac coding systems work in both, isolating the EOL-only aliases.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (append (encode-coding-string "a\nb" 'mac) nil)
      (append (encode-coding-string "a\nb" 'unix) nil)
      (append (encode-coding-string "hi" 'unix) nil)
      (append (encode-coding-string "a\nb" 'latin-1-unix) nil)
      (append (encode-coding-string "a\nb" 'utf-8-mac) nil))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_decode_coding_string_eol_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((97 10 98) (97 10 98) undecided-dos undecided-mac)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((97 10 98) (97 10 98) undecided-dos undecided-mac)
    // Neomacs:   OK ((97 13 10 98) (97 13 98) undecided undecided)
    // decode-coding-string with bare dos/mac aliases does not collapse CRLF/CR
    // to LF in Neomacs (raw bytes retained), and detect-coding-string fails to
    // report the dos/mac EOL variant, returning plain `undecided`.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (append (decode-coding-string "a\r\nb" 'dos) nil)
      (append (decode-coding-string "a\rb" 'mac) nil)
      (detect-coding-string "a\r\nb\r\n" t)
      (detect-coding-string "a\rb\rc" t))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_insert_file_contents_unix_keeps_cr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\\nb\\nc\" (97 13 10 98 13 10 99))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("a\nb\nc" (97 13 10 98 13 10 99))
    // Neomacs:   OK ("a\nb\nc" (97 10 98 10 99))
    // Reading a CRLF file with coding-system-for-read 'dos collapses CRLF->LF
    // in both. With the bare 'unix alias GNU performs no EOL conversion and
    // keeps the raw CR bytes, while Neomacs strips them.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((file (make-temp-file "neo-crlf-read" nil ".txt")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'binary))
          (with-temp-buffer
            (set-buffer-multibyte nil)
            (insert "a\r\nb\r\nc")
            (write-region (point-min) (point-max) file nil 'silent)))
        (let ((read-as-dos
               (with-temp-buffer
                 (let ((coding-system-for-read 'dos))
                   (insert-file-contents file))
                 (buffer-string)))
              (read-as-unix
               (with-temp-buffer
                 (let ((coding-system-for-read 'unix))
                   (insert-file-contents file))
                 (append (buffer-string) nil))))
          (list read-as-dos read-as-unix)))
    (delete-file file)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_missing_program_error_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((file-missing 3 \"Doing vfork\") (file-missing 4 \"Searching for program\"))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((file-missing 3 "Doing vfork")
    //                (file-missing 4 "Searching for program"))
    // Neomacs:   OK ((file-missing 3 "Searching for program")
    //                (file-missing 3 "Searching for program"))
    // For a missing program GNU's make-process error message is "Doing vfork"
    // (Neomacs uses "Searching for program"), and GNU's call-process error
    // data has 4 elements (including the errno string) while Neomacs's has
    // only 3.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((make-err
       (condition-case err
           (make-process
            :name "probe-missing-prog"
            :command '("/nonexistent/neo-probe-xyz")
            :connection-type 'pipe)
         (error err)))
      (call-err
       (condition-case err
           (call-process "/nonexistent/neo-probe-xyz" nil nil nil)
         (error err))))
  (list (list (car make-err) (length make-err) (nth 1 make-err))
        (list (car call-err) (length call-err) (nth 1 call-err))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_start_process_missing_program_deferred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (process exit 127 ((\"exited abnormally with code 127\\n\" exit 127)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (process exit 127 (("exited abnormally with code 127\n" exit 127)))
    // Neomacs:   OK (err file-missing 2)
    // start-process with a missing program returns a live process in GNU and
    // defers the failure to an asynchronous sentinel (status exit, code 127);
    // Neomacs instead raises a synchronous file-missing error.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (condition-case err
      (let ((p (start-process
                "probe-start-missing"
                nil
                "/nonexistent/neo-probe-xyz")))
        (set-process-sentinel
         p
         (lambda (pr e)
           (push (list e (process-status pr) (process-exit-status pr)) log)))
        (let ((i 0))
          (while (and (< i 30)
                      (or (eq (process-status p) 'run) (null log)))
            (accept-process-output nil 0.05)
            (setq i (1+ i))))
        (list 'process
              (process-status p)
              (process-exit-status p)
              (nreverse log)))
    (error (list 'err (car err) (length (cdr err))))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_replace_region_contents_function_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"aXYef\" \"aZZef\")""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("aXYef" "aZZef")
    // Neomacs:   OK ((err wrong-type-argument) (err wrong-type-argument))
    // replace-region-contents accepts a function returning the replacement
    // string/buffer in GNU; Neomacs rejects a callable replacement with
    // wrong-type-argument (expects a string/buffer/vector directly).
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (insert "abcdef")
   (condition-case err
       (progn
         (replace-region-contents 2 5 (lambda () "XY"))
         (buffer-string))
     (error (list 'err (car err)))))
 (with-temp-buffer
   (insert "abcdef")
   (condition-case err
       (let ((src (generate-new-buffer " *probe-rrc-src*")))
         (with-current-buffer src (insert "ZZ"))
         (prog1 (progn
                  (replace-region-contents 2 5 (lambda () src))
                  (buffer-string))
           (kill-buffer src)))
     (error (list 'err (car err))))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_write_region_bare_eol_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((97 10 98) (97 13 10 98) (97 13 98) (97 13 10 98))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((97 10 98) (97 13 10 98) (97 13 98) (97 13 10 98))
    // Neomacs:   OK (nil nil nil (97 13 10 98))
    // write-region with bare unix/dos/mac EOL aliases writes empty output in
    // Neomacs. GNU writes the expected LF/CRLF/CR bytes. Fully-qualified
    // utf-8-dos works in both, isolating the bare EOL alias path.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((write-with-coding
       (lambda (coding text)
         (let ((file (make-temp-file "neo-write-eol" nil ".txt")))
           (unwind-protect
               (progn
                 (let ((coding-system-for-write coding))
                   (with-temp-buffer
                     (insert text)
                     (write-region (point-min) (point-max) file nil 'silent)))
                 (with-temp-buffer
                   (set-buffer-multibyte nil)
                   (let ((coding-system-for-read 'binary))
                     (insert-file-contents file))
                   (append (buffer-string) nil)))
             (delete-file file))))))
  (list (funcall write-with-coding 'unix "a\nb")
        (funcall write-with-coding 'dos "a\nb")
        (funcall write-with-coding 'mac "a\nb")
        (funcall write-with-coding 'utf-8-dos "a\nb")))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_insert_char_inherit_property_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"AXB\" 0 3 (face bold rear-nonsticky nil)) (face bold rear-nonsticky nil) 3) (#(\"AXB\" 0 3 (face bold rear-nonsticky nil)) (face bold rear-nonsticky nil)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((#("AXB" 0 3 (face bold rear-nonsticky nil))
    //                 (face bold rear-nonsticky nil) 3)
    //                (#("AXB" 0 3 (face bold rear-nonsticky nil))
    //                 (face bold rear-nonsticky nil)))
    // Neomacs:   OK ((#("AXB" 0 1 ... 1 2 (face bold) 2 3 ...)
    //                 (face bold) 3)
    //                (#("AXB" 0 1 ... 1 2 (face bold) 2 3 ...)
    //                 (face bold)))
    // GNU's insert-before-markers-and-inherit and insert-char INHERIT both
    // inherit the full text-property plist and coalesce intervals; Neomacs
    // inherits only `face`, leaving fragmented partial-property spans.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (insert (propertize "AB" 'face 'bold 'rear-nonsticky nil))
   (let ((m (copy-marker 2)))
     (goto-char 2)
     (insert-before-markers-and-inherit "X")
     (list (buffer-string)
           (text-properties-at 2)
           (marker-position m))))
 (with-temp-buffer
   (insert (propertize "AB" 'face 'bold 'rear-nonsticky nil))
   (goto-char 2)
   (insert-char ?X 1 t)
   (list (buffer-string)
         (text-properties-at 2))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_thread_signal_condition_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil (join-err arith-error (\"bad\")) nil nil ((caught arith-error (\"bad\"))))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (nil (join-err arith-error ("bad")) nil nil
    //                ((caught arith-error ("bad"))))
    // Neomacs:   OK (nil (join-err arith-error ("bad")) nil nil nil)
    // A signalled arith-error reaches the worker's condition-case handler in
    // GNU (the handler logs `caught`), but Neomacs still makes thread-join
    // report the error without running the worker's matching condition handler.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (let ((th (make-thread
             (lambda ()
               (condition-case err
                   (progn
                     (sleep-for 1)
                     'done)
                 (arith-error
                  (push (list 'caught (car err) (cdr err)) log)
                  'caught)))
             "probe-thread-signal-condition")))
    (let ((signal-result
           (condition-case err
               (thread-signal th 'arith-error '("bad"))
             (error (list 'signal-err (car err) (cdr err))))))
      (let ((join-result
             (condition-case err
                 (thread-join th)
               (error (list 'join-err (car err) (cdr err))))))
        (list signal-result
              join-result
              (thread-live-p th)
              (thread-last-error th)
              log)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_standard_display_ascii_table_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((char-table [91 65 93]) (char-table [66 66]))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((char-table [91 65 93]) (char-table [66 66]))
    // Neomacs:   OK ((err wrong-type-argument arrayp) (char-table nil))
    // standard-display-ascii creates standard-display-table when nil and sets
    // the requested character in GNU. Neomacs errors when the table is nil, and
    // even with a preexisting display table it leaves the character entry nil.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((standard-display-table nil))
   (condition-case err
       (progn
         (standard-display-ascii ?A "[A]")
         (list (type-of standard-display-table)
               (aref standard-display-table ?A)))
     (error (list 'err (car err) (cadr err)))))
 (let ((standard-display-table (make-display-table)))
   (standard-display-ascii ?B "BB")
   (list (type-of standard-display-table)
         (aref standard-display-table ?B))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_standard_display_graphic_table_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((char-table [0]) (char-table [1]))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((char-table [0]) (char-table [1]))
    // Neomacs:   OK ((err wrong-type-argument arrayp) (char-table nil))
    // standard-display-graphic mirrors the standard-display-ascii divergence:
    // GNU creates standard-display-table when nil and mutates existing tables;
    // Neomacs errors when nil and leaves an existing table entry unset.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((standard-display-table nil))
   (condition-case err
       (progn
         (standard-display-graphic ?A ?G)
         (list (type-of standard-display-table)
               (aref standard-display-table ?A)))
     (error (list 'err (car err) (cadr err)))))
 (let ((standard-display-table (make-display-table)))
   (standard-display-graphic ?B ?H)
   (list (type-of standard-display-table)
         (aref standard-display-table ?B))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_standard_display_default_g1_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((char-table nil) (char-table nil) (char-table [0]) (char-table nil nil))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((char-table nil) (char-table nil) (char-table [0]) (char-table nil nil))
    // Neomacs:   OK ((err wrong-type-argument arrayp) (char-table nil) (char-table nil)
    //                (err wrong-type-argument arrayp))
    // More standard-display helpers share the table-creation/mutation bug:
    // standard-display-default and standard-display-8bit create a table when
    // standard-display-table is nil in GNU but error in Neomacs; standard-display-g1
    // mutates an existing table in GNU but leaves the entry nil in Neomacs.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((standard-display-table nil))
   (condition-case err
       (progn
         (standard-display-default ?A ?Z)
         (list (type-of standard-display-table)
               (aref standard-display-table ?A)))
     (error (list 'err (car err) (cadr err)))))
 (let ((standard-display-table (make-display-table)))
   (standard-display-default ?B ?Y)
   (list (type-of standard-display-table)
         (aref standard-display-table ?B)))
 (let ((standard-display-table (make-display-table)))
   (standard-display-g1 ?C ?X)
   (list (type-of standard-display-table)
         (aref standard-display-table ?C)))
 (let ((standard-display-table nil))
   (condition-case err
       (progn
         (standard-display-8bit 160 161)
         (list (type-of standard-display-table)
               (aref standard-display-table 160)
               (aref standard-display-table 161)))
     (error (list 'err (car err) (cadr err))))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_interpreter_auto_mode_change_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (text-mode (change text))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (text-mode (change text))
    // Neomacs:   OK (text-mode (text))
    // set-auto-mode through interpreter-mode-alist should run
    // change-major-mode-hook before the destination mode hook; Neomacs skips
    // change-major-mode-hook on this auto-mode path.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((interpreter-mode-alist '(("probeinterp" . text-mode)))
      (auto-mode-alist nil)
      (log nil))
  (add-hook 'change-major-mode-hook (lambda () (push 'change log)))
  (add-hook 'text-mode-hook (lambda () (push 'text log)))
  (with-temp-buffer
    (insert "#!/usr/bin/env probeinterp\nbody")
    (setq buffer-file-name "x.unknown")
    (set-auto-mode)
    (list major-mode (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_magic_auto_mode_change_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((text-mode (change text)) (text-mode (change text)))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((text-mode (change text)) (text-mode (change text)))
    // Neomacs:   OK ((text-mode (text)) (text-mode (text)))
    // set-auto-mode through magic-mode-alist and magic-fallback-mode-alist
    // should run change-major-mode-hook before text-mode-hook. Neomacs skips
    // change-major-mode-hook on both content-based auto-mode paths.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((log nil)
       (magic-mode-alist '(("\\`PROBE-MAGIC" . text-mode)))
       (magic-fallback-mode-alist nil)
       (auto-mode-alist nil))
   (add-hook 'change-major-mode-hook (lambda () (push 'change log)))
   (add-hook 'text-mode-hook (lambda () (push 'text log)))
   (with-temp-buffer
     (insert "PROBE-MAGIC content here")
     (setq buffer-file-name "x.unknown")
     (set-auto-mode)
     (list major-mode (nreverse log))))
 (let ((log nil)
       (magic-mode-alist nil)
       (magic-fallback-mode-alist '(("\\`PROBE-FALLBACK" . text-mode)))
       (auto-mode-alist nil))
   (add-hook 'change-major-mode-hook (lambda () (push 'change log)))
   (add-hook 'text-mode-hook (lambda () (push 'text log)))
   (with-temp-buffer
     (insert "PROBE-FALLBACK content here")
     (setq buffer-file-name "x.unknown")
     (set-auto-mode)
     (list major-mode (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_normal_mode_fundamental_change_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (fundamental-mode ((change fundamental-mode) (after fundamental-mode) (change fundamental-mode) (after fundamental-mode)))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (fundamental-mode ((change fundamental-mode) (after fundamental-mode)
    //                                  (change fundamental-mode) (after fundamental-mode)))
    // Neomacs:   OK (fundamental-mode ((after fundamental-mode) (after fundamental-mode)))
    // normal-mode with no auto/magic mode match still calls fundamental-mode twice in GNU,
    // and each transition runs change-major-mode-hook before after-change-major-mode-hook.
    // Neomacs runs after-change-major-mode-hook but skips change-major-mode-hook.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (auto-mode-alist nil)
      (magic-mode-alist nil)
      (magic-fallback-mode-alist nil))
  (add-hook 'change-major-mode-hook
            (lambda () (push (list 'change major-mode) log)))
  (add-hook 'after-change-major-mode-hook
            (lambda () (push (list 'after major-mode) log)))
  (with-temp-buffer
    (insert "text")
    (setq buffer-file-name "x.unknown")
    (normal-mode t)
    (list major-mode (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_frame_terminal_type_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (terminal t \"initial_terminal\" (terminal) 0)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (terminal t "initial_terminal" (terminal) 0)
    // Neomacs:   OK (vector t "initial_terminal" (vector) 0)
    // GNU terminal objects have type `terminal`; Neomacs represents the frame
    // terminal object as a vector while otherwise passing terminal-live-p,
    // terminal-name, terminal-list membership, and terminal parameters.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((terminal (frame-terminal (selected-frame))))
  (list (type-of terminal)
        (terminal-live-p terminal)
        (terminal-name terminal)
        (mapcar #'type-of (terminal-list))
        (terminal-parameter terminal 'normal-erase-is-backspace)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_standard_display_underline_table_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((char-table [0]) (char-table [1]))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((char-table [0]) (char-table [1]))
    // Neomacs:   OK ((err wrong-type-argument arrayp) (char-table nil))
    // standard-display-underline creates standard-display-table when nil and
    // mutates existing table entries in GNU; Neomacs errors when nil and leaves
    // an existing table entry nil.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((standard-display-table nil))
   (condition-case err
       (progn
         (standard-display-underline ?A ?_)
         (list (type-of standard-display-table)
               (aref standard-display-table ?A)))
     (error (list 'err (car err) (cadr err)))))
 (let ((standard-display-table (make-display-table)))
   (standard-display-underline ?B ?_)
   (list (type-of standard-display-table)
         (aref standard-display-table ?B))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_process_send_bare_eol_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((97 13 10 98) (97 13 98) (97 10 98) (97 13 10 98))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((97 13 10 98) (97 13 98) (97 10 98) (97 13 10 98))
    // Neomacs:   OK ((97 10 98) (97 10 98) (97 10 98) (97 10 98))
    // With process output coding set to bare dos/mac/unix, GNU applies the
    // requested EOL conversion before sending to the child process. Neomacs
    // sends plain LF for all three, and even fails to apply CRLF for utf-8-dos.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((run
       (lambda (output-coding)
         (let ((out nil))
           (let ((proc (make-process
                        :name "probe-send-eol"
                        :command '("cat")
                        :connection-type 'pipe
                        :filter (lambda (_ s)
                                  (setq out (concat out s))))))
             (set-process-query-on-exit-flag proc nil)
             (set-process-coding-system proc 'binary output-coding)
             (process-send-string proc "a\nb")
             (process-send-eof proc)
             (while (process-live-p proc)
               (accept-process-output proc 0.05))
             (append (or out "") nil))))))
  (list (funcall run 'dos)
        (funcall run 'mac)
        (funcall run 'unix)
        (funcall run 'utf-8-dos)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_process_filter_bare_eol_decoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((97 10 98) (97 10 98) (97 10 98) (97 13 10 98))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((97 10 98) (97 10 98) (97 10 98) (97 13 10 98))
    // Neomacs:   OK ((97 13 10 98) (97 13 98) (97 10 98) (97 13 10 98))
    // With process input coding set to bare dos/mac, GNU decodes CRLF/CR to LF
    // before invoking the process filter. Neomacs leaves raw CR bytes for bare
    // dos/mac, while utf-8-dos and binary behave like GNU.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((run
       (lambda (input-coding payload)
         (let ((out nil))
           (let ((proc (make-process
                        :name "probe-proc-decode"
                        :command `("/bin/sh" "-c" ,(concat "printf '" payload "'"))
                        :connection-type 'pipe
                        :filter (lambda (_ s)
                                  (setq out (concat out s))))))
             (set-process-query-on-exit-flag proc nil)
             (set-process-coding-system proc input-coding 'binary)
             (while (process-live-p proc)
               (accept-process-output proc 0.05))
             (append (or out "") nil))))))
  (list (funcall run 'dos "a\r\nb")
        (funcall run 'mac "a\rb")
        (funcall run 'utf-8-dos "a\r\nb")
        (funcall run 'binary "a\r\nb")))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_unicode_normalization_decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"é\" \"e\u{301}\" 3 \"fi\" \"1\")""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("é" "é" 3 "fi" "1")
    // Neomacs:   OK ("é" "é" 2 "fi" "①")
    // GNU decomposes precomposed é to e + combining acute under NFD, and
    // compatibility-decomposes circled digit one to "1" under NFKD. Neomacs
    // leaves both forms composed/unchanged while NFKC for the fi ligature works.
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (require 'ucs-normalize)
  (list (ucs-normalize-NFC-string "e\u0301")
        (ucs-normalize-NFD-string "é")
        (string-bytes (ucs-normalize-NFD-string "é"))
        (ucs-normalize-NFKC-string "ﬁ")
        (ucs-normalize-NFKD-string "①")))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_call_process_region_eol_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((97 13 10 98) (97 13 10 98) (97 13 98) (97 10 98))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((97 13 10 98) (97 13 10 98) (97 13 98) (97 10 98))
    // Neomacs:   OK ((97 10 98) (97 10 98) (97 10 98) (97 10 98))
    // call-process-region should honor coding-system-for-write when sending
    // region text to the program. GNU applies CRLF/CR/LF for utf-8-dos,
    // bare dos, mac, and unix respectively. Neomacs sends plain LF for all.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((run
       (lambda (write-coding)
         (with-temp-buffer
           (insert "a\nb")
           (let ((coding-system-for-write write-coding)
                 (coding-system-for-read 'binary))
             (call-process-region (point-min) (point-max) "cat" t t nil)
             (append (buffer-string) nil))))))
  (list (funcall run 'utf-8-dos)
        (funcall run 'dos)
        (funcall run 'mac)
        (funcall run 'unix)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_file_name_handler_copy_rename_delete_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((handled copy-file) (handled rename-file) (handled delete-file) ((expand-file-name (\"/h:/a\" nil)) (expand-file-name (\"/h:/b\" nil)) (copy-file (\"/h:/a\" \"/h:/b\" t nil nil nil)) (expand-file-name (\"/h:/b\" nil)) (directory-file-name (\"/h:/b\")) (expand-file-name (\"/h:/c\" nil)) (rename-file (\"/h:/b\" \"/h:/c\" t)) (expand-file-name (\"/h:/c\" nil)) (file-directory-p (\"/h:/c\")) (expand-file-name (\"/h:/c\" nil)) (delete-file (\"/h:/c\" nil))))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs logs expand-file-name / directory-file-name dispatches before
    // copy-file, rename-file, and delete-file, and passes full operation arg
    // lists including optional args normalized to nil.
    // Neomacs dispatches copy-file/rename-file directly with truncated arg
    // lists and misses several GNU handler calls before those operations.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (file-name-handler-alist nil))
  (letrec ((handler
            (lambda (op &rest args)
              (push (list op args) log)
              (cond
               ((memq op '(copy-file rename-file delete-file))
                (list 'handled op))
               ((eq op 'file-directory-p)
                nil)
               (t
                (let ((inhibit-file-name-handlers
                       (cons handler inhibit-file-name-handlers))
                      (inhibit-file-name-operation op))
                  (apply op args)))))))
    (setq file-name-handler-alist `(("\\`/h:" . ,handler)))
    (list (copy-file "/h:/a" "/h:/b" t)
          (rename-file "/h:/b" "/h:/c" t)
          (delete-file "/h:/c")
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_insert_file_contents_literally_handler_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"/h:/f\" 1) \"X\" ((expand-file-name (\"/h:/f\" nil)) (insert-file-contents (\"/h:/f\" nil nil nil nil))))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (("/h:/f" 1) "X" ((expand-file-name ("/h:/f" nil))
    //                (insert-file-contents ("/h:/f" nil nil nil nil))))
    // Neomacs:   OK (("/h:/f" 1) "X" ((insert-file-contents ("/h:/f" nil nil nil nil))))
    // insert-file-contents-literally still dispatches the expand-file-name
    // handler before insert-file-contents in GNU; Neomacs skips that handler.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      (file-name-handler-alist nil))
  (letrec ((handler
            (lambda (op &rest args)
              (push (list op args) log)
              (cond
               ((eq op 'insert-file-contents)
                (insert "X")
                (list (car args) 1))
               (t
                (let ((inhibit-file-name-handlers
                       (cons handler inhibit-file-name-handlers))
                      (inhibit-file-name-operation op))
                  (apply op args)))))))
    (setq file-name-handler-alist `(("\\`/h:" . ,handler)))
    (with-temp-buffer
      (list (insert-file-contents-literally "/h:/f")
            (buffer-string)
            (nreverse log)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_overwrite_mode_self_insert_replaces_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"abXdef\" 4 overwrite-mode-textual \"abXYef\" 5 overwrite-mode-textual)""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ("abXdef" 4 overwrite-mode-textual "abXYef" 5 overwrite-mode-textual)
    // Neomacs:   OK ("abXcdef" 4 overwrite-mode-textual "abXYcdef" 5 overwrite-mode-textual)
    // self-insert-command in overwrite-mode should replace the character at
    // point. GNU replaces `c` with X and `d` with Y; Neomacs inserts X/Y
    // without deleting the overwritten characters.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (goto-char 3)
  (overwrite-mode 1)
  (self-insert-command 1 ?X)
  (let ((after-first (buffer-string))
        (point-after-first (point))
        (mode-after-first overwrite-mode))
    (self-insert-command 1 ?Y)
    (list after-first
          point-after-first
          mode-after-first
          (buffer-string)
          (point)
          overwrite-mode)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_overwrite_mode_tab_clears_to_tabstop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\tfgh\" 3)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs debug: OK ("a\tfgh" 3)
    // Neomacs debug:   OK ("a\tbcdefgh" 3)
    // In overwrite-mode, inserting TAB should replace text through the next
    // tab stop. GNU replaces b/c/d/e with one tab at column 1 (tab-width 4),
    // leaving fgh. Neomacs inserts the tab without deleting overwritten text.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefgh")
  (setq tab-width 4)
  (goto-char 2)
  (overwrite-mode 1)
  (self-insert-command 1 ?\t)
  (list (buffer-string) (point)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_replace_buffer_contents_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"abXdef\" 0 2 (face italic) 2 3 (face bold) 3 6 (face italic)) (face italic) (face bold))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (#("abXdef" 0 2 (face italic) 2 3 (face bold) 3 6 (face italic))
    //                (face italic) (face bold))
    // Neomacs:   OK (#("abXdef" 0 6 (face bold)) (face bold) (face bold))
    // replace-buffer-contents in GNU preserves unchanged destination text
    // properties and only takes source properties for inserted/replaced spans;
    // Neomacs replaces the whole result with the source buffer's properties.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((src (generate-new-buffer " *probe-rbc-src*")))
  (unwind-protect
      (progn
        (with-current-buffer src
          (insert (propertize "abXdef" 'face 'bold)))
        (with-temp-buffer
          (insert (propertize "abcdef" 'face 'italic))
          (replace-buffer-contents src)
          (list (buffer-string)
                (text-properties-at 1)
                (text-properties-at 3))))
    (kill-buffer src)))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_unibyte_search_replace_raw_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 (195 90 65 169 66))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (3 (195 90 65 169 66))
    // Neomacs:   OK (5 (195 169 65 90 66))
    // In a unibyte buffer containing bytes C3 A9 41 A9 42, search-forward for
    // raw byte A9 should find the byte embedded in the C3 A9 sequence first.
    // GNU replaces that embedded byte; Neomacs skips it and replaces only the
    // standalone trailing A9 byte.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 195 169 65 169 66))
  (let ((pat (unibyte-string 169)))
    (goto-char (point-min))
    (let ((match-end (search-forward pat nil t)))
      (replace-match "Z")
      (list match-end
            (append (buffer-string) nil)))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_network_filter_multibyte_string_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((server \"é\" t 2))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((server "é" t 2))
    // Neomacs:   OK ((server "\303\251" nil 2))
    // With :filter-multibyte nil and raw UTF-8 bytes sent over a local network
    // process, GNU delivers a multibyte string to the server filter, while
    // Neomacs delivers a unibyte string with the same bytes.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil)
      server
      client)
  (unwind-protect
      (progn
        (setq server
              (make-network-process
               :name "probe-net-mb-server"
               :server t
               :host 'local
               :service t
               :noquery t
               :filter-multibyte nil
               :filter (lambda (_ s)
                         (push (list 'server
                                     s
                                     (multibyte-string-p s)
                                     (string-bytes s))
                               log))))
        (setq client
              (make-network-process
               :name "probe-net-mb-client"
               :host 'local
               :service (process-contact server :service)
               :noquery t
               :filter-multibyte nil))
        (process-send-string client (string-as-unibyte "é"))
        (let ((i 0))
          (while (and (< i 20) (null log))
            (accept-process-output nil 0.05)
            (setq i (1+ i))))
        (nreverse log))
    (when (processp client) (delete-process client))
    (when (processp server) (delete-process server))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_where_is_ignores_overriding_terminal_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil cmd-b nil nil)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (nil cmd-b nil nil)
    // Neomacs:   OK (nil cmd-b nil [3 98])
    // key-binding honors overriding-terminal-local-map for lookup, but
    // where-is-internal should not report bindings from that transient override
    // as stable command locations. Neomacs reports [C-c b].
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((local-map (make-sparse-keymap))
      (terminal-map (make-sparse-keymap)))
  (define-key local-map (kbd "C-c a") 'cmd-a)
  (define-key terminal-map (kbd "C-c b") 'cmd-b)
  (let ((overriding-local-map local-map)
        (overriding-terminal-local-map terminal-map))
    (list (key-binding (kbd "C-c a"))
          (key-binding (kbd "C-c b"))
          (where-is-internal 'cmd-a nil t)
          (where-is-internal 'cmd-b nil t))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_substitute_command_keys_terminal_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (cmd-x nil #(\"M-x cmd-x\" 0 9 (font-lock-face help-key-binding face help-key-binding)) nil)""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (cmd-x nil #("M-x cmd-x" ...) nil)
    // Neomacs:   OK (cmd-x ([3 120]) #("C-c x" ...) nil)
    // substitute-command-keys relies on where-is-internal. GNU ignores
    // overriding-terminal-local-map for stable command substitution and falls
    // back to M-x; Neomacs treats the transient terminal override as a real
    // command binding and substitutes C-c x.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map (kbd "C-c x") 'cmd-x)
  (let ((overriding-terminal-local-map map))
    (list (key-binding (kbd "C-c x"))
          (where-is-internal 'cmd-x nil)
          (substitute-command-keys "\\[cmd-x]")
          (command-remapping 'cmd-x))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_global_map_special_event_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t ignore t ignore)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((global (current-global-map)))
  (list (keymapp (lookup-key global [tool-bar]))
        (lookup-key global [XF86WakeUp])
        (keymapp (lookup-key global [C-down-mouse-3]))
        (lookup-key global [C-M-drag-mouse-1])))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_regexp_unibyte_char_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 nil 0 0)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (3 3 nil 0 0)
    // Neomacs:   OK (3 3 0 0 0)
    // In GNU, the POSIX regexp class [:unibyte:] does not match a unibyte
    // string produced from the UTF-8 bytes of é here, while Neomacs reports a
    // match at offset 0. Other unicode/nonascii/word/category controls match.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match-p "[[:nonascii:]]" "abcé")
      (string-match-p "[[:multibyte:]]" "abcé")
      (string-match-p "[[:unibyte:]]" (string-as-unibyte "é"))
      (string-match-p "[[:word:]]+" "é")
      (string-match-p "\\cc" "中"))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_regexp_nonascii_unibyte_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 0)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (nil 0)
    // Neomacs:   OK (1 0)
    // For a unibyte string containing raw UTF-8 bytes, GNU does not let the
    // POSIX [:nonascii:] class match the raw high byte; Neomacs reports a
    // match at offset 1. The [:unibyte:] control remains matching in both.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (string-as-unibyte "aé中"))
  (let ((raw (buffer-string)))
    (list (string-match-p "[[:nonascii:]]" raw)
          (string-match-p "[[:unibyte:]]+" (string-as-unibyte "aé")))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_replace_match_case_table_capitalization() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Xy def\"""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK "Xy def"
    // Neomacs:   OK "xy def"
    // With a custom case table making `{` an uppercase letter (pair {/}), the
    // matched text "{bc" reads as capitalized, so case-replace capitalizes the
    // lowercase replacement "xy" to "Xy" in GNU. Neomacs ignores the case
    // table and leaves "xy".
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((table (copy-case-table (standard-case-table))))
  (set-case-syntax-pair ?\{ ?\} table)
  (with-temp-buffer
    (set-case-table table)
    (insert "{bc def")
    (goto-char 1)
    (let ((case-replace t)
          (case-fold-search t))
      (re-search-forward "{bc")
      (replace-match "xy")
      (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_case_fold_search_custom_case_pair() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 2)""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (2 2 2)
    // Neomacs:   OK (3 3 2)
    // With a case table pairing {/} and case-fold-search t, GNU treats } as
    // case-equivalent to { and finds the { at position 1 (match end 2) for both
    // search-forward and re-search-forward. Neomacs ignores the custom case
    // table and only matches the literal } at position 2 (match end 3).
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((table (copy-case-table (standard-case-table))))
  (set-case-syntax-pair ?\{ ?\} table)
  (with-temp-buffer
    (set-case-table table)
    (insert "{}")
    (let ((case-fold-search t))
      (list (progn (goto-char 1) (search-forward "}" nil t))
            (progn (goto-char 1) (re-search-forward "}" nil t))
            (progn (goto-char 1) (search-forward "{" nil t))))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_coding_region_eol_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((97 10 98) 3) ((97 13 10 98) 4))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((97 10 98) 3) and OK ((97 13 10 98) 4)
    // Neomacs:   OK ((97 13 10 98) 4) and OK (nil 0)
    // decode-coding-region with 'dos should collapse CRLF to LF in-buffer
    // (GNU: bytes a LF b, size 3); Neomacs keeps the CR.
    // encode-coding-region with 'dos should expand LF to CRLF (GNU: a CR LF b,
    // size 4); Neomacs empties the region (0 bytes).
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (set-buffer-multibyte nil)
   (insert (unibyte-string ?a 13 10 ?b))
   (decode-coding-region (point-min) (point-max) 'dos)
   (list (append (buffer-string) nil) (buffer-size)))
 (with-temp-buffer
   (insert "a\nb")
   (encode-coding-region (point-min) (point-max) 'dos)
   (list (append (buffer-string) nil) (buffer-size))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_standard_display_bulk_helpers_create_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((char-table nil) (char-table nil) (err wrong-type-argument char-table-p))""#
    ]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK ((char-table nil) (char-table nil)
    //                (err wrong-type-argument char-table-p))
    // Neomacs:   OK ((err wrong-type-argument arrayp) (err wrong-type-argument arrayp)
    //                (err wrong-type-argument arrayp))
    // standard-display-cyrillic-translit and standard-display-european-internal
    // create standard-display-table when nil in GNU; Neomacs errors with
    // wrong-type-argument arrayp. standard-display-unicode-special-glyphs
    // also errors differently: GNU reports char-table-p, Neomacs arrayp.
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (let ((standard-display-table nil))
   (condition-case err
       (progn
         (standard-display-cyrillic-translit)
         (list (type-of standard-display-table)
               (aref standard-display-table #x410)))
     (error (list 'err (car err) (cadr err)))))
 (let ((standard-display-table nil))
   (condition-case err
       (progn
         (standard-display-european-internal)
         (list (type-of standard-display-table)
               (aref standard-display-table 160)))
     (error (list 'err (car err) (cadr err)))))
 (let ((standard-display-table nil))
   (condition-case err
       (progn
         (standard-display-unicode-special-glyphs)
         (list (type-of standard-display-table)
               (aref standard-display-table #x2018)))
     (error (list 'err (car err) (cadr err))))))
"##,
        expect,
    );
}

#[test]
fn div_core_divergence_surface_detect_coding_region_eol_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (undecided-dos (undecided-dos))""#]];
    // Divergence surfaced 2026-06-24:
    // GNU Emacs: OK (undecided-dos (undecided-dos))
    // Neomacs:   OK (undecided (undecided))
    // detect-coding-region reports the CRLF EOL variant (undecided-dos) for a
    // buffer with CRLF line endings; Neomacs returns plain undecided, missing
    // EOL detection (both the highest-priority and full-list forms).
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string ?a 13 10 ?b 13 10))
  (list (detect-coding-region (point-min) (point-max) t)
        (detect-coding-region (point-min) (point-max))))
"##,
        expect,
    );
}
