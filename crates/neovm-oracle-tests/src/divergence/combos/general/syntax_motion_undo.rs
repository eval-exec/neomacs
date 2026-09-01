//! Deep stress: syntax + forward-word + scan-lists + buffer manipulation + undo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_modify_syntax_forward_word_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"msf\")))\n\
         (with-current-buffer buf\n\
         (insert \"hello-world foo_bar baz.qux\")\n\
         (put-text-property 1 12 'group 'first)\n\
         (put-text-property 12 19 'group 'second)\n\
         (put-text-property 20 28 'group 'third)\n\
         (undo-boundary)\n\
         (modify-syntax-entry ?- \"w\" (syntax-table))\n\
         (goto-char 1)\n\
         (let ((pos1 (progn (forward-word 1) (point))))\n\
         (modify-syntax-entry ?_ \"w\" (syntax-table))\n\
         (goto-char 1)\n\
         (let ((pos2 (progn (forward-word 1) (point))))\n\
         (modify-syntax-entry ?. \"w\" (syntax-table))\n\
         (goto-char 1)\n\
         (let ((pos3 (progn (forward-word 1) (point))))\n\
         (list pos1 pos2 pos3\n\
         (get-text-property 1 'group)\n\
         (get-text-property 12 'group)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_scan_lists_with_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 27 33)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"slp\")))\n\
         (with-current-buffer buf\n\
         (insert \"(foo (bar (baz 1 2 3)) quux)\")\n\
         (put-text-property 1 6 'depth 0)\n\
         (put-text-property 6 11 'depth 1)\n\
         (put-text-property 11 16 'depth 2)\n\
         (put-text-property 16 22 'depth 2)\n\
         (put-text-property 22 27 'depth 1)\n\
         (put-text-property 27 33 'depth 0)\n\
         (let ((pos-open (scan-lists 1 1 0))\n\
         (pos-close (scan-lists 1 -1 0))\n\
         (fwd-2 (scan-lists 1 2 0)))\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"[MOD]\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list pos-open pos-close fwd-2 s\n\
         (buffer-string)\n\
         (scan-lists 1 1 0)\n\
         (scan-lists 1 -1 0)\n\
         (scan-lists 1 2 0))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_forward_comment_with_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp (2097163))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"fcp\")))\n\
         (with-current-buffer buf\n\
         (insert \"/* comment1 */ code1 /* comment2 */ code2\")\n\
         (put-text-property 1 14 'type 'comment)\n\
         (put-text-property 15 20 'type 'code)\n\
         (put-text-property 21 35 'type 'comment)\n\
         (put-text-property 36 41 'type 'code)\n\
         (let ((c-rules (list (cons ?/ (string-to-syntax \"< b\"))\n\
         (cons ?* (string-to-syntax \"> b\"))))\n\
         (st (copy-syntax-table (syntax-table))))\n\
         (dolist (rule c-rules)\n\
         (modify-syntax-entry (car rule) (cdr rule) st))\n\
         (set-syntax-table st)\n\
         (goto-char 1)\n\
         (let ((moved (forward-comment 1)))\n\
         (let ((pos (point)))\n\
         (undo-boundary)\n\
         (goto-char 15)\n\
         (insert \"ADDED\")\n\
         (undo-boundary)\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list moved pos\n\
         (buffer-string)\n\
         (get-text-property 1 'type)\n\
         (get-text-property 15 'type)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_parse_partial_sexp_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"pps\")))\n\
         (with-current-buffer buf\n\
         (insert \"(defun foo (x y)\\n  (let ((z (+ x y)))\\n    (list z)))\")\n\
         (put-text-property 1 7 'keyword t)\n\
         (put-text-property 8 11 'name t)\n\
         (put-text-property 12 17 'params t)\n\
         (let ((state-at-10 (parse-partial-sexp 1 10))\n\
         (state-at-20 (parse-partial-sexp 1 20))\n\
         (state-at-30 (parse-partial-sexp 1 30)))\n\
         (list (list (nth 0 state-at-10) (nth 0 state-at-20) (nth 0 state-at-30))\n\
         (list (nth 3 state-at-10) (nth 3 state-at-20) (nth 3 state-at-30))\n\
         (get-text-property 1 'keyword)\n\
         (get-text-property 8 'name)\n\
         (get-text-property 12 'params)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_syntax_table_buffer_local_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"stb\")))\n\
         (with-current-buffer buf\n\
         (set-syntax-table (make-syntax-table))\n\
         (modify-syntax-entry ?@ \"w\" (syntax-table))\n\
         (insert \"user@host.com another@place.org\")\n\
         (put-text-property 1 14 'addr 'first)\n\
         (put-text-property 15 31 'addr 'second)\n\
         (goto-char 1)\n\
         (let ((pos1 (progn (forward-word 1) (point))))\n\
         (undo-boundary)\n\
         (modify-syntax-entry ?. \"w\" (syntax-table))\n\
         (goto-char 1)\n\
         (let ((pos2 (progn (forward-word 1) (point))))\n\
         (list pos1 pos2\n\
         (buffer-string)\n\
         (get-text-property 1 'addr)\n\
         (get-text-property 15 'addr))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_backward_up_list_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 37 43)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"bul\")))\n\
         (with-current-buffer buf\n\
         (insert \"(outer (inner (deep 1) (deep 2)) tail)\")\n\
         (put-text-property 1 8 'level 1)\n\
         (put-text-property 8 15 'level 2)\n\
         (put-text-property 15 22 'level 3)\n\
         (put-text-property 22 32 'level 3)\n\
         (put-text-property 32 37 'level 2)\n\
         (put-text-property 37 43 'level 1)\n\
         (goto-char 20)\n\
         (let ((p1 (condition-case nil (backward-up-list 1) (error nil)))\n\
         (p2 (condition-case nil (backward-up-list 2) (error nil)))\n\
         (p3 (condition-case nil (backward-up-list 3) (error nil))))\n\
         (list (point)\n\
         (get-text-property (point) 'level)\n\
         (when p1 (get-text-property (point) 'level))\n\
         (get-text-property 20 'level)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_sentence_syntax_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 35 52)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ksu\")))\n\
         (with-current-buffer buf\n\
         (insert \"First sentence. Second sentence. Third sentence.\")\n\
         (put-text-property 1 17 'sent 1)\n\
         (put-text-property 17 35 'sent 2)\n\
         (put-text-property 35 52 'sent 3)\n\
         (undo-boundary)\n\
         (goto-char 17)\n\
         (kill-region 17 35)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (s1 (get-text-property 1 'sent))\n\
         (s17 (get-text-property 17 'sent)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s s1 s17\n\
         (buffer-string)\n\
         (get-text-property 1 'sent)\n\
         (get-text-property 17 'sent)\n\
         (get-text-property 35 'sent)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_indent_syntax_based_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"isu\")))\n\
         (with-current-buffer buf\n\
         (insert \"(progn\\n(defun foo ()\\n(let ((x 1))\\n(+ x 2))))\")\n\
         (let ((depths\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (car (parse-partial-sexp 1 i)))))\n\
         (list depths\n\
         (= (nth 0 depths) 0)\n\
         (= (nth 7 depths) 1)\n\
         (= (nth 15 depths) 2)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_narrow_to_defun_with_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 29 29)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ndp\")))\n\
         (with-current-buffer buf\n\
         (insert \"(defun alpha () \\\"doc\\\" body1)\\n(defun beta () body2)\\n(defun gamma () body3)\")\n\
         (put-text-property 1 28 'func 'alpha)\n\
         (put-text-property 29 48 'func 'beta)\n\
         (put-text-property 49 68 'func 'gamma)\n\
         (undo-boundary)\n\
         (narrow-to-defun)\n\
         (let ((s (buffer-string))\n\
         (min (point-min))\n\
         (max (point-max)))\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((full (buffer-string)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s min max full\n\
         (buffer-string)\n\
         (get-text-property 1 'func)\n\
         (get-text-property 29 'func)\n\
         (get-text-property 49 'func))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_thing_motion_with_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"tmp\")))\n\
         (with-current-buffer buf\n\
         (insert \"The quick brown fox jumps over the lazy dog.\")\n\
         (put-text-property 1 4 'word-num 1)\n\
         (put-text-property 5 10 'word-num 2)\n\
         (put-text-property 11 16 'word-num 3)\n\
         (put-text-property 17 20 'word-num 4)\n\
         (put-text-property 21 26 'word-num 5)\n\
         (put-text-property 27 31 'word-num 6)\n\
         (put-text-property 32 36 'word-num 7)\n\
         (put-text-property 37 41 'word-num 8)\n\
         (put-text-property 42 45 'word-num 9)\n\
         (undo-boundary)\n\
         (goto-char 11)\n\
         (let ((bounds (bounds-of-thing-at-point 'word)))\n\
         (let ((start (car bounds))\n\
         (end (cdr bounds)))\n\
         (delete-region start end)\n\
         (insert \"SLOW\")\n\
         (put-text-property start end 'word-num 'replaced)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list start end bounds s\n\
         (buffer-string)\n\
         (get-text-property 11 'word-num)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
