//! Combo: cl-eieio overlay stacking/merging with priorities, invisible, faces
//! + markers + textprop + buflocal + narrow + undo.
//! Tests deeply nested overlapping overlays where priority resolution matters.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_overlay_stack_priority_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ov-stack-snap ()
    ((step :initarg :step :accessor oss-step :initform "")
     (inv-at-8 :initarg :inv :accessor oss-inv :initform nil)
     (m-pos :initarg :m-pos :accessor oss-mp :initform 0)
     (ovs :initarg :ovs :accessor oss-ovs :initform nil)))
  (let* ((buf (generate-new-buffer "os1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAABBBBBBBBBBCCCCCCCCCCDDDDDDDDDD")
      (put-text-property 1 11 'layer 'a)
      (put-text-property 11 21 'layer 'b)
      (put-text-property 21 31 'layer 'c)
      (put-text-property 31 41 'layer 'd)
      (setq-local my-counter 0)
      (let* ((ov1 (make-overlay 5 15))
             (ov2 (make-overlay 10 25))
             (ov3 (make-overlay 18 35))
             (ov4 (make-overlay 3 38))
             (_ (overlay-put ov1 'invisible 'h1))
             (_ (overlay-put ov1 'priority 10))
             (_ (overlay-put ov2 'invisible 'h2))
             (_ (overlay-put ov2 'priority 20))
             (_ (overlay-put ov3 'invisible 'h3))
             (_ (overlay-put ov3 'priority 5))
             (_ (overlay-put ov4 'invisible 'h4))
             (_ (overlay-put ov4 'priority 30))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ov-stack-snap :step "init"
                            :inv (get-char-property 8 'invisible)
                            :m-pos (marker-position m)
                            :ovs (list (overlay-start ov1) (overlay-end ov1)
                                       (overlay-start ov2) (overlay-end ov2)
                                       (overlay-start ov3) (overlay-end ov3)
                                       (overlay-start ov4) (overlay-end ov4))) snaps)
        (add-to-invisibility-spec 'h1)
        (setq my-counter (1+ my-counter))
        (push (ov-stack-snap :step "hide-h1"
                            :inv (get-char-property 8 'invisible)
                            :m-pos (marker-position m)
                            :ovs (list (overlay-start ov1) (overlay-end ov1)
                                       (overlay-start ov2) (overlay-end ov2))) snaps)
        (add-to-invisibility-spec 'h2)
        (add-to-invisibility-spec 'h3)
        (add-to-invisibility-spec 'h4)
        (setq my-counter (1+ my-counter))
        (push (ov-stack-snap :step "hide-all"
                            :inv (get-char-property 8 'invisible)
                            :m-pos (marker-position m)
                            :ovs (list (overlay-start ov1) (overlay-end ov1)
                                       (overlay-start ov2) (overlay-end ov2)
                                       (overlay-start ov3) (overlay-end ov3)
                                       (overlay-start ov4) (overlay-end ov4))) snaps)
        (overlay-put ov2 'priority 50)
        (push (ov-stack-snap :step "ov2-pri-50"
                            :inv (get-char-property 12 'invisible)
                            :m-pos (marker-position m)
                            :ovs (list (overlay-get ov1 'priority)
                                       (overlay-get ov2 'priority)
                                       (overlay-get ov3 'priority)
                                       (overlay-get ov4 'priority))) snaps)
        (goto-char 8)
        (insert "MMMM")
        (setq my-counter (1+ my-counter))
        (push (ov-stack-snap :step "insert-m"
                            :inv (get-char-property 12 'invisible)
                            :m-pos (marker-position m)
                            :ovs (list (overlay-start ov1) (overlay-end ov1)
                                       (overlay-start ov2) (overlay-end ov2))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (oss-step s) (oss-inv s) (oss-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d counter=%d buf-len=%d"
                       results (marker-position m) my-counter (point-max)))
        (put-text-property (1- (point-max)) (point-max) 'oss-log t)
        (set-marker m 3)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps) (marker-position m)
                (overlay-start ov1) (overlay-end ov1)
                (overlay-start ov2) (overlay-end ov2)
                (overlay-start ov3) (overlay-end ov3)
                (overlay-start ov4) (overlay-end ov4)
                my-counter buffer-invisibility-spec))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_overlay_stack_face_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ov-face-snap ()
    ((step :initarg :step :accessor ofs-step :initform "")
     (face-at-10 :initarg :face :accessor ofs-face :initform nil)
     (ov1-face :initarg :ov1-face :accessor ofs-ov1 :initform nil)
     (ov2-face :initarg :ov2-face :accessor ofs-ov2 :initform nil)
     (m-pos :initarg :m-pos :accessor ofs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "os2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "ABCDEFGHIJ KLMNOPQRST UVWXYZ")
      (put-text-property 1 11 'block 'first)
      (put-text-property 12 22 'block 'second)
      (put-text-property 23 29 'block 'third)
      (setq-local my-face-log nil)
      (let* ((ov1 (make-overlay 3 18))
             (ov2 (make-overlay 8 25))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 10))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ov-face-snap :step "init"
                           :face (get-char-property 10 'face)
                           :ov1-face (overlay-get ov1 'face)
                           :ov2-face (overlay-get ov2 'face)
                           :m-pos (marker-position m)) snaps)
        (put-text-property 5 15 'face 'underline)
        (setq my-face-log (cons 'tp-underline my-face-log))
        (push (ov-face-snap :step "tp-face"
                           :face (get-char-property 10 'face)
                           :ov1-face (overlay-get ov1 'face)
                           :ov2-face (overlay-get ov2 'face)
                           :m-pos (marker-position m)) snaps)
        (overlay-put ov1 'face '(bold italic))
        (setq my-face-log (cons 'ov1-bold-italic my-face-log))
        (push (ov-face-snap :step "ov1-list-face"
                           :face (get-char-property 10 'face)
                           :ov1-face (overlay-get ov1 'face)
                           :ov2-face (overlay-get ov2 'face)
                           :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 5 20)
          (push (ov-face-snap :step "narrow"
                             :face (get-char-property 6 'face)
                             :ov1-face (overlay-get ov1 'face)
                             :ov2-face (overlay-get ov2 'face)
                             :m-pos (marker-position m)) snaps)
          (goto-char 8)
          (insert "QQ")
          (setq my-face-log (cons 'insert-in-narrow my-face-log))
          (push (ov-face-snap :step "narrow-edit"
                             :face (get-char-property 6 'face)
                             :ov1-face (overlay-get ov1 'face)
                             :ov2-face (overlay-get ov2 'face)
                             :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ofs-step s) (ofs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d face-log=%S"
                       results (marker-position m) (reverse my-face-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ofs-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps) (marker-position m)
                (overlay-start ov1) (overlay-end ov1)
                (overlay-start ov2) (overlay-end ov2)
                my-face-log))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_overlay_stack_delete_reinsert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ov-del-snap ()
    ((step :initarg :step :accessor ods-step :initform "")
     (ov1-alive :initarg :ov1-alive :accessor ods-a1 :initform nil)
     (ov2-alive :initarg :ov2-alive :accessor ods-a2 :initform nil)
     (ov3-alive :initarg :ov3-alive :accessor ods-a3 :initform nil)
     (m-pos :initarg :m-pos :accessor ods-mp :initform 0)))
  (let* ((buf (generate-new-buffer "os3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-deleted nil)
      (let* ((ov1 (make-overlay 6 10))
             (ov2 (make-overlay 11 20))
             (ov3 (make-overlay 21 30))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov1 'evaporate t))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov3 'face 'underline))
             (_ (overlay-put ov3 'priority 3))
             (_ (overlay-put ov3 'evaporate t))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ov-del-snap :step "init"
                          :ov1-alive (overlay-live-p ov1)
                          :ov2-alive (overlay-live-p ov2)
                          :ov3-alive (overlay-live-p ov3)
                          :m-pos (marker-position m)) snaps)
        (delete-region 6 10)
        (setq my-deleted (cons 'region-b my-deleted))
        (push (ov-del-snap :step "del-b"
                          :ov1-alive (overlay-live-p ov1)
                          :ov2-alive (overlay-live-p ov2)
                          :ov3-alive (overlay-live-p ov3)
                          :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (let ((bs1 (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (push (ov-del-snap :step "undo-del-b"
                            :ov1-alive (overlay-live-p ov1)
                            :ov2-alive (overlay-live-p ov2)
                            :ov3-alive (overlay-live-p ov3)
                            :m-pos (marker-position m)) snaps)
          (setq my-deleted (cons (format "after-undo: buf=%S" bs1) my-deleted)))
        (delete-region 21 30)
        (setq my-deleted (cons 'region-ef my-deleted))
        (push (ov-del-snap :step "del-ef"
                          :ov1-alive (overlay-live-p ov1)
                          :ov2-alive (overlay-live-p ov2)
                          :ov3-alive (overlay-live-p ov3)
                          :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (push (ov-del-snap :step "undo-del-ef"
                          :ov1-alive (overlay-live-p ov1)
                          :ov2-alive (overlay-live-p ov2)
                          :ov3-alive (overlay-live-p ov3)
                          :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ods-step s) (ods-a1 s) (ods-a2 s)
                                                (ods-a3 s) (ods-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d deleted=%S"
                       results (marker-position m) (reverse my-deleted)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ods-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-live-p ov1) (overlay-live-p ov2) (overlay-live-p ov3))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_overlay_stack_reorder_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ov-reorder-snap ()
    ((step :initarg :step :accessor ors-step :initform "")
     (overlays-at-12 :initarg :ov-at :accessor ors-ov-at :initform 0)
     (inv-at-12 :initarg :inv :accessor ors-inv :initform nil)
     (m-pos :initarg :m-pos :accessor ors-mp :initform 0)))
  (let* ((buf (generate-new-buffer "os4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAA-BBBBBBBBBB-CCCCCCCCCC-DDDDDDDDDD")
      (put-text-property 1 11 'zone 'a)
      (put-text-property 12 22 'zone 'b)
      (put-text-property 23 33 'zone 'c)
      (put-text-property 34 44 'zone 'd)
      (setq-local my-reorder-log nil)
      (let* ((ov1 (make-overlay 5 20))
             (ov2 (make-overlay 8 30))
             (ov3 (make-overlay 15 40))
             (m (set-marker (make-marker) 12))
             (results nil)
             (count-overlays-at-12
              (lambda ()
                (length (overlays-at 12)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (overlay-put ov1 'invisible 'h1)
        (overlay-put ov1 'priority 10)
        (overlay-put ov1 'face 'bold)
        (overlay-put ov2 'invisible 'h2)
        (overlay-put ov2 'priority 20)
        (overlay-put ov2 'face 'italic)
        (overlay-put ov3 'invisible 'h3)
        (overlay-put ov3 'priority 30)
        (overlay-put ov3 'face 'underline)
        (push (ov-reorder-snap :step "init"
                              :ov-at (funcall count-overlays-at-12)
                              :inv (get-char-property 12 'invisible)
                              :m-pos (marker-position m)) snaps)
        (overlay-put ov3 'priority 5)
        (setq my-reorder-log (cons 'ov3-pri-down my-reorder-log))
        (push (ov-reorder-snap :step "ov3-low"
                              :ov-at (funcall count-overlays-at-12)
                              :inv (get-char-property 12 'invisible)
                              :m-pos (marker-position m)) snaps)
        (overlay-put ov1 'priority 100)
        (setq my-reorder-log (cons 'ov1-pri-up my-reorder-log))
        (push (ov-reorder-snap :step "ov1-high"
                              :ov-at (funcall count-overlays-at-12)
                              :inv (get-char-property 12 'invisible)
                              :m-pos (marker-position m)) snaps)
        (move-overlay ov2 1 44)
        (setq my-reorder-log (cons 'ov2-expanded my-reorder-log))
        (push (ov-reorder-snap :step "ov2-expand"
                              :ov-at (funcall count-overlays-at-12)
                              :inv (get-char-property 12 'invisible)
                              :m-pos (marker-position m)) snaps)
        (add-to-invisibility-spec 'h1)
        (add-to-invisibility-spec 'h2)
        (add-to-invisibility-spec 'h3)
        (setq my-reorder-log (cons 'all-hidden my-reorder-log))
        (push (ov-reorder-snap :step "hidden"
                              :ov-at (funcall count-overlays-at-12)
                              :inv (get-char-property 12 'invisible)
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ors-step s) (ors-ov-at s)
                                                (ors-inv s) (ors-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d reorder=%S"
                       results (marker-position m) (reverse my-reorder-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ors-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              (overlay-start ov3) (overlay-end ov3)
              my-reorder-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_overlay_stack_nested_edit_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ov-chain-snap ()
    ((step :initarg :step :accessor ocs-step :initform "")
     (buf-len :initarg :buf-len :accessor ocs-bl :initform 0)
     (m-pos :initarg :m-pos :accessor ocs-mp :initform 0)
     (ov1-bounds :initarg :ov1 :accessor ocs-ov1 :initform nil)
     (ov2-bounds :initarg :ov2 :accessor ocs-ov2 :initform nil)))
  (let* ((buf (generate-new-buffer "os5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "PPPP-QQQQ-RRRR-SSSS-TTTT-UUUU")
      (put-text-property 1 5 'zone 'p)
      (put-text-property 6 10 'zone 'q)
      (put-text-property 11 15 'zone 'r)
      (put-text-property 16 20 'zone 's)
      (put-text-property 21 25 'zone 't)
      (put-text-property 26 30 'zone 'u)
      (setq-local my-chain-log nil)
      (let* ((ov1 (make-overlay 6 20))
             (ov2 (make-overlay 11 30))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov1 'modification-hooks
                           (list (lambda (ov after-p beg end &optional _len)
                                   (when after-p
                                     (setq my-chain-log
                                           (cons (format "ov1-hook:%d-%d" beg end)
                                                 my-chain-log))))))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov2 'modification-hooks
                           (list (lambda (ov after-p beg end &optional _len)
                                   (when after-p
                                     (setq my-chain-log
                                           (cons (format "ov2-hook:%d-%d" beg end)
                                                 my-chain-log))))))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ov-chain-snap :step "init"
                            :buf-len (point-max)
                            :m-pos (marker-position m)
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))) snaps)
        (goto-char 8)
        (insert "XXX")
        (undo-boundary)
        (push (ov-chain-snap :step "edit1"
                            :buf-len (point-max)
                            :m-pos (marker-position m)
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))) snaps)
        (goto-char 20)
        (insert "YYY")
        (undo-boundary)
        (push (ov-chain-snap :step "edit2"
                            :buf-len (point-max)
                            :m-pos (marker-position m)
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (ov-chain-snap :step "undo-edit2"
                            :buf-len (point-max)
                            :m-pos (marker-position m)
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (ov-chain-snap :step "undo-edit1"
                            :buf-len (point-max)
                            :m-pos (marker-position m)
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ocs-step s) (ocs-bl s) (ocs-mp s)
                                                (ocs-ov1 s) (ocs-ov2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d chain=%S"
                       results (marker-position m) (reverse my-chain-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ocs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              (length my-chain-log))))
    (kill-buffer buf)))"#,
        expect,
    );
}
