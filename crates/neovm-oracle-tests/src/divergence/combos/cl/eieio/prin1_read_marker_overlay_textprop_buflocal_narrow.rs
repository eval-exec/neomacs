//! Combo: cl-eieio prin1/read roundtrip + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests serialization/deserialization of data structures containing EIEIO objects via prin1/read.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_prin1_read_roundtrip_alists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass entry ()
    ((key :initarg :key :accessor en-key :initform "")
     (val :initarg :val :accessor en-val :initform 0)))
  (let* ((buf (generate-new-buffer "pr1"))
         (e1 (entry :key "alpha" :val 10))
         (e2 (entry :key "beta" :val 20))
         (e3 (entry :key "gamma" :val 30))
         (alist (list (cons "a" (en-val e1))
                     (cons "b" (en-val e2))
                     (cons "c" (en-val e3)))))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'entry e1)
      (put-text-property 6 10 'entry e2)
      (put-text-property 11 15 'entry e3)
      (setq-local my-entries (list e1 e2 e3))
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (serialized nil)
             (deserialized nil)
             (results nil))
        (undo-boundary)
        (setq serialized (prin1-to-string alist))
        (push (list 'serialized serialized) results)
        (setq deserialized (read-from-string serialized))
        (push (list 'deserialized (car deserialized)) results)
        (let ((roundtrip-ok (equal alist (car deserialized))))
          (push (list 'roundtrip-ok roundtrip-ok) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'prin1-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-entries))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_prin1_nested_data_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass node ()
    ((id :initarg :id :accessor nd-id :initform 0)
     (label :initarg :label :accessor nd-label :initform "")
     (children-ids :initarg :children-ids :accessor nd-children :initform nil)))
  (let* ((buf (generate-new-buffer "pr2"))
         (n1 (node :id 1 :label "root" :children-ids '(2 3)))
         (n2 (node :id 2 :label "left" :children-ids '(4)))
         (n3 (node :id 3 :label "right" :children-ids '(5 6)))
         (tree-data (list (cons (nd-id n1) (list (nd-label n1) (nd-children n1)))
                         (cons (nd-id n2) (list (nd-label n2) (nd-children n2)))
                         (cons (nd-id n3) (list (nd-label n3) (nd-children n3))))))
    (with-current-buffer buf
      (insert "ROOT-LEFT-RIGHT")
      (put-text-property 1 5 'node n1)
      (put-text-property 6 10 'node n2)
      (put-text-property 11 15 'node n3)
      (setq-local my-nodes (list n1 n2 n3))
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 6))
             (serialized (prin1-to-string tree-data))
             (deserialized (car (read-from-string serialized)))
             (re-built nil))
        (undo-boundary)
        (dolist (entry deserialized)
          (let* ((id (car entry))
                 (data (cdr entry))
                 (label (car data))
                 (children (cadr data)))
            (push (make-instance 'node :id id :label label :children-ids children) re-built)))
        (setq re-built (reverse re-built))
        (let ((labels (mapcar (lambda (n) (list (nd-id n) (nd-label n) (nd-children n))) re-built)))
          (goto-char (point-max))
          (insert (format " | serial=%s rebuilt=%s m=%d"
                         serialized labels (marker-position m)))
          (set-marker m 4)
          (put-text-property (1- (point-max)) (point-max) 'rt-log t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs
                  (marker-position m)
                  (buffer-string)
                  my-nodes))))
    (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_prin1_vector_hash_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass record ()
    ((fields :initarg :fields :accessor rc-fields :initform nil)
     (tag :initarg :tag :accessor rc-tag :initform "")))
  (let* ((buf (generate-new-buffer "pr3"))
         (r1 (record :fields '(1 2 3) :tag "ints"))
         (r2 (record :fields '("a" "b") :tag "strs"))
         (r3 (record :fields '(t nil t) :tag "bools"))
         (list-data (list (list (rc-tag r1) (rc-fields r1))
                         (list (rc-tag r2) (rc-fields r2))
                         (list (rc-tag r3) (rc-fields r3)))))
    (with-current-buffer buf
      (insert "INTS-STRS-BOOLS")
      (put-text-property 1 5 'rec r1)
      (put-text-property 6 10 'rec r2)
      (put-text-property 11 16 'rec r3)
      (setq-local my-recs (list r1 r2 r3))
      (let* ((ov (make-overlay 6 16))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (serialized (prin1-to-string list-data))
             (deserialized (car (read-from-string serialized)))
             (results nil))
        (undo-boundary)
        (setq results (mapcar (lambda (x) (list (car x) (cadr x))) deserialized))
        (let ((roundtrip-ok (equal list-data deserialized)))
          (goto-char (point-max))
          (insert (format " | serial=%s results=%s ok=%s m=%d"
                         serialized results roundtrip-ok (marker-position m)))
          (set-marker m 5)
          (put-text-property (1- (point-max)) (point-max) 'vec-log t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs
                  (marker-position m)
                  (buffer-string)
                  my-recs))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_prin1_plist_roundtrip_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass config ()
    ((key :initarg :key :accessor cf-key :initform "")
     (value :initarg :value :accessor cf-value :initform nil)
     (type :initarg :type :accessor cf-type :initform "")))
  (let* ((buf (generate-new-buffer "pr4"))
         (c1 (config :key "width" :value 100 :type "int"))
         (c2 (config :key "name" :value "test" :type "string"))
         (c3 (config :key "enabled" :value t :type "bool"))
         (plist (list :width (cf-value c1) :name (cf-value c2) :enabled (cf-value c3))))
    (with-current-buffer buf
      (insert "W-N-E-CONFIG")
      (put-text-property 1 2 'cfg c1)
      (put-text-property 3 4 'cfg c2)
      (put-text-property 5 6 'cfg c3)
      (setq-local my-configs (list c1 c2 c3))
      (let* ((ov (make-overlay 1 6))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 3))
             (results nil))
        (undo-boundary)
        (save-restriction
          (narrow-to-region 3 10)
          (let* ((narrow-plist (list :narrow t :buf (buffer-string)))
                 (serialized (prin1-to-string narrow-plist))
                 (deserialized (car (read-from-string serialized))))
            (push (list 'narrow-plist narrow-plist deserialized (equal narrow-plist deserialized)) results)))
        (let* ((serialized (prin1-to-string plist))
               (deserialized (car (read-from-string serialized)))
               (roundtrip-ok (equal plist deserialized)))
          (push (list 'full-plist plist deserialized roundtrip-ok) results)
          (setq results (reverse results))
          (goto-char (point-max))
          (insert (format " | results=%s m=%d" results (marker-position m)))
          (set-marker m 2)
          (put-text-property (1- (point-max)) (point-max) 'plist-log t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs
                  (marker-position m)
                  (buffer-string)
                  my-configs))))
    (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_prin1_string_escape_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass text-fragment ()
    ((content :initarg :content :accessor tf-content :initform "")
     (escaped :initarg :escaped :accessor tf-escaped :initform "")))
  (let* ((buf (generate-new-buffer "pr5"))
         (t1 (text-fragment :content "hello world" :escaped "hello world"))
         (t2 (text-fragment :content "tab\there" :escaped "tab\\there"))
         (t3 (text-fragment :content "quote\"test" :escaped "quote\\\"test"))
         (strings (list (tf-content t1) (tf-content t2) (tf-content t3))))
    (with-current-buffer buf
      (insert "AAA-BBB-CCC")
      (put-text-property 1 4 'frag t1)
      (put-text-property 5 8 'frag t2)
      (put-text-property 9 12 'frag t3)
      (setq-local my-frags (list t1 t2 t3))
      (let* ((ov (make-overlay 1 8))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (dolist (s strings)
          (let* ((serialized (prin1-to-string s))
                 (deserialized (car (read-from-string serialized)))
                 (ok (equal s deserialized)))
            (push (list s serialized deserialized ok) results)))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'escape-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-frags)))
    (kill-buffer buf))))"#,
        expect,
    );
}
