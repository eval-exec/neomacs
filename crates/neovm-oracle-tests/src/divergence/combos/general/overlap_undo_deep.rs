//! Deep stress: overlapping overlays + text props + markers + narrow + undo chains.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_overlapping_overlays_undo_reorder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"oor\")))\n\
         (with-current-buffer buf\n\
         (insert \"0123456789ABCDEFGHIJ\")\n\
         (let ((ov-a (make-overlay 1 10))\n\
         (ov-b (make-overlay 5 15))\n\
         (ov-c (make-overlay 8 20))\n\
         (ov-d (make-overlay 3 18)))\n\
         (overlay-put ov-a 'priority 10)\n\
         (overlay-put ov-b 'priority 20)\n\
         (overlay-put ov-c 'priority 30)\n\
         (overlay-put ov-d 'priority 5)\n\
         (overlay-put ov-a 'data 'alpha)\n\
         (overlay-put ov-b 'data 'beta)\n\
         (overlay-put ov-c 'data 'gamma)\n\
         (overlay-put ov-d 'data 'delta)\n\
         (put-text-property 1 10 'zone 'numbers)\n\
         (put-text-property 10 20 'zone 'letters)\n\
         (undo-boundary)\n\
         (goto-char 7)\n\
         (insert \"INSERT1\")\n\
         (undo-boundary)\n\
         (goto-char 20)\n\
         (delete-region 20 25)\n\
         (undo-boundary)\n\
         (overlay-put ov-b 'data 'modified-beta)\n\
         (put-text-property 8 15 'zone 'mixed)\n\
         (undo-boundary)\n\
         (let ((snapshot\n\
         (list (buffer-string)\n\
         (overlay-get ov-a 'data)\n\
         (overlay-get ov-b 'data)\n\
         (overlay-get ov-c 'data)\n\
         (overlay-get ov-d 'data)\n\
         (get-text-property 1 'zone)\n\
         (get-text-property 8 'zone)\n\
         (get-text-property 15 'zone)\n\
         (overlay-start ov-a)\n\
         (overlay-start ov-b)\n\
         (overlay-start ov-c)\n\
         (overlay-start ov-d)\n\
         (overlay-end ov-a)\n\
         (overlay-end ov-b)\n\
         (overlay-end ov-c)\n\
         (overlay-end ov-d))))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list snapshot\n\
         (buffer-string)\n\
         (overlay-get ov-a 'data)\n\
         (overlay-get ov-b 'data)\n\
         (overlay-get ov-c 'data)\n\
         (overlay-get ov-d 'data)\n\
         (get-text-property 1 'zone)\n\
         (get-text-property 5 'zone)\n\
         (get-text-property 10 'zone)\n\
         (overlay-start ov-a)\n\
         (overlay-start ov-b)\n\
         (overlay-start ov-c)\n\
         (overlay-start ov-d)\n\
         (overlay-end ov-a)\n\
         (overlay-end ov-b)\n\
         (overlay-end ov-c)\n\
         (overlay-end ov-d))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_yank_textprop_preserve_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 69 78)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kyp\")))\n\
         (with-current-buffer buf\n\
         (insert \"[bold]this is bold[/bold] and this is plain [italic]this is italic[/italic]\")\n\
         (put-text-property 1 7 'face 'bold-tag)\n\
         (put-text-property 7 20 'face 'bold)\n\
         (put-text-property 20 28 'face 'bold-tag)\n\
         (put-text-property 44 53 'face 'italic-tag)\n\
         (put-text-property 53 69 'face 'italic)\n\
         (put-text-property 69 78 'face 'italic-tag)\n\
         (undo-boundary)\n\
         (kill-region 7 20)\n\
         (undo-boundary)\n\
         (goto-char 20)\n\
         (yank)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (face-at-7 (get-text-property 7 'face))\n\
         (face-at-20 (get-text-property 20 'face))\n\
         (face-at-30 (get-text-property 30 'face)))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s face-at-7 face-at-20 face-at-30\n\
         (buffer-string)\n\
         (get-text-property 7 'face)\n\
         (get-text-property 20 'face)\n\
         (get-text-property 44 'face)\n\
         (get-text-property 53 'face)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_narrow_widen_replace_prop_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"nwr\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH\")\n\
         (put-text-property 1 4 'grp 1)\n\
         (put-text-property 5 8 'grp 2)\n\
         (put-text-property 9 12 'grp 3)\n\
         (put-text-property 13 16 'grp 4)\n\
         (put-text-property 17 20 'grp 5)\n\
         (put-text-property 21 24 'grp 6)\n\
         (put-text-property 25 28 'grp 7)\n\
         (put-text-property 29 32 'grp 8)\n\
         (let ((m1 (copy-marker 5))\n\
         (m2 (copy-marker 20))\n\
         (m3 (copy-marker 28)))\n\
         (undo-boundary)\n\
         (narrow-to-region 5 20)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (while (re-search-forward \"B\" nil t)\n\
         (replace-match \"X\"))\n\
         (undo-boundary)\n\
         (put-text-property (point-min) (point-max) 'modified t)\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((s (buffer-string))\n\
         (mod-in-narrow (get-text-property 7 'modified))\n\
         (mod-outside (get-text-property 25 'modified))\n\
         (p1 (marker-position m1))\n\
         (p2 (marker-position m2))\n\
         (p3 (marker-position m3)))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list s mod-in-narrow mod-outside p1 p2 p3\n\
         (buffer-string)\n\
         (get-text-property 5 'grp)\n\
         (get-text-property 7 'grp)\n\
         (get-text-property 9 'grp)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (marker-position m3))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_struct_marker_slots_undo_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (span (:constructor make-span))\n\
         (start nil) (end nil) (label nil))\n\
         (let ((buf (generate-new-buffer \"sms\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\")\n\
         (let ((s1 (make-span :start (copy-marker 3) :end (copy-marker 8) :label 'word1))\n\
         (s2 (make-span :start (copy-marker 10) :end (copy-marker 16) :label 'word2))\n\
         (s3 (make-span :start (copy-marker 18) :end (copy-marker 24) :label 'word3)))\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"MMMM\")\n\
         (undo-boundary)\n\
         (goto-char 15)\n\
         (delete-region 15 19)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"ZZZZ\")\n\
         (undo-boundary)\n\
         (let ((snapshot\n\
         (list (buffer-string)\n\
         (list (marker-position (span-start s1))\n\
         (marker-position (span-end s1))\n\
         (span-label s1))\n\
         (list (marker-position (span-start s2))\n\
         (marker-position (span-end s2))\n\
         (span-label s2))\n\
         (list (marker-position (span-start s3))\n\
         (marker-position (span-end s3))\n\
         (span-label s3)))))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list snapshot\n\
         (buffer-string)\n\
         (list (marker-position (span-start s1))\n\
         (marker-position (span-end s1))\n\
         (span-label s1))\n\
         (list (marker-position (span-start s2))\n\
         (marker-position (span-end s2))\n\
         (span-label s2))\n\
         (list (marker-position (span-start s3))\n\
         (marker-position (span-end s3))\n\
         (span-label s3)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_composition_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function second)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass text-chunk nil\n\
         ((content :initarg :content :accessor chunk-content)\n\
         (props :initarg :props :initform nil :accessor chunk-props)))\n\
         (defclass text-document nil\n\
         ((chunks :initarg :chunks :initform nil :accessor doc-chunks)\n\
         (buffer :initarg :buffer :accessor doc-buffer)))\n\
         (defun doc-render (doc)\n\
         (with-current-buffer (doc-buffer doc)\n\
         (erase-buffer)\n\
         (dolist (c (doc-chunks doc))\n\
         (let ((start (point)))\n\
         (insert (chunk-content c))\n\
         (dolist (p (chunk-props c))\n\
         (put-text-property start (point) (car p) (cdr p)))))))\n\
         (let* ((buf (generate-new-buffer \"eic\"))\n\
         (doc (text-document :buffer buf)))\n\
         (setf (doc-chunks doc)\n\
         (list (text-chunk :content \"Hello \" :props '((face . bold)))\n\
         (text-chunk :content \"World\" :props '((face . italic)))\n\
         (text-chunk :content \"!\" :props '((face . underline)))))\n\
         (with-current-buffer buf\n\
         (doc-render doc)\n\
         (undo-boundary)\n\
         (goto-char 7)\n\
         (insert \"Beautiful \")\n\
         (undo-boundary)\n\
         (setf (chunk-content (second (doc-chunks doc))) \"Beautiful World\")\n\
         (doc-render doc)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (f1 (get-text-property 1 'face))\n\
         (f7 (get-text-property 7 'face))\n\
         (f16 (get-text-property 16 'face)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s f1 f7 f16\n\
         (buffer-string)\n\
         (get-text-property 1 'face)\n\
         (get-text-property 7 'face)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_multi_buf_markers_undo_interleave() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (generate-new-buffer \"mb1\"))\n\
         (buf2 (generate-new-buffer \"mb2\"))\n\
         (buf3 (generate-new-buffer \"mb3\")))\n\
         (with-current-buffer buf1\n\
         (insert \"AAABBBCCC\")\n\
         (let ((m1 (copy-marker 3))\n\
         (m2 (copy-marker 6)))\n\
         (undo-boundary)\n\
         (goto-char 4)\n\
         (insert \"INSERT\")\n\
         (undo-boundary)\n\
         (with-current-buffer buf2\n\
         (insert \"DDDEEEFFF\")\n\
         (let ((m3 (copy-marker 4))\n\
         (m4 (copy-marker 7)))\n\
         (undo-boundary)\n\
         (delete-region 4 7)\n\
         (undo-boundary)\n\
         (with-current-buffer buf3\n\
         (insert \"GGGHHHIII\")\n\
         (let ((m5 (copy-marker 5)))\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"JJJ\")\n\
         (undo-boundary)\n\
         (let ((snap\n\
         (list\n\
         (with-current-buffer buf1 (buffer-string))\n\
         (marker-position m1) (marker-position m2)\n\
         (with-current-buffer buf2 (buffer-string))\n\
         (marker-position m3) (marker-position m4)\n\
         (with-current-buffer buf3 (buffer-string))\n\
         (marker-position m5))))\n\
         (with-current-buffer buf1 (primitive-undo 1 buffer-undo-list))\n\
         (with-current-buffer buf2 (primitive-undo 1 buffer-undo-list))\n\
         (with-current-buffer buf3 (primitive-undo 1 buffer-undo-list))\n\
         (list snap\n\
         (with-current-buffer buf1 (buffer-string))\n\
         (marker-position m1) (marker-position m2)\n\
         (with-current-buffer buf2 (buffer-string))\n\
         (marker-position m3) (marker-position m4)\n\
         (with-current-buffer buf3 (buffer-string))\n\
         (marker-position m5)))))))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)\n\
         (kill-buffer buf3)))",
        expect,
    );
}

#[test]
fn deficiency_advice_chain_text_transform_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar my-transform-count 0)\n\
         (defun my-double-word (start end)\n\
         (let ((word (buffer-substring start end)))\n\
         (delete-region start end)\n\
         (insert (concat word word))))\n\
         (advice-add 'my-double-word :before\n\
         (lambda (start end)\n\
         (put-text-property start end 'doubled t)))\n\
         (advice-add 'my-double-word :after\n\
         (lambda (start end)\n\
         (setq my-transform-count (1+ my-transform-count))\n\
         (let ((new-end (+ start (* 2 (- end start)))))\n\
         (put-text-property start new-end 'transform-count my-transform-count))))\n\
         (let ((buf (generate-new-buffer \"act\")))\n\
         (with-current-buffer buf\n\
         (insert \"one two three four five\")\n\
         (undo-boundary)\n\
         (my-double-word 1 4)\n\
         (undo-boundary)\n\
         (my-double-word 9 14)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (tc1 (get-text-property 1 'transform-count))\n\
         (tc9 (get-text-property 9 'transform-count)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s tc1 tc9 my-transform-count\n\
         (buffer-string)\n\
         (get-text-property 1 'doubled)\n\
         (get-text-property 5 'doubled)))))\n\
         (advice-remove 'my-double-word\n\
         (lambda (start end)\n\
         (put-text-property start end 'doubled t)))\n\
         (advice-remove 'my-double-word\n\
         (lambda (start end)\n\
         (setq my-transform-count (1+ my-transform-count))\n\
         (let ((new-end (+ start (* 2 (- end start)))))\n\
         (put-text-property start new-end 'transform-count my-transform-count))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_hash_table_textprop_key_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"hti\"))\n\
         (tbl (make-hash-table :test 'equal)))\n\
         (with-current-buffer buf\n\
         (insert \"key1-key2-key3-key4\")\n\
         (puthash (buffer-substring 1 5) 'alpha tbl)\n\
         (puthash (buffer-substring 6 10) 'beta tbl)\n\
         (puthash (buffer-substring 11 15) 'gamma tbl)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (delete-region 1 5)\n\
         (insert \"new1\")\n\
         (undo-boundary)\n\
         (let ((before-undo (buffer-string))\n\
         (v1 (gethash \"key1\" tbl))\n\
         (v2 (gethash \"key2\" tbl))\n\
         (v3 (gethash \"key3\" tbl)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((after-undo (buffer-string)))\n\
         (list before-undo v1 v2 v3 after-undo\n\
         (gethash \"key1\" tbl)\n\
         (gethash \"key2\" tbl)\n\
         (gethash \"key3\" tbl)\n\
         (= (hash-table-count tbl) 3))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_buffer_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"clb\")))\n\
         (with-current-buffer buf\n\
         (insert \"3,apple,red\\n7,banana,yellow\\n1,cherry,red\\n5,date,brown\\n9,elderberry,purple\")\n\
         (let ((lines (split-string (buffer-string) \"\\n\" t)))\n\
         (let ((parsed (cl-loop for line in lines\n\
         for parts = (split-string line \",\")\n\
         collect (list (string-to-number (first parts))\n\
         (second parts)\n\
         (third parts)))))\n\
         (let ((sorted (cl-sort (copy-sequence parsed) #'< :key #'first)))\n\
         (erase-buffer)\n\
         (dolist (entry sorted)\n\
         (insert (format \"%d,%s,%s\\n\" (first entry) (second entry) (third entry))))\n\
         (put-text-property 1 8 'sort-order 'ascending)\n\
         (put-text-property 9 20 'sort-order 'ascending)\n\
         (let ((red-count (cl-count 'red parsed :key #'third :test #'equal)))\n\
         (list (buffer-string)\n\
         red-count\n\
         (get-text-property 1 'sort-order)\n\
         (get-text-property 9 'sort-order)\n\
         (length sorted)\n\
         (= (first (first sorted)) 1)\n\
         (= (first (car (last sorted))) 9)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_deeply_nested_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 10 10)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"dnn\")))\n\
         (with-current-buffer buf\n\
         (insert \"0123456789ABCDEFGHIJQRSTUVWXYZ\")\n\
         (put-text-property 1 10 'section 'digits)\n\
         (put-text-property 11 20 'section 'first-letters)\n\
         (put-text-property 21 30 'section 'second-letters)\n\
         (let ((m-total (copy-marker (point-max))))\n\
         (undo-boundary)\n\
         (narrow-to-region 3 28)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"NNN\")\n\
         (undo-boundary)\n\
         (narrow-to-region 6 25)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"MMM\")\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string))\n\
         (min1 (point-min))\n\
         (max1 (point-max))\n\
         (sec1 (get-text-property (point-min) 'section)))\n\
         (widen)\n\
         (widen)\n\
         (let ((s2 (buffer-string))\n\
         (min2 (point-min))\n\
         (max2 (point-max))\n\
         (sec2a (get-text-property 1 'section))\n\
         (sec2b (get-text-property 10 'section))\n\
         (sec2c (get-text-property 20 'section)))\n\
         (primitive-undo 5 buffer-undo-list)\n\
         (list s1 min1 max1 sec1\n\
         s2 min2 max2 sec2a sec2b sec2c\n\
         (buffer-string)\n\
         (get-text-property 1 'section)\n\
         (get-text-property 10 'section)\n\
         (marker-position m-total)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
