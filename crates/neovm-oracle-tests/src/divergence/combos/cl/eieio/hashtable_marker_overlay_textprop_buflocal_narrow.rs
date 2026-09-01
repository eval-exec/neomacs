//! Combo: cl-eieio hash-table with EIEIO object keys/values + overlays
//! + markers + textprop + buflocal + narrow + undo.
//! Tests hash table operations with EIEIO objects as keys and values,
//! including sxhash, object-equal, and hash table mutation during editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_hashtable_objects_as_values_with_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function ht-entry-val)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ht-entry ()
    ((key :initarg :key :accessor hte-key :initform "")
     (val :initarg :val :accessor hte-val :initform 0)
     (marker-pos :initarg :mpos :accessor hte-mpos :initform 0)))
  (let* ((buf (generate-new-buffer "ht1"))
         (ht (make-hash-table :test 'equal :size 10))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-ht-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (puthash "zone-a" (ht-entry :key "zone-a" :val 1 :mpos (marker-position m)) ht)
        (puthash "zone-b" (ht-entry :key "zone-b" :val 2 :mpos 0) ht)
        (push (list "init" (hash-table-count ht)
                    (ht-entry-val (gethash "zone-a" ht))) results)
        (let ((entry (gethash "zone-a" ht)))
          (setf (hte-mpos entry) (marker-position m)))
        (goto-char 8)
        (insert "XXX")
        (setq my-ht-log (cons "ins@8" my-ht-log))
        (let ((entry (gethash "zone-a" ht)))
          (setf (hte-mpos entry) (marker-position m)))
        (push (list "edit" (hash-table-count ht)
                    (hte-mpos (gethash "zone-a" ht))) results)
        (save-restriction
          (narrow-to-region 5 22)
          (push (list "narrow" (hte-mpos (gethash "zone-a" ht))
                      (line-number-at-pos (marker-position m))) results)
          (goto-char 7)
          (insert "YY")
          (setq my-ht-log (cons "ins-narrow@7" my-ht-log))
          (let ((entry (gethash "zone-b" ht)))
            (setf (hte-mpos entry) (marker-position m))))
        (push (list "widen" (hte-mpos (gethash "zone-a" ht))
                    (hte-mpos (gethash "zone-b" ht))
                    (hash-table-count ht)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ht-log=%S count=%d"
                       results (reverse my-ht-log) (hash-table-count ht)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ht-log t)
        (list (buffer-string)
              (hash-table-count ht)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ht-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_hashtable_sxhash_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass hashable-obj ()
    ((name :initarg :name :accessor ho-name :initform "")
     (value :initarg :value :accessor ho-val :initform 0)))
  (defmethod ho-equal ((a hashable-obj) (b hashable-obj))
    (equal (ho-name a) (ho-name b)))
  (let* ((buf (generate-new-buffer "ht2"))
         (ht (make-hash-table :test 'eql :size 10))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (setq-local my-ho-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (obj1 (hashable-obj :name "alpha" :value 1))
             (obj2 (hashable-obj :name "beta" :value 2))
             (obj3 (hashable-obj :name "gamma" :value 3))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (puthash obj1 "val1" ht)
        (puthash obj2 "val2" ht)
        (push (list "init" (hash-table-count ht)
                    (gethash obj1 ht)
                    (sxhash obj1)
                    (sxhash obj2)) results)
        (goto-char 8)
        (insert "MMM")
        (setq my-ho-log (cons "ins@8" my-ho-log))
        (setf (ho-val obj1) 99)
        (push (list "edit" (hash-table-count ht)
                    (gethash obj1 ht)
                    (sxhash obj1)
                    (ho-val obj1)) results)
        (remhash obj2 ht)
        (setq my-ho-log (cons "remhash-obj2" my-ho-log))
        (push (list "remhash" (hash-table-count ht)
                    (gethash obj2 ht)
                    (gethash obj3 ht)) results)
        (puthash obj3 "val3" ht)
        (maphash (lambda (k v) (setq my-ho-log (cons (format "%S->%S" k v) my-ho-log))) ht)
        (push (list "after-add" (hash-table-count ht)
                    (gethash obj3 ht)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ho-log=%S"
                       results (reverse my-ho-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ho-log t)
        (list (buffer-string)
              (hash-table-count ht)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ho-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_hashtable_with_marker_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass marker-entry ()
    ((label :initarg :label :accessor me-label :initform "")
     (stored-pos :initarg :pos :accessor me-pos :initform 0)))
  (let* ((buf (generate-new-buffer "ht3"))
         (ht (make-hash-table :test 'equal :size 10))
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
      (setq-local my-me-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 18))
             (results nil)
             (snap-ht
              (lambda ()
                (let ((entries nil))
                  (maphash (lambda (k v)
                            (push (list k (me-pos v)) entries)) ht)
                  (sort entries (lambda (a b) (string< (car a) (car b))))))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (puthash "m1" (marker-entry :label "m1" :pos 5) ht)
        (puthash "m2" (marker-entry :label "m2" :pos 10) ht)
        (puthash "m3" (marker-entry :label "m3" :pos 20) ht)
        (puthash "m4" (marker-entry :label "m4" :pos 30) ht)
        (push (list "init" (funcall snap-ht) (hash-table-count ht)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-me-log (cons "ins@8" my-me-log))
        (let ((e (gethash "m2" ht)))
          (setf (me-pos e) (marker-position m)))
        (push (list "edit" (funcall snap-ht) (hash-table-count ht)) results)
        (save-restriction
          (narrow-to-region 5 25)
          (puthash "m5" (marker-entry :label "m5" :pos (marker-position m)) ht)
          (setq my-me-log (cons "narrow-add" my-me-log))
          (push (list "narrow" (funcall snap-ht) (hash-table-count ht)) results))
        (remhash "m1" ht)
        (setq my-me-log (cons "rem-m1" my-me-log))
        (push (list "rem" (funcall snap-ht) (hash-table-count ht)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S me-log=%S"
                       results (reverse my-me-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'me-log t)
        (list (buffer-string)
              (hash-table-count ht)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-me-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_hashtable_clrhash_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass clr-entry ()
    ((key :initarg :key :accessor ce-key :initform "")
     (data :initarg :data :accessor ce-data :initform nil)))
  (let* ((buf (generate-new-buffer "ht4"))
         (ht (make-hash-table :test 'equal :size 10))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-ce-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (puthash "a" (clr-entry :key "a" :data (list 1 2 3)) ht)
        (puthash "b" (clr-entry :key "b" :data (list 4 5 6)) ht)
        (puthash "c" (clr-entry :key "c" :data (list 7 8 9)) ht)
        (push (list "init" (hash-table-count ht)
                    (ce-data (gethash "a" ht))) results)
        (clrhash ht)
        (setq my-ce-log (cons "clrhash" my-ce-log))
        (push (list "cleared" (hash-table-count ht)
                    (gethash "a" ht)) results)
        (puthash "d" (clr-entry :key "d" :data (list 10 11)) ht)
        (setq my-ce-log (cons "add-d" my-ce-log))
        (push (list "re-add" (hash-table-count ht)
                    (ce-data (gethash "d" ht))) results)
        (goto-char 8)
        (insert "PPP")
        (setq my-ce-log (cons "ins@8" my-ce-log))
        (push (list "edit" (hash-table-count ht)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ce-log=%S"
                       results (reverse my-ce-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ce-log t)
        (list (buffer-string)
              (hash-table-count ht)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ce-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_hashtable_copy_table_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cpy-entry ()
    ((id :initarg :id :accessor cpe-id :initform 0)
     (payload :initarg :payload :accessor cpe-payload :initform "")))
  (let* ((buf (generate-new-buffer "ht5"))
         (ht1 (make-hash-table :test 'eql :size 10))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-cpe-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (obj1 (cpy-entry :id 1 :payload "first"))
             (obj2 (cpy-entry :id 2 :payload "second"))
             (obj3 (cpy-entry :id 3 :payload "third"))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (puthash obj1 "v1" ht1)
        (puthash obj2 "v2" ht1)
        (push (list "init" (hash-table-count ht1)
                    (gethash obj1 ht1)) results)
        (let ((ht2 (copy-hash-table ht1)))
          (puthash obj3 "v3" ht2)
          (setq my-cpe-log (cons "copy+add" my-cpe-log))
          (push (list "copy" (hash-table-count ht1)
                      (hash-table-count ht2)
                      (gethash obj3 ht1)
                      (gethash obj3 ht2)) results))
        (goto-char 8)
        (insert "QQQ")
        (setq my-cpe-log (cons "ins@8" my-cpe-log))
        (setf (cpe-payload obj1) "modified-first")
        (push (list "edit" (hash-table-count ht1)
                    (cpe-payload obj1)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S cpe-log=%S"
                       results (reverse my-cpe-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cpe-log t)
        (list (buffer-string)
              (hash-table-count ht1)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-cpe-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
