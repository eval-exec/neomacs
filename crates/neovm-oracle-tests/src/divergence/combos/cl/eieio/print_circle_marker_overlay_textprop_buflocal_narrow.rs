//! Combo: cl-eieio print/read circle + shared references + marker + overlay + textprop + buflocal + undo.
//! Tests prin1/prin1-to-string with circular object references, shared object identity, with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_shared_object_identity_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 19 26)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass linked-node ()
    ((value :initarg :value :accessor ln-value :initform 0)
     (next :initarg :next :accessor ln-next :initform nil)))
  (let* ((buf (generate-new-buffer "pr1"))
         (n1 (linked-node :value 1))
         (n2 (linked-node :value 2))
         (n3 (linked-node :value 3)))
    (setf (ln-next n1) n2)
    (setf (ln-next n2) n3)
    (setf (ln-next n3) n1)
    (with-current-buffer buf
      (insert "NODES:1->2->3->circle")
      (put-text-property 1 6 'field 'header)
      (put-text-property 7 8 'field 'n1)
      (put-text-property 9 11 'field 'link)
      (put-text-property 12 13 'field 'n2)
      (put-text-property 14 16 'field 'link)
      (put-text-property 17 18 'field 'n3)
      (put-text-property 19 26 'field 'circle)
      (setq-local n1-obj n1)
      (setq-local n2-obj n2)
      (setq-local n3-obj n3)
      (let* ((ov (make-overlay 7 18))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 9))
             (identity-tests (list (eq n1 (ln-next n3))
                                   (eq n2 (ln-next n1))
                                   (eq n3 (ln-next n2))))
             (v1 (ln-value n1))
             (v2 (ln-value n2))
             (v3 (ln-value n3)))
        (undo-boundary)
        (setf (ln-value n1) 10
              (ln-value n2) 20
              (ln-value n3) 30)
        (let ((v1a (ln-value (ln-next (ln-next (ln-next n1)))))
              (v2a (ln-value n1))
              (print-result (let ((print-circle t)
                                  (print-length 10))
                              (prin1-to-string (list v1 v2 v3 v1a identity-tests)))))
          (goto-char (point-max))
          (insert (format " | vals=%s" print-result))
          (setf (marker-position m) 12)
          (put-text-property (1- (point-max)) (point-max) 'print-data t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (val1 (ln-value n1-obj))
              (val2 (ln-value n2-obj))
              (val3 (ln-value n3-obj))
              (still-circular (eq n1-obj (ln-next (ln-next (ln-next n1-obj))))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs val1 val2 val3 still-circular identity-tests
                (marker-position m)
                (buffer-string)
                n1-obj n2-obj n3-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_shared_list_object_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 12 26)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ref-holder ()
    ((tag :initarg :tag :accessor rh-tag :initform "")
     (data :initarg :data :accessor rh-data :initform nil)))
  (let* ((buf (generate-new-buffer "pr2"))
         (shared-list (list 1 2 3))
         (h1 (ref-holder :tag "a" :data shared-list))
         (h2 (ref-holder :tag "b" :data shared-list))
         (h3 (ref-holder :tag "c" :data (list 4 5 6))))
    (with-current-buffer buf
      (insert "REFS:a,b,c:shared+unique")
      (put-text-property 1 5 'field 'header)
      (put-text-property 6 11 'field 'tags)
      (put-text-property 12 26 'field 'desc)
      (setq-local holders (list h1 h2 h3))
      (setq-local shared-ref shared-list)
      (let* ((ov (make-overlay 6 11))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (same-data (eq (rh-data h1) (rh-data h2)))
             (diff-data (not (eq (rh-data h1) (rh-data h3)))))
        (undo-boundary)
        (push 0 (rh-data h1))
        (let ((h2-data (rh-data h2))
              (h1-data (rh-data h1))
              (still-shared (eq (rh-data h1) (rh-data h2)))
              (printed (let ((print-circle t))
                         (prin1-to-string (list (rh-data h1) (rh-data h2) (rh-data h3))))))
          (goto-char 12)
          (insert (format "shared=%s:%s[%s]"
                         still-shared h2-data
                         (substring printed 0 (min 80 (length printed)))))
          (setf (marker-position m) 15)
          (put-text-property 12 (+ 12 (length (format "shared=%s:%s[%s]"
                                                        still-shared h2-data
                                                        (substring printed 0 (min 80 (length printed))))))
                            'ref-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (d1 (rh-data (car holders)))
              (d2 (rh-data (cadr holders)))
              (d3 (rh-data (caddr holders)))
              (shared-still (eq d1 d2))
              (shared-val shared-ref))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs d1 d2 d3 shared-still shared-val
                (marker-position m)
                (buffer-string)
                holders)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_print_gensym_object_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass keyed-item ()
    ((key :initarg :key :accessor ki-key :initform nil)
     (name :initarg :name :accessor ki-name :initform "")))
  (let* ((buf (generate-new-buffer "pr3"))
         (g1 (gensym "k"))
         (g2 (gensym "k"))
         (i1 (keyed-item :key g1 :name "alpha"))
         (i2 (keyed-item :key g2 :name "beta"))
         (ht (make-hash-table :test 'eq)))
    (puthash g1 i1 ht)
    (puthash g2 i2 ht)
    (with-current-buffer buf
      (insert "KEYED:alpha:beta")
      (put-text-property 1 6 'field 'header)
      (put-text-property 7 12 'field 'n1)
      (put-text-property 13 17 'field 'n2)
      (setq-local items (list i1 i2))
      (setq-local item-hash ht)
      (setq-local keys (list g1 g2))
      (let* ((ov (make-overlay 7 17))
             (_ (overlay-put ov 'face 'underline))
             (m (make-marker))
             (_ (set-marker m 9))
             (lookup1 (gethash g1 ht))
             (lookup2 (gethash g2 ht))
             (same-i1 (eq lookup1 i1))
             (same-i2 (eq lookup2 i2)))
        (undo-boundary)
        (setf (ki-name i1) "alpha-v2"
              (ki-name i2) "beta-v2")
        (let ((re-lookup1 (ki-name (gethash g1 item-hash)))
              (re-lookup2 (ki-name (gethash g2 item-hash)))
              (printed (let ((print-circle t)
                             (print-gensym t))
                         (prin1-to-string (list g1 g2 (ki-key i1))))))
          (goto-char 7)
          (insert (format "%s,%s[%s|%s]"
                         re-lookup1 re-lookup2
                         same-i1 same-i2))
          (setf (marker-position m) 12)
          (put-text-property 7 (+ 7 (length (format "%s,%s[%s|%s]"
                                                      re-lookup1 re-lookup2
                                                      same-i1 same-i2)))
                            'keyed-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (n1 (ki-name (car items)))
              (n2 (ki-name (cadr items)))
              (k1-still (gethash (car keys) item-hash))
              (k2-still (gethash (cadr keys) item-hash)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs n1 n2
                (and k1-still (ki-name k1-still))
                (and k2-still (ki-name k2-still))
                (marker-position m)
                (buffer-string)
                items)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_tree_structure_print_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 11 31)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tree-node ()
    ((val :initarg :val :accessor tn-val :initform "")
     (children :initarg :children :accessor tn-children :initform nil)))
  (let* ((buf (generate-new-buffer "pr4"))
         (leaf1 (tree-node :val "a"))
         (leaf2 (tree-node :val "b"))
         (leaf3 (tree-node :val "c"))
         (leaf4 (tree-node :val "d"))
         (mid1 (tree-node :val "m1" :children (list leaf1 leaf2)))
         (mid2 (tree-node :val "m2" :children (list leaf3 leaf4)))
         (root (tree-node :val "root" :children (list mid1 mid2))))
    (with-current-buffer buf
      (insert "TREE:root(m1(a,b),m2(c,d))")
      (put-text-property 1 5 'field 'type)
      (put-text-property 6 10 'field 'root)
      (put-text-property 11 31 'field 'structure)
      (setq-local root-node root)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (leaf-count (let ((count 0))
                           (cl-labels ((count-leaves (n)
                                        (if (tn-children n)
                                            (dolist (c (tn-children n))
                                              (count-leaves c))
                                          (setq count (1+ count)))))
                             (count-leaves root)
                             count)))
             (all-vals (let ((vals nil))
                         (cl-labels ((collect (n)
                                      (push (tn-val n) vals)
                                      (dolist (c (tn-children n))
                                        (collect c))))
                           (collect root)
                           (reverse vals)))))
        (undo-boundary)
        (setf (tn-val leaf1) "A"
              (tn-val leaf2) "B"
              (tn-val mid1) "M1-upd")
        (push (tree-node :val "e") (tn-children mid2))
        (let ((new-vals (let ((vals nil))
                          (cl-labels ((collect (n)
                                       (push (tn-val n) vals)
                                       (dolist (c (tn-children n))
                                         (collect c))))
                            (collect root)
                            (reverse vals))))
              (new-count (let ((count 0))
                           (cl-labels ((count-leaves (n)
                                        (if (tn-children n)
                                            (dolist (c (tn-children n))
                                              (count-leaves c))
                                          (setq count (1+ count)))))
                             (count-leaves root)
                             count))))
          (goto-char 11)
          (insert (format "%s->%s:count=%d->%d"
                         all-vals new-vals leaf-count new-count))
          (setf (marker-position m) 15)
          (put-text-property 11 (+ 11 (length (format "%s->%s:count=%d->%d"
                                                        all-vals new-vals leaf-count new-count)))
                            'tree-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (root-val (tn-val root-node))
              (mid1-children (length (tn-children (car (tn-children root-node)))))
              (mid2-children (length (tn-children (cadr (tn-children root-node))))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs root-val mid1-children mid2-children
                (marker-position m)
                (buffer-string)
                root-node)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_doubly_linked_with_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 21 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dlink ()
    ((payload :initarg :payload :accessor dl-payload :initform nil)
     (prev :initarg :prev :accessor dl-prev :initform nil)
     (next :initarg :next :accessor dl-next :initform nil)))
  (let* ((buf (generate-new-buffer "pr5"))
         (a (dlink :payload "A"))
         (b (dlink :payload "B"))
         (c (dlink :payload "C")))
    (setf (dl-next a) b (dl-prev b) a)
    (setf (dl-next b) c (dl-prev c) b)
    (setf (dl-next c) a (dl-prev a) c)
    (with-current-buffer buf
      (insert "DLINK:A<->B<->C<->A")
      (put-text-property 1 6 'field 'header)
      (put-text-property 7 8 'field 'a)
      (put-text-property 9 12 'field 'link)
      (put-text-property 13 14 'field 'b)
      (put-text-property 15 18 'field 'link)
      (put-text-property 19 20 'field 'c)
      (put-text-property 21 24 'field 'link)
      (setq-local dlist (list a b c))
      (let* ((ov (make-overlay 7 20))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 9))
             (cycle-verified (eq a (dl-next (dl-next (dl-next a)))))
             (backward-verified (eq c (dl-prev (dl-prev (dl-prev c))))))
        (undo-boundary)
        (setf (dl-payload a) "AA"
              (dl-payload b) "BB"
              (dl-payload c) "CC")
        (let ((forward (let ((cur a) (result nil) (count 0))
                         (while (and cur (< count 6))
                           (push (dl-payload cur) result)
                           (setq cur (dl-next cur)
                                 count (1+ count)))
                         (reverse result)))
              (backward (let ((cur c) (result nil) (count 0))
                          (while (and cur (< count 6))
                            (push (dl-payload cur) result)
                            (setq cur (dl-prev cur)
                                  count (1+ count)))
                          (reverse result)))))
          (goto-char 7)
          (insert (format "fwd=%s:bwd=%s" forward backward))
          (setf (marker-position m) 10)
          (put-text-property 7 (+ 7 (length (format "fwd=%s:bwd=%s" forward backward)))
                            'dlink-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (pa (dl-payload a))
              (pb (dl-payload b))
              (pc (dl-payload c)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs pa pb pc cycle-verified backward-verified
                (marker-position m)
                (buffer-string)
                dlist)))
      (kill-buffer buf))))"#,
        expect,
    );
}
