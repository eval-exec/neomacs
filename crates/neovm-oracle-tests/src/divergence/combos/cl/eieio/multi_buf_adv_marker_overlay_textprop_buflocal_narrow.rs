//! Combo: cl-eieio buffer-list / get-buffer-create / kill-buffer + overlays
//! + markers + textprop + buflocal + narrow + undo.
//! Tests multi-buffer operations with shared markers, overlays that reference
//! other buffers, and buffer lifecycle interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_multi_buf_marker_overlay_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable my-val)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mbuf-snap ()
    ((step :initarg :step :accessor mbs-step :initform "")
     (buf1-len :initarg :b1 :accessor mbs-b1 :initform 0)
     (buf2-len :initarg :b2 :accessor mbs-b2 :initform 0)
     (m-pos :initarg :m-pos :accessor mbs-mp :initform 0)))
  (let* ((buf1 (generate-new-buffer "mb1a"))
         (buf2 (generate-new-buffer "mb1b"))
         (snaps nil))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (with-current-buffer buf2
          (insert "XXXX-YYYY-ZZZZ")
          (put-text-property 1 5 'face 'error)
          (put-text-property 6 10 'face 'success)
          (put-text-property 11 15 'face 'warning))
        (push (mbuf-snap :step "init"
                        :b1 (with-current-buffer buf1 (point-max))
                        :b2 (with-current-buffer buf2 (point-max))
                        :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "MM")
        (push (mbuf-snap :step "edit-buf1"
                        :b1 (point-max)
                        :b2 (with-current-buffer buf2 (point-max))
                        :m-pos (marker-position m)) snaps)
        (let ((sub (buffer-substring 6 15)))
          (with-current-buffer buf2
            (goto-char (point-max))
            (insert sub)
            (setq-local buf2-edited t)))
        (push (mbuf-snap :step "cross-insert"
                        :b1 (point-max)
                        :b2 (with-current-buffer buf2 (point-max))
                        :m-pos (marker-position m)) snaps)
        (with-current-buffer buf2
          (setq-local my-val 42)
          (goto-char 6)
          (insert "QQ"))
        (push (mbuf-snap :step "edit-buf2"
                        :b1 (point-max)
                        :b2 (with-current-buffer buf2 (point-max))
                        :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mbs-step s) (mbs-b1 s)
                                                (mbs-b2 s) (mbs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mbs-log t)
        (list (buffer-string)
              (with-current-buffer buf2 (buffer-string))
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (with-current-buffer buf2 (default-value 'my-val)))))
    (kill-buffer buf1)
    (kill-buffer buf2)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_multi_buf_kill_overlay_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass kill-buf-snap ()
    ((step :initarg :step :accessor kbs-step :initform "")
     (buf-live :initarg :live :accessor kbs-live :initform nil)
     (m-pos :initarg :m-pos :accessor kbs-mp :initform 0)
     (ov-alive :initarg :ov :accessor kbs-ov :initform nil)))
  (let* ((buf1 (generate-new-buffer "mb2a"))
         (buf2 (generate-new-buffer "mb2b"))
         (snaps nil)
         (m nil)
         (ov nil))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (setq-local my-shared-val 10)
      (setq m (set-marker (make-marker) 10))
      (setq ov (make-overlay 6 15))
      (overlay-put ov 'face 'shadow)
      (overlay-put ov 'priority 5))
    (with-current-buffer buf2
      (insert "XXXX-YYYY-ZZZZ"))
    (push (kill-buf-snap :step "init"
                        :live (buffer-live-p buf1)
                        :m-pos (marker-position m)
                        :ov (overlay-live-p ov)) snaps)
    (with-current-buffer buf1
      (goto-char 8)
      (insert "PP"))
    (push (kill-buf-snap :step "edit"
                        :live (buffer-live-p buf1)
                        :m-pos (marker-position m)
                        :ov (overlay-live-p ov)) snaps)
    (kill-buffer buf1)
    (push (kill-buf-snap :step "killed"
                        :live (buffer-live-p buf1)
                        :m-pos (marker-position m)
                        :ov (overlay-live-p ov)) snaps)
    (with-current-buffer buf2
      (goto-char 5)
      (insert "RR"))
    (push (kill-buf-snap :step "edit-buf2"
                        :live (buffer-live-p buf1)
                        :m-pos (marker-position m)
                        :ov (overlay-live-p ov)) snaps)
    (setq snaps (reverse snaps))
    (with-current-buffer buf2
      (goto-char (point-max))
      (insert (format " | snaps=%S"
                     (mapcar (lambda (s) (list (kbs-step s) (kbs-live s)
                                              (kbs-mp s) (kbs-ov s))) snaps))))
    (list (with-current-buffer buf2 (buffer-string))
          (length snaps)
          (marker-position m)
          (overlay-live-p ov)
          (buffer-live-p buf1))
    (kill-buffer buf2)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_multi_buf_get_buffer_create_reuse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass reuse-snap ()
    ((step :initarg :step :accessor rs-step :initform "")
     (buf-name :initarg :name :accessor rs-name :initform "")
     (buf-len :initarg :bl :accessor rs-bl :initform 0)
     (m-pos :initarg :m-pos :accessor rs-mp :initform 0)))
  (let* ((name " *test-reuse-buf*")
         (snaps nil)
         (m nil)
         (ov nil))
    (when (get-buffer name) (kill-buffer name))
    (let ((buf (get-buffer-create name)))
      (with-current-buffer buf
        (insert "AAAA-BBBB-CCCC")
        (put-text-property 1 5 'face 'bold)
        (put-text-property 6 10 'face 'italic)
        (put-text-property 11 15 'face 'underline)
        (setq-local my-reuse-count 1)
        (setq m (set-marker (make-marker) 8))
        (setq ov (make-overlay 3 10))
        (overlay-put ov 'face 'shadow)
        (overlay-put ov 'priority 5))
      (push (reuse-snap :step "create"
                       :name (buffer-name buf)
                       :bl (with-current-buffer buf (point-max))
                       :m-pos (marker-position m)) snaps)
      (with-current-buffer buf
        (goto-char 5)
        (insert "XX"))
      (push (reuse-snap :step "edit"
                       :name (buffer-name buf)
                       :bl (with-current-buffer buf (point-max))
                       :m-pos (marker-position m)) snaps)
      (kill-buffer buf)
      (let ((buf2 (get-buffer-create name)))
        (push (reuse-snap :step "reuse"
                         :name (buffer-name buf2)
                         :bl (with-current-buffer buf2 (point-max))
                         :m-pos (marker-position m)) snaps)
        (with-current-buffer buf2
          (insert "NEW-CONTENT")
          (setq-local my-reuse-count 2))
        (push (reuse-snap :step "new-content"
                         :name (buffer-name buf2)
                         :bl (with-current-buffer buf2 (point-max))
                         :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (with-current-buffer buf2
          (goto-char (point-max))
          (insert (format " | snaps=%S"
                         (mapcar (lambda (s) (list (rs-step s) (rs-name s)
                                                  (rs-bl s) (rs-mp s))) snaps))))
        (prog1
            (list (with-current-buffer buf2 (buffer-string))
                  (length snaps)
                  (marker-position m)
                  (overlay-live-p ov)
                  (with-current-buffer buf2 (default-value 'my-reuse-count)))
          (kill-buffer buf2))))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_multi_buf_overlay_cross_buf_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf2-has-copy)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cross-ov-snap ()
    ((step :initarg :step :accessor cos-step :initform "")
     (buf1-ov-start :initarg :ovs :accessor cos-ovs :initform 0)
     (buf1-ov-end :initarg :ove :accessor cos-ove :initform 0)
     (m-pos :initarg :m-pos :accessor cos-mp :initform 0)
     (buf2-len :initarg :b2 :accessor cos-b2 :initform 0)))
  (let* ((buf1 (generate-new-buffer "mb4a"))
         (buf2 (generate-new-buffer "mb4b"))
         (snaps nil))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-cross-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil))
        (with-current-buffer buf2
          (insert "XXXX-YYYY-ZZZZ-WWWW"))
        (push (cross-ov-snap :step "init"
                            :ovs (overlay-start ov)
                            :ove (overlay-end ov)
                            :m-pos (marker-position m)
                            :b2 (with-current-buffer buf2 (point-max))) snaps)
        (let ((sub (buffer-substring 6 20)))
          (with-current-buffer buf2
            (goto-char (point-max))
            (insert sub)
            (setq-local buf2-has-copy t)))
        (setq my-cross-log (cons "copy-to-buf2" my-cross-log))
        (push (cross-ov-snap :step "cross-copy"
                            :ovs (overlay-start ov)
                            :ove (overlay-end ov)
                            :m-pos (marker-position m)
                            :b2 (with-current-buffer buf2 (point-max))) snaps)
        (delete-region 6 15)
        (setq my-cross-log (cons "del@6-15" my-cross-log))
        (push (cross-ov-snap :step "del-buf1"
                            :ovs (overlay-start ov)
                            :ove (overlay-end ov)
                            :m-pos (marker-position m)
                            :b2 (with-current-buffer buf2 (point-max))) snaps)
        (with-current-buffer buf2
          (goto-char 5)
          (insert "QQ")
          (setq-local buf2-q-inserted t))
        (push (cross-ov-snap :step "edit-buf2"
                            :ovs (overlay-start ov)
                            :ove (overlay-end ov)
                            :m-pos (marker-position m)
                            :b2 (with-current-buffer buf2 (point-max))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cos-step s) (cos-ovs s)
                                                (cos-ove s) (cos-mp s)
                                                (cos-b2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S cross-log=%S"
                       results (reverse my-cross-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cos-log t)
        (list (buffer-string)
              (with-current-buffer buf2 (buffer-string))
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (with-current-buffer buf2 (default-value 'buf2-has-copy))
              my-cross-log)))
    (kill-buffer buf1)
    (kill-buffer buf2)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_multi_buf_buflocal_inherit_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass inherit-snap ()
    ((step :initarg :step :accessor is-step :initform "")
     (buf1-tw :initarg :b1tw :accessor is-b1tw :initform 8)
     (buf2-tw :initarg :b2tw :accessor is-b2tw :initform 8)
     (buf1-narrow :initarg :b1n :accessor is-b1n :initform nil)
     (m-pos :initarg :m-pos :accessor is-mp :initform 0)))
  (let* ((buf1 (generate-new-buffer "mb5a"))
         (buf2 (generate-new-buffer "mb5b"))
         (snaps nil))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (setq-local tab-width 4)
      (setq-local fill-column 50)
      (setq-local my-inherit-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (with-current-buffer buf2
          (insert "XXXX-YYYY-ZZZZ-WWWW")
          (setq-local tab-width 2)
          (setq-local fill-column 80))
        (push (inherit-snap :step "init"
                           :b1tw tab-width
                           :b2tw (with-current-buffer buf2 tab-width)
                           :b1n (list (point-min) (point-max))
                           :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 8 28)
          (push (inherit-snap :step "narrow"
                             :b1tw tab-width
                             :b2tw (with-current-buffer buf2 tab-width)
                             :b1n (list (point-min) (point-max))
                             :m-pos (marker-position m)) snaps)
          (goto-char 10)
          (insert "NN")
          (setq my-inherit-log (cons "ins-narrow" my-inherit-log))
          (push (inherit-snap :step "edit-narrow"
                             :b1tw tab-width
                             :b2tw (with-current-buffer buf2 tab-width)
                             :b1n (list (point-min) (point-max))
                             :m-pos (marker-position m)) snaps))
        (let ((sub (buffer-substring 8 20)))
          (with-current-buffer buf2
            (goto-char (point-max))
            (insert sub)))
        (setq my-inherit-log (cons "cross-insert" my-inherit-log))
        (push (inherit-snap :step "cross-insert"
                           :b1tw tab-width
                           :b2tw (with-current-buffer buf2 tab-width)
                           :b1n (list (point-min) (point-max))
                           :m-pos (marker-position m)) snaps)
        (setq-local tab-width 6)
        (push (inherit-snap :step "buflocal-change"
                           :b1tw tab-width
                           :b2tw (with-current-buffer buf2 tab-width)
                           :b1n (list (point-min) (point-max))
                           :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (is-step s) (is-b1tw s)
                                                (is-b2tw s) (is-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ilog=%S"
                       results (reverse my-inherit-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'is-log t)
        (list (buffer-string)
              (with-current-buffer buf2 (buffer-string))
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              tab-width
              (with-current-buffer buf2 tab-width)
              my-inherit-log)))
    (kill-buffer buf1)
    (kill-buffer buf2)))"#,
        expect,
    );
}
