//! Combo: cl-eieio hash-table with object keys/values + marker + overlay + textprop + buflocal + undo.
//! Tests using EIEIO objects as hash table keys/values with sxhash/equal, and buffer operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_objects_as_hash_values_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function hash-table-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass registry-entry ()
    ((id :initarg :id :accessor re-id :initform 0)
     (label :initarg :label :accessor re-label :initform "")
     (active :initarg :active :accessor re-active :initform t)))
  (let* ((buf (generate-new-buffer "ht1"))
         (ht (make-hash-table :test 'equal))
         (e1 (registry-entry :id 1 :label "alpha" :active t))
         (e2 (registry-entry :id 2 :label "beta" :active nil))
         (e3 (registry-entry :id 3 :label "gamma" :active t)))
    (puthash "a" e1 ht)
    (puthash "b" e2 ht)
    (puthash "c" e3 ht)
    (with-current-buffer buf
      (insert "REG:a-alpha:b-beta:c-gamma")
      (put-text-property 1 4 'reg 'header)
      (put-text-property 5 6 'key "a")
      (put-text-property 7 12 'val 'alpha)
      (put-text-property 13 14 'key "b")
      (put-text-property 15 19 'val 'beta)
      (put-text-property 20 21 'key "c")
      (put-text-property 22 27 'val 'gamma)
      (setq-local registry ht)
      (setq-local entries (list e1 e2 e3))
      (let* ((ov (make-overlay 7 19))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (let ((a-entry (gethash "a" ht))
              (b-entry (gethash "b" ht)))
          (setf (re-label a-entry) "alpha-v2")
          (setf (re-active b-entry) t)
          (let ((a-label (re-label (gethash "a" ht)))
                (b-active (re-active (gethash "b" ht)))
                (all-keys (sort (hash-table-keys ht) #'string<)))
            (goto-char 7)
            (insert (format "[%s:%s|%s:%s]"
                           (car all-keys) a-label
                           (cadr all-keys) b-active))
            (setf (marker-position m) 15)
            (put-text-property 7 (+ 7 (length (format "[%s:%s|%s:%s]"
                                                        (car all-keys) a-label
                                                        (cadr all-keys) b-active)))
                              'hash-update t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (a-label (re-label (gethash "a" registry)))
              (b-active (re-active (gethash "b" registry)))
              (c-entry (gethash "c" registry))
              (ht-count (hash-table-count registry)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs a-label b-active (re-label c-entry) ht-count
                (marker-position m)
                (buffer-string)
                registry entries)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_object_registry_hash_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 19 21)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tracked-item ()
    ((code :initarg :code :accessor ti-code :initform "")
     (score :initarg :score :accessor ti-score :initform 0)))
  (let* ((buf (generate-new-buffer "ht2"))
         (ht (make-hash-table :test 'equal))
         (items nil))
    (dotimes (i 5)
      (let ((item (tracked-item :code (format "item-%d" i) :score (* i 10))))
        (push item items)
        (puthash (ti-code item) item ht)))
    (setq items (nreverse items))
    (with-current-buffer buf
      (insert "ITEMS:0:10:20:30:40")
      (put-text-property 1 6 'field 'header)
      (dotimes (i 5)
        (let ((start (+ 7 (* i 3)))
              (end (+ 9 (* i 3))))
          (put-text-property start end 'score (* i 10))))
      (setq-local item-hash ht)
      (setq-local item-list items)
      (let* ((ov0 (make-overlay 7 9))
             (ov4 (make-overlay 19 21))
             (_ (overlay-put ov0 'priority 0))
             (_ (overlay-put ov4 'priority 4))
             (m (make-marker))
             (_ (set-marker m 8))
             (scores-before (mapcar (lambda (item) (ti-score item)) items)))
        (undo-boundary)
        (dolist (item items)
          (setf (ti-score item) (+ (ti-score item) 5))
          (puthash (ti-code item) item ht))
        (let ((scores-after (mapcar (lambda (item) (ti-score item)) items))
              (hash-scores (mapcar (lambda (code)
                                     (ti-score (gethash code ht)))
                                   (sort (hash-table-keys ht) #'string<))))
          (goto-char 7)
          (insert (format "%s->%s" scores-before scores-after))
          (setf (marker-position m) 15)
          (put-text-property 7 (+ 7 (length (format "%s->%s" scores-before scores-after)))
                            'score-change t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os0 (overlay-start ov0))
              (oe0 (overlay-end ov0))
              (os4 (overlay-start ov4))
              (oe4 (overlay-end ov4))
              (bs (buffer-string))
              (final-scores (mapcar (lambda (item) (ti-score item)) item-list))
              (hash-count (hash-table-count item-hash)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os0 oe0 os4 oe4 bs final-scores hash-count
                (marker-position m)
                (buffer-string)
                item-hash item-list)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_hash_maphash_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass metric-point ()
    ((timestamp :initarg :timestamp :accessor mp-ts :initform 0)
     (value :initarg :value :accessor mp-val :initform 0.0)))
  (let* ((buf (generate-new-buffer "ht3"))
         (ht (make-hash-table :test 'eql))
         (p1 (metric-point :timestamp 100 :value 1.5))
         (p2 (metric-point :timestamp 200 :value 2.5))
         (p3 (metric-point :timestamp 300 :value 3.5)))
    (puthash 1 p1 ht)
    (puthash 2 p2 ht)
    (puthash 3 p3 ht)
    (with-current-buffer buf
      (insert "METRICS:1=1.5:2=2.5:3=3.5")
      (put-text-property 1 8 'field 'header)
      (put-text-property 9 13 'field 'm1)
      (put-text-property 14 19 'field 'm2)
      (put-text-property 20 25 'field 'm3)
      (setq-local metrics ht)
      (setq-local points (list p1 p2 p3))
      (let* ((ov (make-overlay 9 25))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 12)))
        (undo-boundary)
        (let ((sum 0.0))
          (maphash (lambda (k v) (setq sum (+ sum (mp-val v)))) ht)
          (let ((avg (/ sum (hash-table-count ht))))
            (setf (mp-val p1) (+ (mp-val p1) 1.0))
            (setf (mp-val p2) (+ (mp-val p2) 2.0))
            (let ((new-sum 0.0))
              (maphash (lambda (k v) (setq new-sum (+ new-sum (mp-val v)))) ht)
              (goto-char 9)
              (insert (format "sum=%.1f->%.1f:avg=%.1f" sum new-sum avg))
              (setf (marker-position m) 18)
              (put-text-property 9 (+ 9 (length (format "sum=%.1f->%.1f:avg=%.1f" sum new-sum avg)))
                                'metric-change t))))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (v1 (mp-val (car points)))
              (v2 (mp-val (cadr points)))
              (v3 (mp-val (caddr points)))
              (hc (hash-table-count metrics)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs v1 v2 v3 hc
                (marker-position m)
                (buffer-string)
                metrics points)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_hash_removal_with_object_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 12 28)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass session ()
    ((sid :initarg :sid :accessor session-sid :initform "")
     (user :initarg :user :accessor session-user :initform "")
     (active :initarg :active :accessor session-active :initform t)))
  (let* ((buf (generate-new-buffer "ht4"))
         (ht (make-hash-table :test 'equal))
         (s1 (session :sid "s001" :user "alice" :active t))
         (s2 (session :sid "s002" :user "bob" :active t))
         (s3 (session :sid "s003" :user "carol" :active t)))
    (puthash (session-sid s1) s1 ht)
    (puthash (session-sid s2) s2 ht)
    (puthash (session-sid s3) s3 ht)
    (with-current-buffer buf
      (insert "SESSIONS:3:alice,bob,carol")
      (put-text-property 1 9 'field 'header)
      (put-text-property 10 11 'field 'count)
      (put-text-property 12 28 'field 'users)
      (setq-local sessions ht)
      (setq-local session-list (list s1 s2 s3))
      (let* ((ov (make-overlay 12 28))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 15)))
        (undo-boundary)
        (let ((active-users (list (session-user s1) (session-user s2) (session-user s3))))
          (setf (session-active s2) nil)
          (remhash (session-sid s2) ht)
          (setf (session-user s1) "alice-v2")
          (let ((remaining-users nil)
                (remaining-keys (sort (hash-table-keys ht) #'string<)))
            (dolist (k remaining-keys)
              (push (session-user (gethash k ht)) remaining-users))
            (goto-char 10)
            (insert (format "%d->%d[%s]"
                           3 (hash-table-count ht)
                           (mapconcat #'identity (reverse remaining-users) ",")))
            (setf (marker-position m) 12)
            (put-text-property 10 (+ 10 (length (format "%d->%d[%s]"
                                                          3 (hash-table-count ht)
                                                          (mapconcat #'identity (reverse remaining-users) ","))))
                              'session-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (hc (hash-table-count sessions))
              (s2-active (session-active s2))
              (s1-user (session-user s1)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs hc s2-active s1-user
                (marker-position m)
                (buffer-string)
                sessions session-list)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_hash_cl_loop_with_objects_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 30 35)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass node-info ()
    ((name :initarg :name :accessor ni-name :initform "")
     (weight :initarg :weight :accessor ni-weight :initform 0)
     (group :initarg :group :accessor ni-group :initform "")))
  (let* ((buf (generate-new-buffer "ht5"))
         (ht (make-hash-table :test 'equal))
         (n1 (node-info :name "a" :weight 10 :group "x"))
         (n2 (node-info :name "b" :weight 20 :group "y"))
         (n3 (node-info :name "c" :weight 30 :group "x"))
         (n4 (node-info :name "d" :weight 40 :group "y"))
         (n5 (node-info :name "e" :weight 50 :group "z")))
    (dolist (n (list n1 n2 n3 n4 n5))
      (puthash (ni-name n) n ht))
    (with-current-buffer buf
      (insert "NODES:a=10:b=20:c=30:d=40:e=50")
      (put-text-property 1 6 'field 'header)
      (put-text-property 7 11 'node "a")
      (put-text-property 12 17 'node "b")
      (put-text-property 18 23 'node "c")
      (put-text-property 24 29 'node "d")
      (put-text-property 30 35 'node "e")
      (setq-local node-hash ht)
      (setq-local nodes (list n1 n2 n3 n4 n5))
      (let* ((ov (make-overlay 7 23))
             (_ (overlay-put ov 'priority 3))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (let* ((x-weight (cl-loop for n being the hash-values of ht
                                  when (equal (ni-group n) "x")
                                  sum (ni-weight n)))
               (y-weight (cl-loop for n being the hash-values of ht
                                  when (equal (ni-group n) "y")
                                  sum (ni-weight n)))
               (all-names (sort (cl-loop for n being the hash-values of ht
                                         collect (ni-name n))
                               #'string<)))
          (setf (ni-weight n1) 15
                (ni-weight n4) 45
                (ni-group n5) "x")
          (let* ((new-x-weight (cl-loop for n being the hash-values of ht
                                        when (equal (ni-group n) "x")
                                        sum (ni-weight n)))
                 (new-y-weight (cl-loop for n being the hash-values of ht
                                        when (equal (ni-group n) "y")
                                        sum (ni-weight n))))
            (goto-char 7)
            (insert (format "x:%d->%d|y:%d->%d" x-weight new-x-weight y-weight new-y-weight))
            (setf (marker-position m) 18)
            (put-text-property 7 (+ 7 (length (format "x:%d->%d|y:%d->%d"
                                                        x-weight new-x-weight y-weight new-y-weight)))
                              'weight-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (w1 (ni-weight (car nodes)))
              (w4 (ni-weight (cadddr nodes)))
              (g5 (ni-group (car (cddddr nodes))))
              (hc (hash-table-count node-hash)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs w1 w4 g5 hc
                (marker-position m)
                (buffer-string)
                node-hash nodes)))
      (kill-buffer buf))))"#,
        expect,
    );
}
