//! Combo: cl-eieio inhibit-read-only / buffer-read-only + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests write protection interactions with EIEIO objects mediating protection state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_read_only_insert_protected() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass protected-region ()
    ((name :initarg :name :accessor pr-name :initform "")
     (start :initarg :start :accessor pr-start :initform 1)
     (end :initarg :end :accessor pr-end :initform 1)
     (write-count :initarg :write-count :accessor pr-writes :initform 0)))
  (let* ((buf (generate-new-buffer "ro1"))
         (r1 (protected-region :name "header" :start 1 :end 10))
         (r2 (protected-region :name "body" :start 11 :end 20)))
    (with-current-buffer buf
      (insert "AAAAAAAAAABBBBBBBBBB")
      (put-text-property 1 10 'region r1)
      (put-text-property 11 20 'region r2)
      (setq-local my-regions (list r1 r2))
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'read-only t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (setq buffer-read-only t)
        (condition-case err
            (progn
              (goto-char 5)
              (insert "XXX")
              (push 'bad-write results))
          (buffer-read-only
           (push (list 'caught-global-read-only (cdr err)) results)))
        (let ((inhibit-read-only t))
          (goto-char 5)
          (insert "YYY")
          (setf (pr-writes r1) (1+ (pr-writes r1)))
          (push (list 'wrote-with-inhibit (buffer-string) (marker-position m)) results))
        (setq buffer-read-only nil)
        (goto-char 15)
        (insert "ZZZ")
        (setf (pr-writes r2) (1+ (pr-writes r2)))
        (push (list 'wrote-after-unlock (buffer-string) (marker-position m)) results)
        (condition-case err
            (progn
              (goto-char 2)
              (insert "QQQ")
              (push 'bad-overlay-write results))
          (buffer-read-only
           (push (list 'caught-overlay-read-only (cdr err)) results)))
        (let ((inhibit-read-only t))
          (goto-char 2)
          (insert "RRR")
          (setf (pr-writes r1) (1+ (pr-writes r1)))
          (push (list 'overlay-override (buffer-string) (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s r1=%d r2=%d m=%d"
                       results (pr-writes r1) (pr-writes r2) (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ro-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-regions))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_read_only_text_prop_overlay_clash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass lock-state ()
    ((layer :initarg :layer :accessor ls-layer :initform "")
     (locked :initarg :locked :accessor ls-locked :initform nil)))
  (let* ((buf (generate-new-buffer "ro2"))
         (ls1 (lock-state :layer "text-prop" :locked t))
         (ls2 (lock-state :layer "overlay" :locked t))
         (ls3 (lock-state :layer "global" :locked nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'lock ls1)
      (put-text-property 6 10 'lock ls2)
      (put-text-property 11 15 'lock ls3)
      (setq-local my-locks (list ls1 ls2 ls3))
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'read-only t))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (put-text-property 1 5 'read-only t)
        (setf (ls-locked ls1) t)
        (condition-case err
            (progn
              (goto-char 3)
              (insert "X")
              (push 'bad-text-prop-write results))
          (buffer-read-only
           (push (list 'text-prop-blocked (cdr err)) results)))
        (condition-case err
            (progn
              (goto-char 8)
              (insert "Y")
              (push 'bad-overlay-write results))
          (buffer-read-only
           (push (list 'overlay-blocked (cdr err)) results)))
        (goto-char 13)
        (insert "Z")
        (setf (ls-locked ls3) t)
        (push (list 'wrote-unlocked (buffer-string) (marker-position m)) results)
        (let ((inhibit-read-only t))
          (goto-char 3)
          (insert "P")
          (goto-char 9)
          (insert "Q"))
        (push (list 'after-inhibit (buffer-string) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s ls1=%s ls2=%s ls3=%s m=%d"
                       results
                       (ls-locked ls1) (ls-locked ls2) (ls-locked ls3)
                       (marker-position m)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'lock-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-locks))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_read_only_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable m)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass guard ()
    ((zone :initarg :zone :accessor gz-zone :initform "")
     (attempts :initarg :attempts :accessor gz-attempts :initform 0)
     (blocked :initarg :blocked :accessor gz-blocked :initform 0)))
  (let* ((buf (generate-new-buffer "ro3"))
         (g1 (guard :zone "narrow" :attempts 0 :blocked 0))
         (g2 (guard :zone "wide" :attempts 0 :blocked 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'guard g1)
      (put-text-property 6 10 'guard g2)
      (setq-local my-guards (list g1 g2))
      (let* ((ov (make-overlay 1 5))
             (_ (overlay-put ov 'read-only t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (save-restriction
          (narrow-to-region 1 5)
          (setf (gz-attempts g1) (1+ (gz-attempts g1)))
          (condition-case err
              (progn
                (goto-char (point-min))
                (insert "X")
                (push 'bad-narrow-write results))
            (buffer-read-only
             (setf (gz-blocked g1) (1+ (gz-blocked g1)))
             (push (list 'narrow-blocked (cdr err)) results)))
          (let ((inhibit-read-only t))
            (goto-char (point-min))
            (insert "OK")
            (push (list 'narrow-inhibit (buffer-string)) results))))
        (setf (gz-attempts g2) (1+ (gz-attempts g2)))
        (goto-char 10)
        (insert "FREE")
        (push (list 'wide-write (buffer-string) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s g1=%s g2=%s m=%d"
                       results
                       (list (gz-attempts g1) (gz-blocked g1))
                       (list (gz-attempts g2) (gz-blocked g2))
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'guard-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-guards))))
    (kill-buffer buf))"#,
        expect,
    );
}

#[test]
fn combo_eieio_read_only_kill_yank_protected() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer ro4> 7 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass kill-guard ()
    ((region :initarg :region :accessor kg-region :initform "")
     (kill-attempts :initarg :kill-attempts :accessor kg-kill-attempts :initform 0)
     (yank-attempts :initarg :yank-attempts :accessor kg-yank-attempts :initform 0)))
  (let* ((buf (generate-new-buffer "ro4"))
         (kg1 (kill-guard :region "protected"))
         (kg2 (kill-guard :region "free")))
    (with-current-buffer buf
      (insert "PPPPP-FFFFF")
      (put-text-property 1 6 'kg kg1)
      (put-text-property 7 11 'kg kg2)
      (setq-local my-kg (list kg1 kg2))
      (let* ((ov (make-overlay 1 6))
             (_ (overlay-put ov 'read-only t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil))
        (undo-boundary)
        (setf (kg-kill-attempts kg1) (1+ (kg-kill-attempts kg1)))
        (condition-case err
            (kill-region 1 6)
          (buffer-read-only
           (push (list 'kill-blocked (cdr err)) results)))
        (setf (kg-yank-attempts kg1) (1+ (kg-yank-attempts kg1)))
        (kill-region 7 11)
        (push (list 'kill-free (buffer-string)) results)
        (let ((inhibit-read-only t))
          (goto-char 1)
          (yank)
          (push (list 'yank-inhibit (buffer-string) (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s kg1=%s kg2=%s m=%d"
                       results
                       (list (kg-kill-attempts kg1) (kg-yank-attempts kg1))
                       (list (kg-kill-attempts kg2) (kg-yank-attempts kg2))
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'kg-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-kg))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_read_only_overlay_priority_layers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass lock-layer ()
    ((name :initarg :name :accessor ll-name :initform "")
     (priority :initarg :priority :accessor ll-priority :initform 0)
     (read-only :initarg :read-only :accessor ll-ro :initform nil)))
  (let* ((buf (generate-new-buffer "ro5"))
         (l1 (lock-layer :name "base" :priority 1 :read-only t))
         (l2 (lock-layer :name "override" :priority 2 :read-only nil))
         (l3 (lock-layer :name "top" :priority 3 :read-only t)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'layer l1)
      (put-text-property 6 10 'layer l2)
      (put-text-property 11 15 'layer l3)
      (setq-local my-layers (list l1 l2 l3))
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 6 15))
             (ov3 (make-overlay 1 15))
             (_ (overlay-put ov1 'read-only t))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'read-only nil))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov3 'read-only t))
             (_ (overlay-put ov3 'priority 3))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (condition-case err
            (progn (goto-char 3) (insert "X") (push 'bad-write results))
          (buffer-read-only (push (list 'blocked-pos3 (cdr err)) results)))
        (condition-case err
            (progn (goto-char 8) (insert "Y") (push 'bad-write results))
          (buffer-read-only (push (list 'blocked-pos8 (cdr err)) results)))
        (condition-case err
            (progn (goto-char 13) (insert "Z") (push 'bad-write results))
          (buffer-read-only (push (list 'blocked-pos13 (cdr err)) results)))
        (let ((inhibit-read-only t))
          (goto-char 3) (insert "A")
          (goto-char 9) (insert "B")
          (goto-char 15) (insert "C"))
        (push (list 'after-inhibit (buffer-string) (marker-position m)
                   (overlay-start ov1) (overlay-end ov1)
                   (overlay-start ov2) (overlay-end ov2)
                   (overlay-start ov3) (overlay-end ov3)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'layer-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe3 (overlay-end ov3))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe3 bs
                (marker-position m)
                (buffer-string)
                my-layers))))
    (kill-buffer buf)))"#,
        expect,
    );
}
