//! Divergence tests: fringe bitmaps, window fringes, buffer-local fringe vars,
//! fringe indicator/cursor alists, frame-fringe-width, fringe-bitmaps-at-pos,
//! overlay fringe display specs, and fringe-mode customization.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_define_fringe_bitmap_known() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil error nil test-dfbk2-xxx t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((bm (define-fringe-bitmap 'test-dfbk-xxx
              [#b10000000 #b01000000 #b00100000 #b00010000
               #b00001000 #b00000100 #b00000010 #b00000001]
              nil nil 'top)))
    (list (eq bm 'test-dfbk-xxx)
          (symbolp bm)
          (set-fringe-bitmap-face 'right-triangle 'font-lock-warning-face)
          (condition-case err
              (set-fringe-bitmap-face 'test-nonexistent-bitmap-xxx 'default)
            (error (car err)))
          (destroy-fringe-bitmap 'test-dfbk-xxx)
          (define-fringe-bitmap 'test-dfbk2-xxx [1 2 3] 3 8 'center)
          (eq (define-fringe-bitmap 'test-dfbk2-xxx [1 2 3] 3 8 'center)
              'test-dfbk2-xxx)
          (destroy-fringe-bitmap 'test-dfbk2-xxx)))) "#,
        expect,
    );
}

#[test]
fn divergence_window_fringes_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t (nil) (nil) t t (0 0 nil nil) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((fringes (window-fringes)))
    (list (= (length fringes) 4)
          (integerp (nth 0 fringes))
          (>= (nth 0 fringes) 0)
          (integerp (nth 1 fringes))
          (>= (nth 1 fringes) 0)
          (memq (nth 2 fringes) '(t nil))
          (memq (nth 3 fringes) '(t nil))
          (listp fringes)
          (= (safe-length fringes) 4)
          (window-fringes (selected-window))
          (equal (window-fringes (selected-window)) fringes)))) "#,
        expect,
    );
}

#[test]
fn divergence_set_window_fringes_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((w (selected-window))
        (orig (window-fringes)))
    (set-window-fringes w 12 8 t t)
    (let ((after-set (window-fringes w)))
      (list (nth 0 after-set)
            (nth 1 after-set)
            (nth 2 after-set)
            (nth 3 after-set)
            (>= (nth 0 after-set) 0)
            (>= (nth 1 after-set) 0)
            (set-window-fringes w 0 0 nil nil)
            (let ((zeroed (window-fringes w)))
              (set-window-fringes w (nth 0 orig) (nth 1 orig)
                                  (nth 2 orig) (nth 3 orig))
              (list after-set zeroed
                    (<= (nth 0 zeroed) (nth 0 after-set))
                    (= (length zeroed) 4)))))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_buffer_local_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (20 t 15 t t t t t t t t left-fringe-width right-fringe-width fringes-outside-margins t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((default-left (default-value 'left-fringe-width))
        (default-right (default-value 'right-fringe-width))
        (default-outside (default-value 'fringes-outside-margins)))
    (setq-local left-fringe-width 20)
    (setq-local right-fringe-width 15)
    (setq-local fringes-outside-margins t)
    (let ((local-left left-fringe-width)
          (local-right right-fringe-width)
          (local-outside fringes-outside-margins))
      (list local-left
            (= local-left 20)
            local-right
            (= local-right 15)
            local-outside
            (eq local-outside t)
            (equal default-left (default-value 'left-fringe-width))
            (equal default-right (default-value 'right-fringe-width))
            (equal default-outside (default-value 'fringes-outside-margins))
            (not (equal left-fringe-width (default-value 'left-fringe-width)))
            (not (equal right-fringe-width (default-value 'right-fringe-width)))
            (kill-local-variable 'left-fringe-width)
            (kill-local-variable 'right-fringe-width)
            (kill-local-variable 'fringes-outside-margins)
            (equal left-fringe-width (default-value 'left-fringe-width))
            (equal right-fringe-width (default-value 'right-fringe-width)))))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_indicator_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 24 76)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((orig fringe-indicator-alist))
    (list (listp fringe-indicator-alist)
          (consp (assq 'truncation fringe-indicator-alist))
          (consp (assq 'continuation fringe-indicator-alist))
          (consp (assq 'overlay-arrow fringe-indicator-alist))
          (= (safe-length (assq 'truncation fringe-indicator-alist)) 3)
          (= (safe-length (assq 'continuation fringe-indicator-alist)) 3)
          (let ((trunc (cdr (assq 'truncation fringe-indicator-alist))))
            (and (listp trunc)
                 (or (symbolp (car trunc)) (consp (car trunc)))))
          (let ((cont (cdr (assq 'continuation fringe-indicator-alist))))
            (and (listp cont)
                 (or (symbolp (car cont)) (consp (car cont)))))
          (setq fringe-indicator-alist
                (cons '(test-custom-indicator-xxx right-triangle left-triangle)
                      fringe-indicator-alist))
          (consp (assq 'test-custom-indicator-xxx fringe-indicator-alist))
          (eq (nth 1 (assq 'test-custom-indicator-xxx fringe-indicator-alist))
              'right-triangle)
          (eq (nth 2 (assq 'test-custom-indicator-xxx fringe-indicator-alist))
              'left-triangle)
          (setq fringe-indicator-alist orig)
          (not (assq 'test-custom-indicator-xxx fringe-indicator-alist)))))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_cursor_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 70)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((orig fringe-cursor-alist))
    (list (listp fringe-cursor-alist)
          (consp (assq 'box fringe-cursor-alist))
          (consp (assq 'hollow fringe-cursor-alist))
          (consp (assq 'bar fringe-cursor-alist))
          (consp (assq 'hbar fringe-cursor-alist))
          (eq (car (assq 'box fringe-cursor-alist)) 'box)
          (eq (car (assq 'hollow fringe-cursor-alist)) 'hollow)
          (eq (car (assq 'bar fringe-cursor-alist)) 'bar)
          (eq (car (assq 'hbar fringe-cursor-alist)) 'hbar)
          (setq fringe-cursor-alist
                (cons '(test-custom-cursor-xxx . filled-square)
                      fringe-cursor-alist))
          (consp (assq 'test-custom-cursor-xxx fringe-cursor-alist))
          (eq (cdr (assq 'test-custom-cursor-xxx fringe-cursor-alist))
              'filled-square)
          (setq fringe-cursor-alist orig)
          (not (assq 'test-custom-cursor-xxx fringe-cursor-alist)))))) "#,
        expect,
    );
}

#[test]
fn divergence_frame_fringe_width_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t 0 0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((fw (frame-fringe-width))
        (fw-selected (frame-fringe-width (selected-frame))))
    (list (integerp fw)
          (>= fw 0)
          (integerp fw-selected)
          (>= fw-selected 0)
          (= fw fw-selected)
          (let ((fr (selected-frame)))
            (frame-fringe-width fr))
          (frame-fringe-width nil)
          (= (frame-fringe-width nil) fw)))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_bitmaps_at_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Line with fringe indicators test content here")
  (let ((at-nil (fringe-bitmaps-at-pos))
        (at-point (fringe-bitmaps-at-pos (point)))
        (at-bob (fringe-bitmaps-at-pos (point-min)))
        (at-eob (fringe-bitmaps-at-pos (point-max)))
        (at-win (fringe-bitmaps-at-pos (point) (selected-window))))
    (list (or (null at-nil) (consp at-nil))
          (or (null at-point) (consp at-point))
          (or (null at-bob) (consp at-bob))
          (or (null at-eob) (consp at-eob))
          (or (null at-win) (consp at-win))
          (or (null at-point)
              (and (consp at-point)
                   (or (null (car at-point)) (symbolp (car at-point)))
                   (or (null (cdr at-point)) (symbolp (cdr at-point)))))))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_overlay_display_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t left-fringe-marker t right-fringe-marker t left-fringe-filled t 5 t 9 t (left-fringe right-triangle) t (right-fringe left-arrow) t (left-fringe filled-square warning) t \"AAAA-BBBB-CCCC-DDDD-EEEE-FFFF\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
  (let ((ov1 (make-overlay 5 9))
        (ov2 (make-overlay 13 17))
        (ov3 (make-overlay 21 25)))
    (overlay-put ov1 'before-string
                 (propertize "x" 'display '(left-fringe right-triangle)))
    (overlay-put ov2 'before-string
                 (propertize "y" 'display '(right-fringe left-arrow)))
    (overlay-put ov3 'before-string
                 (propertize "z" 'display '(left-fringe filled-square warning)))
    (overlay-put ov1 'tag 'left-fringe-marker)
    (overlay-put ov2 'tag 'right-fringe-marker)
    (overlay-put ov3 'tag 'left-fringe-filled)
    (let ((bs1 (overlay-get ov1 'before-string))
          (bs2 (overlay-get ov2 'before-string))
          (bs3 (overlay-get ov3 'before-string)))
      (list (stringp bs1)
            (stringp bs2)
            (stringp bs3)
            (overlay-get ov1 'tag)
            (eq (overlay-get ov1 'tag) 'left-fringe-marker)
            (overlay-get ov2 'tag)
            (eq (overlay-get ov2 'tag) 'right-fringe-marker)
            (overlay-get ov3 'tag)
            (eq (overlay-get ov3 'tag) 'left-fringe-filled)
            (overlay-start ov1)
            (= (overlay-start ov1) 5)
            (overlay-end ov1)
            (= (overlay-end ov1) 9)
            (get-text-property 0 'display bs1)
            (equal (get-text-property 0 'display bs1)
                   '(left-fringe right-triangle))
            (get-text-property 0 'display bs2)
            (equal (get-text-property 0 'display bs2)
                   '(right-fringe left-arrow))
            (get-text-property 0 'display bs3)
            (equal (get-text-property 0 'display bs3)
                   '(left-fringe filled-square warning))
            (buffer-string)
            (= (buffer-size) 29))))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_mode_customization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t nil t 0 t (nil . 0) t (0) t (8 . 8) t nil t t ((funcall #'#[0 \"��\" [nil] 1])) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'fringe)
  (let ((orig fringe-mode))
    (list (or (null fringe-mode)
              (integerp fringe-mode)
              (and (consp fringe-mode)
                   (or (null (car fringe-mode)) (integerp (car fringe-mode)))
                   (or (null (cdr fringe-mode)) (integerp (cdr fringe-mode)))))
          (setq fringe-mode nil)
          (null fringe-mode)
          (setq fringe-mode 0)
          (eq fringe-mode 0)
          (setq fringe-mode '(nil . 0))
          (equal fringe-mode '(nil . 0))
          (setq fringe-mode '(0 . nil))
          (equal fringe-mode '(0 . nil))
          (setq fringe-mode '(8 . 8))
          (equal fringe-mode '(8 . 8))
          (setq fringe-mode orig)
          (equal fringe-mode orig)
          (boundp 'fringe-mode)
          (custom-variable-p 'fringe-mode)
          (not (custom-variable-p 'nonexistent-fringe-var-xxx))))) "#,
        expect,
    );
}
