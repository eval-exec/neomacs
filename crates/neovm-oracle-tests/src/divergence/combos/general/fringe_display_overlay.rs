//! Divergence tests: fringe + display property + overlay + text-property + buffer combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_fringe_display_prop_with_margin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t margin-marked t (margin left-margin) t (margin right-margin) t warning t highlight t 4 t 4 t \"AAAA-BBBB-CCCC-DDDD\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (setq left-margin-width 4)
  (setq right-margin-width 4)
  (let ((ov (make-overlay 5 9)))
    (overlay-put ov 'before-string
                 (propertize ">" 'display
                              `(margin left-margin)
                              'face 'warning))
    (overlay-put ov 'after-string
                 (propertize "<" 'display
                              `(margin right-margin)
                              'face 'highlight))
    (overlay-put ov 'tag 'margin-marked)
    (let ((bs (overlay-get ov 'before-string))
          (as (overlay-get ov 'after-string)))
      (list (stringp bs)
            (stringp as)
            (overlay-get ov 'tag)
            (eq (overlay-get ov 'tag) 'margin-marked)
            (get-text-property 0 'display bs)
            (equal (get-text-property 0 'display bs)
                   '(margin left-margin))
            (get-text-property 0 'display as)
            (equal (get-text-property 0 'display as)
                   '(margin right-margin))
            (get-text-property 0 'face bs)
            (eq (get-text-property 0 'face bs) 'warning)
            (get-text-property 0 'face as)
            (eq (get-text-property 0 'face as) 'highlight)
            left-margin-width
            (= left-margin-width 4)
            right-margin-width
            (= right-margin-width 4)
            (buffer-string)
            (= (buffer-size) 17))))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_bitmap_face_per_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 8 0 0 t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (generate-new-buffer " test-fbfpb1-xxx"))
        (buf2 (generate-new-buffer " test-fbfpb2-xxx")))
    (with-current-buffer buf1
      (setq-local left-fringe-width 8)
      (setq-local right-fringe-width 8)
      (insert "BUFFER1")
      (put-text-property 1 7 'buf 'one)
      (setq-local fringe-indicator-alist
                  (cons '(test-buf1-indicator-xxx left-arrow right-arrow)
                        fringe-indicator-alist)))
    (with-current-buffer buf2
      (setq-local left-fringe-width 0)
      (setq-local right-fringe-width 0)
      (insert "BUFFER2")
      (put-text-property 1 7 'buf 'two)
      (setq-local fringe-indicator-alist
                  (cons '(test-buf2-indicator-xxx right-triangle left-triangle)
                        fringe-indicator-alist)))
    (let ((b1-left (buffer-local-value 'left-fringe-width buf1))
          (b1-right (buffer-local-value 'right-fringe-width buf1))
          (b2-left (buffer-local-value 'left-fringe-width buf2))
          (b2-right (buffer-local-value 'right-fringe-width buf2))
          (b1-ind (assq 'test-buf1-indicator-xxx
                        (buffer-local-value 'fringe-indicator-alist buf1)))
          (b2-ind (assq 'test-buf2-indicator-xxx
                        (buffer-local-value 'fringe-indicator-alist buf2))))
      (kill-buffer buf1)
      (kill-buffer buf2)
      (list b1-left b1-right b2-left b2-right
            (= b1-left 8) (= b1-right 8)
            (= b2-left 0) (= b2-right 0)
            (consp b1-ind)
            (eq (nth 1 b1-ind) 'left-arrow)
            (eq (nth 2 b1-ind) 'right-arrow)
            (consp b2-ind)
            (eq (nth 1 b2-ind) 'right-triangle)
            (eq (nth 2 b2-ind) 'left-triangle))))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_overlay_chain_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((left-fringe right-arrow) (right-fringe left-arrow) (left-fringe up-arrow success) (right-fringe down-arrow error)) nil nil nil nil t \"AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH")
  (let ((ov1 (make-overlay 1 3))
        (ov2 (make-overlay 5 7))
        (ov3 (make-overlay 9 11))
        (ov4 (make-overlay 13 15)))
    (overlay-put ov1 'before-string
                 (propertize "1" 'display '(left-fringe right-arrow)))
    (overlay-put ov2 'before-string
                 (propertize "2" 'display '(right-fringe left-arrow)))
    (overlay-put ov3 'before-string
                 (propertize "3" 'display '(left-fringe up-arrow success)))
    (overlay-put ov4 'before-string
                 (propertize "4" 'display '(right-fringe down-arrow error)))
    (let ((ovs (list ov1 ov2 ov3 ov4))
          (specs nil))
      (dolist (ov ovs)
        (let ((bs (overlay-get ov 'before-string)))
          (push (get-text-property 0 'display bs) specs)))
      (list (nreverse specs)
            (equal (nth 0 (nreverse specs))
                   '(left-fringe right-arrow))
            (equal (nth 1 (nreverse specs))
                   '(right-fringe left-arrow))
            (equal (nth 2 (nreverse specs))
                   '(left-fringe up-arrow success))
            (equal (nth 3 (nreverse specs))
                   '(right-fringe down-arrow error))
            (= (length ovs) 4)
            (buffer-string)
            (= (buffer-size) 31))))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_with_narrow_and_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"BB-CC-DD-EE\" 0 1 (block b) 3 4 (block c)) 4 6 t #(\"AA-BB-CC-DD-EE-FF-GG\" 0 1 (block a) 3 4 (block b) 6 7 (block c)) t a t b t active t (left-fringe filled-square) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AA-BB-CC-DD-EE-FF-GG")
  (let ((ov (make-overlay 4 6)))
    (overlay-put ov 'before-string
                 (propertize "*" 'display '(left-fringe filled-square)))
    (overlay-put ov 'fringe-helper 'active)
    (put-text-property 1 2 'block 'a)
    (put-text-property 4 5 'block 'b)
    (put-text-property 7 8 'block 'c)
    (narrow-to-region 4 15)
    (let ((narrowed (buffer-string))
          (ov-start (overlay-start ov))
          (ov-end (overlay-end ov))
          (ov-fringe (overlay-get ov 'fringe-helper)))
      (widen)
      (list narrowed ov-start ov-end
            (eq ov-fringe 'active)
            (buffer-string)
            (= (buffer-size) 20)
            (get-text-property 1 'block)
            (eq (get-text-property 1 'block) 'a)
            (get-text-property 4 'block)
            (eq (get-text-property 4 'block) 'b)
            (overlay-get ov 'fringe-helper)
            (eq (overlay-get ov 'fringe-helper) 'active)
            (get-text-property 0 'display (overlay-get ov 'before-string))
            (equal (get-text-property 0 'display
                     (overlay-get ov 'before-string))
                   '(left-fringe filled-square)))))) "#,
        expect,
    );
}

#[test]
fn divergence_display_space_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t (space :width 5) t (space :width 3) t spacer t \"BEFORE-AFTER\" t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "BEFORE-AFTER")
  (let ((ov (make-overlay 7 7)))
    (overlay-put ov 'before-string
                 (propertize " " 'display '(space :width 5)))
    (overlay-put ov 'after-string
                 (propertize " " 'display '(space :width 3)))
    (overlay-put ov 'tag 'spacer)
    (let ((bs (overlay-get ov 'before-string))
          (as (overlay-get ov 'after-string)))
      (list (stringp bs)
            (string= bs " ")
            (stringp as)
            (string= as " ")
            (get-text-property 0 'display bs)
            (equal (get-text-property 0 'display bs)
                   '(space :width 5))
            (get-text-property 0 'display as)
            (equal (get-text-property 0 'display as)
                   '(space :width 3))
            (overlay-get ov 'tag)
            (eq (overlay-get ov 'tag) 'spacer)
            (buffer-string)
            (string= (buffer-string) "BEFORE-AFTER")
            (= (buffer-size) 13))))) "#,
        expect,
    );
}

#[test]
fn divergence_fringe_with_text_property_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t 16 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "XXXXXXXXXXXXXXXX")
  (put-text-property 1 5 'display '(left-fringe right-triangle))
  (put-text-property 6 10 'display '(right-fringe left-arrow warning))
  (put-text-property 11 16 'display '(left-fringe hollow-square))
  (let ((d1 (get-text-property 1 'display))
        (d2 (get-text-property 6 'display))
        (d3 (get-text-property 11 'display))
        (d-none (get-text-property 5 'display)))
    (list (equal d1 '(left-fringe right-triangle))
          (equal d2 '(right-fringe left-arrow warning))
          (equal d3 '(left-fringe hollow-square))
          (null d-none)
          (buffer-size)
          (= (buffer-size) 16)))) "#,
        expect,
    );
}

#[test]
fn divergence_window_margins_with_fringes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((10 . 5) t t t (0 0 nil nil) t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((w (selected-window)))
    (set-window-margins w 10 5)
    (let ((margins (window-margins w)))
      (set-window-fringes w 6 6 t t)
      (let ((fringes (window-fringes w)))
        (set-window-margins w nil nil)
        (set-window-fringes w 0 0 nil nil)
        (list margins
              (consp margins)
              (= (or (car margins) 0) 10)
              (= (or (cdr margins) 0) 5)
              fringes
              (= (length fringes) 4)
              (>= (nth 0 fringes) 0)
              (>= (nth 1 fringes) 0)))))) "#,
        expect,
    );
}

#[test]
fn divergence_truncate_lines_with_fringe_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t truncate t (right-fringe right-arrow) t (right-fringe left-arrow) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq truncate-lines t)
  (insert (make-string 200 ?X))
  (let ((ov (make-overlay 1 201)))
    (overlay-put ov 'before-string
                 (propertize ">" 'display '(right-fringe right-arrow)))
    (overlay-put ov 'after-string
                 (propertize "<" 'display '(right-fringe left-arrow)))
    (overlay-put ov 'wrap-indicator 'truncate)
    (list truncate-lines
          (eq truncate-lines t)
          (= (buffer-size) 200)
          (overlay-get ov 'wrap-indicator)
          (eq (overlay-get ov 'wrap-indicator) 'truncate)
          (get-text-property 0 'display (overlay-get ov 'before-string))
          (equal (get-text-property 0 'display
                   (overlay-get ov 'before-string))
                 '(right-fringe right-arrow))
          (get-text-property 0 'display (overlay-get ov 'after-string))
          (equal (get-text-property 0 'display
                   (overlay-get ov 'after-string))
                 '(right-fringe left-arrow))
          (= (overlay-start ov) 1)
          (= (overlay-end ov) 201)))) "#,
        expect,
    );
}

#[test]
fn divergence_word_wrap_fringe_indicators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq word-wrap t)
  (setq truncate-lines nil)
  (setq fringe-indicator-alist
        (cons '(continuation (left-curly-arrow) (right-curly-arrow))
              fringe-indicator-alist))
  (insert "This is a very long line that should wrap when displayed and show fringe indicators at the wrap points in visual line mode")
  (let ((cont (assq 'continuation fringe-indicator-alist)))
    (list word-wrap
          (eq word-wrap t)
          truncate-lines
          (null truncate-lines)
          (consp cont)
          (>= (safe-length cont) 2)
          (consp (nth 1 cont))
          (eq (car (nth 1 cont)) 'left-curly-arrow)
          (consp (nth 2 cont))
          (eq (car (nth 2 cont)) 'right-curly-arrow)
          (> (buffer-size) 100)
          (= (buffer-size) 131)))) "#,
        expect,
    );
}

#[test]
fn divergence_visual_line_fringe_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil t t nil nil t ((funcall #'#[0 \"��\" [(nil nil)] 1])))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'fringe)
  (let ((orig-vlfi visual-line-fringe-indicators)
        (orig-fia fringe-indicator-alist))
    (setq visual-line-fringe-indicators '(left-curly-arrow right-curly-arrow))
    (let ((fia-cont (assq 'continuation fringe-indicator-alist))
          (fia-trunc (assq 'truncation fringe-indicator-alist)))
      (setq visual-line-fringe-indicators orig-vlfi)
      (setq fringe-indicator-alist orig-fia)
      (list (equal visual-line-fringe-indicators '(left-curly-arrow right-curly-arrow))
            (listp visual-line-fringe-indicators)
            (= (length visual-line-fringe-indicators) 2)
            (eq (car visual-line-fringe-indicators) 'left-curly-arrow)
            (eq (cadr visual-line-fringe-indicators) 'right-curly-arrow)
            (boundp 'visual-line-fringe-indicators)
            (custom-variable-p 'visual-line-fringe-indicators))))) "#,
        expect,
    );
}
