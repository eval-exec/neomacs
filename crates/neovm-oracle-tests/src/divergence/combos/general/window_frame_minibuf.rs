//! Divergence tests: window + frame + minibuffer + buffer combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_window_configuration_save_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t #(\"WINDOW-CONFIG-TEST\" 0 7 (tag wconf)) t wconf nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((wconf (current-window-configuration))
        (w (selected-window))
        (b (current-buffer)))
    (insert "WINDOW-CONFIG-TEST")
    (put-text-property 1 8 'tag 'wconf)
    (let ((s1 (buffer-string))
          (p1 (point))
          (w1 (selected-window)))
      (list (windowp w1)
            (window-live-p w1)
            (bufferp b)
            (buffer-live-p b)
            (window-configuration-p wconf)
            s1
            (string= s1 "WINDOW-CONFIG-TEST")
            (get-text-property 1 'tag)
            (eq (get-text-property 1 'tag) 'tag))))) "#,
        expect,
    );
}

#[test]
fn divergence_window_edges_and_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function window-total-edges)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((w (selected-window)))
    (let ((edges (window-edges w))
          (body-edges (window-body-edges w))
          (pixel-edges (window-pixel-edges w))
          (total-edges (window-total-edges w)))
      (list (= (length edges) 4)
            (= (length body-edges) 4)
            (= (length pixel-edges) 4)
            (= (length total-edges) 4)
            (<= (nth 0 edges) (nth 2 edges))
            (<= (nth 1 edges) (nth 3 edges))
            (<= (nth 0 body-edges) (nth 2 body-edges))
            (<= (nth 0 pixel-edges) (nth 2 pixel-edges))
            (<= (nth 0 total-edges) (nth 2 total-edges))
            (window-total-width w)
            (>= (window-total-width w) 0)
            (window-total-height w)
            (>= (window-total-height w) 0)
            (window-body-width w t)
            (>= (window-body-width w t) 0)
            (window-body-height w t)
            (>= (window-body-height w t) 0))))) "#,
        expect,
    );
}

#[test]
fn divergence_frame_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t nil t nil t t t (t) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((f (selected-frame)))
    (let ((params (frame-parameters f)))
      (list (consp params)
            (plist-get params 'name)
            (stringp (or (plist-get params 'name) ""))
            (plist-get params 'width)
            (integerp (or (plist-get params 'width) 0))
            (plist-get params 'height)
            (integerp (or (plist-get params 'height) 0))
            (frame-live-p f)
            (framep f)
            (memq (framep f) '(x w32 ns pc t))
            (not (framep 'nonexistent-xxx)))))) "#,
        expect,
    );
}

#[test]
fn divergence_minibuffer_window_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((mw (minibuffer-window)))
    (list (windowp mw)
          (window-live-p mw)
          (window-minibuffer-p mw)
          (eq (window-minibuffer-p mw) t)
          (not (window-minibuffer-p (selected-window)))
          (bufferp (window-buffer mw))
          (buffer-live-p (window-buffer mw))
          (minibufferp (window-buffer mw))
          (eq (minibufferp (window-buffer mw)) t)
          (active-minibuffer-window)
          (null (active-minibuffer-window))))) "#,
        expect,
    );
}

#[test]
fn divergence_window_buffer_relationship() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let* ((buf1 (generate-new-buffer " test-wbr1-xxx"))
         (buf2 (generate-new-buffer " test-wbr2-xxx"))
         (w (selected-window)))
    (with-current-buffer buf1
      (insert "BUFFER1")
      (put-text-property 1 7 'src 'buf1))
    (with-current-buffer buf2
      (insert "BUFFER2")
      (put-text-property 1 7 'src 'buf2))
    (set-window-buffer w buf1)
    (let ((wb1 (window-buffer w))
          (s1 (with-current-buffer buf1 (buffer-string)))
          (p1 (with-current-buffer buf1 (get-text-property 1 'src))))
      (set-window-buffer w buf2)
      (let ((wb2 (window-buffer w))
            (s2 (with-current-buffer buf2 (buffer-string)))
            (p2 (with-current-buffer buf2 (get-text-property 1 'src))))
        (set-window-buffer w buf1)
        (kill-buffer buf1)
        (kill-buffer buf2)
        (list (eq wb1 buf1)
              (eq wb2 buf2)
              (string= s1 "BUFFER1")
              (string= s2 "BUFFER2")
              (eq p1 'buf1)
              (eq p2 'buf2)))))) "#,
        expect,
    );
}

#[test]
fn divergence_window_point_and_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 500 ?X))
  (let ((w (selected-window)))
    (set-window-point w 100)
    (let ((wp (window-point w))
          (bp (buffer-size)))
      (set-window-point w 1)
      (list (>= wp 1)
            (<= wp bp)
            (= wp 100)
            (window-point w)
            (= (window-point w) 1)
            (= bp 500))))) "#,
        expect,
    );
}

#[test]
fn divergence_window_hscroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 200 ?Y))
  (let ((w (selected-window)))
    (set-window-hscroll w 10)
    (let ((hs1 (window-hscroll w)))
      (set-window-hscroll w 0)
      (let ((hs2 (window-hscroll w)))
        (list (>= hs1 0)
              (>= hs2 0)
              (<= hs2 hs1)
              (integerp hs1)
              (integerp hs2)))))) "#,
        expect,
    );
}

#[test]
fn divergence_window_dedicated() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((w (selected-window)))
    (list (window-dedicated-p w)
          (null (window-dedicated-p w))
          (set-window-dedicated-p w t)
          (window-dedicated-p w)
          (eq (window-dedicated-p w) t)
          (set-window-dedicated-p w nil)
          (window-dedicated-p w)
          (null (window-dedicated-p w))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_display_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t \"DISPLAY-TIME-TEST\" t t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "DISPLAY-TIME-TEST")
  (let ((buf (current-buffer))
        (w (selected-window)))
    (list (bufferp buf)
          (windowp w)
          (eq (window-buffer w) buf)
          (buffer-string)
          (string= (buffer-string) "DISPLAY-TIME-TEST")
          (buffer-modified-p)
          (null (buffer-modified-p))
          (set-buffer-modified-p nil)
          (null (buffer-modified-p))))) "#,
        expect,
    );
}

#[test]
fn divergence_frame_list_and_selected() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((frames (frame-list))
        (sel (selected-frame)))
    (list (consp frames)
          (>= (length frames) 1)
          (and (memq sel frames) t)
          (frame-live-p sel)
          (and (memq sel frames) t)
          (consp (visible-frame-list))
          (>= (length (visible-frame-list)) 1)
          (eq (selected-frame) sel)
          (frame-live-p (next-frame sel))))) "#,
        expect,
    );
}
