//! Deep stress: EIEIO inheritance + multiple dispatch + undo + text props + closures.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_eieio_multilevel_inherit_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (s buf) (progn (save-current-buffer (set-buffer buf) (insert (format \"Circle r=%.1f area=%.2f\" (circle-radius s) (area s)))))) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass shape nil\n\
         ((name :initarg :name :accessor shape-name)))\n\
         (defclass circle (shape)\n\
         ((radius :initarg :radius :accessor circle-radius)))\n\
         (defclass rectangle (shape)\n\
         ((width :initarg :width :accessor rect-width)\n\
         (height :initarg :height :accessor rect-height)))\n\
         (defclass colored-circle (circle)\n\
         ((color :initarg :color :accessor shape-color)))\n\
         (cl-defgeneric area (s)\n\
         \"Compute area.\")\n\
         (cl-defmethod area ((s circle))\n\
         (* float-pi (expt (circle-radius s) 2)))\n\
         (cl-defmethod area ((s rectangle))\n\
         (* (rect-width s) (rect-height s)))\n\
         (cl-defgeneric describe-shape (s buf)\n\
         \"Insert description into buffer.\")\n\
         (cl-defmethod describe-shape ((s circle) buf)\n\
         (with-current-buffer buf\n\
         (insert (format \"Circle r=%.1f area=%.2f\"\n\
         (circle-radius s) (area s)))))\n\
         (cl-defmethod describe-shape ((s colored-circle) buf)\n\
         (with-current-buffer buf\n\
         (insert (format \"%s color=%s\"\n\
         (progn\n\
         (let ((start (point)))\n\
         (cl-call-next-method buf)\n\
         (buffer-substring start (point))))\n\
         (shape-color s)))))\n\
         (cl-defmethod describe-shape ((s rectangle) buf)\n\
         (with-current-buffer buf\n\
         (insert (format \"Rect %dx%d area=%d\"\n\
         (rect-width s) (rect-height s) (area s)))))\n\
         (let* ((buf (generate-new-buffer \"eim\"))\n\
         (shapes (list (colored-circle :name \"c1\" :radius 5 :color 'red)\n\
         (rectangle :name \"r1\" :width 3 :height 4)\n\
         (circle :name \"c2\" :radius 10))))\n\
         (with-current-buffer buf\n\
         (dolist (s shapes)\n\
         (describe-shape s buf)\n\
         (insert \"\\n\"))\n\
         (put-text-property 1 30 'type 'shapes)\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (insert \"[MODIFIED]\")\n\
         (undo-boundary)\n\
         (let ((snap (buffer-string))\n\
         (tp (get-text-property 1 'type)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list snap tp\n\
         (buffer-string)\n\
         (get-text-property 1 'type)\n\
         (= (hash-table-count\n\
         (let ((h (make-hash-table)))\n\
         (dolist (s shapes)\n\
         (puthash (shape-name s) (area s) h))\n\
         h))\n\
         3))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_closure_capture_buflocal_undo_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar my-global-val 0)\n\
         (let ((buf (generate-new-buffer \"cbi\"))\n\
         (accum nil)\n\
         (fns nil))\n\
         (with-current-buffer buf\n\
         (make-variable-buffer-local 'my-global-val)\n\
         (setq my-global-val 100)\n\
         (insert \"ABCDE\")\n\
         (undo-boundary)\n\
         (dotimes (i 5)\n\
         (let ((captured-val my-global-val))\n\
         (push (lambda ()\n\
         (setq accum (cons (list i captured-val (buffer-string)) accum)))\n\
         fns)))\n\
         (setq my-global-val 200)\n\
         (goto-char 3)\n\
         (insert \"XXX\")\n\
         (undo-boundary)\n\
         (dolist (fn fns)\n\
         (funcall fn))\n\
         (let ((results (nreverse accum)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list results\n\
         (buffer-string)\n\
         my-global-val\n\
         (length results)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_evaporation_after_kill_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"oek\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAABBBBBBBBBB\")\n\
         (let ((ov1 (make-overlay 1 10))\n\
         (ov2 (make-overlay 11 20))\n\
         (ov3 (make-overlay 5 15)))\n\
         (overlay-put ov1 'face 'bold)\n\
         (overlay-put ov2 'face 'italic)\n\
         (overlay-put ov3 'face 'underline)\n\
         (overlay-put ov1 'evaporate t)\n\
         (overlay-put ov2 'evaporate t)\n\
         (overlay-put ov3 'evaporate t)\n\
         (undo-boundary)\n\
         (delete-region 1 10)\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string))\n\
         (o1-live (overlay-start ov1))\n\
         (o2-live (overlay-start ov2))\n\
         (o3-live (overlay-start ov3)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s1 o1-live o2-live o3-live\n\
         (buffer-string)\n\
         (and (overlay-start ov1) t)\n\
         (and (overlay-start ov2) t)\n\
         (and (overlay-start ov3) t)\n\
         (overlay-get ov1 'face)\n\
         (overlay-get ov2 'face)\n\
         (overlay-get ov3 'face))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_text_property_rear_nonsticky_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"tpn\")))\n\
         (with-current-buffer buf\n\
         (insert \"LLRRRRRRRRRR\")\n\
         (put-text-property 1 3 'face 'bold)\n\
         (put-text-property 3 12 'face 'default)\n\
         (let ((p1 (text-property-any 1 12 'face 'bold))\n\
         (p2 (text-property-not-all 1 12 'face 'bold)))\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (insert \"MMMM\")\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string))\n\
         (f-at-3 (get-text-property 3 'face))\n\
         (f-at-7 (get-text-property 7 'face)))\n\
         (put-text-property 3 7 'face 'italic)\n\
         (undo-boundary)\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list p1 p2 s1 f-at-3 f-at-7\n\
         (buffer-string)\n\
         (get-text-property 1 'face)\n\
         (get-text-property 3 'face)\n\
         (get-text-property 7 'face))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_marker_insertion_type_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mit\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (let ((m-before (copy-marker 5))\n\
         (m-after (copy-marker 5))\n\
         (m-stay (copy-marker 5)))\n\
         (set-marker-insertion-type m-before nil)\n\
         (set-marker-insertion-type m-after t)\n\
         (set-marker-insertion-type m-stay nil)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"111\")\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"222\")\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"333\")\n\
         (undo-boundary)\n\
         (let ((snap\n\
         (list (buffer-string)\n\
         (marker-position m-before)\n\
         (marker-position m-after)\n\
         (marker-position m-stay)\n\
         (marker-insertion-type m-before)\n\
         (marker-insertion-type m-after))))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list snap\n\
         (buffer-string)\n\
         (marker-position m-before)\n\
         (marker-position m-after)\n\
         (marker-position m-stay)\n\
         (marker-insertion-type m-before)\n\
         (marker-insertion-type m-after))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_replace_regexp_undo_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rru\")))\n\
         (with-current-buffer buf\n\
         (insert \"foo1 foo2 foo3 foo4 foo5\")\n\
         (put-text-property 1 5 'idx 1)\n\
         (put-text-property 6 10 'idx 2)\n\
         (put-text-property 11 15 'idx 3)\n\
         (put-text-property 16 20 'idx 4)\n\
         (put-text-property 21 25 'idx 5)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (while (re-search-forward \"foo\\\\([0-9]+\\\\)\" nil t)\n\
         (replace-match (concat \"bar\" (match-string 1))))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (i1 (get-text-property 1 'idx))\n\
         (i6 (get-text-property 6 'idx))\n\
         (i11 (get-text-property 11 'idx))\n\
         (i16 (get-text-property 16 'idx))\n\
         (i21 (get-text-property 21 'idx)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s i1 i6 i11 i16 i21\n\
         (buffer-string)\n\
         (get-text-property 1 'idx)\n\
         (get-text-property 6 'idx)\n\
         (get-text-property 11 'idx)\n\
         (get-text-property 16 'idx)\n\
         (get-text-property 21 'idx)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_cl_defmethod_specializer_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass editable-item nil\n\
         ((text :initarg :text :accessor item-text)))\n\
         (defclass code-item (editable-item)\n\
         ((language :initarg :language :accessor item-language)))\n\
         (cl-defmethod edit-item ((item editable-item) buf)\n\
         (with-current-buffer buf\n\
         (insert (item-text item))))\n\
         (cl-defmethod edit-item ((item code-item) buf)\n\
         (with-current-buffer buf\n\
         (insert (format \"```%s\\n%s\\n```\"\n\
         (item-language item) (item-text item)))))\n\
         (let ((buf (generate-new-buffer \"eis\"))\n\
         (items (list (editable-item :text \"Hello World\")\n\
         (code-item :text \"(+ 1 2)\" :language 'elisp)\n\
         (code-item :text \"fn main() {}\" :language 'rust)\n\
         (editable-item :text \"Goodbye\"))))\n\
         (with-current-buffer buf\n\
         (dolist (item items)\n\
         (edit-item item buf)\n\
         (insert \"\\n---\\n\"))\n\
         (put-text-property 1 11 'source 'plain)\n\
         (put-text-property 12 30 'source 'code)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"PREFIX\")\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string))\n\
         (src1 (get-text-property 1 'source))\n\
         (src7 (get-text-property 7 'source))\n\
         (src17 (get-text-property 17 'source)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s1 src1 src7 src17\n\
         (buffer-string)\n\
         (get-text-property 1 'source)\n\
         (get-text-property 7 'source)\n\
         (get-text-property 17 'source)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_buffer_substring_props_with_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer bsp> 1 10)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"bsp\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ0123456789\")\n\
         (put-text-property 1 10 'half 'alpha)\n\
         (put-text-property 10 11 'boundary 'yes)\n\
         (put-text-property 11 20 'half 'numeric)\n\
         (undo-boundary)\n\
         (narrow-to-region 5 15)\n\
         (let ((narrowed-sub (buffer-substring 1 10))\n\
         (narrowed-sub-no-props (buffer-substring-no-properties 1 10))\n\
         (p1 (get-text-property 1 'half))\n\
         (p6 (get-text-property 6 'half))\n\
         (p10 (get-text-property 10 'half)))\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((full-sub (buffer-substring 1 20))\n\
         (full-p1 (get-text-property 1 'half))\n\
         (full-p10 (get-text-property 10 'boundary))\n\
         (full-p11 (get-text-property 11 'half)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list narrowed-sub narrowed-sub-no-props p1 p6 p10\n\
         full-sub full-p1 full-p10 full-p11\n\
         (buffer-string)\n\
         (get-text-property 1 'half)\n\
         (get-text-property 10 'boundary)\n\
         (get-text-property 11 'half))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_propertize_replace_undo_intervals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"pri\")))\n\
         (with-current-buffer buf\n\
         (insert (propertize \"AAAA\" 'level 1 'tag 'first))\n\
         (insert (propertize \"BBBB\" 'level 2 'tag 'second))\n\
         (insert (propertize \"CCCC\" 'level 3 'tag 'third))\n\
         (insert (propertize \"DDDD\" 'level 4 'tag 'fourth))\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"BBBB\")\n\
         (replace-match \"XXXX\")\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"CCCC\")\n\
         (replace-match \"YYYY\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (l1 (get-text-property 1 'level))\n\
         (l5 (get-text-property 5 'level))\n\
         (l9 (get-text-property 9 'level))\n\
         (l13 (get-text-property 13 'level))\n\
         (t1 (get-text-property 1 'tag))\n\
         (t5 (get-text-property 5 'tag))\n\
         (t9 (get-text-property 9 'tag))\n\
         (t13 (get-text-property 13 'tag)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s l1 l5 l9 l13 t1 t5 t9 t13\n\
         (buffer-string)\n\
         (get-text-property 1 'level)\n\
         (get-text-property 5 'level)\n\
         (get-text-property 9 'level)\n\
         (get-text-property 13 'level)\n\
         (get-text-property 1 'tag)\n\
         (get-text-property 5 'tag)\n\
         (get-text-property 9 'tag)\n\
         (get-text-property 13 'tag)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_in_read_only_with_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uro\")))\n\
         (with-current-buffer buf\n\
         (insert \"LINE1 AAAA\\nLINE2 BBBB\\nLINE3 CCCC\\nLINE4 DDDD\")\n\
         (put-text-property 1 11 'ro t)\n\
         (undo-boundary)\n\
         (setq buffer-read-only t)\n\
         (condition-case nil\n\
         (progn\n\
         (goto-char 12)\n\
         (insert \"FAILED\"))\n\
         (buffer-read-only nil))\n\
         (setq buffer-read-only nil)\n\
         (undo-boundary)\n\
         (narrow-to-region 6 20)\n\
         (goto-char (point-min))\n\
         (insert \"MODIFIED\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (ro-prop (get-text-property 1 'ro)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (widen)\n\
         (list s ro-prop\n\
         (buffer-string)\n\
         (get-text-property 1 'ro)\n\
         (get-text-property 6 'ro)\n\
         (get-text-property 12 'ro)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
