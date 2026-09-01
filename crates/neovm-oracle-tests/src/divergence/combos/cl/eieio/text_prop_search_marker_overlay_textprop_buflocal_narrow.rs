//! Combo: text-property-search-forward/backward + EIEIO state tracking
//! + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests the newer text-property-search API with EIEIO objects recording
//! search results through editing and narrowing operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_tps_forward_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tps-fwd ()
    ((buf-name :initarg :buf :accessor tf-buf :initform "")
     (matches :initarg :matches :accessor tf-matches :initform nil)
     (log :initarg :log :accessor tf-log :initform nil)))
  (defmethod tf-search ((s tps-fwd) prop value start end)
    (with-current-buffer (tf-buf s)
      (setf (tf-matches s) nil)
      (goto-char start)
      (let ((count 0))
        (while (and (< (point) end) (< count 20))
          (let ((match (text-property-search-forward prop value t end)))
            (if match
                (progn
                  (push (list (prop-match-beginning match)
                              (prop-match-end match)
                              (prop-match-value match))
                        (tf-matches s))
                  (goto-char (prop-match-end match))
                  (setq count (1+ count)))
              (goto-char end))))
        (push (format "search:%S=%S:%d" prop value count) (tf-log s))
        (setq (tf-matches s) (reverse (tf-matches s))))))
  (let* ((buf (generate-new-buffer "tps1"))
         (s (tps-fwd :buf (buffer-name buf) :matches nil :log nil))
         (results nil))
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
      (setq-local my-tf-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (tf-search s 'zone 'a 1 40)
        (push (list "search-a" (tf-matches s) (marker-position m)) results)
        (tf-search s 'zone 'c 1 40)
        (push (list "search-c" (tf-matches s) (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-tf-log (cons "ins@8" my-tf-log))
        (tf-search s 'zone 'a 1 50)
        (push (list "search-a-edit" (tf-matches s) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 40)
          (tf-search s 'zone 'b 5 40)
          (push (list "narrow-search" (tf-matches s) (point-min) (point-max)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (tf-matches s) (tf-log s)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-tf-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_tps_backward_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tps-rev ()
    ((buf-name :initarg :buf :accessor tr-buf :initform "")
     (matches :initarg :matches :accessor tr-matches :initform nil)
     (log :initarg :log :accessor tr-log :initform nil)))
  (defmethod tr-search-back ((s tps-rev) prop value start)
    (with-current-buffer (tr-buf s)
      (setf (tr-matches s) nil)
      (goto-char start)
      (let ((count 0))
        (while (and (> (point) 1) (< count 20))
          (let ((match (text-property-search-backward prop value t 1)))
            (if match
                (progn
                  (push (list (prop-match-beginning match)
                              (prop-match-end match)
                              (prop-match-value match))
                        (tr-matches s))
                  (goto-char (max 1 (1- (prop-match-beginning match))))
                  (setq count (1+ count)))
              (goto-char 1))))
        (push (format "rev:%S=%S:%d" prop value count) (tr-log s))
        (setq (tr-matches s) (reverse (tr-matches s))))))
  (let* ((buf (generate-new-buffer "tps2"))
         (s (tps-rev :buf (buffer-name buf) :matches nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'layer 'l1)
      (put-text-property 6 10 'layer 'l2)
      (put-text-property 11 15 'layer 'l3)
      (put-text-property 16 20 'layer 'l4)
      (put-text-property 21 25 'layer 'l5)
      (put-text-property 26 30 'layer 'l6)
      (put-text-property 31 35 'layer 'l7)
      (put-text-property 36 40 'layer 'l8)
      (setq-local my-tr-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (tr-search-back s 'layer 'l5 40)
        (push (list "rev-l5" (tr-matches s) (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-tr-log (cons "ins@8" my-tr-log))
        (tr-search-back s 'layer 'l3 50)
        (push (list "rev-l3" (tr-matches s) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 40)
          (tr-search-back s 'layer 'l4 40)
          (push (list "narrow-rev" (tr-matches s) (point-min) (point-max)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (tr-matches s) (tr-log s)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-tr-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_tps_non_strict_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tps-nonstrict ()
    ((buf-name :initarg :buf :accessor tn-buf :initform "")
     (matches :initarg :matches :accessor tn-matches :initform nil)
     (log :initarg :log :accessor tn-log :initform nil)))
  (defmethod tn-search-non-strict ((s tps-nonstrict) prop start end)
    (with-current-buffer (tn-buf s)
      (setf (tn-matches s) nil)
      (goto-char start)
      (let ((count 0))
        (while (and (< (point) end) (< count 20))
          (let ((match (text-property-search-forward prop nil nil end)))
            (if match
                (progn
                  (push (list (prop-match-beginning match)
                              (prop-match-end match)
                              (prop-match-value match))
                        (tn-matches s))
                  (goto-char (prop-match-end match))
                  (setq count (1+ count)))
              (goto-char end))))
        (push (format "non-strict:%S:%d" prop count) (tn-log s))
        (setq (tn-matches s) (reverse (tn-matches s))))))
  (let* ((buf (generate-new-buffer "tps3"))
         (s (tps-nonstrict :buf (buffer-name buf) :matches nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 11 15 'face 'italic)
      (put-text-property 21 25 'face 'underline)
      (put-text-property 31 35 'face 'default)
      (put-text-property 41 45 'face 'shadow)
      (setq-local my-tn-log nil)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (tn-search-non-strict s 'face 1 50)
        (push (list "all-face" (length (tn-matches s)) (tn-matches s)
                    (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-tn-log (cons "ins@8" my-tn-log))
        (tn-search-non-strict s 'face 1 55)
        (push (list "after-edit" (length (tn-matches s))
                    (marker-position m)) results)
        (put-text-property 9 12 'face 'error)
        (tn-search-non-strict s 'face 1 55)
        (push (list "new-prop" (length (tn-matches s))
                    (tn-matches s) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 50)
          (tn-search-non-strict s 'face 5 50)
          (push (list "narrow" (length (tn-matches s)) (point-min) (point-max)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (tn-matches s) (tn-log s)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-tn-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_tps_overlay_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tps-ov ()
    ((buf-name :initarg :buf :accessor to-buf :initform "")
     (matches :initarg :matches :accessor to-matches :initform nil)
     (log :initarg :log :accessor to-log :initform nil)))
  (defmethod to-scan ((s tps-ov) prop start end)
    (with-current-buffer (to-buf s)
      (setf (to-matches s) nil)
      (goto-char start)
      (let ((count 0))
        (while (and (< (point) end) (< count 20))
          (let ((match (text-property-search-forward prop nil nil end)))
            (if match
                (progn
                  (push (list (prop-match-beginning match)
                              (prop-match-end match)
                              (prop-match-value match))
                        (to-matches s))
                  (goto-char (prop-match-end match))
                  (setq count (1+ count)))
              (goto-char end))))
        (push (format "scan:%d" count) (to-log s))
        (setq (to-matches s) (reverse (to-matches s))))))
  (let* ((buf (generate-new-buffer "tps4"))
         (s (tps-ov :buf (buffer-name buf) :matches nil :log nil))
         (results nil))
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
      (setq-local my-to-log nil)
      (let* ((ov1 (make-overlay 6 15))
             (ov2 (make-overlay 26 35))
             (_ (overlay-put ov1 'zone 'ov-zone-1))
             (_ (overlay-put ov1 'priority 10))
             (_ (overlay-put ov2 'zone 'ov-zone-2))
             (_ (overlay-put ov2 'priority 10))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (to-scan s 'zone 1 40)
        (push (list "with-ov" (length (to-matches s)) (to-matches s)
                    (marker-position m)) results)
        (delete-overlay ov1)
        (to-scan s 'zone 1 40)
        (push (list "del-ov1" (length (to-matches s))
                    (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-to-log (cons "ins@8" my-to-log))
        (to-scan s 'zone 1 50)
        (push (list "edit" (length (to-matches s))
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 45)
          (to-scan s 'zone 5 45)
          (push (list "narrow" (length (to-matches s)) (point-min) (point-max)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (to-matches s) (to-log s)
              (marker-position m)
              (overlay-start ov2) (overlay-end ov2)
              my-to-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_tps_undo_redo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tps-undo ()
    ((buf-name :initarg :buf :accessor tu-buf :initform "")
     (snapshots :initarg :snaps :accessor tu-snaps :initform nil)
     (log :initarg :log :accessor tu-log :initform nil)))
  (defmethod tu-snap-search ((s tps-undo) prop label)
    (with-current-buffer (tu-buf s)
      (goto-char 1)
      (let ((end (point-max))
            (matches nil)
            (count 0))
        (while (and (< (point) end) (< count 20))
          (let ((match (text-property-search-forward prop nil nil end)))
            (if match
                (progn
                  (push (list (prop-match-beginning match)
                              (prop-match-end match)
                              (prop-match-value match))
                        matches)
                  (goto-char (prop-match-end match))
                  (setq count (1+ count)))
              (goto-char end))))
        (push (list label (reverse matches) count) (tu-snaps s))
        (push (format "snap:%s:%d" label count) (tu-log s))
        count)))
  (let* ((buf (generate-new-buffer "tps5"))
         (s (tps-undo :buf (buffer-name buf) :snaps nil :log nil))
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
      (setq-local my-tu-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (tu-snap-search s 'face "init")
        (push (list "init" (car (tu-snaps s)) (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (put-text-property 9 12 'face 'error)
        (setq my-tu-log (cons "ins+prop" my-tu-log))
        (tu-snap-search s 'face "after-edit")
        (push (list "edit" (car (tu-snaps s)) (marker-position m)) results)
        (undo-boundary)
        (undo-more 1)
        (tu-snap-search s 'face "after-undo1")
        (push (list "undo1" (car (tu-snaps s)) (marker-position m)) results)
        (undo-more 1)
        (tu-snap-search s 'face "after-undo2")
        (push (list "undo2" (car (tu-snaps s)) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 40)
          (tu-snap-search s 'face "narrow")
          (push (list "narrow" (car (tu-snaps s)) (point-min) (point-max)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S snaps=%d"
                       results (length (tu-snaps s))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (length (tu-snaps s))
              (tu-log s)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-tu-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
