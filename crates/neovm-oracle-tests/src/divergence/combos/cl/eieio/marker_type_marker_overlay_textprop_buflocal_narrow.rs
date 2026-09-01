//! Combo: cl-eieio marker insertion types (advance/stay) + overlays + textprop
//! + buflocal + narrow + undo.
//! Tests marker insertion-type behavior under complex editing scenarios.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_mtype_advance_vs_stay_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mtype-snap ()
    ((step :initarg :step :accessor ms-step :initform "")
     (adv-pos :initarg :adv :accessor ms-adv :initform 0)
     (stay-pos :initarg :stay :accessor ms-stay :initform 0)
     (buf-string :initarg :bs :accessor ms-bs :initform "")))
  (let* ((buf (generate-new-buffer "mt1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "ABCDEFGH-IJKLMNOP-QRSTUVWX")
      (put-text-property 1 9 'block 'first)
      (put-text-property 10 18 'block 'second)
      (put-text-property 19 27 'block 'third)
      (setq-local my-log nil)
      (let* ((m-adv (set-marker (make-marker) 10))
             (m-stay (set-marker (make-marker) 10))
             (_ (set-marker-insertion-type m-adv t))
             (_ (set-marker-insertion-type m-stay nil))
             (ov (make-overlay 5 15))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 1))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mtype-snap :step "init"
                         :adv (marker-position m-adv)
                         :stay (marker-position m-stay)
                         :bs (buffer-string)) snaps)
        (goto-char 10)
        (insert "ZZ")
        (setq my-log (cons "insert-at-10" my-log))
        (push (mtype-snap :step "insert-at-10"
                         :adv (marker-position m-adv)
                         :stay (marker-position m-stay)
                         :bs (buffer-string)) snaps)
        (goto-char 5)
        (insert "YY")
        (setq my-log (cons "insert-at-5" my-log))
        (push (mtype-snap :step "insert-at-5"
                         :adv (marker-position m-adv)
                         :stay (marker-position m-stay)
                         :bs (buffer-string)) snaps)
        (delete-region 10 14)
        (setq my-log (cons "delete-10-14" my-log))
        (push (mtype-snap :step "delete-mid"
                         :adv (marker-position m-adv)
                         :stay (marker-position m-stay)
                         :bs (buffer-string)) snaps)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (push (mtype-snap :step "undo-del"
                           :adv (marker-position m-adv)
                           :stay (marker-position m-stay)
                           :bs (buffer-string)) snaps)
          (setq my-log (cons (format "after-undo:%S" bs) my-log)))
        (primitive-undo 1 buffer-undo-list)
        (push (mtype-snap :step "undo-ins-5"
                         :adv (marker-position m-adv)
                         :stay (marker-position m-stay)
                         :bs (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ms-step s) (ms-adv s) (ms-stay s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S log=%S"
                       results (reverse my-log)))
        (put-text-property (1- (point-max)) (point-max) 'ms-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m-adv) (marker-position m-stay)
              (marker-insertion-type m-adv) (marker-insertion-type m-stay)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_mtype_narrow_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mtype-narrow-snap ()
    ((step :initarg :step :accessor mns-step :initform "")
     (adv-pos :initarg :adv :accessor mns-adv :initform 0)
     (stay-pos :initarg :stay :accessor mns-stay :initform 0)
     (narrow-min :initarg :nmin :accessor mns-nmin :initform 1)
     (narrow-max :initarg :nmax :accessor mns-nmax :initform 0)))
  (let* ((buf (generate-new-buffer "mt2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (setq-local my-narrow-log nil)
      (let* ((m-adv (set-marker (make-marker) 11))
             (m-stay (set-marker (make-marker) 11))
             (_ (set-marker-insertion-type m-adv t))
             (_ (set-marker-insertion-type m-stay nil))
             (ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'italic))
             (_ (overlay-put ov 'priority 2))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mtype-narrow-snap :step "init"
                                :adv (marker-position m-adv)
                                :stay (marker-position m-stay)
                                :nmin (point-min)
                                :nmax (point-max)) snaps)
        (save-restriction
          (narrow-to-region 6 20)
          (push (mtype-narrow-snap :step "narrow"
                                  :adv (marker-position m-adv)
                                  :stay (marker-position m-stay)
                                  :nmin (point-min)
                                  :nmax (point-max)) snaps)
          (goto-char (point-min))
          (insert "QQ")
          (setq my-narrow-log (cons "insert-narrow-min" my-narrow-log))
          (push (mtype-narrow-snap :step "insert-nmin"
                                  :adv (marker-position m-adv)
                                  :stay (marker-position m-stay)
                                  :nmin (point-min)
                                  :nmax (point-max)) snaps)
          (goto-char (point-max))
          (insert "RR")
          (setq my-narrow-log (cons "insert-narrow-max" my-narrow-log))
          (push (mtype-narrow-snap :step "insert-nmax"
                                  :adv (marker-position m-adv)
                                  :stay (marker-position m-stay)
                                  :nmin (point-min)
                                  :nmax (point-max)) snaps))
        (push (mtype-narrow-snap :step "widen"
                                :adv (marker-position m-adv)
                                :stay (marker-position m-stay)
                                :nmin (point-min)
                                :nmax (point-max)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mns-step s) (mns-adv s) (mns-stay s)
                                                (mns-nmin s) (mns-nmax s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S log=%S"
                       results (reverse my-narrow-log)))
        (put-text-property (1- (point-max)) (point-max) 'mns-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m-adv) (marker-position m-stay)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_mtype_multiple_markers_edit_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mtype-chain-snap ()
    ((step :initarg :step :accessor mcs-step :initform "")
     (positions :initarg :pos :accessor mcs-pos :initform nil)
     (buf-string :initarg :bs :accessor mcs-bs :initform "")))
  (let* ((buf (generate-new-buffer "mt3"))
         (snaps nil)
         (make-typed-marker
          (lambda (pos type)
            (let ((m (set-marker (make-marker) pos)))
              (set-marker-insertion-type m type)
              m))))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-chain nil)
      (let* ((m1 (funcall make-typed-marker 6 t))
             (m2 (funcall make-typed-marker 6 nil))
             (m3 (funcall make-typed-marker 11 t))
             (m4 (funcall make-typed-marker 11 nil))
             (m5 (funcall make-typed-marker 30 t))
             (m6 (funcall make-typed-marker 30 nil))
             (ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (results nil)
             (all-markers (list m1 m2 m3 m4 m5 m6)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mtype-chain-snap :step "init"
                               :pos (mapcar 'marker-position all-markers)
                               :bs (buffer-string)) snaps)
        (goto-char 6)
        (insert "XX")
        (setq my-chain (cons "ins@6" my-chain))
        (push (mtype-chain-snap :step "ins@6"
                               :pos (mapcar 'marker-position all-markers)
                               :bs (buffer-string)) snaps)
        (undo-boundary)
        (goto-char 15)
        (insert "YY")
        (setq my-chain (cons "ins@15" my-chain))
        (push (mtype-chain-snap :step "ins@15"
                               :pos (mapcar 'marker-position all-markers)
                               :bs (buffer-string)) snaps)
        (undo-boundary)
        (delete-region 8 12)
        (setq my-chain (cons "del@8-12" my-chain))
        (push (mtype-chain-snap :step "del@8-12"
                               :pos (mapcar 'marker-position all-markers)
                               :bs (buffer-string)) snaps)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (push (mtype-chain-snap :step "undo-del"
                               :pos (mapcar 'marker-position all-markers)
                               :bs (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (mtype-chain-snap :step "undo-ins15"
                               :pos (mapcar 'marker-position all-markers)
                               :bs (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (mtype-chain-snap :step "undo-ins6"
                               :pos (mapcar 'marker-position all-markers)
                               :bs (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mcs-step s) (mcs-pos s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S chain=%S"
                       results (reverse my-chain)))
        (put-text-property (1- (point-max)) (point-max) 'mcs-log t)
        (list (buffer-string)
              (length snaps)
              (mapcar 'marker-position all-markers)
              (mapcar 'marker-insertion-type all-markers)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_mtype_overlay_modification_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mtype-hook-snap ()
    ((step :initarg :step :accessor mhs-step :initform "")
     (adv-pos :initarg :adv :accessor mhs-adv :initform 0)
     (stay-pos :initarg :stay :accessor mhs-stay :initform 0)
     (hook-count :initarg :hc :accessor mhs-hc :initform 0)))
  (let* ((buf (generate-new-buffer "mt4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local hook-fire-count 0)
      (let* ((m-adv (set-marker (make-marker) 10))
             (m-stay (set-marker (make-marker) 10))
             (_ (set-marker-insertion-type m-adv t))
             (_ (set-marker-insertion-type m-stay nil))
             (ov (make-overlay 6 20))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'modification-hooks
                           (list (lambda (ov after-p beg end &optional _len)
                                   (when after-p
                                     (setq hook-fire-count
                                           (1+ hook-fire-count))))))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mtype-hook-snap :step "init"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :hc hook-fire-count) snaps)
        (goto-char 6)
        (insert "ZZZZ")
        (push (mtype-hook-snap :step "ins-before"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :hc hook-fire-count) snaps)
        (undo-boundary)
        (goto-char 10)
        (insert "WWWW")
        (push (mtype-hook-snap :step "ins-at-marker"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :hc hook-fire-count) snaps)
        (undo-boundary)
        (delete-region 6 14)
        (push (mtype-hook-snap :step "del-overlap"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :hc hook-fire-count) snaps)
        (undo-boundary)
        (primitive-undo 3 buffer-undo-list)
        (push (mtype-hook-snap :step "undo-all"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :hc hook-fire-count) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mhs-step s) (mhs-adv s)
                                                (mhs-stay s) (mhs-hc s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S"
                       results))
        (put-text-property (1- (point-max)) (point-max) 'mhs-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m-adv) (marker-position m-stay)
              (overlay-start ov) (overlay-end ov)
              hook-fire-count)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_mtype_buflocal_undo_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mtype-buflocal-snap ()
    ((step :initarg :step :accessor mbs-step :initform "")
     (adv-pos :initarg :adv :accessor mbs-adv :initform 0)
     (stay-pos :initarg :stay :accessor mbs-stay :initform 0)
     (tab-w :initarg :tw :accessor mbs-tw :initform 8)
     (fill-col :initarg :fc :accessor mbs-fc :initform 70)))
  (let* ((buf (generate-new-buffer "mt5"))
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
      (setq-local tab-width 4)
      (setq-local fill-column 40)
      (setq-local my-buflog nil)
      (let* ((m-adv (set-marker (make-marker) 15))
             (m-stay (set-marker (make-marker) 15))
             (_ (set-marker-insertion-type m-adv t))
             (_ (set-marker-insertion-type m-stay nil))
             (ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'underline))
             (_ (overlay-put ov 'priority 3))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (mtype-buflocal-snap :step "init"
                                  :adv (marker-position m-adv)
                                  :stay (marker-position m-stay)
                                  :tw tab-width
                                  :fc fill-column) snaps)
        (goto-char 10)
        (insert "MMMM")
        (setq my-buflog (cons "ins@10" my-buflog))
        (push (mtype-buflocal-snap :step "edit1"
                                  :adv (marker-position m-adv)
                                  :stay (marker-position m-stay)
                                  :tw tab-width
                                  :fc fill-column) snaps)
        (undo-boundary)
        (setq-local tab-width 8)
        (setq-local fill-column 80)
        (push (mtype-buflocal-snap :step "buflocal-change"
                                  :adv (marker-position m-adv)
                                  :stay (marker-position m-stay)
                                  :tw tab-width
                                  :fc fill-column) snaps)
        (save-restriction
          (narrow-to-region 5 30)
          (goto-char 10)
          (insert "NNNN")
          (setq my-buflog (cons "ins-narrow@10" my-buflog))
          (push (mtype-buflocal-snap :step "narrow-edit"
                                    :adv (marker-position m-adv)
                                    :stay (marker-position m-stay)
                                    :tw tab-width
                                    :fc fill-column) snaps))
        (push (mtype-buflocal-snap :step "widen"
                                  :adv (marker-position m-adv)
                                  :stay (marker-position m-stay)
                                  :tw tab-width
                                  :fc fill-column) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mbs-step s) (mbs-adv s) (mbs-stay s)
                                                (mbs-tw s) (mbs-fc s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S buflog=%S"
                       results (reverse my-buflog)))
        (put-text-property (1- (point-max)) (point-max) 'mbs-log t)
        (set-marker m-adv 3)
        (set-marker m-stay 3)
        (list (buffer-string)
              (length snaps)
              (marker-position m-adv) (marker-position m-stay)
              (overlay-start ov) (overlay-end ov)
              tab-width fill-column)))
    (kill-buffer buf)))"#,
        expect,
    );
}
