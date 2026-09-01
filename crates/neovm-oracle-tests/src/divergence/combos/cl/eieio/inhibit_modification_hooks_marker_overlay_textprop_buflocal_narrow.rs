//! Combo: cl-eieio inhibit-modification-hooks + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests interaction between inhibit-modification-hooks and EIEIO change tracking.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_inhibit_modification_hooks_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass hook-log-entry ()
    ((hook-type :initarg :hook-type :accessor hle-type :initform "")
     (beg :initarg :beg :accessor hle-beg :initform 0)
     (end :initarg :end :accessor hle-end :initform 0)
     (len :initarg :len :accessor hle-len :initform 0)))
  (let* ((buf (generate-new-buffer "im1"))
         (before-entries nil)
         (after-entries nil)
         (log-obj (hook-log-entry)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'block 'a)
      (put-text-property 6 10 'block 'b)
      (put-text-property 11 15 'block 'c)
      (put-text-property 16 20 'block 'd)
      (setq-local my-before before-entries
                  my-after after-entries)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (add-hook 'before-change-functions
                  (lambda (beg end)
                    (push (hook-log-entry :hook-type "before"
                                         :beg beg :end end :len (- end beg))
                          before-entries))
                  nil t)
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (hook-log-entry :hook-type "after"
                                         :beg beg :end end :len len)
                          after-entries))
                  nil t)
        (undo-boundary)
        (goto-char 3)
        (insert "XXX")
        (push (list 'normal-insert (length before-entries) (length after-entries)) results)
        (let ((inhibit-modification-hooks t))
          (goto-char 10)
          (insert "YYY")
          (push (list 'inhibited-insert (length before-entries) (length after-entries)) results))
        (goto-char 14)
        (insert "ZZZ")
        (push (list 'resumed-insert (length before-entries) (length after-entries)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'im-log t)
        (remove-hook 'before-change-functions (car (default-value 'before-change-functions)) t)
        (remove-hook 'after-change-functions (car (default-value 'after-change-functions)) t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string) results
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_inhibit_with_overlay_modification_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ov-mod-event ()
    ((overlay-name :initarg :overlay-name :accessor ome-name :initform "")
     (is-before :initarg :is-before :accessor ome-before :initform nil)
     (ov-start :initarg :ov-start :accessor ome-start :initform 0)
     (ov-end :initarg :ov-end :accessor ome-end :initform 0)
     (change-beg :initarg :change-beg :accessor ome-beg :initform 0)))
  (let* ((buf (generate-new-buffer "im2"))
         (ov-events nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAAAAAA")
      (put-text-property 1 6 'field 'f1)
      (put-text-property 7 12 'field 'f2)
      (put-text-property 13 20 'field 'f3)
      (setq-local my-ov-events ov-events)
      (let* ((ov1 (make-overlay 1 6))
             (ov2 (make-overlay 7 12))
             (ov3 (make-overlay 13 20))
             (_ (overlay-put ov1 'modification-hooks
                            (list (lambda (ov after-p beg end &optional _len)
                                    (push (ov-mod-event :overlay-name "ov1"
                                                       :is-before (not after-p)
                                                       :ov-start (overlay-start ov)
                                                       :ov-end (overlay-end ov)
                                                       :change-beg beg)
                                          ov-events)))))
             (_ (overlay-put ov2 'modification-hooks
                            (list (lambda (ov after-p beg end &optional _len)
                                    (push (ov-mod-event :overlay-name "ov2"
                                                       :is-before (not after-p)
                                                       :ov-start (overlay-start ov)
                                                       :ov-end (overlay-end ov)
                                                       :change-beg beg)
                                          ov-events)))))
             (_ (overlay-put ov3 'modification-hooks
                            (list (lambda (ov after-p beg end &optional _len)
                                    (push (ov-mod-event :overlay-name "ov3"
                                                       :is-before (not after-p)
                                                       :ov-start (overlay-start ov)
                                                       :ov-end (overlay-end ov)
                                                       :change-beg beg)
                                          ov-events)))))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov3 'priority 3))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (goto-char 4)
        (insert "XX")
        (push (list 'normal-edit (length ov-events)) results)
        (let ((inhibit-modification-hooks t))
          (goto-char 10)
          (insert "YY")
          (push (list 'inhibited-edit (length ov-events)) results))
        (goto-char 16)
        (insert "ZZ")
        (push (list 'resumed-edit (length ov-events)) results)
        (setq ov-events (reverse ov-events))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | events=%d results=%s m=%d"
                       (length ov-events) results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ome-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length ov-events)
                (marker-position m)
                (overlay-start ov1) (overlay-end ov1)
                (overlay-start ov2) (overlay-end ov2)
                (overlay-start ov3) (overlay-end ov3)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_inhibit_narrow_overlay_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass inhibit-snap ()
    ((label :initarg :label :accessor is-label :initform "")
     (inhibited :initarg :inhibited :accessor is-inhibited :initform nil)
     (hook-count :initarg :hook-count :accessor is-hooks :initform 0)
     (buf-string :initarg :buf-string :accessor is-bs :initform "")))
  (let* ((buf (generate-new-buffer "im3"))
         (after-count 0)
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'region 'r1)
      (put-text-property 6 10 'region 'r2)
      (put-text-property 11 15 'region 'r3)
      (put-text-property 16 20 'region 'r4)
      (put-text-property 21 25 'region 'r5)
      (setq-local my-after-count after-count
                  my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (setq after-count (1+ after-count)))
                  nil t)
        (undo-boundary)
        (push (inhibit-snap :label "init"
                           :inhibited inhibit-modification-hooks
                           :hook-count after-count
                           :buf-string (buffer-string)) snaps)
        (save-restriction
          (narrow-to-region 6 15)
          (goto-char 8)
          (insert "XX")
          (push (inhibit-snap :label "narrow-insert"
                             :inhibited inhibit-modification-hooks
                             :hook-count after-count
                             :buf-string (buffer-string)) snaps)
          (let ((inhibit-modification-hooks t))
            (goto-char 10)
            (insert "YY")
            (push (inhibit-snap :label "narrow-inhibited"
                               :inhibited inhibit-modification-hooks
                               :hook-count after-count
                               :buf-string (buffer-string)) snaps))
          (delete-region 7 9)
          (push (inhibit-snap :label "narrow-delete"
                             :inhibited inhibit-modification-hooks
                             :hook-count after-count
                             :buf-string (buffer-string)) snaps))
        (goto-char 3)
        (insert "PRE")
        (push (inhibit-snap :label "widen-insert"
                           :inhibited inhibit-modification-hooks
                           :hook-count after-count
                           :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (is-label s) (is-hooks s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'isn-log t)
        (remove-hook 'after-change-functions (car (default-value 'after-change-functions)) t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_inhibit_insert_before_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass marker-snapshot ()
    ((name :initarg :name :accessor mks-name :initform "")
     (m1-pos :initarg :m1-pos :accessor mks-m1 :initform 0)
     (m2-pos :initarg :m2-pos :accessor mks-m2 :initform 0)
     (m3-pos :initarg :m3-pos :accessor mks-m3 :initform 0)
     (hook-count :initarg :hook-count :accessor mks-hooks :initform 0)))
  (let* ((buf (generate-new-buffer "im4"))
         (hook-count 0)
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAA")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 16 'zone 'z3)
      (setq-local my-hook-count hook-count
                  my-snaps snaps)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (m3 (make-marker))
             (_ (set-marker m1 4))
             (_ (set-marker m2 8))
             (_ (set-marker m3 12))
             (_ (set-marker-insertion-type m2 t))
             (results nil))
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (setq hook-count (1+ hook-count)))
                  nil t)
        (undo-boundary)
        (push (marker-snapshot :name "init" :m1-pos (marker-position m1)
                              :m2-pos (marker-position m2) :m3-pos (marker-position m3)
                              :hook-count hook-count) snaps)
        (goto-char 5)
        (insert "BBB")
        (push (marker-snapshot :name "after-insert" :m1-pos (marker-position m1)
                              :m2-pos (marker-position m2) :m3-pos (marker-position m3)
                              :hook-count hook-count) snaps)
        (let ((inhibit-modification-hooks t))
          (goto-char 8)
          (insert "CCC")
          (push (marker-snapshot :name "after-inhibit" :m1-pos (marker-position m1)
                                :m2-pos (marker-position m2) :m3-pos (marker-position m3)
                                :hook-count hook-count) snaps))
        (delete-region 6 9)
        (push (marker-snapshot :name "after-delete" :m1-pos (marker-position m1)
                              :m2-pos (marker-position m2) :m3-pos (marker-position m3)
                              :hook-count hook-count) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s)
                               (list (mks-name s) (mks-m1 s) (mks-m2 s) (mks-m3 s) (mks-hooks s)))
                             snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m1=%d m2=%d m3=%d"
                       results (marker-position m1) (marker-position m2) (marker-position m3)))
        (put-text-property (1- (point-max)) (point-max) 'mks-log t)
        (remove-hook 'after-change-functions (car (default-value 'after-change-functions)) t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m1)
                (marker-position m2)
                (marker-position m3)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_inhibit_overlay_insert_in_behind_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ib-hook-event ()
    ((phase :initarg :phase :accessor ib-phase :initform "")
     (hook-count :initarg :hook-count :accessor ib-count :initform 0)
     (ov-start :initarg :ov-start :accessor ib-ovs :initform 0)
     (ov-end :initarg :ov-end :accessor ib-ove :initform 0)
     (buf-len :initarg :buf-len :accessor ib-blen :initform 0)))
  (let* ((buf (generate-new-buffer "im5"))
         (hook-count 0)
         (events nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'level 1)
      (put-text-property 6 10 'level 2)
      (put-text-property 11 15 'level 3)
      (put-text-property 16 20 'level 4)
      (setq-local my-hook-count hook-count
                  my-events events)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'insert-behind-hooks
                            (list (lambda (ov after-p beg end &optional _len)
                                    (when after-p
                                      (push (ib-hook-event
                                            :phase "behind-after"
                                            :hook-count hook-count
                                            :ov-start (overlay-start ov)
                                            :ov-end (overlay-end ov)
                                            :buf-len (buffer-size))
                                            events))))))
             (_ (overlay-put ov 'insert-in-front-hooks
                            (list (lambda (ov after-p beg end &optional _len)
                                    (when after-p
                                      (push (ib-hook-event
                                            :phase "front-after"
                                            :hook-count hook-count
                                            :ov-start (overlay-start ov)
                                            :ov-end (overlay-end ov)
                                            :buf-len (buffer-size))
                                            events))))))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8)))
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (setq hook-count (1+ hook-count)))
                  nil t)
        (undo-boundary)
        (goto-char 3)
        (insert "FRONT")
        (push (list 'front-edit (length events) hook-count) events)
        (goto-char 14)
        (insert "BEHIND")
        (push (list 'behind-edit (length events) hook-count) events)
        (let ((inhibit-modification-hooks t))
          (goto-char 5)
          (insert "INHIBITED")
          (push (list 'inhibited-edit (length events) hook-count) events))
        (setq events (reverse events))
        (goto-char (point-max))
        (insert (format " | events=%d m=%d ov=[%d,%d]"
                       (length events) (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ib-log t)
        (remove-hook 'after-change-functions (car (default-value 'after-change-functions)) t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length events)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
