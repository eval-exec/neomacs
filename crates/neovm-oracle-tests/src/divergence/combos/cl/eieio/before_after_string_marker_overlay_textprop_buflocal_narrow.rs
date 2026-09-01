//! Combo: cl-eieio overlay before-string/after-string + markers + textprop
//! + buflocal + narrow + undo.
//! Tests overlay before-string and after-string display properties with
//! complex editing, narrowing, and undo interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_ov_before_after_string_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ba-snap ()
    ((step :initarg :step :accessor bas-step :initform "")
     (ov1-before :initarg :ov1b :accessor bas-ov1b :initform nil)
     (ov1-after :initarg :ov1a :accessor bas-ov1a :initform nil)
     (ov2-before :initarg :ov2b :accessor bas-ov2b :initform nil)
     (ov2-after :initarg :ov2a :accessor bas-ov2a :initform nil)
     (m-pos :initarg :m-pos :accessor bas-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ba1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-ba-log nil)
      (let* ((ov1 (make-overlay 5 11))
             (ov2 (make-overlay 10 20))
             (_ (overlay-put ov1 'before-string "["))
             (_ (overlay-put ov1 'after-string "]"))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov2 'before-string "{"))
             (_ (overlay-put ov2 'after-string "}"))
             (_ (overlay-put ov2 'priority 10))
             (m (set-marker (make-marker) 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ba-snap :step "init"
                      :ov1b (overlay-get ov1 'before-string)
                      :ov1a (overlay-get ov1 'after-string)
                      :ov2b (overlay-get ov2 'before-string)
                      :ov2a (overlay-get ov2 'after-string)
                      :m-pos (marker-position m)) snaps)
        (goto-char 7)
        (insert "XX")
        (setq my-ba-log (cons "ins@7" my-ba-log))
        (push (ba-snap :step "edit"
                      :ov1b (overlay-get ov1 'before-string)
                      :ov1a (overlay-get ov1 'after-string)
                      :ov2b (overlay-get ov2 'before-string)
                      :ov2a (overlay-get ov2 'after-string)
                      :m-pos (marker-position m)) snaps)
        (overlay-put ov1 'before-string "<<")
        (overlay-put ov1 'after-string ">>")
        (setq my-ba-log (cons "change-str" my-ba-log))
        (push (ba-snap :step "change-str"
                      :ov1b (overlay-get ov1 'before-string)
                      :ov1a (overlay-get ov1 'after-string)
                      :ov2b (overlay-get ov2 'before-string)
                      :ov2a (overlay-get ov2 'after-string)
                      :m-pos (marker-position m)) snaps)
        (delete-region 8 14)
        (setq my-ba-log (cons "del@8-14" my-ba-log))
        (push (ba-snap :step "delete"
                      :ov1b (overlay-get ov1 'before-string)
                      :ov1a (overlay-get ov1 'after-string)
                      :ov2b (overlay-get ov2 'before-string)
                      :ov2a (overlay-get ov2 'after-string)
                      :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (bas-step s) (bas-ov1b s)
                                                (bas-ov2b s) (bas-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S log=%S"
                       results (reverse my-ba-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bas-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_ov_before_after_with_face_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ba-face-snap ()
    ((step :initarg :step :accessor bfs-step :initform "")
     (before-face :initarg :bface :accessor bfs-bface :initform nil)
     (after-face :initarg :aface :accessor bfs-aface :initform nil)
     (m-pos :initarg :m-pos :accessor bfs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ba2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-bf-log nil)
      (let* ((ov (make-overlay 6 20))
             (before-str (propertize "[" 'face 'bold))
             (after-str (propertize "]" 'face 'italic))
             (_ (overlay-put ov 'before-string before-str))
             (_ (overlay-put ov 'after-string after-str))
             (_ (overlay-put ov 'priority 10))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ba-face-snap :step "init"
                           :bface (get-text-property 0 'face
                                    (overlay-get ov 'before-string))
                           :aface (get-text-property 0 'face
                                   (overlay-get ov 'after-string))
                           :m-pos (marker-position m)) snaps)
        (put-text-property 6 20 'face 'underline)
        (setq my-bf-log (cons "tp-underline" my-bf-log))
        (push (ba-face-snap :step "tp-face"
                           :bface (get-text-property 0 'face
                                    (overlay-get ov 'before-string))
                           :aface (get-text-property 0 'face
                                   (overlay-get ov 'after-string))
                           :m-pos (marker-position m)) snaps)
        (let ((new-before (propertize "<<" 'face 'error)))
          (overlay-put ov 'before-string new-before))
        (setq my-bf-log (cons "new-before" my-bf-log))
        (push (ba-face-snap :step "new-before"
                           :bface (get-text-property 0 'face
                                    (overlay-get ov 'before-string))
                           :aface (get-text-property 0 'face
                                   (overlay-get ov 'after-string))
                           :m-pos (marker-position m)) snaps)
        (goto-char 10)
        (insert "MMM")
        (setq my-bf-log (cons "edit" my-bf-log))
        (push (ba-face-snap :step "edit"
                           :bface (get-text-property 0 'face
                                    (overlay-get ov 'before-string))
                           :aface (get-text-property 0 'face
                                   (overlay-get ov 'after-string))
                           :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (bfs-step s) (bfs-bface s)
                                                (bfs-aface s) (bfs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S bf-log=%S"
                       results (reverse my-bf-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bfs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-bf-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_ov_before_after_narrow_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ba-narrow-snap ()
    ((step :initarg :step :accessor bns-step :initform "")
     (ov-before :initarg :ovb :accessor bns-ovb :initform nil)
     (ov-after :initarg :ova :accessor bns-ova :initform nil)
     (narrow-bounds :initarg :narrow :accessor bns-narrow :initform nil)
     (m-pos :initarg :m-pos :accessor bns-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ba3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-bn-log nil)
      (let* ((ov (make-overlay 10 25))
             (_ (overlay-put ov 'before-string "|"))
             (_ (overlay-put ov 'after-string "|"))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ba-narrow-snap :step "init"
                             :ovb (overlay-get ov 'before-string)
                             :ova (overlay-get ov 'after-string)
                             :narrow (list (point-min) (point-max))
                             :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 8 22)
          (push (ba-narrow-snap :step "narrow"
                               :ovb (overlay-get ov 'before-string)
                               :ova (overlay-get ov 'after-string)
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps)
          (goto-char 12)
          (insert "QQ")
          (setq my-bn-log (cons "ins@narrow-12" my-bn-log))
          (push (ba-narrow-snap :step "narrow-edit"
                               :ovb (overlay-get ov 'before-string)
                               :ova (overlay-get ov 'after-string)
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps))
        (push (ba-narrow-snap :step "widen"
                             :ovb (overlay-get ov 'before-string)
                             :ova (overlay-get ov 'after-string)
                             :narrow (list (point-min) (point-max))
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (bns-step s) (bns-ovb s)
                                                (bns-ova s) (bns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S bn-log=%S"
                       results (reverse my-bn-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bns-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-bn-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_ov_before_after_evaporate_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ba-evap-snap ()
    ((step :initarg :step :accessor bes-step :initform "")
     (ov-alive :initarg :alive :accessor bes-alive :initform nil)
     (m-pos :initarg :m-pos :accessor bes-mp :initform 0)
     (buf-len :initarg :bl :accessor bes-bl :initform 0)))
  (let* ((buf (generate-new-buffer "ba4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-evap-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'before-string "["))
             (_ (overlay-put ov 'after-string "]"))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ba-evap-snap :step "init"
                           :alive (overlay-live-p ov)
                           :m-pos (marker-position m)
                           :bl (point-max)) snaps)
        (delete-region 6 15)
        (setq my-evap-log (cons "del@6-15" my-evap-log))
        (push (ba-evap-snap :step "deleted"
                           :alive (overlay-live-p ov)
                           :m-pos (marker-position m)
                           :bl (point-max)) snaps)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (push (ba-evap-snap :step "undo"
                             :alive (overlay-live-p ov)
                             :m-pos (marker-position m)
                             :bl (point-max)) snaps
          (setq my-evap-log (cons (format "after-undo:%S" bs) my-evap-log))))
        (goto-char 3)
        (insert "ZZZ")
        (setq my-evap-log (cons "ins@3" my-evap-log))
        (push (ba-evap-snap :step "edit2"
                           :alive (overlay-live-p ov)
                           :m-pos (marker-position m)
                           :bl (point-max)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (bes-step s) (bes-alive s)
                                                (bes-mp s) (bes-bl s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S evap-log=%S"
                       results (reverse my-evap-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bes-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (if (overlay-live-p ov) (overlay-start ov) -1)
              (if (overlay-live-p ov) (overlay-end ov) -1)
              my-evap-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_ov_before_after_multiple_overlays_stacked() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ba-stack-snap ()
    ((step :initarg :step :accessor bss-step :initform "")
     (ov1-before :initarg :ov1b :accessor bss-ov1b :initform nil)
     (ov1-after :initarg :ov1a :accessor bss-ov1a :initform nil)
     (ov2-before :initarg :ov2b :accessor bss-ov2b :initform nil)
     (ov2-after :initarg :ov2a :accessor bss-ov2a :initform nil)
     (m-pos :initarg :m-pos :accessor bss-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ba5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-stack-log nil)
      (let* ((ov1 (make-overlay 5 15))
             (ov2 (make-overlay 10 20))
             (_ (overlay-put ov1 'before-string "A"))
             (_ (overlay-put ov1 'after-string "B"))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov2 'before-string "C"))
             (_ (overlay-put ov2 'after-string "D"))
             (_ (overlay-put ov2 'priority 10))
             (_ (overlay-put ov2 'face 'italic))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ba-stack-snap :step "init"
                            :ov1b (overlay-get ov1 'before-string)
                            :ov1a (overlay-get ov1 'after-string)
                            :ov2b (overlay-get ov2 'before-string)
                            :ov2a (overlay-get ov2 'after-string)
                            :m-pos (marker-position m)) snaps)
        (overlay-put ov1 'priority 20)
        (setq my-stack-log (cons "ov1-pri-up" my-stack-log))
        (push (ba-stack-snap :step "pri-swap"
                            :ov1b (overlay-get ov1 'before-string)
                            :ov1a (overlay-get ov1 'after-string)
                            :ov2b (overlay-get ov2 'before-string)
                            :ov2a (overlay-get ov2 'after-string)
                            :m-pos (marker-position m)) snaps)
        (move-overlay ov2 1 25)
        (setq my-stack-log (cons "ov2-expand" my-stack-log))
        (push (ba-stack-snap :step "ov2-expand"
                            :ov1b (overlay-get ov1 'before-string)
                            :ov1a (overlay-get ov1 'after-string)
                            :ov2b (overlay-get ov2 'before-string)
                            :ov2a (overlay-get ov2 'after-string)
                            :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "NNN")
        (setq my-stack-log (cons "edit@8" my-stack-log))
        (push (ba-stack-snap :step "edit"
                            :ov1b (overlay-get ov1 'before-string)
                            :ov1a (overlay-get ov1 'after-string)
                            :ov2b (overlay-get ov2 'before-string)
                            :ov2a (overlay-get ov2 'after-string)
                            :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 5 20)
          (push (ba-stack-snap :step "narrow"
                              :ov1b (overlay-get ov1 'before-string)
                              :ov1a (overlay-get ov1 'after-string)
                              :ov2b (overlay-get ov2 'before-string)
                              :ov2a (overlay-get ov2 'after-string)
                              :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (bss-step s) (bss-ov1b s)
                                                (bss-ov2b s) (bss-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S stack-log=%S"
                       results (reverse my-stack-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bss-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              my-stack-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
