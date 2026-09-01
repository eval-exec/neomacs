//! Combo: cl-eieio plist/alist manipulation + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests complex plist/alist operations with EIEIO objects as keys/values stored in text properties.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_plist_from_text_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function plist-delete)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass attr ()
    ((key :initarg :key :accessor at-key :initform "")
     (val :initarg :val :accessor at-val :initform nil)))
  (let* ((buf (generate-new-buffer "pl1"))
         (a1 (attr :key "color" :val "red"))
         (a2 (attr :key "size" :val 10))
         (a3 (attr :key "weight" :val 5)))
    (with-current-buffer buf
      (insert "XXXX-YYYY-ZZZZ")
      (put-text-property 1 5 'attr a1)
      (put-text-property 6 9 'attr a2)
      (put-text-property 10 13 'attr a3)
      (setq-local my-attrs (list a1 a2 a3))
      (let* ((ov (make-overlay 1 9))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (plist nil)
             (results nil))
        (undo-boundary)
        (let ((pos 1))
          (while (< pos (point-max))
            (let ((val (get-text-property pos 'attr)))
              (when val
                (setq plist (plist-put plist (intern (at-key val)) (at-val val))))
              (setq pos (or (next-single-property-change pos 'attr (current-buffer) (point-max))
                            (point-max))))))
        (push (list 'plist plist) results)
        (push (list 'color (plist-get plist 'color)) results)
        (push (list 'size (plist-get plist 'size)) results)
        (push (list 'weight (plist-get plist 'weight)) results)
        (setq plist (plist-put plist 'extra 42))
        (push (list 'after-put (plist-get plist 'extra)) results)
        (setq plist (plist-delete plist 'size))
        (push (list 'after-delete (plist-get plist 'size)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 3)
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
                my-attrs))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_alist_from_overlay_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function remove-if-not)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass overlay-attr ()
    ((prop :initarg :prop :accessor oa-prop :initform "")
     (value :initarg :value :accessor oa-value :initform nil)))
  (let* ((buf (generate-new-buffer "pl2"))
         (oa1 (overlay-attr :prop "face" :value 'bold))
         (oa2 (overlay-attr :prop "priority" :value 5))
         (oa3 (overlay-attr :prop "invisible" :value nil)))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAA")
      (setq-local my-oas (list oa1 oa2 oa3))
      (let* ((ov (make-overlay 1 13))
             (_ (overlay-put ov 'priority (oa-value oa2)))
             (_ (overlay-put ov 'face (oa-value oa1)))
             (_ (overlay-put ov 'invisible (oa-value oa3)))
             (m (make-marker))
             (_ (set-marker m 1))
             (alist nil)
             (results nil))
        (undo-boundary)
        (push (cons "priority" (overlay-get ov 'priority)) alist)
        (push (cons "face" (overlay-get ov 'face)) alist)
        (push (cons "invisible" (overlay-get ov 'invisible)) alist)
        (setq alist (reverse alist))
        (push (list 'alist alist) results)
        (let ((face-entry (assoc "face" alist)))
          (push (list 'face-found (cdr face-entry)) results))
        (setq alist (cons (cons "extra" 99) alist))
        (push (list 'after-cons (assoc "extra" alist)) results)
        (let ((filtered (remove-if-not (lambda (x) (cdr x)) alist)))
          (push (list 'filtered (length filtered)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'alist-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-oas))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_plist_narrow_overlay_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable text-plist)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass merged-prop ()
    ((source :initarg :source :accessor mp-source :initform "")
     (plist-key :initarg :plist-key :accessor mp-key :initform nil)
     (plist-val :initarg :plist-val :accessor mp-val :initform nil)))
  (let* ((buf (generate-new-buffer "pl3"))
         (mp1 (merged-prop :source "text" :plist-key 'a :plist-val 1))
         (mp2 (merged-prop :source "text" :plist-key 'b :plist-val 2))
         (mp3 (merged-prop :source "overlay" :plist-key 'c :plist-val 3)))
    (with-current-buffer buf
      (insert "ABAB-CCCC-ABAB")
      (put-text-property 1 5 'mp mp1)
      (put-text-property 6 9 'mp mp2)
      (put-text-property 10 13 'mp mp1)
      (setq-local my-mps (list mp1 mp2 mp3))
      (let* ((ov (make-overlay 6 9))
             (_ (overlay-put ov 'mp mp3))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (let ((merged-plist nil)
              (pos 1))
          (while (< pos (point-max))
            (let ((val (get-char-property pos 'mp)))
              (when val
                (setq merged-plist (plist-put merged-plist (mp-key val) (mp-val val))))
              (setq pos (or (next-char-property-change pos (point-max))
                            (point-max)))))
          (push (list 'char-props merged-plist) results))
        (let ((text-plist nil)
              (pos 1))
          (while (< pos (point-max))
            (let ((val (get-text-property pos 'mp)))
              (when val
                (setq text-plist (plist-put text-plist (mp-key val) (mp-val val))))
              (setq pos (or (next-single-property-change pos 'mp (current-buffer) (point-max))
                            (point-max))))))
          (push (list 'text-props text-plist) results))
        (save-restriction
          (narrow-to-region 6 9)
          (let ((narrow-plist nil)
                (pos (point-min)))
            (while (< pos (point-max))
              (let ((val (get-char-property pos 'mp)))
                (when val
                  (setq narrow-plist (plist-put narrow-plist (mp-key val) (mp-val val))))
                (setq pos (or (next-char-property-change pos (point-max))
                              (point-max))))))
            (push (list 'narrow-props narrow-plist) results)))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'merge-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-mps))))
    (kill-buffer buf)"#,
        expect,
    );
}

#[test]
fn combo_eieio_plist_object_key_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable results)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass key-obj ()
    ((name :initarg :name :accessor ko-name :initform "")))
  (let* ((buf (generate-new-buffer "pl4"))
         (k1 (key-obj :name "alpha"))
         (k2 (key-obj :name "beta"))
         (ht (make-hash-table :test 'eq)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'key k1)
      (put-text-property 6 9 'key k2)
      (setq-local my-keys (list k1 k2))
      (let* ((ov (make-overlay 1 9))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (puthash k1 100 ht)
        (puthash k2 200 ht)
        (let ((pos 1))
          (while (< pos (point-max))
            (let ((key (get-text-property pos 'key)))
              (when key
                (let ((existing (gethash key ht)))
                  (push (list pos (ko-name key) existing (1+ (or existing 0))) results)
                  (puthash key (1+ (or existing 0)) ht))))
              (setq pos (or (next-single-property-change pos 'key (current-buffer) (point-max))
                            (point-max))))))
        (setq results (reverse results))
        (push (list 'hash-alpha (gethash k1 ht)) results)
        (push (list 'hash-beta (gethash k2 ht)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'key-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-keys))))
    (kill-buffer buf))"#,
        expect,
    );
}

#[test]
fn combo_eieio_alist_sort_by_object_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass scored-item ()
    ((name :initarg :name :accessor si-name :initform "")
     (score :initarg :score :accessor si-score :initform 0)))
  (let* ((buf (generate-new-buffer "pl5"))
         (si1 (scored-item :name "delta" :score 4))
         (si2 (scored-item :name "alpha" :score 1))
         (si3 (scored-item :name "charlie" :score 3))
         (si4 (scored-item :name "bravo" :score 2)))
    (with-current-buffer buf
      (insert "D-A-C-B")
      (put-text-property 1 2 'si si1)
      (put-text-property 3 4 'si si2)
      (put-text-property 5 6 'si si3)
      (put-text-property 7 8 'si si4)
      (setq-local my-items (list si1 si2 si3 si4))
      (let* ((ov (make-overlay 1 8))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 3))
             (alist nil)
             (results nil))
        (undo-boundary)
        (let ((pos 1))
          (while (< pos (point-max))
            (let ((val (get-text-property pos 'si)))
              (when val
                (push (cons (si-name val) (si-score val)) alist))
              (setq pos (or (next-single-property-change pos 'si (current-buffer) (point-max))
                            (point-max))))))
        (setq alist (reverse alist))
        (push (list 'original alist) results)
        (let ((sorted (sort (copy-alist alist)
                           (lambda (a b) (< (cdr a) (cdr b))))))
          (push (list 'sorted sorted) results))
        (let ((assq-result (assoc "charlie" alist)))
          (push (list 'assq-charlie assq-result) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 2)
        (put-text-property (1- (point-max)) (point-max) 'score-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-items))))
    (kill-buffer buf)))"#,
        expect,
    );
}
