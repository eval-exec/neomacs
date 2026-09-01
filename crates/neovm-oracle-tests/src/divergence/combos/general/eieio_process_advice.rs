//! Divergence tests: EIEIO + advice + closure + buffer-local + overlay combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_eieio_advice_text_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-widget nil\n\
         ((name :initarg :name :accessor widget-name)\n\
         (content :initarg :content :accessor widget-content)))\n\
         (defun widget-insert-content (w buf)\n\
         (with-current-buffer buf\n\
         (insert (widget-content w))))\n\
         (advice-add 'widget-insert-content :around\n\
         (lambda (fn w buf)\n\
         (with-current-buffer buf\n\
         (insert \"[\")\n\
         (funcall fn w buf)\n\
         (insert \"]\"))))\n\
         (let ((buf (generate-new-buffer \"wgt\"))\n\
         (w (test-widget :name \"btn\" :content \"CLICK\")))\n\
         (widget-insert-content w buf)\n\
         (with-current-buffer buf\n\
         (let ((s (buffer-string)))\n\
         (kill-buffer buf)\n\
         (list s (length s) (string= s \"[CLICK]\")))))",
        expect,
    );
}

#[test]
fn deficiency_closure_buflocal_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar my-counter 0)\n\
         (make-variable-buffer-local 'my-counter)\n\
         (let ((buf (generate-new-buffer \"clos\"))\n\
         (adder (lambda (n) (+ my-counter n))))\n\
         (with-current-buffer buf\n\
         (setq my-counter 5)\n\
         (insert \"AAAA\")\n\
         (undo-boundary)\n\
         (insert \"BBBB\")\n\
         (undo-boundary)\n\
         (let ((before-undo (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((after-undo (buffer-string))\n\
         (res (funcall adder 10)))\n\
         (kill-buffer buf)\n\
         (list before-undo after-undo res)))))",
        expect,
    );
}

#[test]
fn deficiency_overlay_narrow_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ovn\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (let ((ov1 (make-overlay 2 5))\n\
         (ov2 (make-overlay 6 9)))\n\
         (overlay-put ov1 'face 'bold)\n\
         (overlay-put ov2 'face 'italic)\n\
         (undo-boundary)\n\
         (narrow-to-region 3 8)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"XXX\")\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((s (buffer-string))\n\
         (p1 (and (overlay-start ov1) t))\n\
         (p2 (and (overlay-start ov2) t)))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s p1 p2\n\
         (buffer-string)\n\
         (= (point-min) 1)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_hash_closure_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function hash-table-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((tbl (make-hash-table :test 'equal))\n\
         (count 0))\n\
         (dolist (k '(\"alpha\" \"beta\" \"gamma\" \"delta\" \"alpha\" \"beta\" \"alpha\"))\n\
         (let ((old (gethash k tbl 0)))\n\
         (puthash k (1+ old) tbl)\n\
         (setq count (1+ count))))\n\
         (let ((keys (sort (hash-table-keys tbl) #'string<))\n\
         (vals (mapcar (lambda (k) (gethash k tbl)) (sort (hash-table-keys tbl) #'string<))))\n\
         (list keys vals count (= count 7)\n\
         (= (gethash \"alpha\" tbl) 3)\n\
         (= (gethash \"beta\" tbl) 2)\n\
         (= (gethash \"gamma\" tbl) 1)))))",
        expect,
    );
}

#[test]
fn deficiency_marker_ring_narrow_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mrk\")))\n\
         (with-current-buffer buf\n\
         (insert \"The quick brown fox jumps over the lazy dog\")\n\
         (let ((m1 (point-marker))\n\
         (m2 (copy-marker 10))\n\
         (m3 (copy-marker 30)))\n\
         (set-marker-insertion-type m2 t)\n\
         (undo-boundary)\n\
         (narrow-to-region 5 40)\n\
         (goto-char (point-min))\n\
         (re-search-forward \"brown\" nil t)\n\
         (replace-match \"RED\")\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (re-search-forward \"lazy\" nil t)\n\
         (replace-match \"SLOW\")\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((result (list (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (marker-position m3))))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list result\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (marker-position m3))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_defstruct_propertize_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (require 'cl-lib)\n\
         (cl-defstruct (point3d (:constructor mk-pt3)) x y z)\n\
         (let ((buf (generate-new-buffer \"pt3\"))\n\
         (p (mk-pt3 :x 1 :y 2 :z 3)))\n\
         (with-current-buffer buf\n\
         (insert \"NNNNNN\")\n\
         (put-text-property 1 6 'point (point3d-x p))\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (insert \"MMMM\")\n\
         (put-text-property 3 7 'point (point3d-y p))\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string))\n\
         (p1 (get-text-property 1 'point))\n\
         (p3 (get-text-property 3 'point))\n\
         (p7 (get-text-property 7 'point)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s1 p1 p3 p7\n\
         (buffer-string)\n\
         (get-text-property 1 'point)\n\
         (get-text-property 3 'point)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_advice_buffer_swap_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun my-insert-hello ()\n\
         (insert \"HELLO\"))\n\
         (advice-add 'my-insert-hello :before\n\
         (lambda () (insert \"[BEFORE]\")))\n\
         (advice-add 'my-insert-hello :after\n\
         (lambda () (insert \"[AFTER]\")))\n\
         (let ((buf (generate-new-buffer \"adv\")))\n\
         (with-current-buffer buf\n\
         (undo-boundary)\n\
         (my-insert-hello)\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s1\n\
         (buffer-string)\n\
         (= (buffer-size) 0)))))\n\
         (advice-remove 'my-insert-hello (lambda () (insert \"[BEFORE]\")))\n\
         (advice-remove 'my-insert-hello (lambda () (insert \"[AFTER]\")))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_method_hash_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (27 t 1 2 15 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-node nil\n\
         ((value :initarg :value :accessor node-value)\n\
         (children :initarg :children :initform nil :accessor node-children)))\n\
         (cl-defgeneric node-sum (n)\n\
         \"Sum all values in tree.\")\n\
         (cl-defmethod node-sum ((n test-node))\n\
         (+ (node-value n)\n\
         (cl-reduce #'+ (mapcar #'node-sum (node-children n)) :initial-value 0)))\n\
         (let* ((leaf1 (test-node :value 3))\n\
         (leaf2 (test-node :value 7))\n\
         (leaf3 (test-node :value 11))\n\
         (mid (test-node :value 5 :children (list leaf1 leaf2)))\n\
         (root (test-node :value 1 :children (list mid leaf3))))\n\
         (list (node-sum root)\n\
         (= (node-sum root) 27)\n\
         (node-value root)\n\
         (length (node-children root))\n\
         (node-sum mid)\n\
         (= (node-sum mid) 15))))",
        expect,
    );
}

#[test]
fn deficiency_multibyte_overlay_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mbo\")))\n\
         (with-current-buffer buf\n\
         (insert \"\\u00e9\\u00e8\\u00ea\\u00eb Hello \\u4e16\\u754c\")\n\
         (let ((ov1 (make-overlay 1 5))\n\
         (ov2 (make-overlay 6 12)))\n\
         (overlay-put ov1 'priority 10)\n\
         (overlay-put ov2 'priority 20)\n\
         (overlay-put ov1 'face 'underline)\n\
         (overlay-put ov2 'face 'bold)\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"INSERTED\")\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string))\n\
         (o1-start (overlay-start ov1))\n\
         (o2-start (overlay-start ov2))\n\
         (o2-end (overlay-end ov2)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s1 o1-start o2-start o2-end\n\
         (buffer-string)\n\
         (overlay-start ov1)\n\
         (overlay-start ov2)\n\
         (overlay-end ov2))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_hash_marker_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (require 'cl-lib)\n\
         (let ((buf (generate-new-buffer \"clh\"))\n\
         (tbl (make-hash-table :test 'equal)))\n\
         (with-current-buffer buf\n\
         (dotimes (i 5)\n\
         (insert (format \"line%d\\n\" i)))\n\
         (let ((markers (cl-loop for i from 1 to 20 by 4\n\
         collect (copy-marker i))))\n\
         (cl-loop for m in markers\n\
         for i from 0\n\
         do (puthash (format \"m%d\" i) (marker-position m) tbl))\n\
         (goto-char 1)\n\
         (insert \"PREFIX\")\n\
         (let ((positions (cl-loop for i from 0 below 5\n\
         collect (gethash (format \"m%d\" i) tbl))))\n\
         (list positions\n\
         (mapcar #'marker-position markers)\n\
         (buffer-substring 1 10))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
