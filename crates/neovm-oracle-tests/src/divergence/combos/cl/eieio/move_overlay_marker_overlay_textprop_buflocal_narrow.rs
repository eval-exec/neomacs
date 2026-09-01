//! Combo: cl-eieio overlay move + detach + reattach + markers + textprop
//! + buflocal + narrow + undo.
//! Tests complex move-overlay scenarios including cross-region moves,
//! overlapping overlays after moves, and marker tracking through moves.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_move_overlay_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mov-snap ()
    ((step :initarg :step :accessor mvs-step :initform "")
     (ov-start :initarg :ovs :accessor mvs-ovs :initform 0)
     (ov-end :initarg :ove :accessor mvs-ove :initform 0)
     (ov-face :initarg :ovf :accessor mvs-ovf :initform nil)
     (m-pos :initarg :m-pos :accessor mvs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "mv1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-mov-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mov-snap :step "init"
                       :ovs (overlay-start ov)
                       :ove (overlay-end ov)
                       :ovf (overlay-get ov 'face)
                       :m-pos (marker-position m)) snaps)
        (move-overlay ov 16 25)
        (setq my-mov-log (cons "move@16-25" my-mov-log))
        (push (mov-snap :step "moved"
                       :ovs (overlay-start ov)
                       :ove (overlay-end ov)
                       :ovf (overlay-get ov 'face)
                       :m-pos (marker-position m)) snaps)
        (put-text-property 16 25 'face 'italic)
        (setq my-mov-log (cons "tp-italic@16-25" my-mov-log))
        (push (mov-snap :step "tp-face"
                       :ovs (overlay-start ov)
                       :ove (overlay-end ov)
                       :ovf (overlay-get ov 'face)
                       :m-pos (marker-position m)) snaps)
        (move-overlay ov 1 30)
        (setq my-mov-log (cons "expand@1-30" my-mov-log))
        (push (mov-snap :step "expanded"
                       :ovs (overlay-start ov)
                       :ove (overlay-end ov)
                       :ovf (overlay-get ov 'face)
                       :m-pos (marker-position m)) snaps)
        (goto-char 12)
        (insert "XXX")
        (setq my-mov-log (cons "ins@12" my-mov-log))
        (push (mov-snap :step "edit"
                       :ovs (overlay-start ov)
                       :ove (overlay-end ov)
                       :ovf (overlay-get ov 'face)
                       :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mvs-step s) (mvs-ovs s)
                                                (mvs-ove s) (mvs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S mov-log=%S"
                       results (reverse my-mov-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mvs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-mov-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_move_overlay_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mov-narrow-snap ()
    ((step :initarg :step :accessor mvns-step :initform "")
     (ov-start :initarg :ovs :accessor mvns-ovs :initform 0)
     (ov-end :initarg :ove :accessor mvns-ove :initform 0)
     (narrow-bounds :initarg :narrow :accessor mvns-narrow :initform nil)
     (m-pos :initarg :m-pos :accessor mvns-mp :initform 0)))
  (let* ((buf (generate-new-buffer "mv2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (setq-local my-mvn-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mov-narrow-snap :step "init"
                              :ovs (overlay-start ov)
                              :ove (overlay-end ov)
                              :narrow (list (point-min) (point-max))
                              :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 8 28)
          (push (mov-narrow-snap :step "narrow"
                                :ovs (overlay-start ov)
                                :ove (overlay-end ov)
                                :narrow (list (point-min) (point-max))
                                :m-pos (marker-position m)) snaps)
          (move-overlay ov 10 25)
          (setq my-mvn-log (cons "move-in-narrow@10-25" my-mvn-log))
          (push (mov-narrow-snap :step "moved-in-narrow"
                                :ovs (overlay-start ov)
                                :ove (overlay-end ov)
                                :narrow (list (point-min) (point-max))
                                :m-pos (marker-position m)) snaps)
          (goto-char 12)
          (insert "QQ")
          (setq my-mvn-log (cons "ins-narrow@12" my-mvn-log))
          (push (mov-narrow-snap :step "edit-in-narrow"
                                :ovs (overlay-start ov)
                                :ove (overlay-end ov)
                                :narrow (list (point-min) (point-max))
                                :m-pos (marker-position m)) snaps))
        (push (mov-narrow-snap :step "widen"
                              :ovs (overlay-start ov)
                              :ove (overlay-end ov)
                              :narrow (list (point-min) (point-max))
                              :m-pos (marker-position m)) snaps)
        (move-overlay ov 1 40)
        (setq my-mvn-log (cons "expand-full" my-mvn-log))
        (push (mov-narrow-snap :step "full-expand"
                              :ovs (overlay-start ov)
                              :ove (overlay-end ov)
                              :narrow (list (point-min) (point-max))
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mvns-step s) (mvns-ovs s)
                                                (mvns-ove s) (mvns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S mvn-log=%S"
                       results (reverse my-mvn-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mvns-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-mvn-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_move_overlay_multi_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mov-swap-snap ()
    ((step :initarg :step :accessor mvss-step :initform "")
     (ov1-bounds :initarg :ov1 :accessor mvss-ov1 :initform nil)
     (ov2-bounds :initarg :ov2 :accessor mvss-ov2 :initform nil)
     (m-pos :initarg :m-pos :accessor mvss-mp :initform 0)))
  (let* ((buf (generate-new-buffer "mv3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (setq-local my-swap-log nil)
      (let* ((ov1 (make-overlay 6 15))
             (ov2 (make-overlay 21 35))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 10))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mov-swap-snap :step "init"
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))
                            :m-pos (marker-position m)) snaps)
        (move-overlay ov1 26 40)
        (setq my-swap-log (cons "ov1-to-end" my-swap-log))
        (push (mov-swap-snap :step "ov1-moved"
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))
                            :m-pos (marker-position m)) snaps)
        (move-overlay ov2 1 10)
        (setq my-swap-log (cons "ov2-to-start" my-swap-log))
        (push (mov-swap-snap :step "ov2-moved"
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))
                            :m-pos (marker-position m)) snaps)
        (goto-char 15)
        (insert "MMMM")
        (setq my-swap-log (cons "ins@15" my-swap-log))
        (push (mov-swap-snap :step "edit"
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))
                            :m-pos (marker-position m)) snaps)
        (move-overlay ov1 10 20)
        (move-overlay ov2 25 35)
        (setq my-swap-log (cons "ovs-to-mid" my-swap-log))
        (push (mov-swap-snap :step "re-centered"
                            :ov1 (list (overlay-start ov1) (overlay-end ov1))
                            :ov2 (list (overlay-start ov2) (overlay-end ov2))
                            :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mvss-step s) (mvss-ov1 s)
                                                (mvss-ov2 s) (mvss-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S swap-log=%S"
                       results (reverse my-swap-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mvss-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              my-swap-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_move_overlay_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mov-undo-snap ()
    ((step :initarg :step :accessor mvus-step :initform "")
     (ov-bounds :initarg :ov :accessor mvus-ov :initform nil)
     (buf-string :initarg :bs :accessor mvus-bs :initform "")
     (m-pos :initarg :m-pos :accessor mvus-mp :initform 0)))
  (let* ((buf (generate-new-buffer "mv4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-mvu-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mov-undo-snap :step "init"
                            :ov (list (overlay-start ov) (overlay-end ov))
                            :bs (buffer-string)
                            :m-pos (marker-position m)) snaps)
        (move-overlay ov 16 25)
        (undo-boundary)
        (setq my-mvu-log (cons "move@16-25" my-mvu-log))
        (push (mov-undo-snap :step "moved"
                            :ov (list (overlay-start ov) (overlay-end ov))
                            :bs (buffer-string)
                            :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "XXX")
        (undo-boundary)
        (setq my-mvu-log (cons "ins@8" my-mvu-log))
        (push (mov-undo-snap :step "edit"
                            :ov (list (overlay-start ov) (overlay-end ov))
                            :bs (buffer-string)
                            :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (mov-undo-snap :step "undo-edit"
                            :ov (list (overlay-start ov) (overlay-end ov))
                            :bs (buffer-string)
                            :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mvus-step s) (mvus-ov s)
                                                (mvus-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S mvu-log=%S"
                       results (reverse my-mvu-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mvus-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-mvu-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_move_overlay_with_mod_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mov-hook-snap ()
    ((step :initarg :step :accessor mvhs-step :initform "")
     (ov-bounds :initarg :ov :accessor mvhs-ov :initform nil)
     (hook-count :initarg :hc :accessor mvhs-hc :initform 0)
     (m-pos :initarg :m-pos :accessor mvhs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "mv5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (setq-local hook-fire-count 0)
      (setq-local my-mvh-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (_ (overlay-put ov 'modification-hooks
                           (list (lambda (ov after-p beg end &optional _len)
                                   (when after-p
                                     (setq hook-fire-count
                                           (1+ hook-fire-count)))))))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mov-hook-snap :step "init"
                            :ov (list (overlay-start ov) (overlay-end ov))
                            :hc hook-fire-count
                            :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "XX")
        (setq my-mvh-log (cons "ins@8" my-mvh-log))
        (push (mov-hook-snap :step "edit1"
                            :ov (list (overlay-start ov) (overlay-end ov))
                            :hc hook-fire-count
                            :m-pos (marker-position m)) snaps)
        (move-overlay ov 21 35)
        (setq my-mvh-log (cons "move@21-35" my-mvh-log))
        (push (mov-hook-snap :step "moved"
                            :ov (list (overlay-start ov) (overlay-end ov))
                            :hc hook-fire-count
                            :m-pos (marker-position m)) snaps)
        (goto-char 25)
        (insert "YY")
        (setq my-mvh-log (cons "ins@25" my-mvh-log))
        (push (mov-hook-snap :step "edit-after-move"
                            :ov (list (overlay-start ov) (overlay-end ov))
                            :hc hook-fire-count
                            :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 20 38)
          (goto-char 22)
          (insert "ZZ")
          (setq my-mvh-log (cons "ins-narrow@22" my-mvh-log))
          (push (mov-hook-snap :step "edit-narrow"
                              :ov (list (overlay-start ov) (overlay-end ov))
                              :hc hook-fire-count
                              :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mvhs-step s) (mvhs-ov s)
                                                (mvhs-hc s) (mvhs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S mvh-log=%S"
                       results (reverse my-mvh-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mvhs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              hook-fire-count my-mvh-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
