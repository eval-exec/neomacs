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

    let expect =
        expect_test::expect![[r#""OK (0 0 nil nil t t nil ((0 0 nil nil) (0 0 nil nil) t t))""#]];
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
                    (= (length zeroed) 4)))))) )"#,
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

    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t ((test-custom-indicator-xxx right-triangle left-triangle) (truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) t t t ((truncation left-arrow right-arrow) (continuation left-curly-arrow right-curly-arrow) (overlay-arrow . right-triangle) (up . up-arrow) (down . down-arrow) (top top-left-angle top-right-angle) (bottom bottom-left-angle bottom-right-angle top-right-angle top-left-angle) (top-bottom left-bracket right-bracket top-right-angle top-left-angle) (empty-line . empty-line) (unknown . question-mark)) t)""#
    ]];
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
          (not (assq 'test-custom-indicator-xxx fringe-indicator-alist)))))"#,
        expect,
    );
}

#[test]
fn divergence_fringe_cursor_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t t ((test-custom-cursor-xxx . filled-square) (box . filled-rectangle) (hollow . hollow-rectangle) (bar . vertical-bar) (hbar . horizontal-bar) (hollow-small . hollow-square)) t t ((box . filled-rectangle) (hollow . hollow-rectangle) (bar . vertical-bar) (hbar . horizontal-bar) (hollow-small . hollow-square)) t)""#
    ]];
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
          (not (assq 'test-custom-cursor-xxx fringe-cursor-alist)))))"#,
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

#[test]
fn divergence_fringe_display_measurement_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((4 . 1) (4 . 1) (4 . 1) (4 . 1) (3 . 1) (4 . 1) (4 . 1) (5 . 1) (5 . 1) (0 . 0) (0 . 1) (2 . 1) (3 . 1) (5 . 1) (0 . 0) (4 . 1) (7 . 1) (4 . 1) (5 . 1) (4 . 1) (4 . 1) (5 . 1) 0 2 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure
       (lambda (setup)
         (with-temp-buffer
           (funcall setup)
           (switch-to-buffer (current-buffer))
           (window-text-pixel-size)))))
  (list
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(left-fringe left-arrow))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(right-fringe right-arrow bold))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((left-fringe left-arrow)))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display [(raise 0.5) (left-fringe left-arrow)])))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 3 'display '(left-fringe nil))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(left-fringe no-such-bitmap-xyz))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(left-fringe 123))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(left-fringe))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(left-fringe . left-arrow))))
   (funcall measure (lambda () (insert "ab")
                      (put-text-property 1 3 'display '(left-fringe left-arrow))))
   (funcall measure (lambda () (insert "ab\n")
                      (put-text-property 1 3 'display '(left-fringe left-arrow))))
   (funcall measure (lambda () (insert "ab\ncd")
                      (put-text-property 1 4 'display '(left-fringe left-arrow))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 3)))
                        (overlay-put ov 'display '(left-fringe left-arrow)))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1)))
                        (overlay-put ov 'before-string
                                     (propertize "XX" 'display '((left-fringe left-arrow)))))))
   (funcall measure (lambda ()
                      (let ((ov (make-overlay 1 1)))
                        (overlay-put ov 'before-string
                                     (propertize "XX" 'display '((left-fringe left-arrow)))))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((left-fringe left-arrow) (space :width 3)))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((space :width 3) (left-fringe left-arrow)))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(when (eq 1 1) left-fringe left-arrow))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(when (eq 1 2) left-fringe left-arrow))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(disable-eval (left-fringe left-arrow)))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(disable-eval (when t left-fringe left-arrow)))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(disable-eval (when (eq 1 1) left-fringe left-arrow)))))
   (string-pixel-width (propertize "ab" 'display '(left-fringe left-arrow)))
   (string-pixel-width (concat (propertize "ab" 'display '((right-fringe nil))) "cd"))
   (string-pixel-width (propertize "ab" 'display '(left-fringe)))))"#,
        expect,
    );
}

#[test]
fn divergence_display_string_measurement_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((6 . 1) (4 . 1) (0 . 0) (0 . 1) (6 . 1) (6 . 1) (4 . 1) (7 . 1) (6 . 1) (4 . 1) (6 . 1) (6 . 1) (6 . 1) (6 . 1) (6 . 1) (5 . 1) (5 . 1) (6 . 1) (6 . 1) (5 . 1) (6 . 2) (3 . 1) (13 . 1) (6 . 1) (6 . 1) (5 . 1) (7 . 1) 4 2 2 0 0 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure
       (lambda (setup)
         (with-temp-buffer
           (funcall setup)
           (switch-to-buffer (current-buffer))
           (window-text-pixel-size)))))
  (list
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "XY")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "")))
   (funcall measure (lambda () (insert "ab")
                      (put-text-property 1 3 'display "")))
   (funcall measure (lambda () (insert "ab\n")
                      (put-text-property 1 3 'display "")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '("XY" (left-fringe left-arrow)))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '("XY" (space :width 3)))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((left-fringe left-arrow) "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((space :width 3) "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '("XY" "ZZZ"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '("" (space :width 3)))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((raise 0.5) "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display [(raise 0.5) "XY"])))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((left-fringe) "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((image :type png :data "") "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(when (eq 1 1) . "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(when (eq 1 2) . "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(when t "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(disable-eval "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(disable-eval (when t . "XY")))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(disable-eval (when (eq 1 1) . "XY")))))
   (funcall measure (lambda () (insert "xhello")
                      (put-text-property 2 3 'display "AB\nCD")))
   (funcall measure (lambda () (insert "xh")
                      (put-text-property 2 3 'display "ab\n")))
   (funcall measure (lambda () (insert "xhello")
                      (put-text-property 2 3 'display "a\tb")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display (propertize "XY" 'display "ZZZZZ"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display (propertize "XY" 'display '(left-fringe left-arrow)))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 3)))
                        (overlay-put ov 'display '("XY" (left-fringe left-arrow))))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1)))
                        (overlay-put ov 'before-string
                                     (propertize "a" 'display '("XY" (space :width 9)))))))
   (string-pixel-width (propertize "a" 'display "XYZW"))
   (string-pixel-width (propertize "a" 'display '("XY" (left-fringe left-arrow))))
   (string-pixel-width (propertize "a" 'display '("XY" (space :width 5))))
   (string-pixel-width (propertize "a" 'display '((left-fringe left-arrow) "XY")))
   (string-pixel-width (propertize "a" 'display ""))
   (string-pixel-width (concat "x" (propertize "h" 'display "AB\nCD")))))"#,
        expect,
    );
}

#[test]
fn divergence_margin_display_measurement_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((4 . 1) (4 . 1) (4 . 1) (4 . 1) (6 . 1) (7 . 1) (6 . 1) (6 . 1) (6 . 1) (0 . 1) (6 . 1) (5 . 1) (6 . 1) (4 . 1) (0 . 0) (0 . 1) (0 . 1) (0 . 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure
       (lambda (setup)
         (with-temp-buffer
           (funcall setup)
           (switch-to-buffer (current-buffer))
           (window-text-pixel-size)))))
  (list
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((margin left-margin) "M"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(((margin left-margin) "M") "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(((margin right-margin) "M") "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(((margin left-margin) (space :width 5)) "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((margin nil) "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((margin nil) (space :width 3)))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(((margin nil) "XY") "ZZZZ"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(((margin bogus) "M") "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(((margin left-margin) (left-fringe left-arrow)) "XY"))))
   (funcall measure (lambda () (insert "ab")
                      (put-text-property 1 3 'display '((margin left-margin) "M"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(((margin left-margin) (image :type png :data "")) "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1)))
                        (overlay-put ov 'before-string
                                     (propertize "XX" 'display '(((margin left-margin) "M") "ZZ"))))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((margin) "XY"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((margin left-margin) (space :width 5)))))
   (funcall measure (lambda () (insert "ab")
                      (put-text-property 1 3 'display '((margin left-margin) ""))))
   (funcall measure (lambda () (insert "ab")
                      (put-text-property 1 3 'display '((margin left-margin) (space :width 5)))))
   (funcall measure (lambda () (insert "ab")
                      (put-text-property 1 3 'display '((margin left-margin) "A\nB"))))
   (funcall measure (lambda () (insert "ab")
                      (put-text-property 1 3 'display `((margin left-margin) ,(propertize "M" 'invisible t)))))))"#,
        expect,
    );
}

#[test]
fn divergence_invisible_display_string_measurement_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((4 . 1) (6 . 1) (4 . 1) (7 . 1) (7 . 1) (7 . 1) (0 . 0) (5 . 1) (5 . 1) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure
       (lambda (setup)
         (with-temp-buffer
           (funcall setup)
           (switch-to-buffer (current-buffer))
           (window-text-pixel-size)))))
  (list
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display (propertize "XYZ" 'invisible t))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display
                                         (concat "A" (propertize "BBB" 'invisible t) "C"))))
   (funcall measure (lambda ()
                      (setq-local buffer-invisibility-spec '(foo))
                      (insert "hello")
                      (put-text-property 1 2 'display (propertize "XYZ" 'invisible 'foo))))
   (funcall measure (lambda ()
                      (setq-local buffer-invisibility-spec '((foo . t)))
                      (insert "hello")
                      (put-text-property 1 2 'display (propertize "XYZ" 'invisible 'foo))))
   (funcall measure (lambda ()
                      (setq-local buffer-invisibility-spec '(bar))
                      (insert "hello")
                      (put-text-property 1 2 'display (propertize "XYZ" 'invisible 'foo))))
   (funcall measure (lambda ()
                      (setq-local buffer-invisibility-spec nil)
                      (insert "hello")
                      (put-text-property 1 2 'display (propertize "XYZ" 'invisible t))))
   (funcall measure (lambda () (insert "ab")
                      (put-text-property 1 3 'display (propertize "XYZ" 'invisible t))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1)))
                        (overlay-put ov 'before-string (propertize "XYZ" 'invisible t)))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1)))
                        (overlay-put ov 'before-string
                                     (propertize "a" 'display (propertize "XYZ" 'invisible t))))))
   (string-pixel-width (propertize "a" 'display (propertize "XYZ" 'invisible t)))))"#,
        expect,
    );
}

#[test]
fn divergence_newline_display_string_at_from_measurement_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((4 . 1) (4 . 1) (4 . 1) (4 . 1) (0 . 0) (0 . 1) (4 . 1) (4 . 1) (4 . 1) (6 . 2) (4 . 2) (4 . 1) (3 . 1) (0 . 1) (1 . 1) (3 . 1) (2 . 2) (4 . 1) 0 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure
       (lambda (setup &optional from to)
         (with-temp-buffer
           (funcall setup)
           (switch-to-buffer (current-buffer))
           (window-text-pixel-size nil from to)))))
  (list
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "\n")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "AB\nCD")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "AB\n")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "\nCD")))
   (funcall measure (lambda () (insert "h")
                      (put-text-property 1 2 'display "AB\n")))
   (funcall measure (lambda () (insert "h")
                      (put-text-property 1 2 'display "AB\nCD")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "A\nB\nC")))
   (funcall measure (lambda () (insert "xhello")
                      (put-text-property 2 3 'display "AB\nCD")) 2 nil)
   (funcall measure (lambda () (insert "xhello")
                      (put-text-property 2 3 'display "\n")) 2 nil)
   (funcall measure (lambda () (insert "ab\nhello")
                      (put-text-property 4 5 'display "AB\nCD")) 4 nil)
   (funcall measure (lambda () (insert "ab\nhello")
                      (put-text-property 4 5 'display "\n")) 4 nil)
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 2)))
                        (overlay-put ov 'display "\n"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 3 'display "AB\nCD")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "AB\nCD")) 1 2)
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "AB\nCD")) 1 3)
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 3 'display "AB\nCD")) 2 nil)
   (funcall measure (lambda () (insert "h\ncd")
                      (put-text-property 1 2 'display "AB\nCD")))
   (funcall measure (lambda () (insert "ab\nhello")
                      (narrow-to-region 4 9)
                      (put-text-property 4 5 'display "AB\nCD")) 4 nil)
   (string-pixel-width (propertize "h" 'display "\n"))
   (string-pixel-width (concat (propertize "h" 'display "AB\nCD") "ello"))))"#,
        expect,
    );
}

#[test]
fn divergence_newline_overlay_string_at_from_measurement_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((5 . 1) (5 . 1) (5 . 1) (5 . 1) (5 . 1) (0 . 1) (0 . 0) (7 . 1) (5 . 1) (5 . 1) (5 . 1) (7 . 2) (5 . 2) (5 . 1) (5 . 1) (7 . 2) (2 . 2) (1 . 2) (1 . 2) (7 . 1) (7 . 1) (5 . 1) (1 . 1) (6 . 2) (5 . 3) (4 . 1) (3 . 1) (10 . 3) (13 . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure
       (lambda (setup &optional from to)
         (with-temp-buffer
           (funcall setup)
           (switch-to-buffer (current-buffer))
           (window-text-pixel-size nil from to)))))
  (list
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "\n"))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "AB\nCD"))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "AB\n"))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "\nCD"))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "A\nB\nC"))))
   (funcall measure (lambda ()
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "AB\nCD"))))
   (funcall measure (lambda ()
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "AB\n"))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "XX"))))
   (funcall measure (lambda () (insert "xhello")
                      (let ((ov (make-overlay 2 2))) (overlay-put ov 'before-string "AB\nCD"))) 2 nil)
   (funcall measure (lambda () (insert "xhello")
                      (let ((ov (make-overlay 2 2))) (overlay-put ov 'before-string "\n"))) 2 nil)
   (funcall measure (lambda () (insert "xhello")
                      (let ((ov (make-overlay 2 2))) (overlay-put ov 'before-string "XX"))) 2 nil)
   (funcall measure (lambda () (insert "ab\nhello")
                      (let ((ov (make-overlay 4 4))) (overlay-put ov 'before-string "AB\nCD"))) 4 nil)
   (funcall measure (lambda () (insert "ab\nhello")
                      (let ((ov (make-overlay 4 4))) (overlay-put ov 'before-string "\n"))) 4 nil)
   (funcall measure (lambda () (insert "ab\nhello") (narrow-to-region 4 9)
                      (let ((ov (make-overlay 4 4))) (overlay-put ov 'before-string "AB\nCD"))) 4 nil)
   (funcall measure (lambda () (insert "xhello")
                      (let ((ov (make-overlay 1 2))) (overlay-put ov 'after-string "AB\nCD"))) 2 nil)
   (funcall measure (lambda () (insert "ab\nhello")
                      (let ((ov (make-overlay 1 4))) (overlay-put ov 'after-string "AB\nCD"))) 4 nil)
   (funcall measure (lambda () (insert "h\ncd")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "AB\nCD"))))
   (funcall measure (lambda () (insert "h\nc")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "ABCD\nEFG"))))
   (funcall measure (lambda () (insert "h\nc")
                      (put-text-property 1 2 'display "ABCD\nEFG")))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1)))
                        (overlay-put ov 'before-string (concat "A" (propertize "\n" 'invisible t) "B")))))
   (funcall measure (lambda () (insert "hello")
                      (let ((o1 (make-overlay 1 1)) (o2 (make-overlay 1 1)))
                        (overlay-put o1 'priority 1) (overlay-put o1 'before-string "AB\n")
                        (overlay-put o2 'priority 2) (overlay-put o2 'before-string "ZZ"))))
   (funcall measure (lambda () (insert "hello")
                      (let ((o1 (make-overlay 1 1)) (o2 (make-overlay 1 1)))
                        (overlay-put o1 'priority 1) (overlay-put o1 'before-string "ZZ")
                        (overlay-put o2 'priority 2) (overlay-put o2 'before-string "AB\nCD"))))
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "AB\nCD"))) 1 2)
   (funcall measure (lambda () (insert "hello")
                      (let ((ov (make-overlay 2 2))) (overlay-put ov 'before-string "AB\nCD"))))
   (funcall measure (lambda () (insert "M-x ")
                      (let ((ov (make-overlay 5 5))) (overlay-put ov 'before-string "\ncand1\ncand2"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "AB\nCD")
                      (let ((ov (make-overlay 1 1))) (overlay-put ov 'before-string "ZZ"))))
   (with-temp-buffer
     (insert "abc")
     (overlay-put (make-overlay (point-min) (point-min)) 'before-string "short\nten-cols!!\nx")
     (switch-to-buffer (current-buffer))
     (buffer-text-pixel-size))
   (with-temp-buffer
     (insert "abc")
     (overlay-put (make-overlay (point-max) (point-max)) 'before-string "short\nten-cols!!\nx")
     (switch-to-buffer (current-buffer))
     (buffer-text-pixel-size))
   (with-temp-buffer
     (insert "abc")
     (overlay-put (make-overlay (point-min) (point-min)) 'before-string "ten-cols!!")
     (switch-to-buffer (current-buffer))
     (buffer-text-pixel-size))))"#,
        expect,
    );
}

#[test]
fn divergence_measurement_hotpath_textget_and_from_defer_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((3 . 1) (11 . 1) (6 . 1) (4 . 1) (14 . 1) (0 . 0) (1 . 1) (0 . 0) (5 . 1) (5 . 1) (5 . 1) (8 . 1) (7 . 1) (6 . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure (lambda (setup &optional from to)
                  (with-temp-buffer
                    (funcall setup)
                    (switch-to-buffer (current-buffer))
                    (window-text-pixel-size nil from to))))
      (ov (lambda (b e &rest props)
            (let ((o (make-overlay b e)))
              (while props (overlay-put o (pop props) (pop props)))
              o))))
  (put 'n5dcat 'invisible t)
  (put 'n5dcatd 'display "QQQQQQQQ")
  (list
   ;; F1: category fallback (G1-G5)
   (funcall measure (lambda () (insert "hello") (put-text-property 1 3 'category 'n5dcat)))
   (funcall measure (lambda () (insert "hello") (put-text-property 1 3 'category 'n5dcatd)))
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'before-string (concat "X" (propertize "ABC" 'category 'n5dcat)))))
   (funcall measure (lambda () (insert "xA")
                      (put-text-property 2 3 'display (concat "AB" (propertize "CD" 'category 'n5dcat) "E"))))
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'before-string (concat "X" (propertize "ABC" 'category 'n5dcatd)))))
   ;; F2: default-text-properties fallback (D3, D4, D19)
   (funcall measure (lambda () (insert "hello") (setq-local default-text-properties '(invisible t))))
   (funcall measure (lambda () (insert "hello") (setq-local default-text-properties '(display "Q"))))
   (funcall measure (lambda () (insert "hello")
                      (setq-local default-text-properties '(invisible t))
                      (funcall ov 1 1 'before-string "ABC")))
   ;; F3: refresh env even when `(when FORM . SPEC)` returns nil (E2, E4, overlay string)
   (funcall measure (lambda () (setq buffer-invisibility-spec nil)
                      (insert "abc" (propertize "d" 'display '(when (progn (setq buffer-invisibility-spec t) nil) . "Q")) "ef")
                      (put-text-property 5 6 'invisible t)))
   (funcall measure (lambda ()
                      (insert "abc" (propertize "d" 'display '(when (progn (setq-local char-property-alias-alist '((invisible myinv))) nil) . "Q")) "ef")
                      (put-text-property 5 6 'myinv t)))
   (funcall measure (lambda () (setq buffer-invisibility-spec nil)
                      (insert "abc")
                      (funcall ov 1 1 'before-string
                        (concat "X"
                          (propertize "Y" 'display '(when (progn (setq buffer-invisibility-spec t) nil) . "Q"))
                          (propertize "W" 'invisible t)))))
   ;; F4: resolved display element decides FROM defer (F1-F3)
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display (concat "A" (propertize "\n" 'invisible t) "B"))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(when nil . "A\nB"))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((margin left-margin) "A\nB"))
                      (funcall ov 1 1 'before-string "ZZ")))))"#,
        expect,
    );
}

#[test]
fn divergence_from_rewind_f5_and_nits_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((5 . 1) (7 . 1) (4 . 1) (5 . 1) (5 . 1) (5 . 1) (4 . 1) (4 . 1) (14 . 1) (14 . 1) (4 . 1) (4 . 1) (6 . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure (lambda (setup &optional from to)
                  (with-temp-buffer
                    (funcall setup)
                    (switch-to-buffer (current-buffer))
                    (window-text-pixel-size nil from to))))
      (ov (lambda (b e &rest props)
            (let ((o (make-overlay b e)))
              (while props (overlay-put o (pop props) (pop props)))
              o))))
  (list
   ;; F5 A1: mid-string newline pops subsequent overlay string
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'priority 1 'before-string "AB\nCD")
                      (funcall ov 1 1 'priority 2 'before-string "ZZ")))
   ;; Control A2: trailing newline stops at char 0 of next overlay string
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'before-string "AB\n" 'after-string "CD")))
   ;; F5 A9: display-string newline inside before-string (string_from_display_prop_p = 1)
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'before-string (propertize "Q" 'display "AB\nCD"))))
   ;; F5 A11: wrapping single-line before-string (100 chars on 80-col TTY)
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'before-string (make-string 100 ?a))))
   ;; F5 A11 boundary: 80 chars wraps at col 79 -> mid-string
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'before-string (make-string 80 ?a))))
   ;; F5 A14: three-overlay mix
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'priority 1 'before-string "AB\n")
                      (funcall ov 1 1 'priority 2 'before-string "C\nDDDD")
                      (funcall ov 1 1 'priority 3 'before-string "ZZ")))
   ;; F5 A17: mid-string newline before-string + overlay display at FROM
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 2 'before-string "AB\nCD" 'display "Q")))
   ;; F5 B8: mid-line FROM before-string newline + display Q
   (funcall measure (lambda () (insert "xhello")
                      (put-text-property 2 3 'display "Q")
                      (funcall ov 2 2 'before-string "AB\nCD")) 2 nil)
   ;; N1 H1 & H1f: ((space :align-to 10) ...) after before-string ZZ
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((space :align-to 10) "A\nB"))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((space :align-to 10) (margin left-margin "A\nB")))
                      (funcall ov 1 1 'before-string "ZZ")))
   ;; N2 J3 & J4: 9 (raise 0) specs before newline display string
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((raise 0) (raise 0) (raise 0) (raise 0) (raise 0) (raise 0) (raise 0) (raise 0) (raise 0) "A\nB"))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((raise 0) (raise 0) (raise 0) (raise 0) (raise 0) (raise 0) (raise 0) (raise 0) (raise 0) "A\nBCDEFGH"))
                      (funcall ov 1 1 'before-string "ZZ")))
   ;; N3 K1: overlay string `when` runs before buffer display at FROM
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "A\nB")
                      (funcall ov 1 1 'before-string
                        (propertize "Z" 'display '(when (progn (put-text-property 1 2 'display "Q") nil) . "X")))))))"#,
        expect,
    );
}

#[test]
fn divergence_display_before_invisible_and_tty_fringe_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((3 . 1) (3 . 1) (4 . 1) (4 . 1) (8 . 1) (8 . 1) (8 . 1) (5 . 1) (6 . 1) (4 . 1) (4 . 1) (4 . 1) (6 . 1) (6 . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure (lambda (setup &optional from to)
                  (with-temp-buffer
                    (funcall setup)
                    (switch-to-buffer (current-buffer))
                    (window-text-pixel-size nil from to))))
      (ov (lambda (b e &rest props)
            (let ((o (make-overlay b e)))
              (while props (overlay-put o (pop props) (pop props)))
              o))))
  (list
   ;; E5: display runs before invisible (GNU it_props[]), hiding current char too
   (funcall measure (lambda ()
                      (insert "abc" (propertize "d" 'display '(when (progn (setq-local default-text-properties '(invisible t)) nil) . "Q")) "ef")))
   (funcall measure (lambda ()
                      (insert "abc" (propertize "ddd" 'display '(when (progn (setq-local default-text-properties '(invisible t)) nil) . "Q")) "ef")))
   ;; Overlay string equivalent: when-nil enabling buffer-invisibility-spec hides Y itself
   (funcall measure (lambda () (setq buffer-invisibility-spec nil) (insert "ab")
                      (funcall ov 2 2 'before-string
                        (concat "X" (propertize "Y" 'invisible t 'display '(when (progn (setq buffer-invisibility-spec t) nil) . "Q")) "Z"))))
   ;; F5: replacing display beats invisible at FROM (and rewinds before-string ZZ)
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'invisible t)
                      (put-text-property 1 2 'display "A\nB")
                      (funcall ov 1 1 'before-string "ZZ")))
   ;; Display beats invisible: text-prop, overlay display, overlay invisible, partial overlap, overlay string
   (funcall measure (lambda () (insert "hello") (put-text-property 2 4 'invisible t) (put-text-property 2 4 'display "QQQQQ")))
   (funcall measure (lambda () (insert "hello") (put-text-property 2 4 'invisible t) (funcall ov 2 4 'display "QQQQQ")))
   (funcall measure (lambda () (insert "hello") (put-text-property 2 4 'display "QQQQQ") (funcall ov 2 4 'invisible t)))
   (funcall measure (lambda () (insert "hello") (put-text-property 2 4 'display "QQQ") (put-text-property 2 5 'invisible t)))
   (funcall measure (lambda () (insert "x")
                      (funcall ov 1 1 'before-string (propertize "ab" 'invisible t 'display "QQQQQ"))))
   ;; F4 & TTY fringe overlay-string skip (src/xdisp.c:6387-6402) + controls
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((left-fringe left-arrow) "A\nB"))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(left-fringe left-arrow))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 2 3 'display '(left-fringe left-arrow))
                      (funcall ov 1 2 'after-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 2 3 'display '(left-fringe left-arrow))
                      (funcall ov 2 3 'after-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((margin left-margin) "M"))
                      (funcall ov 1 1 'before-string "ZZ")))))"#,
        expect,
    );
}

#[test]
fn divergence_unified_tty_fringe_when_and_space_list_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((4 . 1) (7 . 1) (7 . 1) (4 . 1) (4 . 1) (7 . 1) (7 . 1) (6 . 1) (6 . 1) (4 . 1) (7 . 1) (6 . 1) (4 . 1) (6 . 1) (8 . 1) (9 . 1) (8 . 1) (7 . 2) (4 . 1) (8 . 1) (3 . 1) (3 . 1) (3 . 1) (0 . 1) (0 . 1) (3 . 1) (5 . 1) (5 . 1) (5 . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure (lambda (setup &optional from to)
                  (with-temp-buffer
                    (funcall setup)
                    (switch-to-buffer (current-buffer))
                    (window-text-pixel-size nil from to))))
      (ov (lambda (b e &rest props)
            (let ((o (make-overlay b e)))
              (while props (overlay-put o (pop props) (pop props)))
              o))))
  (list
   ;; MEDIUM-1: Lisp-evaluated `when` before TTY fringe + controls
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(when (fboundp 'car) left-fringe left-arrow))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(when (fboundp 'no-such-fn-n5g-xyz) left-fringe left-arrow))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(disable-eval (when (fboundp 'car) left-fringe left-arrow)))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 2 'display '(when (fboundp 'car) left-fringe left-arrow))
                      (funcall ov 1 1 'before-string "ZZ")))
   ;; `when` side-effect creates overlay at pos 1: skipped on fringe, shown on string or nil
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display
                        '(when (progn (unless (overlays-in 1 1) (overlay-put (make-overlay 1 1) 'before-string "ZZ")) t) left-fringe left-arrow))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display
                        '(when (progn (unless (overlays-in 1 1) (overlay-put (make-overlay 1 1) 'before-string "ZZ")) t) . "Q"))))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display
                        '(when (progn (unless (overlays-in 1 1) (overlay-put (make-overlay 1 1) 'before-string "ZZ")) nil) left-fringe left-arrow))))
   ;; MEDIUM-2: `(space ...)` / margin-space vs string before TTY fringe in list/vector
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display [(space :width 2) (left-fringe left-arrow)])
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((left-fringe left-arrow) (space :width 2)))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '("A" (left-fringe left-arrow)))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(((margin left-margin) "M") (left-fringe left-arrow)))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(((margin left-margin) (space :width 2)) (left-fringe left-arrow)))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((space :width 2) (when (fboundp 'car) left-fringe left-arrow)))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((space :width 2) (when nil left-fringe left-arrow)))
                      (funcall ov 1 1 'before-string "ZZ")))
   ;; Live `:align-to` column after scan overlay / after run_end overlay before stretch
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '(space :align-to 5))
                      (funcall ov 1 1 'before-string "ZZ")))
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 2 3 'display '((space :align-to 5) (left-fringe left-arrow)))
                      (funcall ov 3 3 'before-string "XX")))
   ;; Run-end overlays before stretch: multiline at pos 3, EOB, and mid-line FROM
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 2 3 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 3 3 'before-string "A\nBB")))
   (funcall measure (lambda () (insert "hi")
                      (put-text-property 1 3 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 3 3 'before-string "ZZ")))
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 3 4 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 4 4 'before-string "ZZ")) 3 nil)
   ;; Spanning-FROM and ending-at-FROM runs (scan < FROM): stretch & run_end overlays stay in prefix
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 2 5 'display '((space :width 2) (left-fringe left-arrow)))) 3 nil)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 2 5 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 5 5 'before-string "ZZ")) 3 nil)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 2 5 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 5 5 'before-string "A\nBB")) 3 nil)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 2 8 'display '((space :width 2) (left-fringe left-arrow)))) 3 nil)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 2 8 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 8 8 'before-string "ZZ")) 3 nil)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 2 5 'display '(((margin left-margin) (space :width 2)) (left-fringe left-arrow)))
                      (funcall ov 5 5 'before-string "ZZ")) 3 nil)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 2 3 'display '((space :width 2) (left-fringe left-arrow)))) 3 nil)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 2 3 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 3 3 'before-string "ZZ")) 3 nil)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 2 3 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 3 3 'before-string "A\nBB")) 3 nil)))"#,
        expect,
    );
}

#[test]
fn divergence_window_text_pixel_size_x_limit_from_rewind_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((4 . 3) (4 . 2) (3 . 4) (4 . 3) (4 . 2) (3 . 3) (4 . 3) (4 . 2) (4 . 3) (4 . 2) (4 . 3) (3 . 3) (4 . 3) (4 . 2) (4 . 2) (4 . 2) (4 . 3) (-1 . 1) (4 . 1) (-1 . 1) (4 . 2) (5 . 1) (5 . 1) (4 . 1) (4 . 1) (4 . 1) (4 . 2) (2 . 1) (2 . 1) (3 . 3) (3 . 3) (4 . 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((measure (lambda (setup &optional from to xlim ylim)
                  (with-temp-buffer
                    (funcall setup)
                    (switch-to-buffer (current-buffer))
                    (window-text-pixel-size nil from to xlim ylim))))
      (measure-buf (lambda (setup &optional xlim ylim)
                     (with-temp-buffer
                       (funcall setup)
                       (switch-to-buffer (current-buffer))
                       (buffer-text-pixel-size nil nil xlim ylim))))
      (ov (lambda (b e &rest props)
            (let ((o (make-overlay b e)))
              (while props (overlay-put o (pop props) (pop props)))
              o))))
  (list
   ;; N5-g LOW-1: space+fringe + run_end overlay under X-LIMIT
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 3 4 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 4 4 'before-string "ZZZZ")) 3 nil 4)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 3 4 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 4 4 'before-string "ZZZZ")) 3 4 4)
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 2 2 'before-string "ZZZZ")) 1 nil 3)
   ;; Plain overlay at FROM under X-LIMIT
   (funcall measure (lambda () (insert "abhello")
                      (funcall ov 3 3 'before-string "ZZZZ")) 3 nil 4)
   (funcall measure (lambda () (insert "abhello")
                      (funcall ov 3 3 'before-string "ZZZZ")) 3 4 4)
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'before-string "ZZZZ")) 1 nil 3)
   ;; Multi-overlay at mid-line FROM (F1): earlier overlay ending in \n resets before_origin
   (funcall measure (lambda () (insert "abhello")
                      (funcall ov 3 3 'priority 1 'before-string "A\n")
                      (funcall ov 3 3 'priority 2 'before-string "ZZZZZ")) 3 nil 4)
   (funcall measure (lambda () (insert "abhello")
                      (funcall ov 3 3 'priority 1 'before-string "A\n")
                      (funcall ov 3 3 'priority 2 'before-string "ZZZZZ")) 3 4 4)
   (funcall measure (lambda () (insert "abhello")
                      (funcall ov 1 3 'after-string "A\n")
                      (funcall ov 3 3 'before-string "ZZZZZ")) 3 nil 4)
   (funcall measure (lambda () (insert "abhello")
                      (funcall ov 3 3 'priority 1 'before-string "A\n")
                      (funcall ov 3 3 'priority 2 'before-string (make-string 85 ?Z))) 3 nil 4)
   ;; Display string at FROM under X-LIMIT
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 3 4 'display "ZZZZ")) 3 nil 4)
   (funcall measure (lambda () (insert "hello")
                      (put-text-property 1 2 'display "ZZZZ")) 1 nil 3)
   ;; Plain prefix > X-LIMIT + wrap before origin (82/85 a's at body_edge=79) then X-LIMIT=4
   (funcall measure (lambda () (insert "abcdefghij")) 6 nil 4)
   (funcall measure (lambda () (insert "abcdefghij")) 6 7 4)
   (funcall measure (lambda () (insert (concat (make-string 82 ?a) "bcdef"))) 83 85 4)
   (funcall measure (lambda () (insert (concat (make-string 82 ?a) "bcdef"))) 83 nil 4)
   (funcall measure (lambda () (insert (concat (make-string 85 ?a) "bcdef"))) 86 nil 4)
   ;; truncate-lines prefix > X-LIMIT clamps before subtracting start_x -> (-1 . 1)
   (funcall measure (lambda () (setq truncate-lines t) (insert "abcdefghij")) 6 nil 4)
   ;; N3: FROM == TO with origin_x > X-LIMIT: wrapping -> (4 . 1), truncating -> (-1 . 1)
   (funcall measure (lambda () (insert "abcdefghij")) 6 6 4)
   (funcall measure (lambda () (setq truncate-lines t) (insert "abcdefghij")) 6 6 4)
   ;; Controls: wrap past body_edge=79 and explicit \n still rewind; Y-LIMIT + X-LIMIT
   (funcall measure (lambda () (insert "abhello")
                      (funcall ov 3 3 'before-string (make-string 85 ?Z))) 3 nil 4)
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'before-string (make-string 85 ?Z))) 1 nil 100)
   (funcall measure (lambda () (insert "hello")
                      (funcall ov 1 1 'before-string (make-string 85 ?Z))) 1 nil t)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 3 4 'display (make-string 85 ?Z))) 3 nil 4)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 3 4 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 4 4 'before-string "A\nZZZZ")) 3 nil 4)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 3 4 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 4 4 'before-string "ZZZZ")) 3 nil 4 1)
   (funcall measure (lambda () (insert "abhello")
                      (put-text-property 3 4 'display '((space :width 2) (left-fringe left-arrow)))
                      (funcall ov 4 4 'before-string "ZZZZ")) 3 nil 4 2)
   (funcall measure (lambda () (insert "ab")
                      (funcall ov 1 1 'before-string (make-string 85 ?Z))) 1 nil 10 1)
   (funcall measure (lambda () (insert "ab")
                      (funcall ov 1 1 'before-string "123456789012345\nBB")) 1 nil 10 1)
   ;; F5: buffer-text-pixel-size BEGV overlay/display under X-LIMIT + 85-Z rewind control
   (funcall measure-buf (lambda () (insert "hello")
                          (funcall ov 1 1 'before-string "ZZZZ")) 3)
   (funcall measure-buf (lambda () (insert "hello")
                          (put-text-property 1 2 'display "ZZZZ")) 3)
   (funcall measure-buf (lambda () (insert "hello")
                          (funcall ov 1 1 'before-string (make-string 85 ?Z))) 4)))"#,
        expect,
    );
}
