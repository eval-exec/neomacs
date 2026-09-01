//! Combo: cl-eieio object-let / with-slots + marker + overlay + textprop + buflocal + narrow + undo.
//! Tests with-slots macro and slot mutation with complex buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_with_slots_buffer_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-with-slots)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass config-item ()
    ((key :initarg :key :accessor config-key :initform "")
     (value :initarg :value :accessor config-value :initform nil)
     (mutable :initarg :mutable :accessor config-mutable :initform t)))
  (let* ((buf (generate-new-buffer "ws1"))
         (cfg (config-item :key "buffer-encoding" :value 'utf-8 :mutable t)))
    (with-current-buffer buf
      (insert "KEY=encoding-VAL=utf-8-MUT=t")
      (put-text-property 1 4 'field 'key)
      (put-text-property 5 18 'field 'val)
      (put-text-property 19 24 'field 'mut)
      (setq-local cfg-obj cfg)
      (let* ((ov (make-overlay 5 18))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 10)))
        (narrow-to-region 5 18)
        (undo-boundary)
        (cl-with-slots (key value mutable) cfg
          (let ((orig-key key)
                (orig-value value))
            (setf key "buffer-line-ending"
                  value 'unix
                  mutable nil)
            (goto-char (point-min))
            (insert (format "[%s=%s:%s]" key value mutable))
            (setf (marker-position m) (+ (point-min) 5))))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-substring (point-min) (point-max)))
              (ck (config-key cfg-obj))
              (cv (config-value cfg-obj))
              (cm (config-mutable cfg-obj)))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe bs ck cv cm
                (marker-position m)
                (buffer-string)
                cfg-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_with_accessors_multi_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass account ()
    ((id :initarg :id :accessor account-id :initform 0)
     (balance :initarg :balance :accessor account-balance :initform 0.0)))
  (defclass savings-account (account)
    ((interest-rate :initarg :interest-rate :accessor savings-rate :initform 0.01)))
  (defclass checking-account (account)
    ((overdraft :initarg :overdraft :accessor checking-overdraft :initform 0.0)))
  (let* ((buf (generate-new-buffer "wa1"))
         (sav (savings-account :id 1 :balance 1000.0 :interest-rate 0.05))
         (chk (checking-account :id 2 :balance 500.0 :overdraft 200.0)))
    (with-current-buffer buf
      (insert "SAV:1000.0-CHK:500.0-END")
      (put-text-property 1 10 'acct 'savings)
      (put-text-property 11 19 'acct 'checking)
      (put-text-property 20 23 'acct 'end)
      (setq-local accounts (list sav chk))
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 19))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 5)))
        (undo-boundary)
        (cl-with-accessors ((bal account-balance)
                            (id account-id)
                            (rate savings-rate)) sav
          (let ((new-bal (+ bal (* bal rate))))
            (setf bal new-bal)
            (goto-char 5)
            (insert (format "%.1f" new-bal))
            (setf (marker-position m) 10)
            (put-text-property 1 10 'updated t)))
        (undo-boundary)
        (cl-with-accessors ((bal account-balance)
                            (od checking-overdraft)
                            (id account-id)) chk
          (let ((withdrawal 300.0))
            (setf bal (- bal withdrawal))
            (when (< bal 0)
              (setf od (+ od (abs bal)))
              (setf bal 0.0))
            (goto-char 16)
            (insert (format "%.1f" bal))))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (tp (get-text-property 1 'acct))
              (bs (buffer-string))
              (sav-bal (account-balance sav))
              (chk-bal (account-balance chk))
              (chk-od (checking-overdraft chk)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe1 os2 oe2 tp bs sav-bal chk-bal chk-od
                (marker-position m)
                (buffer-string)
                accounts)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_slot_setf_via_setv_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass point ()
    ((x :initarg :x :accessor pt-x :initform 0)
     (y :initarg :y :accessor pt-y :initform 0)))
  (defclass point3d (point)
    ((z :initarg :z :accessor pt-z :initform 0)))
  (let* ((buf (generate-new-buffer "ws2"))
         (p (point3d :x 10 :y 20 :z 30)))
    (with-current-buffer buf
      (insert "P(10,20,30)-P(0,0,0)")
      (put-text-property 1 11 'dim 'xyz)
      (put-text-property 12 20 'dim 'origin)
      (setq-local my-point p)
      (let* ((ov (make-overlay 1 11))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 3)))
        (narrow-to-region 1 11)
        (undo-boundary)
        (let ((old-x (pt-x p))
              (old-y (pt-y p))
              (old-z (pt-z p)))
          (setf (pt-x p) (+ old-x 5))
          (setf (pt-y p) (- old-y 10))
          (setf (pt-z p) (* old-z 2))
          (goto-char (point-min))
          (insert (format "P(%d,%d,%d)-" (pt-x p) (pt-y p) (pt-z p)))
          (setf (marker-position m) 8)
          (put-text-property (point-min) (+ (point-min) 14) 'modified t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-substring (point-min) (point-max)))
              (px (pt-x my-point))
              (py (pt-y my-point))
              (pz (pt-z my-point)))
          (primitive-undo 1 buffer-undo-list)
          (let ((mp2 (marker-position m))
                (bs2 (buffer-substring (point-min) (point-max))))
            (widen)
            (list mp os oe bs px py pz mp2 bs2
                  (marker-position m)
                  (buffer-string)
                  my-point))))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_with_slots_clone_overlay_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 27 35)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass task ()
    ((title :initarg :title :accessor task-title :initform "")
     (status :initarg :status :accessor task-status :initform 'todo)
     (priority :initarg :priority :accessor task-priority :initform 5)))
  (let* ((buf (generate-new-buffer "ws3"))
         (t1 (task :title "write tests" :status 'todo :priority 3))
         (t2 (task :title "fix bug" :status 'in-progress :priority 1)))
    (with-current-buffer buf
      (insert "TASK1:write-tests-TASK2:fix-bug")
      (put-text-property 1 6 'task-id 1)
      (put-text-property 7 20 'task-body "write tests")
      (put-text-property 21 26 'task-id 2)
      (put-text-property 27 35 'task-body "fix bug")
      (setq-local task-list (list t1 t2))
      (let* ((ov (make-overlay 7 20))
             (_ (overlay-put ov 'task-highlight t))
             (m (make-marker))
             (_ (set-marker m 12))
             (clone (clone-buffer "ws3-clone")))
        (with-current-buffer clone
          (setq-local task-list (list t1 t2))
          (narrow-to-region 1 20)
          (undo-boundary)
          (cl-with-slots (title status priority) t1
            (setf title "write combo tests"
                  status 'in-progress
                  priority 1)
            (goto-char (point-min))
            (insert (format "[%s:%s:p%d]" title status priority))
            (setf (marker-position m) 10)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (t1-title (task-title t1))
              (t1-status (task-status t1))
              (t1-pri (task-priority t1))
              (clone-bs (with-current-buffer clone (buffer-string))))
          (with-current-buffer clone (widen))
          (let ((clone-full (with-current-buffer clone (buffer-string))))
            (primitive-undo 1 buffer-undo-list)
            (kill-buffer clone)
            (list mp os oe t1-title t1-status t1-pri clone-bs clone-full
                  (marker-position m)
                  (buffer-string)
                  task-list))))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_slot_makeunbound_with_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function slot-makunbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass resource ()
    ((name :initarg :name :accessor res-name :initform "")
     (size :initarg :size :accessor res-size :initform 0)
     (active :initarg :active :accessor res-active :initform t)))
  (let* ((buf (generate-new-buffer "ws4"))
         (r (resource :name "disk1" :size 1024 :active t)))
    (with-current-buffer buf
      (insert "RES:disk1-SIZE:1024-ACTIVE:t")
      (put-text-property 1 4 'field 'type)
      (put-text-property 5 10 'field 'name)
      (put-text-property 11 15 'field 'size-label)
      (put-text-property 16 20 'field 'size-val)
      (put-text-property 21 27 'field 'active-label)
      (put-text-property 28 29 'field 'active-val)
      (setq-local res-obj r)
      (let* ((ov (make-overlay 5 20))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (let ((name-before (res-name r))
              (size-before (res-size r))
              (active-before (res-active r)))
          (slot-makunbound r 'size)
          (slot-makunbound r 'active)
          (setf (res-name r) "disk2")
          (goto-char 5)
          (insert "disk2")
          (delete-char (- 5))
          (setf (marker-position m) 10)
          (put-text-property 5 10 'field 'name))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (name-after (res-name r))
              (size-bound (slot-boundp r 'size))
              (active-bound (slot-boundp r 'active))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe name-after size-bound active-bound bs
                (marker-position m)
                (buffer-string)
                res-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}
