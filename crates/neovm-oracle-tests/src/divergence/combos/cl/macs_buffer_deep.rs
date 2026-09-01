//! Deep stress: cl-macs + condition-case + unwind-protect + buffer manipulation combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_cl_flet_with_buffer_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-flet)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cfb\")))\n\
         (with-current-buffer buf\n\
         (insert \"HELLO WORLD\")\n\
         (put-text-property 1 6 'case 'upper)\n\
         (put-text-property 6 11 'case 'upper)\n\
         (cl-flet ((transform (s) (concat \"[\" (downcase s) \"]\")))\n\
         (let ((result (transform (buffer-string))))\n\
         (undo-boundary)\n\
         (erase-buffer)\n\
         (insert result)\n\
         (put-text-property 1 1 'transformed t)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'transformed)\n\
         (length result)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_labels_recursive_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"clr\")))\n\
         (with-current-buffer buf\n\
         (cl-labels\n\
         ((count-words (s)\n\
         (let ((words (split-string s \"\\\\s-+\" t)))\n\
         (length words)))\n\
         (process-level (n acc)\n\
         (if (= n 0)\n\
         (nreverse acc)\n\
         (let ((line (format \"word%d word%d \" n (1+ n))))\n\
         (insert line)\n\
         (process-level (1- n)\n\
         (cons (count-words (buffer-string)) acc))))))\n\
         (undo-boundary)\n\
         (let ((result (process-level 5 nil)))\n\
         (list result\n\
         (buffer-string)\n\
         (length result)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_unwind_protect_buffer_cleanup_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"upb\"))\n\
         (cleanup-log nil))\n\
         (with-current-buffer buf\n\
         (insert \"IMPORTANT DATA\")\n\
         (put-text-property 1 14 'critical t)\n\
         (undo-boundary)\n\
         (unwind-protect\n\
         (progn\n\
         (goto-char 1)\n\
         (insert \"PREFIX\")\n\
         (put-text-property 1 6 'added 'prefix)\n\
         (undo-boundary)\n\
         (condition-case nil\n\
         (error \"simulated error\")\n\
         (error\n\
         (push 'caught cleanup-log))))\n\
         (push 'cleanup cleanup-log)\n\
         (push (buffer-string) cleanup-log)))\n\
         (list (nreverse cleanup-log)\n\
         (with-current-buffer buf (get-text-property 1 'added))\n\
         (with-current-buffer buf (get-text-property 7 'critical))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_macrolet_buffer_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-macrolet)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cmb\")))\n\
         (with-current-buffer buf\n\
         (insert \"alpha beta gamma delta\")\n\
         (put-text-property 1 6 'idx 0)\n\
         (put-text-property 7 11 'idx 1)\n\
         (put-text-property 12 17 'idx 2)\n\
         (put-text-property 18 23 'idx 3)\n\
         (cl-macrolet ((swap-words (a b)\n\
         (let ((tmp (gensym)))\n\
         (list 'let (list (list tmp a))\n\
         (list 'setq a b)\n\
         (list 'setq b tmp)))))\n\
         (let ((words (split-string (buffer-string))))\n\
         (swap-words (nth 0 words) (nth 2 words))\n\
         (list words\n\
         (get-text-property 1 'idx)\n\
         (get-text-property 7 'idx)\n\
         (get-text-property 12 'idx)\n\
         (get-text-property 18 'idx)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_typecase_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-typecase)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((items (list 42 \"hello\" '(1 2 3) [a b c] 'symbol\n\
         (make-hash-table) (make-marker) (make-overlay 1 1))))\n\
         (let ((types (mapcar\n\
         (lambda (x)\n\
         (cl-typecase x\n\
         (integer 'int)\n\
         (string 'str)\n\
         (cons 'list)\n\
         (vector 'vec)\n\
         (symbol 'sym)\n\
         (hash-table 'hash)\n\
         (marker 'mark)\n\
         (overlay 'ov)\n\
         (t 'other)))\n\
         items)))\n\
         (list types (= (length types) 8))))\n\
         (let ((ov (car (last items))))\n\
         (when (overlayp ov) (delete-overlay ov))))\n\
         (delete-overlay (make-overlay 1 1)))",
        expect,
    );
}

#[test]
fn deficiency_cl_opt_dynamic_extent_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun process-with-locals (buf data)\n\
         (let ((result nil)\n\
         (temp-hash (make-hash-table :test 'equal)))\n\
         (with-current-buffer buf\n\
         (dolist (item data)\n\
         (cl-destructuring-bind (key val . props) item\n\
         (let ((existing (gethash key temp-hash 0)))\n\
         (puthash key (+ existing val) temp-hash)\n\
         (insert (format \"%s=%d \" key (+ existing val)))\n\
         (dolist (p props)\n\
         (put-text-property\n\
         (- (point) (length (format \"%s=%d \" key (+ existing val))))\n\
         (point)\n\
         (car p) (cdr p))))))\n\
         (push (buffer-string) result)\n\
         (push (hash-table-count temp-hash) result)\n\
         (nreverse result)))\n\
         (let ((buf (generate-new-buffer \"odb\")))\n\
         (let ((r (process-with-locals buf\n\
         '((\"a\" 1 color . red)\n\
         (\"b\" 2 size . large)\n\
         (\"a\" 3 color . blue)\n\
         (\"c\" 5 size . small)\n\
         (\"b\" 1 weight . heavy)))))\n\
         (prog1\n\
         (list r\n\
         (gethash \"a\" (make-hash-table :test 'equal))\n\
         (with-current-buffer buf (get-text-property 1 'color))\n\
         (with-current-buffer buf (get-text-property 7 'color)))\n\
         (kill-buffer buf)))))",
        expect,
    );
}

#[test]
fn deficiency_condition_case_signal_undo_preserve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"csu\")))\n\
         (with-current-buffer buf\n\
         (insert \"LINE1 LINE2 LINE3\")\n\
         (put-text-property 1 6 'num 1)\n\
         (put-text-property 7 12 'num 2)\n\
         (put-text-property 13 18 'num 3)\n\
         (undo-boundary)\n\
         (condition-case err\n\
         (progn\n\
         (goto-char 7)\n\
         (delete-region 7 12)\n\
         (put-text-property 7 7 'deleted t)\n\
         (undo-boundary)\n\
         (signal 'error '(\"forced error\")))\n\
         (error\n\
         (list 'caught (car (cdr err)))))\n\
         (let ((s (buffer-string))\n\
         (n7 (get-text-property 7 'num))\n\
         (d7 (get-text-property 7 'deleted)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s n7 d7\n\
         (buffer-string)\n\
         (get-text-property 7 'num)\n\
         (get-text-property 7 'deleted)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_defun_with_cl_parse_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defun)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defun process-text (&key buf (prefix \"\") (suffix \"\") transforms)\n\
         (with-current-buffer buf\n\
         (let ((content (buffer-string)))\n\
         (dolist (tf transforms)\n\
         (setq content (funcall tf content)))\n\
         (erase-buffer)\n\
         (insert (concat prefix content suffix))\n\
         (put-text-property 1 (1+ (length prefix)) 'prefix t)\n\
         (put-text-property (1+ (length prefix))\n\
         (+ 1 (length prefix) (length content))\n\
         'body t))))\n\
         (let ((buf (generate-new-buffer \"cdk\")))\n\
         (with-current-buffer buf (insert \"middle\"))\n\
         (process-text\n\
         :buf buf\n\
         :prefix \"[START]\"\n\
         :suffix \"[END]\"\n\
         :transforms (list #'upcase #'(lambda (s) (concat s \"!\"))))\n\
         (let ((r (list (with-current-buffer buf (buffer-string))\n\
         (with-current-buffer buf (get-text-property 1 'prefix))\n\
         (with-current-buffer buf (get-text-property 8 'body)))))\n\
         (kill-buffer buf)\n\
         r)))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_hash_buf_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"clh\"))\n\
         (tbl (make-hash-table :test 'equal)))\n\
         (with-current-buffer buf\n\
         (cl-loop for i from 1 to 20\n\
         for ch = (char-after (progn (goto-char (point-max)) (insert (char-to-string (+ 64 i))) (point)))\n\
         do (puthash (char-to-string (+ 64 i)) i tbl)\n\
         count t into total\n\
         finally do (insert (format \" total=%d\" total))))\n\
         (let ((result\n\
         (list (buffer-string)\n\
         (= (gethash \"A\" tbl) 1)\n\
         (= (gethash \"T\" tbl) 20)\n\
         (hash-table-count tbl))))\n\
         (kill-buffer buf)\n\
         result)))",
        expect,
    );
}

#[test]
fn deficiency_nested_unwind_protect_signal_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"inner\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"nup\"))\n\
         (log nil))\n\
         (with-current-buffer buf\n\
         (insert \"ORIGINAL\")\n\
         (put-text-property 1 9 'state 'initial)\n\
         (undo-boundary)\n\
         (unwind-protect\n\
         (unwind-protect\n\
         (unwind-protect\n\
         (progn\n\
         (goto-char 5)\n\
         (insert \"MODIFIED\")\n\
         (put-text-property 5 13 'state 'modified)\n\
         (undo-boundary)\n\
         (push 'inner-body log)\n\
         (signal 'error '(\"inner\")))\n\
         (push 'inner-cleanup log)\n\
         (goto-char 1)\n\
         (insert \"[RECOVERED]\"))\n\
         (push 'middle-cleanup log)\n\
         (put-text-property 1 1 'recovered t))\n\
         (push 'outer-cleanup log)))\n\
         (let ((r (list (nreverse log)\n\
         (buffer-string)\n\
         (get-text-property 1 'state)\n\
         (get-text-property 1 'recovered)\n\
         (get-text-property 16 'state))))\n\
         (kill-buffer buf)\n\
         r)))",
        expect,
    );
}
