//! Combo: cl-eieio initialization lifecycle (make-instance, shared-initialize, initialize-instance,
//! reinitialize-instance) + marker + overlay + textprop + buflocal + narrow + undo.
//! Tests full object lifecycle with buffer state interaction.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_make_instance_buffer_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tracked-obj ()
    ((id :initarg :id :accessor obj-id :initform 0)
     (created :initarg :created :accessor obj-created :initform nil)
     (tag :initarg :tag :accessor obj-tag :initform "")))
  (defmethod initialize-instance :after ((obj tracked-obj) &rest args)
    (setf (obj-created obj) (format "t%d" (obj-id obj))))
  (let* ((buf (generate-new-buffer "mi1"))
         (o1 (make-instance 'tracked-obj :id 1 :tag "alpha"))
         (o2 (make-instance 'tracked-obj :id 2 :tag "beta")))
    (with-current-buffer buf
      (insert "OBJ1:alpha-OBJ2:beta")
      (put-text-property 1 5 'obj-id 1)
      (put-text-property 6 11 'obj-tag "alpha")
      (put-text-property 12 16 'obj-id 2)
      (put-text-property 17 21 'obj-tag "beta")
      (setq-local tracked (list o1 o2))
      (let* ((ov (make-overlay 6 11))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (let ((c1 (obj-created o1))
              (c2 (obj-created o2))
              (t1 (obj-tag o1))
              (t2 (obj-tag o2)))
          (setf (obj-tag o1) "gamma"
                (obj-tag o2) "delta")
          (goto-char 6)
          (insert (format "%s->%s|%s->%s" t1 (obj-tag o1) t2 (obj-tag o2)))
          (setf (marker-position m) 15)
          (put-text-property 6 (+ 6 (length (format "%s->%s|%s->%s"
                                                      t1 (obj-tag o1) t2 (obj-tag o2))))
                            'tag-change t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (o1-created (obj-created (car tracked)))
              (o2-created (obj-created (cadr tracked)))
              (o1-tag (obj-tag (car tracked)))
              (o2-tag (obj-tag (cadr tracked))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs o1-created o2-created o1-tag o2-tag
                (marker-position m)
                (buffer-string)
                tracked)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_shared_initialize_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass versioned-doc ()
    ((title :initarg :title :accessor doc-title :initform "")
     (version :initarg :version :accessor doc-version :initform 1)
     (content :initarg :content :accessor doc-content :initform "")
     (metadata :initarg :metadata :accessor doc-metadata :initform nil)))
  (defmethod shared-initialize :after ((doc versioned-doc) slots &rest args)
    (when (and (slot-boundp doc 'title) (slot-boundp doc 'version))
      (setf (doc-metadata doc)
            (list :title (doc-title doc)
                  :version (doc-version doc)
                  :initialized t))))
  (let* ((buf (generate-new-buffer "si1"))
         (d (versioned-doc :title "test" :version 2 :content "hello")))
    (with-current-buffer buf
      (insert "DOC:test:v2:content:hello")
      (put-text-property 1 4 'doc-field 'type)
      (put-text-property 5 9 'doc-field 'title)
      (put-text-property 10 12 'doc-field 'version)
      (put-text-property 13 20 'doc-field 'content)
      (put-text-property 21 26 'doc-field 'content-val)
      (setq-local my-doc d)
      (let* ((ov (make-overlay 5 12))
             (_ (overlay-put ov 'face 'italic))
             (m (make-marker))
             (_ (set-marker m 7)))
        (narrow-to-region 5 20)
        (undo-boundary)
        (let ((meta-before (doc-metadata d))
              (title-before (doc-title d))
              (ver-before (doc-version d)))
          (setf (doc-title d) "updated"
                (doc-version d) (+ ver-before 1))
          (let ((meta-after (doc-metadata d)))
            (goto-char (point-min))
            (insert (format "v%d:%s" (doc-version d) (doc-title d)))
            (setf (marker-position m) 8)
            (put-text-property (point-min) (+ (point-min) 10) 'updated t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-substring (point-min) (point-max)))
              (title (doc-title my-doc))
              (ver (doc-version my-doc))
              (meta (doc-metadata my-doc)))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe bs title ver meta
                (marker-position m)
                (buffer-string)
                my-doc)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_reinitialize_instance_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass connection ()
    ((host :initarg :host :accessor conn-host :initform "localhost")
     (port :initarg :port :accessor conn-port :initform 8080)
     (status :initarg :status :accessor conn-status :initform 'disconnected)
     (last-error :initarg :last-error :accessor conn-last-error :initform nil)))
  (defmethod initialize-instance :after ((c connection) &rest args)
    (setf (conn-status c) 'initialized))
  (let* ((buf (generate-new-buffer "ri1"))
         (conn (connection :host "db.example.com" :port 5432)))
    (with-current-buffer buf
      (insert "CONN:db.example.com:5432:init")
      (put-text-property 1 5 'field 'conn)
      (put-text-property 6 21 'field 'host)
      (put-text-property 22 26 'field 'port)
      (put-text-property 27 31 'field 'status)
      (setq-local my-conn conn)
      (let* ((ov (make-overlay 6 26))
             (_ (overlay-put ov 'priority 10))
             (m (make-marker))
             (_ (set-marker m 15)))
        (undo-boundary)
        (let ((status-before (conn-status conn))
              (host-before (conn-host conn))
              (port-before (conn-port conn)))
          (setf (conn-status conn) 'connected)
          (cl-reinitialize-instance conn :host "db2.example.com" :port 3306)
          (let ((new-status (conn-status conn))
                (new-host (conn-host conn))
                (new-port (conn-port conn))
                (new-error (conn-last-error conn)))
            (goto-char 6)
            (insert (format "%s:%d[%s]" new-host new-port new-status))
            (setf (marker-position m) 20)
            (put-text-property 6 (+ 6 (length (format "%s:%d[%s]"
                                                        new-host new-port new-status)))
                              'reinit t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (final-host (conn-host my-conn))
              (final-port (conn-port my-conn))
              (final-status (conn-status my-conn)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs final-host final-port final-status
                (marker-position m)
                (buffer-string)
                my-conn)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_allocation_class_slots_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass counter ()
    ((name :initarg :name :accessor counter-name :initform "")
     (count :initarg :count :accessor counter-count :initform 0)
     (total :allocation :class :accessor counter-total :initform 0)))
  (let* ((buf (generate-new-buffer "ac1"))
         (c1 (counter :name "clicks" :count 10))
         (c2 (counter :name "views" :count 20)))
    (with-current-buffer buf
      (insert "C1:clicks:10-C2:views:20-TOTAL:0")
      (put-text-property 1 3 'field 'c1)
      (put-text-property 4 10 'field 'c1-name)
      (put-text-property 11 13 'field 'c1-count)
      (put-text-property 14 16 'field 'c2)
      (put-text-property 17 22 'field 'c2-name)
      (put-text-property 23 25 'field 'c2-count)
      (put-text-property 26 31 'field 'total)
      (setq-local counters (list c1 c2))
      (let* ((ov (make-overlay 26 31))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 28)))
        (undo-boundary)
        (let ((t0 (counter-total c1)))
          (incf (counter-count c1) 5)
          (incf (counter-total c1) 5)
          (incf (counter-count c2) 15)
          (incf (counter-total c2) 15)
          (let ((t1 (counter-total c1))
                (c1c (counter-count c1))
                (c2c (counter-count c2)))
            (goto-char 26)
            (insert (format "TOTAL:%d+" t1))
            (setf (marker-position m) 33)
            (put-text-property 26 (+ 26 (length (format "TOTAL:%d+" t1)))
                              'class-slot t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (total (counter-total (car counters)))
              (c1-count (counter-count (car counters)))
              (c2-count (counter-count (cadr counters))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs total c1-count c2-count
                (marker-position m)
                (buffer-string)
                counters)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_constructor_allocation_multi_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buffer-state ()
    ((buf-name :initarg :buf-name :accessor state-buf-name :initform "")
     (point-pos :initarg :point-pos :accessor state-point :initform 1)
     (modified :initarg :modified :accessor state-modified :initform nil)
     (instance-count :allocation :class :accessor state-instance-count :initform 0)))
  (defmethod initialize-instance :after ((s buffer-state) &rest args)
    (incf (state-instance-count s)))
  (let* ((buf1 (generate-new-buffer "bs1"))
         (buf2 (generate-new-buffer "bs2")))
    (with-current-buffer buf1
      (insert "STATE1-ACTIVE")
      (put-text-property 1 7 'status 'state1)
      (put-text-property 8 14 'status 'active)
      (setq-local s1 (buffer-state :buf-name "bs1" :point-pos 8 :modified nil)))
    (with-current-buffer buf2
      (insert "STATE2-PENDING")
      (put-text-property 1 7 'status 'state2)
      (put-text-property 8 15 'status 'pending)
      (setq-local s2 (buffer-state :buf-name "bs2" :point-pos 1 :modified t)))
    (let* ((ov1 (with-current-buffer buf1
                  (let ((ov (make-overlay 8 14)))
                    (overlay-put ov 'priority 1) ov)))
           (ov2 (with-current-buffer buf2
                  (let ((ov (make-overlay 8 15)))
                    (overlay-put ov 'priority 2) ov)))
           (m1 (with-current-buffer buf1
                 (let ((m (make-marker))) (set-marker m 10) m)))
           (m2 (with-current-buffer buf2
                 (let ((m (make-marker))) (set-marker m 10) m))))
      (with-current-buffer buf1
        (undo-boundary)
        (let ((ic (state-instance-count s1)))
          (setf (state-point s1) 14)
          (setf (state-modified s1) t)
          (goto-char 8)
          (insert (format "ic=%d" ic))
          (setf (marker-position m1) 12)))
      (with-current-buffer buf2
        (undo-boundary)
        (setf (state-modified s2) nil)
        (goto-char 8)
        (insert "DONE:")
        (setf (marker-position m2) 14))
      (let ((mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2))
            (s1-mod (with-current-buffer buf1 (state-modified s1)))
            (s2-mod (with-current-buffer buf2 (state-modified s2)))
            (s1-name (with-current-buffer buf1 (state-buf-name s1)))
            (s2-name (with-current-buffer buf2 (state-buf-name s2)))
            (ic (with-current-buffer buf1 (state-instance-count s1)))
            (bs1 (with-current-buffer buf1 (buffer-string)))
            (bs2 (with-current-buffer buf2 (buffer-string))))
        (with-current-buffer buf1 (primitive-undo 1 buffer-undo-list))
        (with-current-buffer buf2 (primitive-undo 1 buffer-undo-list))
        (list mp1 mp2 os1 oe1 os2 oe2 s1-mod s2-mod s1-name s2-name ic bs1 bs2
              (marker-position m1) (marker-position m2)
              (with-current-buffer buf1 (buffer-string))
              (with-current-buffer buf2 (buffer-string)))))
    (kill-buffer buf1)
    (kill-buffer buf2)))"#,
        expect,
    );
}
