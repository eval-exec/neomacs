//! Deep stress: buffer-local + default-dir + abbrev + auto-fill + undo combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_buffer_local_chain_across_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar my-shared-counter 0)\n\
         (let ((buf1 (generate-new-buffer \"bl1\"))\n\
         (buf2 (generate-new-buffer \"bl2\"))\n\
         (buf3 (generate-new-buffer \"bl3\")))\n\
         (dolist (b (list buf1 buf2 buf3))\n\
         (with-current-buffer b\n\
         (make-variable-buffer-local 'my-shared-counter)\n\
         (setq my-shared-counter (random 100))\n\
         (insert (format \"counter=%d\\n\" my-shared-counter))))\n\
         (let ((vals (list (with-current-buffer buf1 my-shared-counter)\n\
         (with-current-buffer buf2 my-shared-counter)\n\
         (with-current-buffer buf3 my-shared-counter))))\n\
         (with-current-buffer buf1 (setq my-shared-counter 999))\n\
         (list vals\n\
         (with-current-buffer buf1 my-shared-counter)\n\
         (with-current-buffer buf2 my-shared-counter)\n\
         (with-current-buffer buf3 my-shared-counter)))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)\n\
         (kill-buffer buf3)))",
        expect,
    );
}

#[test]
fn deficiency_default_directory_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ddb\")))\n\
         (with-current-buffer buf\n\
         (let ((dir (expand-file-name \"./\")))\n\
         (insert (format \"dir: %s\\n\" dir))\n\
         (put-text-property 1 6 'field 'label)\n\
         (put-text-property 6 (point-max) 'field 'value)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'field)\n\
         (get-text-property 6 'field)\n\
         (stringp dir)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_abbrev_expansion_undo_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"aeu\"))\n\
         (table (make-abbrev-table)))\n\
         (with-current-buffer buf\n\
         (define-abbrev table \"fn\" \"function\" nil)\n\
         (define-abbrev table \"var\" \"variable\" nil)\n\
         (define-abbrev table \"expr\" \"expression\" nil)\n\
         (setq local-abbrev-table table)\n\
         (insert \"fn var expr\")\n\
         (put-text-property 1 3 'abbrev 'fn)\n\
         (put-text-property 4 7 'abbrev 'var)\n\
         (put-text-property 8 12 'abbrev 'expr)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (goto-char 1)\n\
         (abbrev-insert (abbrev-symbol \"fn\" table))\n\
         (undo-boundary)\n\
         (let ((s2 (buffer-string))\n\
         (a1 (get-text-property 1 'abbrev)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s s2 a1\n\
         (buffer-string)\n\
         (get-text-property 1 'abbrev))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_fill_region_undo_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"fru\")))\n\
         (with-current-buffer buf\n\
         (insert \"This is a very long line of text that should be filled at some column width and it continues on and on with more words to fill.\")\n\
         (put-text-property 1 10 'para 1)\n\
         (put-text-property 10 30 'para 1)\n\
         (put-text-property 30 50 'para 1)\n\
         (put-text-property 50 70 'para 1)\n\
         (put-text-property 70 100 'para 1)\n\
         (let ((left-margin 0)\n\
         (fill-column 40))\n\
         (undo-boundary)\n\
         (fill-region 1 (point-max))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (p1 (get-text-property 1 'para))\n\
         (line-count (count-lines 1 (point-max))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s p1 line-count\n\
         (buffer-string)\n\
         (get-text-property 1 'para)\n\
         (count-lines 1 (point-max))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_justify_text_undo_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 32 49)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"jtu\")))\n\
         (with-current-buffer buf\n\
         (insert \"Short line\\nAnother short line\\nThird short line\")\n\
         (put-text-property 1 11 'line 1)\n\
         (put-text-property 12 31 'line 2)\n\
         (put-text-property 32 49 'line 3)\n\
         (let ((fill-column 40))\n\
         (undo-boundary)\n\
         (goto-char 12)\n\
         (insert \"          \")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (l12 (get-text-property 12 'line)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s l12\n\
         (buffer-string)\n\
         (get-text-property 1 'line)\n\
         (get-text-property 12 'line)\n\
         (get-text-property 32 'line))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_multiple_buf_local_vars_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar my-buf-mode 'normal)\n\
         (defvar my-buf-depth 0)\n\
         (defvar my-buf-tag nil)\n\
         (let ((buf (generate-new-buffer \"mbl\")))\n\
         (with-current-buffer buf\n\
         (make-variable-buffer-local 'my-buf-mode)\n\
         (make-variable-buffer-local 'my-buf-depth)\n\
         (make-variable-buffer-local 'my-buf-tag)\n\
         (setq my-buf-mode 'edit)\n\
         (setq my-buf-depth 3)\n\
         (setq my-buf-tag 'important)\n\
         (insert (format \"mode=%S depth=%d tag=%S\" my-buf-mode my-buf-depth my-buf-tag))\n\
         (put-text-property 1 10 'field 'mode)\n\
         (put-text-property 10 20 'field 'depth)\n\
         (put-text-property 20 30 'field 'tag)\n\
         (undo-boundary)\n\
         (setq my-buf-mode 'view)\n\
         (setq my-buf-depth 5)\n\
         (erase-buffer)\n\
         (insert (format \"mode=%S depth=%d tag=%S\" my-buf-mode my-buf-depth my-buf-tag))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (f1 (get-text-property 1 'field)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s f1\n\
         (buffer-string)\n\
         (get-text-property 1 'field)\n\
         (get-text-property 10 'field)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_with_temp_buffer_props_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"INNER BUFFER\" 0 12 (location inner)) inner) (#(\"OUTER BUFFER\" 0 12 (location outer)) outer))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"wtp\"))\n\
         (result nil))\n\
         (with-current-buffer buf\n\
         (insert \"OUTER BUFFER\")\n\
         (put-text-property 1 13 'location 'outer)\n\
         (with-temp-buffer\n\
         (insert \"INNER BUFFER\")\n\
         (put-text-property 1 13 'location 'inner)\n\
         (push (list (buffer-string) (get-text-property 1 'location)) result))\n\
         (push (list (buffer-string) (get-text-property 1 'location)) result))\n\
         (kill-buffer buf)\n\
         (nreverse result)))",
        expect,
    );
}

#[test]
fn deficiency_generate_new_buffer_unique_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"test-4\" #(\"Buffer 4\" 0 8 (idx 4)) 4) (\"test-3\" #(\"Buffer 3\" 0 8 (idx 3)) 3) (\"test-2\" #(\"Buffer 2\" 0 8 (idx 2)) 2) (\"test-1\" #(\"Buffer 1\" 0 8 (idx 1)) 1) (\"test-0\" #(\"Buffer 0\" 0 8 (idx 0)) 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((bufs nil)\n\
         (names nil))\n\
         (dotimes (i 5)\n\
         (let ((b (generate-new-buffer (format \"test-%d\" i))))\n\
         (push b bufs)\n\
         (with-current-buffer b\n\
         (insert (format \"Buffer %d\" i))\n\
         (put-text-property 1 9 'idx i))))\n\
         (dolist (b bufs)\n\
         (push (list (buffer-name b)\n\
         (with-current-buffer b (buffer-string))\n\
         (with-current-buffer b (get-text-property 1 'idx)))\n\
         names))\n\
         (dolist (b bufs) (kill-buffer b))\n\
         (nreverse names)))",
        expect,
    );
}

#[test]
fn deficiency_get_buffer_create_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\" *test-hidden*\" \"test-visible\" hidden shown 0)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (get-buffer-create \" *test-hidden*\"))\n\
         (buf2 (get-buffer-create \"test-visible\")))\n\
         (with-current-buffer buf1\n\
         (insert \"hidden content\")\n\
         (put-text-property 1 8 'vis 'hidden))\n\
         (with-current-buffer buf2\n\
         (insert \"visible content\")\n\
         (put-text-property 1 8 'vis 'shown))\n\
         (let ((result\n\
         (list (buffer-name buf1)\n\
         (buffer-name buf2)\n\
         (with-current-buffer buf1 (get-text-property 1 'vis))\n\
         (with-current-buffer buf2 (get-text-property 1 'vis))\n\
         (string-match \"^ \" (buffer-name buf1)))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)\n\
         result)))",
        expect,
    );
}

#[test]
fn deficiency_buffer_swap_text_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-swap-text 2)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (generate-new-buffer \"bs1\"))\n\
         (buf2 (generate-new-buffer \"bs2\")))\n\
         (with-current-buffer buf1\n\
         (insert \"CONTENT-A\")\n\
         (put-text-property 1 10 'source 'buf1))\n\
         (with-current-buffer buf2\n\
         (insert \"CONTENT-B\")\n\
         (put-text-property 1 10 'source 'buf2))\n\
         (buffer-swap-text buf1 buf2)\n\
         (list (with-current-buffer buf1 (buffer-string))\n\
         (with-current-buffer buf1 (get-text-property 1 'source))\n\
         (with-current-buffer buf2 (buffer-string))\n\
         (with-current-buffer buf2 (get-text-property 1 'source))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)))",
        expect,
    );
}
