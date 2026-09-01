//! Combo: cl-print-unreadable-object + marker + overlay + textprop + buflocal + narrow + undo.
//! Tests print-unreadable-object with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_cl_print_unreadable_marker_overlay_textprop_buflocal_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "pub")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 15)
        (undo-boundary)
        (let* ((ht (make-hash-table :test 'equal))
               (_ (puthash "a" 1 ht))
               (_ (puthash "b" 2 ht))
               (_ (puthash "c" 3 ht))
               (_ (puthash "d" 4 ht))
               (_ (puthash "e" 5 ht))
               (result (cl-loop for k being the hash-keys of ht
                                using (hash-values v)
                                collect (list k v))))
          (put-text-property (point-min) (point-max) 'z 'changed)
          (setf (char-after (point-min)) ?Z)
          (setf (marker-position m) 11)
          (goto-char (point-min))
          (insert (format "%s-%s-" result my-var)))
        (undo-boundary)
        (let ((v my-var)
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'z))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list v mp os oe k bs
                my-var
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_cl_print_unreadable_clone_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "puc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "puc-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (narrow-to-region 6 15)
          (undo-boundary)
          (let* ((ht (make-hash-table :test 'equal))
                 (_ (puthash "x" 10 ht))
                 (_ (puthash "y" 20 ht))
                 (_ (puthash "z" 30 ht))
                 (result (cl-loop for k being the hash-keys of ht
                                  using (hash-values v)
                                  collect (list k v))))
            (put-text-property (point-min) (point-max) 'z 'changed)
            (setf (char-after (point-min)) ?Z)
            (setf (marker-position m) 11)
            (goto-char (point-min))
            (insert (format "%s-%s-" result my-var)))
          (undo-boundary)
          (let ((v my-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'z))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v mp os oe k bs
                  my-var
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_cl_print_unreadable_multi_buffer_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "pu1"))
        (b2 (generate-new-buffer "pu2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local my-var 'b1-base))
    (with-current-buffer b2
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'z 'd)
      (put-text-property 6 10 'z 'e)
      (put-text-property 11 15 'z 'f)
      (setq-local my-var 'b2-base))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 6 10)))
                    (overlay-put ov 'face 'bold) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 6 10)))
                    (overlay-put ov 'face 'italic) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m))))
      (with-current-buffer b1
        (undo-boundary)
        (let* ((ht (make-hash-table :test 'equal))
               (_ (puthash "a" 1 ht))
               (_ (puthash "b" 2 ht))
               (result (cl-loop for k being the hash-keys of ht
                                using (hash-values v)
                                collect (list k v))))
          (goto-char 6)
          (insert (format "%s-%s-" result my-var)))
        (undo-boundary))
      (with-current-buffer b2
        (undo-boundary)
        (let* ((ht (make-hash-table :test 'equal))
               (_ (puthash "d" 4 ht))
               (_ (puthash "e" 5 ht))
               (result (cl-loop for k being the hash-keys of ht
                                using (hash-values v)
                                collect (list k v))))
          (goto-char 6)
          (insert (format "%s-%s-" result my-var)))
        (undo-boundary))
      (let ((mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2))
            (s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string))))
        (with-current-buffer b1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer b2
          (primitive-undo 1 buffer-undo-list))
        (list mp1 mp2 os1 oe1 os2 oe2 s1 s2
              (marker-position m1)
              (marker-position m2)
              (with-current-buffer b1 (buffer-string))
              (with-current-buffer b2 (buffer-string)))))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_cl_print_unreadable_setf_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "psr")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (let* ((ht (make-hash-table :test 'equal))
               (_ (puthash "a" 1 ht))
               (_ (puthash "b" 2 ht))
               (_ (puthash "c" 3 ht))
               (result (cl-loop for k being the hash-keys of ht
                                using (hash-values v)
                                collect (list k v))))
          (setf (char-after 6) ?Z)
          (setf (marker-position m) 11)
          (goto-char 6)
          (re-search-forward "BBBB")
          (replace-match (format "%s-%s" result my-var)))
        (undo-boundary)
        (let ((v my-var)
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list v mp os oe s
                my-var
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_cl_print_unreadable_multi_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "pmo")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (put-text-property 21 25 'z 'e)
      (setq-local my-var 'base)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (let* ((ht (make-hash-table :test 'equal))
               (_ (puthash "a" 1 ht))
               (_ (puthash "b" 2 ht))
               (_ (puthash "c" 3 ht))
               (_ (puthash "d" 4 ht))
               (result (cl-loop for k being the hash-keys of ht
                                using (hash-values v)
                                collect (list k v))))
          (setf (char-after 6) ?Z)
          (setf (marker-position m) 11)
          (goto-char 6)
          (insert (format "%s-%s-" result my-var)))
        (undo-boundary)
        (let ((v my-var)
              (mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list v mp os1 oe1 os2 oe2 s
                my-var
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
