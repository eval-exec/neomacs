//! Combo: change-hooks (round 2) + EIEIO state tracking + overlays
//! + markers + textprop + buflocal + narrow + undo.
//! Tests before/after-change-functions with EIEIO recording combined
//! with overlay interaction, multi-handler dispatch, and before/after pairs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_ch2_basic_record() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ch2-log ()
    ((buf-name :initarg :buf :accessor c2-buf :initform "")
     (before-list :initarg :bl :accessor c2-bl :initform nil)
     (after-list :initarg :al :accessor c2-al :initform nil)
     (edit-count :initarg :ec :accessor c2-ec :initform 0)
     (log :initarg :log :accessor c2-log :initform nil)))
  (defmethod c2-setup ((log ch2-log))
    (with-current-buffer (c2-buf log)
      (add-hook 'before-change-functions
                (lambda (beg end)
                  (push (list beg end (buffer-substring-no-properties
                                       (max 1 beg) (max 1 end)))
                        (c2-bl log)))
                nil t)
      (add-hook 'after-change-functions
                (lambda (beg end len)
                  (setf (c2-ec log) (1+ (c2-ec log)))
                  (push (list beg end len (buffer-substring-no-properties
                                           (max 1 beg) (min end (point-max))))
                        (c2-al log)))
                nil t)))
  (let* ((buf (generate-new-buffer "ch2_1"))
         (clog (ch2-log :buf (buffer-name buf) :bl nil :al nil :ec 0 :log nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-c2-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (c2-setup clog)
        (goto-char 8)
        (insert "XXX")
        (push (list "edit1" (c2-ec clog) (length (c2-bl clog)) (length (c2-al clog))
                    (marker-position m)) results)
        (goto-char 20)
        (insert "YYY")
        (push (list "edit2" (c2-ec clog) (length (c2-bl clog)) (length (c2-al clog))
                    (marker-position m)) results)
        (delete-region 5 10)
        (push (list "del" (c2-ec clog) (length (c2-bl clog)) (length (c2-al clog))
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ec=%d" results (c2-ec clog)))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (c2-ec clog) (length (c2-bl clog)) (length (c2-al clog))
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-c2-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_ch2_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ch2-narrow ()
    ((buf-name :initarg :buf :accessor cn-buf :initform "")
     (changes :initarg :changes :accessor cn-changes :initform nil)
     (narrow-count :initarg :nc :accessor cn-nc :initform 0)
     (log :initarg :log :accessor cn-log :initform nil)))
  (defmethod cn-setup ((log ch2-narrow))
    (with-current-buffer (cn-buf log)
      (add-hook 'after-change-functions
                (lambda (beg end len)
                  (push (list beg end len (buffer-substring-no-properties
                                           (max 1 beg) (min end (point-max))))
                        (cn-changes log)))
                nil t)))
  (let* ((buf (generate-new-buffer "ch2_2"))
         (clog (ch2-narrow :buf (buffer-name buf) :changes nil :nc 0 :log nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (dotimes (i 10)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) 50) 'zone i))
      (setq-local my-cn-log nil)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (cn-setup clog)
        (goto-char 8)
        (insert "XXX")
        (push (list "edit1" (length (cn-changes clog)) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 45)
          (setf (cn-nc clog) (1+ (cn-nc clog)))
          (goto-char 10)
          (insert "YYY")
          (push (list "narrow-edit" (length (cn-changes clog))
                      (marker-position m) (point-min) (point-max)) results)
          (delete-region 8 12)
          (push (list "narrow-del" (length (cn-changes clog))
                      (marker-position m)) results))
        (push (list "after-narrow" (length (cn-changes clog))
                    (marker-position m)) results)
        (undo-boundary)
        (undo-more 1)
        (push (list "undo1" (length (cn-changes clog)) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S nc=%d" results (cn-nc clog)))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (length (cn-changes clog)) (cn-nc clog)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-cn-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_ch2_overlay_interact() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ch2-ov ()
    ((buf-name :initarg :buf :accessor co-buf :initform "")
     (ov-before :initarg :ovb :accessor co-ovb :initform nil)
     (ov-after :initarg :ova :accessor co-ova :initform nil)
     (ec :initarg :ec :accessor co-ec :initform 0)
     (log :initarg :log :accessor co-log :initform nil)))
  (defmethod co-setup ((log ch2-ov))
    (with-current-buffer (co-buf log)
      (add-hook 'before-change-functions
                (lambda (beg end)
                  (push (list beg end
                              (mapcar (lambda (ov)
                                       (list (overlay-start ov) (overlay-end ov)))
                                      (overlays-in beg end)))
                        (co-ovb log)))
                nil t)
      (add-hook 'after-change-functions
                (lambda (beg end len)
                  (setf (co-ec log) (1+ (co-ec log)))
                  (push (list beg end len
                              (mapcar (lambda (ov)
                                       (list (overlay-start ov) (overlay-end ov)))
                                      (overlays-in beg (min end (point-max)))))
                        (co-ova log)))
                nil t)))
  (let* ((buf (generate-new-buffer "ch2_3"))
         (clog (ch2-ov :buf (buffer-name buf) :ovb nil :ova nil :ec 0 :log nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-co-log nil)
      (let* ((ov1 (make-overlay 6 15))
             (ov2 (make-overlay 16 25))
             (ov3 (make-overlay 26 35))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 10))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 5))
             (_ (overlay-put ov3 'face 'underline))
             (_ (overlay-put ov3 'priority 15))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (co-setup clog)
        (goto-char 8) (insert "XXX")
        (push (list "in-ov1" (co-ec clog)
                    (length (co-ovb clog)) (length (co-ova clog))
                    (marker-position m)) results)
        (goto-char 20) (insert "YYY")
        (push (list "in-ov2" (co-ec clog)
                    (length (co-ovb clog)) (length (co-ova clog))
                    (marker-position m)) results)
        (goto-char 30) (insert "ZZZ")
        (push (list "in-ov3" (co-ec clog)
                    (length (co-ovb clog)) (length (co-ova clog))
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ec=%d ov-pos=%S"
                       results (co-ec clog)
                       (list (overlay-start ov1) (overlay-end ov1)
                             (overlay-start ov2) (overlay-end ov2)
                             (overlay-start ov3) (overlay-end ov3))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (co-ec clog) (marker-position m) my-co-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_ch2_multi_handler_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass hh2-base ()
    ((buf-name :initarg :buf :accessor hb-buf :initform "")
     (changes :initarg :changes :accessor hb-changes :initform nil)
     (log :initarg :log :accessor hb-log :initform nil)))
  (defclass hh2-strict (hh2-base)
    ((threshold :initarg :thr :accessor hs-thr :initform 5)))
  (defclass hh2-recording (hh2-base)
    ((verbose :initarg :verbose :accessor hr-verbose :initform t)))
  (defmethod hb-install ((handler hh2-base))
    (with-current-buffer (hb-buf handler)
      (add-hook 'after-change-functions
                (lambda (beg end len)
                  (push (list beg end len) (hb-changes handler)))
                nil t)))
  (defmethod hb-install ((handler hh2-strict))
    (with-current-buffer (hb-buf handler)
      (add-hook 'after-change-functions
                (lambda (beg end len)
                  (when (> (- end beg) (hs-thr handler))
                    (push (list 'large beg end len) (hb-changes handler))))
                nil t)))
  (defmethod hb-install ((handler hh2-recording))
    (with-current-buffer (hb-buf handler)
      (add-hook 'after-change-functions
                (lambda (beg end len)
                  (push (list 'rec beg end len
                              (buffer-substring-no-properties
                               (max 1 beg) (min end (point-max))))
                        (hb-changes handler)))
                nil t)))
  (let* ((buf (generate-new-buffer "ch2_4"))
         (base (hh2-base :buf (buffer-name buf) :changes nil :log nil))
         (strict (hh2-strict :buf (buffer-name buf) :changes nil :log nil :thr 3))
         (rec (hh2-recording :buf (buffer-name buf) :changes nil :log nil :verbose t))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-hb-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (hb-install base)
        (goto-char 8) (insert "XXX")
        (push (list "base1" (length (hb-changes base)) (marker-position m)) results)
        (hb-install strict)
        (goto-char 20) (insert "YYYYY")
        (push (list "strict1" (length (hb-changes strict))
                    (length (hb-changes base)) (marker-position m)) results)
        (hb-install rec)
        (goto-char 30) (insert "Z")
        (push (list "rec1" (length (hb-changes rec))
                    (hb-changes rec) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 50)
          (goto-char 10) (insert "NNNNN")
          (push (list "narrow" (length (hb-changes base))
                      (length (hb-changes strict))
                      (length (hb-changes rec))
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (length (hb-changes base))
              (length (hb-changes strict))
              (length (hb-changes rec))
              (hb-changes rec)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-hb-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_ch2_before_after_pairs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ch2-paired ()
    ((buf-name :initarg :buf :accessor cp-buf :initform "")
     (pairs :initarg :pairs :accessor cp-pairs :initform nil)
     (pending :initarg :pending :accessor cp-pending :initform nil)
     (log :initarg :log :accessor cp-log :initform nil)))
  (defmethod cp-install ((log ch2-paired))
    (with-current-buffer (cp-buf log)
      (add-hook 'before-change-functions
                (lambda (beg end)
                  (setf (cp-pending log)
                        (list beg end (buffer-substring-no-properties
                                       (max 1 beg) (max 1 end)))))
                nil t)
      (add-hook 'after-change-functions
                (lambda (beg end len)
                  (push (list (cp-pending log)
                              (list beg end len
                                    (buffer-substring-no-properties
                                     (max 1 beg) (min end (point-max)))))
                        (cp-pairs log))
                  (push (format "pair@%d" beg) (cp-log log)))
                nil t)))
  (let* ((buf (generate-new-buffer "ch2_5"))
         (clog (ch2-paired :buf (buffer-name buf) :pairs nil :pending nil :log nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (put-text-property 41 45 'face 'error)
      (put-text-property 46 50 'face 'match)
      (setq-local my-cp-log nil)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (cp-install clog)
        (goto-char 8) (insert "XXX")
        (push (list "ins1" (length (cp-pairs clog)) (cp-log clog)
                    (marker-position m)) results)
        (goto-char 20) (delete-region 18 22)
        (push (list "del1" (length (cp-pairs clog)) (cp-log clog)
                    (marker-position m)) results)
        (goto-char 25) (insert "YYYYY")
        (push (list "ins2" (length (cp-pairs clog)) (cp-log clog)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 55)
          (goto-char 10) (insert "NNN")
          (push (list "narrow" (length (cp-pairs clog)) (cp-log clog)
                      (marker-position m) (point-min) (point-max)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S pairs=%d log=%S"
                       results (length (cp-pairs clog)) (reverse (cp-log clog))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (length (cp-pairs clog)) (cp-pairs clog) (cp-log clog)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-cp-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
