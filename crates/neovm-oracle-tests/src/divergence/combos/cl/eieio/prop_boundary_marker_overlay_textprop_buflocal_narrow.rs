//! Combo: cl-eieio property-boundary navigation (next-single-property-change,
//! previous-single-property-change, next-single-char-property-change,
//! previous-single-char-property-change) + overlays + markers + textprop
//! + buflocal + narrow + undo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_prop_boundary_textprop_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass pb-snap ()
    ((step :initarg :step :accessor pbs-step :initform "")
     (next-change :initarg :next :accessor pbs-next :initform nil)
     (prev-change :initarg :prev :accessor pbs-prev :initform nil)
     (m-pos :initarg :m-pos :accessor pbs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "pb1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAABBBBBBBBBBCCCCCCCCCCDDDDDDDDDD")
      (setq-local my-pb-log nil)
      (let* ((ov (make-overlay 10 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil)
             (snap-bounds
              (lambda ()
                (let* ((n1 (next-single-property-change 1 'face))
                       (n2 (and n1 (next-single-property-change n1 'face)))
                       (p1 (previous-single-property-change 41 'face))
                       (p2 (and p1 (previous-single-property-change p1 'face))))
                  (list n1 n2 p1 p2)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (pb-snap :step "init"
                      :next (funcall snap-bounds)
                      :prev nil
                      :m-pos (marker-position m)) snaps)
        (put-text-property 1 10 'face 'italic)
        (put-text-property 21 30 'face 'underline)
        (put-text-property 31 40 'face 'default)
        (setq my-pb-log (cons "set-faces" my-pb-log))
        (push (pb-snap :step "faces"
                      :next (funcall snap-bounds)
                      :prev (previous-single-property-change 41 'face)
                      :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "XXX")
        (setq my-pb-log (cons "ins@8" my-pb-log))
        (push (pb-snap :step "edit"
                      :next (funcall snap-bounds)
                      :prev (previous-single-property-change 44 'face)
                      :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (pbs-step s) (pbs-next s)
                                                (pbs-prev s) (pbs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S pb-log=%S"
                       results (reverse my-pb-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'pbs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-pb-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_prop_boundary_char_prop_with_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cpb-snap ()
    ((step :initarg :step :accessor cpbs-step :initform "")
     (next-char-change :initarg :next :accessor cpbs-next :initform nil)
     (prev-char-change :initarg :prev :accessor cpbs-prev :initform nil)
     (m-pos :initarg :m-pos :accessor cpbs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "pb2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAABBBBBBBBBBCCCCCCCCCCDDDDDDDDDD")
      (put-text-property 1 10 'zone 'a)
      (put-text-property 11 20 'zone 'b)
      (put-text-property 21 30 'zone 'c)
      (put-text-property 31 40 'zone 'd)
      (setq-local my-cpb-log nil)
      (let* ((ov1 (make-overlay 5 15))
             (ov2 (make-overlay 25 35))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 10))
             (m (set-marker (make-marker) 20))
             (results nil)
             (snap-char-bounds
              (lambda ()
                (let* ((n1 (next-single-char-property-change 1 'face))
                       (n2 (and n1 (next-single-char-property-change n1 'face)))
                       (n3 (and n2 (next-single-char-property-change n2 'face)))
                       (p1 (previous-single-char-property-change 41 'face))
                       (p2 (and p1 (previous-single-char-property-change p1 'face))))
                  (list n1 n2 n3 p1 p2)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (cpb-snap :step "init"
                       :next (funcall snap-char-bounds)
                       :prev nil
                       :m-pos (marker-position m)) snaps)
        (put-text-property 16 24 'face 'underline)
        (setq my-cpb-log (cons "tp-face@16-24" my-cpb-log))
        (push (cpb-snap :step "tp-face"
                       :next (funcall snap-char-bounds)
                       :prev (previous-single-char-property-change 41 'face)
                       :m-pos (marker-position m)) snaps)
        (goto-char 12)
        (insert "MMM")
        (setq my-cpb-log (cons "ins@12" my-cpb-log))
        (push (cpb-snap :step "edit"
                       :next (funcall snap-char-bounds)
                       :prev (previous-single-char-property-change 44 'face)
                       :m-pos (marker-position m)) snaps)
        (overlay-put ov1 'priority 100)
        (setq my-cpb-log (cons "ov1-pri-100" my-cpb-log))
        (push (cpb-snap :step "pri-change"
                       :next (funcall snap-char-bounds)
                       :prev (previous-single-char-property-change 44 'face)
                       :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cpbs-step s) (cpbs-next s)
                                                (cpbs-prev s) (cpbs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S cpb-log=%S"
                       results (reverse my-cpb-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cpbs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              my-cpb-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_prop_boundary_narrow_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass pb-narrow-snap ()
    ((step :initarg :step :accessor pbns-step :initform "")
     (next-boundary :initarg :next :accessor pbns-next :initform nil)
     (narrow-bounds :initarg :narrow :accessor pbns-narrow :initform nil)
     (m-pos :initarg :m-pos :accessor pbns-mp :initform 0)))
  (let* ((buf (generate-new-buffer "pb3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-pbn-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 18))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (pb-narrow-snap :step "init"
                             :next (next-single-property-change 1 'face)
                             :narrow (list (point-min) (point-max))
                             :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 8 28)
          (push (pb-narrow-snap :step "narrow"
                               :next (next-single-property-change (point-min) 'face)
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps)
          (goto-char 10)
          (insert "XXX")
          (setq my-pbn-log (cons "ins@10" my-pbn-log))
          (push (pb-narrow-snap :step "edit-narrow"
                               :next (next-single-property-change (point-min) 'face)
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps))
        (push (pb-narrow-snap :step "widen"
                             :next (next-single-property-change 1 'face)
                             :narrow (list (point-min) (point-max))
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (pbns-step s) (pbns-next s)
                                                (pbns-narrow s) (pbns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S pbn-log=%S"
                       results (reverse my-pbn-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'pbns-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-pbn-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_prop_boundary_overlay_face_priority_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass pb-face-snap ()
    ((step :initarg :step :accessor pbfs-step :initform "")
     (char-face-at-12 :initarg :cf :accessor pbfs-cf :initform nil)
     (next-face-boundary :initarg :nfb :accessor pbfs-nfb :initform nil)
     (m-pos :initarg :m-pos :accessor pbfs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "pb4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAABBBBBBBBBBCCCCCCCCCCDDDDDDDDDD")
      (put-text-property 1 10 'face 'bold)
      (put-text-property 11 20 'face 'italic)
      (put-text-property 21 30 'face 'underline)
      (put-text-property 31 40 'face 'default)
      (setq-local my-pbf-log nil)
      (let* ((ov1 (make-overlay 5 15))
             (ov2 (make-overlay 15 25))
             (_ (overlay-put ov1 'face 'error))
             (_ (overlay-put ov1 'priority 10))
             (_ (overlay-put ov2 'face 'success))
             (_ (overlay-put ov2 'priority 20))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (pb-face-snap :step "init"
                           :cf (get-char-property 12 'face)
                           :nfb (next-single-char-property-change 1 'face)
                           :m-pos (marker-position m)) snaps)
        (overlay-put ov1 'priority 100)
        (setq my-pbf-log (cons "ov1-pri-100" my-pbf-log))
        (push (pb-face-snap :step "pri-ov1"
                           :cf (get-char-property 12 'face)
                           :nfb (next-single-char-property-change 1 'face)
                           :m-pos (marker-position m)) snaps)
        (overlay-put ov2 'priority 1)
        (setq my-pbf-log (cons "ov2-pri-1" my-pbf-log))
        (push (pb-face-snap :step "pri-ov2"
                           :cf (get-char-property 12 'face)
                           :nfb (next-single-char-property-change 1 'face)
                           :m-pos (marker-position m)) snaps)
        (goto-char 10)
        (insert "XXX")
        (setq my-pbf-log (cons "ins@10" my-pbf-log))
        (push (pb-face-snap :step "edit"
                           :cf (get-char-property 12 'face)
                           :nfb (next-single-char-property-change 1 'face)
                           :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (pbfs-step s) (pbfs-cf s)
                                                (pbfs-nfb s) (pbfs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S pbf-log=%S"
                       results (reverse my-pbf-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'pbfs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              my-pbf-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_prop_boundary_undo_restores_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable n1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass pb-undo-snap ()
    ((step :initarg :step :accessor pbus-step :initform "")
     (boundaries :initarg :bnds :accessor pbus-bnds :initform nil)
     (m-pos :initarg :m-pos :accessor pbus-mp :initform 0)))
  (let* ((buf (generate-new-buffer "pb5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-pbu-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 10))
             (m (set-marker (make-marker) 12))
              (results nil)
              (snap-bounds
               (lambda ()
                 (let ((n1 (next-single-char-property-change 1 'face))
                       (n2 (and n1 (next-single-char-property-change n1 'face)))
                       (n3 (and n2 (next-single-char-property-change n2 'face))))
                   (list n1 n2 n3)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (pb-undo-snap :step "init"
                           :bnds (funcall snap-bounds)
                           :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "XXX")
        (undo-boundary)
        (setq my-pbu-log (cons "ins@8" my-pbu-log))
        (push (pb-undo-snap :step "edit1"
                           :bnds (funcall snap-bounds)
                           :m-pos (marker-position m)) snaps)
        (put-text-property 6 10 'face 'error)
        (undo-boundary)
        (setq my-pbu-log (cons "face-change" my-pbu-log))
        (push (pb-undo-snap :step "face-change"
                           :bnds (funcall snap-bounds)
                           :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (pb-undo-snap :step "undo-face"
                           :bnds (funcall snap-bounds)
                           :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (pb-undo-snap :step "undo-edit"
                           :bnds (funcall snap-bounds)
                           :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (pbus-step s) (pbus-bnds s)
                                                (pbus-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S pbu-log=%S"
                       results (reverse my-pbu-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'pbus-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-pbu-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
