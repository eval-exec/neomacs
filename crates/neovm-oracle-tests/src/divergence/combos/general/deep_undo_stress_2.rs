//! Divergence tests: deep undo+textprop+overlay+marker+replace+narrow combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_undo_propertized_replace_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"upr\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAABBBBCCCCDDDD\")\n\
         (put-text-property 1 5 'group 'a)\n\
         (put-text-property 5 9 'group 'b)\n\
         (put-text-property 9 13 'group 'c)\n\
         (put-text-property 13 17 'group 'd)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"AAAA\")\n\
         (replace-match \"1111\")\n\
         (put-text-property 1 5 'group 'num)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (re-search-forward \"BBBB\")\n\
         (replace-match \"2222\")\n\
         (put-text-property 5 9 'group 'num)\n\
         (undo-boundary)\n\
         (goto-char 9)\n\
         (re-search-forward \"CCCC\")\n\
         (replace-match \"3333\")\n\
         (put-text-property 9 13 'group 'num)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (g1 (get-text-property 1 'group))\n\
         (g5 (get-text-property 5 'group))\n\
         (g9 (get-text-property 9 'group))\n\
         (g13 (get-text-property 13 'group)))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s g1 g5 g9 g13\n\
         (buffer-string)\n\
         (get-text-property 1 'group)\n\
         (get-text-property 5 'group)\n\
         (get-text-property 9 'group)\n\
         (get-text-property 13 'group)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_overlay_stack_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uop\")))\n\
         (with-current-buffer buf\n\
         (insert \"XXXXXXXXXXYYYYYYYYYY\")\n\
         (let ((ov1 (make-overlay 1 10))\n\
         (ov2 (make-overlay 5 15))\n\
         (ov3 (make-overlay 11 20)))\n\
         (overlay-put ov1 'priority 1)\n\
         (overlay-put ov2 'priority 2)\n\
         (overlay-put ov3 'priority 3)\n\
         (overlay-put ov1 'tag 'first)\n\
         (overlay-put ov2 'tag 'second)\n\
         (overlay-put ov3 'tag 'third)\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (insert \"MMM\")\n\
         (undo-boundary)\n\
         (goto-char 16)\n\
         (insert \"NNN\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (t1 (overlay-get ov1 'tag))\n\
         (t2 (overlay-get ov2 'tag))\n\
         (t3 (overlay-get ov3 'tag)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s t1 t2 t3\n\
         (buffer-string)\n\
         (overlay-get ov1 'tag)\n\
         (overlay-get ov2 'tag)\n\
         (overlay-get ov3 'tag))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_narrow_marker_insert_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"nmi\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\")\n\
         (let ((m1 (copy-marker 5))\n\
         (m2 (copy-marker 15))\n\
         (m3 (copy-marker 25)))\n\
         (set-marker-insertion-type m1 nil)\n\
         (set-marker-insertion-type m2 t)\n\
         (set-marker-insertion-type m3 nil)\n\
         (undo-boundary)\n\
         (narrow-to-region 3 20)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"PP\")\n\
         (undo-boundary)\n\
         (goto-char (point-max))\n\
         (insert \"QQ\")\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list s\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (marker-position m3))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_ring_rectangle_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"krr\")))\n\
         (with-current-buffer buf\n\
         (insert \"line1 AAAA\\nline2 BBBB\\nline3 CCCC\\n\")\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (forward-line 0)\n\
         (let ((start (point)))\n\
         (delete-region start (+ start 6))\n\
         (insert \"LINE1\")\n\
         (undo-boundary))\n\
         (goto-char (point-min))\n\
         (forward-line 1)\n\
         (let ((start (point)))\n\
         (delete-region start (+ start 6))\n\
         (insert \"LINE2\")\n\
         (undo-boundary))\n\
         (goto-char (point-min))\n\
         (forward-line 2)\n\
         (let ((start (point)))\n\
         (delete-region start (+ start 6))\n\
         (insert \"LINE3\")\n\
         (undo-boundary))\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_textprop_merge_after_undo_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"tpm\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAA\")\n\
         (put-text-property 1 5 'level 1)\n\
         (put-text-property 6 10 'level 2)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"BBBB\")\n\
         (undo-boundary)\n\
         (let ((props (list (get-text-property 1 'level)\n\
         (get-text-property 5 'level)\n\
         (get-text-property 9 'level)\n\
         (get-text-property 14 'level)))\n\
         (s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list props s\n\
         (buffer-string)\n\
         (get-text-property 1 'level)\n\
         (get-text-property 5 'level)\n\
         (get-text-property 6 'level)\n\
         (get-text-property 10 'level)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_instance_in_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-token nil\n\
         ((type :initarg :type :accessor token-type)\n\
         (value :initarg :value :accessor token-value)))\n\
         (let ((buf (generate-new-buffer \"eit\"))\n\
         (tok1 (test-token :type 'keyword :value \"if\"))\n\
         (tok2 (test-token :type 'ident :value \"foo\")))\n\
         (with-current-buffer buf\n\
         (insert \"if foo then bar\")\n\
         (put-text-property 1 3 'token tok1)\n\
         (put-text-property 4 7 'token tok2)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (delete-char 3)\n\
         (insert \"when\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (t1 (and (get-text-property 1 'token)\n\
         (token-type (get-text-property 1 'token)))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s t1\n\
         (buffer-string)\n\
         (and (get-text-property 1 'token)\n\
         (token-type (get-text-property 1 'token)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_buflocal_marker_undo_across_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (generate-new-buffer \"bl1\"))\n\
         (buf2 (generate-new-buffer \"bl2\")))\n\
         (setq mark-ring nil)\n\
         (with-current-buffer buf1\n\
         (insert \"BUFFER1 CONTENT\")\n\
         (setq my-buf-val \"one\")\n\
         (make-variable-buffer-local 'my-buf-val)\n\
         (setq my-buf-val \"buf1\")\n\
         (push-mark 5)\n\
         (push-mark 10)\n\
         (undo-boundary)\n\
         (goto-char 8)\n\
         (insert \"INSERT\")\n\
         (undo-boundary))\n\
         (with-current-buffer buf2\n\
         (insert \"BUFFER2 CONTENT\")\n\
         (make-variable-buffer-local 'my-buf-val)\n\
         (setq my-buf-val \"buf2\")\n\
         (push-mark 5)\n\
         (undo-boundary)\n\
         (goto-char 8)\n\
         (insert \"ADDED\")\n\
         (undo-boundary))\n\
         (let ((b1-str (with-current-buffer buf1 (buffer-string)))\n\
         (b2-str (with-current-buffer buf2 (buffer-string)))\n\
         (b1-val (with-current-buffer buf1 my-buf-val))\n\
         (b2-val (with-current-buffer buf2 my-buf-val)))\n\
         (with-current-buffer buf1\n\
         (primitive-undo 1 buffer-undo-list))\n\
         (with-current-buffer buf2\n\
         (primitive-undo 1 buffer-undo-list))\n\
         (list b1-str b2-str b1-val b2-val\n\
         (with-current-buffer buf1 (buffer-string))\n\
         (with-current-buffer buf2 (buffer-string)))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)))",
        expect,
    );
}

#[test]
fn deficiency_advice_around_undo_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun my-transform-region (start end)\n\
         (let ((s (buffer-substring start end)))\n\
         (delete-region start end)\n\
         (insert (upcase s))))\n\
         (let ((advice-count 0))\n\
         (advice-add 'my-transform-region :around\n\
         (lambda (fn start end)\n\
         (setq advice-count (1+ advice-count))\n\
         (put-text-property start end 'advised t)\n\
         (funcall fn start end)))\n\
         (let ((buf (generate-new-buffer \"aat\")))\n\
         (with-current-buffer buf\n\
         (insert \"hello world test\")\n\
         (undo-boundary)\n\
         (my-transform-region 1 6)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (p1 (get-text-property 1 'advised)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s p1 advice-count\n\
         (buffer-string)\n\
         (get-text-property 1 'advised)))))\n\
         (advice-remove 'my-transform-region\n\
         (lambda (fn start end)\n\
         (setq advice-count (1+ advice-count))\n\
         (put-text-property start end 'advised t)\n\
         (funcall fn start end)))\n\
         (kill-buffer buf))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_destructuring_overlay_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cdo\")))\n\
         (with-current-buffer buf\n\
         (insert \"one two three four five six\")\n\
         (let ((words (split-string (buffer-string))))\n\
         (cl-loop for w in words\n\
         for start = (point-min) then (+ start 1 (length w))\n\
         do (put-text-property start (+ start (length w))\n\
         'word-length (length w)))\n\
         (let ((ov (make-overlay 1 27)))\n\
         (overlay-put ov 'total-length (length (buffer-string)))\n\
         (list (cl-loop for i from 1 to 26\n\
         collect (get-text-property i 'word-length))\n\
         (overlay-get ov 'total-length)\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_nested_undo_groups_with_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"nug\"))\n\
         (log nil))\n\
         (with-current-buffer buf\n\
         (let ((logger (lambda (msg)\n\
         (setq log (append log (list msg))))))\n\
         (insert \"INITIAL\")\n\
         (undo-boundary)\n\
         (funcall logger \"step1\")\n\
         (goto-char 1)\n\
         (insert \"A\")\n\
         (undo-boundary)\n\
         (funcall logger \"step2\")\n\
         (goto-char (point-max))\n\
         (insert \"Z\")\n\
         (undo-boundary)\n\
         (funcall logger \"step3\")\n\
         (delete-region 3 5)\n\
         (undo-boundary)\n\
         (funcall logger \"step4\")\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list log s\n\
         (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
