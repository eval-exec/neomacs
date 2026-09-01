//! Divergence tests: window configuration + buffer list + point + marker.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_buffer_list_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\" test-blo1-xxx\") (\" test-blo2-xxx\") (\" test-blo3-xxx\") t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer " test-blo1-xxx"))
        (b2 (generate-new-buffer " test-blo2-xxx"))
        (b3 (generate-new-buffer " test-blo3-xxx")))
    (with-current-buffer b1 (insert "BBB"))
    (with-current-buffer b2 (insert "AAA"))
    (with-current-buffer b3 (insert "CCC"))
    (let ((visible (mapcar (lambda (b)
                             (cons (buffer-name b)
                                   (buffer-local-value 'buffer-read-only b)))
                           (buffer-list))))
      (let ((b1-visible (assoc (buffer-name b1) visible))
            (b2-visible (assoc (buffer-name b2) visible))
            (b3-visible (assoc (buffer-name b3) visible)))
        (kill-buffer b1)
        (kill-buffer b2)
        (kill-buffer b3)
        (list b1-visible b2-visible b3-visible
              (consp b1-visible)
              (consp b2-visible)
              (consp b3-visible)
              (null (cdr b1-visible))
              (null (cdr b2-visible))
              (null (cdr b3-visible))))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_name_unique() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (t t t t t 0 0 t t t t t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((v1 (generate-new-buffer "test-bnu-xxx"))
        (v2 (generate-new-buffer "test-bnu-xxx"))
        (v3 (generate-new-buffer "test-bnu-xxx"))
        (h1 (generate-new-buffer " test-bnu-xxx"))
        (h2 (generate-new-buffer " test-bnu-xxx"))
        (h3 (generate-new-buffer " test-bnu-xxx")))
    (let ((visible-names (mapcar 'buffer-name (list v1 v2 v3)))
          (hidden-names (mapcar 'buffer-name (list h1 h2 h3))))
      (list
       (= (length visible-names) 3)
       (equal visible-names
              '("test-bnu-xxx" "test-bnu-xxx<2>" "test-bnu-xxx<3>"))
       (= (length (delete-dups (copy-sequence visible-names))) 3)
       (= (length hidden-names) 3)
       (string= (nth 0 hidden-names) " test-bnu-xxx")
       (string-match-p "\\` test-bnu-xxx-[0-9]+\\'" (nth 1 hidden-names))
       (string-match-p "\\` test-bnu-xxx-[0-9]+\\'" (nth 2 hidden-names))
       (= (length (delete-dups (copy-sequence hidden-names))) 3)
       (buffer-live-p v1) (buffer-live-p v2) (buffer-live-p v3)
       (buffer-live-p h1) (buffer-live-p h2) (buffer-live-p h3)
       (kill-buffer v1) (kill-buffer v2) (kill-buffer v3)
       (kill-buffer h1) (kill-buffer h2) (kill-buffer h3)
       (not (buffer-live-p v1)) (not (buffer-live-p v2)) (not (buffer-live-p v3))
       (not (buffer-live-p h1)) (not (buffer-live-p h2)) (not (buffer-live-p h3)))))) "#,
        expect,
    );
}

#[test]
fn divergence_marker_point_in_multiple_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (2 7 8 nil nil t #(\"AAXXAA\" 0 2 (buf b1) 4 5 (buf b1)) #(\"YYBBBBBBBB\" 2 9 (buf b2)) #(\"CCCCCCCCCC\" 0 9 (buf b3)) nil t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer " test-mpimb1-xxx"))
        (b2 (generate-new-buffer " test-mpimb2-xxx"))
        (b3 (generate-new-buffer " test-mpimb3-xxx")))
    (with-current-buffer b1
      (insert "AAAA")
      (put-text-property 1 4 'buf 'b1)
      (setq mark1 (copy-marker 2 t)))
    (with-current-buffer b2
      (insert "BBBBBBBB")
      (put-text-property 1 8 'buf 'b2)
      (setq mark2 (copy-marker 5 nil)))
    (with-current-buffer b3
      (insert "CCCCCCCCCC")
      (put-text-property 1 10 'buf 'b3)
      (setq mark3 (copy-marker 8 t)))
    (with-current-buffer b1
      (goto-char 3)
      (insert "XX"))
    (with-current-buffer b2
      (goto-char 1)
      (insert "YY"))
    (let ((p1 (marker-position mark1))
          (p2 (marker-position mark2))
          (p3 (marker-position mark3)))
      (list p1 p2 p3
            (= p1 4) (= p2 5) (= p3 8)
            (with-current-buffer b1 (buffer-string))
            (with-current-buffer b2 (buffer-string))
            (with-current-buffer b3 (buffer-string))
            (get-text-property 1 'buf (buffer-base-buffer b1))
            (kill-buffer b1) (kill-buffer b2) (kill-buffer b3))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_swap_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"CONTENT-1\" 0 8 (src buf1)) #(\"CONTENT-2\" 0 8 (src buf2)) buf1 buf2 #(\"CONTENT-2\" 0 8 (src buf2)) #(\"CONTENT-1\" 0 8 (src buf1)) t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer " test-bsc1-xxx"))
        (b2 (generate-new-buffer " test-bsc2-xxx")))
    (with-current-buffer b1
      (insert "CONTENT-1")
      (put-text-property 1 9 'src 'buf1))
    (with-current-buffer b2
      (insert "CONTENT-2")
      (put-text-property 1 9 'src 'buf2))
    (let ((s1 (with-current-buffer b1 (buffer-string)))
          (s2 (with-current-buffer b2 (buffer-string)))
          (p1 (with-current-buffer b1 (get-text-property 1 'src)))
          (p2 (with-current-buffer b2 (get-text-property 1 'src))))
      (with-current-buffer b1
        (erase-buffer)
        (insert s2))
      (with-current-buffer b2
        (erase-buffer)
        (insert s1))
      (let ((ns1 (with-current-buffer b1 (buffer-string)))
            (ns2 (with-current-buffer b2 (buffer-string))))
        (kill-buffer b1)
        (kill-buffer b2)
        (list s1 s2 p1 p2 ns1 ns2
              (string= ns1 "CONTENT-2")
              (string= ns2 "CONTENT-1")
              (eq p1 'buf1)
              (eq p2 'buf2)))))) "#,
        expect,
    );
}

#[test]
fn divergence_point_marker_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 11 3 13 nil t 3 nil 13 t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (put-text-property 1 5 'half 'first)
  (put-text-property 6 10 'half 'second)
  (let ((m (point-marker)))
    (goto-char 5)
    (let ((p5 (point))
          (mp (marker-position m)))
      (save-excursion
        (goto-char 1)
        (insert "XX")
        (let ((inside-exc (point))
              (m-inside (marker-position m)))
          (list p5 mp inside-exc m-inside
                (= inside-exc 1)
                (= m-inside (+ mp 2))
                (point)
                (= (point) 7)
                (marker-position m)
                (= (marker-position m) (+ mp 2))
                (get-text-property 1 'half)
                (eq (get-text-property 1 'half) 'first))))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_modified_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((tick1 (buffer-modified-tick))
        (chars-mod1 (buffer-chars-modified-tick)))
    (insert "hello")
    (let ((tick2 (buffer-modified-tick))
          (chars-mod2 (buffer-chars-modified-tick)))
      (put-text-property 1 5 'test 'val)
      (let ((tick3 (buffer-modified-tick))
            (chars-mod3 (buffer-chars-modified-tick)))
        (list (>= tick2 tick1)
              (> tick2 tick1)
              (>= chars-mod2 chars-mod1)
              (> chars-mod2 chars-mod1)
              (>= tick3 tick2)
              (> tick3 tick2)
              (>= chars-mod3 chars-mod2)
              (= chars-mod3 chars-mod2)
              (= (buffer-size) 5)
              (string= (buffer-string) "hello")
              (get-text-property 1 'test)
              (eq (get-text-property 1 'test) 'val))))) "#,
        expect,
    );
}

#[test]
fn divergence_narrow_widen_marker_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"XXBBBB-CCC\" 2 5 (section b) 7 10 (section c)) (1 8 13 18 23) #(\"AAAA-XXBBBB-CCCC-DDDD-EEEE\" 0 3 (section a) 7 10 (section b) 12 15 (section c) 17 20 (section d) 22 25 (section e)) t 1 t 8 t 13 t 18 nil 23 nil a t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((m1 (copy-marker 1 t))
        (m2 (copy-marker 6 t))
        (m3 (copy-marker 11 t))
        (m4 (copy-marker 16 t))
        (m5 (copy-marker 21)))
    (put-text-property 1 4 'section 'a)
    (put-text-property 6 9 'section 'b)
    (put-text-property 11 14 'section 'c)
    (put-text-property 16 19 'section 'd)
    (put-text-property 21 24 'section 'e)
    (narrow-to-region 6 14)
    (goto-char (point-min))
    (insert "XX")
    (let ((narrowed-s (buffer-string))
          (positions (mapcar 'marker-position (list m1 m2 m3 m4 m5))))
      (widen)
      (list narrowed-s positions
            (buffer-string)
            (= (buffer-size) 26)
            (marker-position m1)
            (= (marker-position m1) 1)
            (marker-position m2)
            (= (marker-position m2) 8)
            (marker-position m3)
            (= (marker-position m3) 13)
            (marker-position m4)
            (= (marker-position m4) 16)
            (marker-position m5)
            (= (marker-position m5) 21)
            (get-text-property 1 'section)
            (eq (get-text-property 1 'section) 'a)
            (get-text-property 6 'section)
            (eq (get-text-property 6 'section) 'b))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_visibility_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil t t t t test-bvf-xxx t 46 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "VISIBLE1-INVISIBLE-VISIBLE2-INVISIBLE-VISIBLE3")
  (let ((ov1 (make-overlay 9 18))
        (ov2 (make-overlay 27 36)))
    (overlay-put ov1 'invisible 'test-bvf-xxx)
    (overlay-put ov2 'invisible 'test-bvf-xxx)
    (let ((invis1 (buffer-substring 1 8))
          (invis2 (buffer-substring 19 26))
          (invis3 (buffer-substring 37 45)))
      (list (string= invis1 "VISIBLE1")
            (string= invis2 "VISIBLE2")
            (string= invis3 "VISIBLE3")
            (= (overlay-start ov1) 9)
            (= (overlay-end ov1) 18)
            (= (overlay-start ov2) 27)
            (= (overlay-end ov2) 36)
            (overlay-get ov1 'invisible)
            (eq (overlay-get ov1 'invisible) 'test-bvf-xxx)
            (buffer-size)
            (= (buffer-size) 45))))) "#,
        expect,
    );
}

#[test]
fn divergence_temp_buffer_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"temp content\" 0 3 (tag temp)) t temp t t t) #(\"temp content\" 0 3 (tag temp)) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((result nil)
        (buf-name nil))
    (with-temp-buffer
      (setq buf-name (buffer-name))
      (insert "temp content")
      (put-text-property 1 4 'tag 'temp)
      (setq result
            (list (buffer-string)
                  (string= (buffer-string) "temp content")
                  (get-text-property 1 'tag)
                  (eq (get-text-property 1 'tag) 'temp)
                  (= (buffer-size) 12)
                  (buffer-live-p (current-buffer)))))
    (list result
          (car result)
          (eq (nth 2 result) 'temp)
          (eq (nth 5 result) t)))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_local_variables_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (default buf1-val buf2-val b1 b2 t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-blvc-xxx 'default)
  (let ((b1 (generate-new-buffer " test-blvc1-xxx"))
        (b2 (generate-new-buffer " test-blvc2-xxx")))
    (with-current-buffer b1
      (setq-local test-blvc-xxx 'buf1-val)
      (insert "B1")
      (put-text-property 1 2 'buf 'b1))
    (with-current-buffer b2
      (setq-local test-blvc-xxx 'buf2-val)
      (insert "B2")
      (put-text-property 1 2 'buf 'b2))
    (let ((v-default test-blvc-xxx)
          (v-b1 (buffer-local-value 'test-blvc-xxx b1))
          (v-b2 (buffer-local-value 'test-blvc-xxx b2))
          (p-b1 (with-current-buffer b1 (get-text-property 1 'buf)))
          (p-b2 (with-current-buffer b2 (get-text-property 1 'buf))))
      (kill-buffer b1)
      (kill-buffer b2)
      (list v-default v-b1 v-b2 p-b1 p-b2
            (eq v-default 'default)
            (eq v-b1 'buf1-val)
            (eq v-b2 'buf2-val)
            (eq p-b1 'b1)
            (eq p-b2 'b2))))) "#,
        expect,
    );
}
