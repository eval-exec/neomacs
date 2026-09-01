//! Deep stress: cl-loop advanced + hash-table + vector + string combos + undo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_cl_loop_hash_accumulate_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((tbl (make-hash-table :test 'equal))\n\
         (vec [10 20 30 40 50]))\n\
         (cl-loop for i across vec\n\
         for key = (format \"key%d\" i)\n\
         do (puthash key (* i i) tbl)\n\
         count t into total)\n\
         (let ((keys (sort (hash-table-keys tbl) #'string<))\n\
         (vals (mapcar (lambda (k) (gethash k tbl))\n\
         (sort (hash-table-keys tbl) #'string<))))\n\
         (list keys vals\n\
         (= (gethash \"key10\" tbl) 100)\n\
         (= (gethash \"key50\" tbl) 2500)\n\
         (= (hash-table-count tbl) 5))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_buffers_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((bufs (cl-loop for i from 1 to 5\n\
         collect (let ((b (generate-new-buffer (format \"clb%d\" i))))\n\
         (with-current-buffer b\n\
         (insert (format \"content%d\" i))\n\
         (put-text-property 1 9 'idx i))\n\
         b))))\n\
         (let ((result\n\
         (cl-loop for b in bufs\n\
         collect (list (buffer-name b)\n\
         (with-current-buffer b (buffer-string))\n\
         (with-current-buffer b (get-text-property 1 'idx))))))\n\
         (dolist (b bufs) (kill-buffer b))\n\
         result)))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_destructuring_with_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((data '((\"alice\" 30 95) (\"bob\" 25 87) (\"carol\" 28 92)))\n\
         (fns nil))\n\
         (cl-loop for (name age score) in data\n\
         do (push (lambda () (format \"%s: age=%d score=%d\" name age score)) fns))\n\
         (let ((results (mapcar #'funcall (nreverse fns))))\n\
         (list results\n\
         (= (length results) 3)\n\
         (string= (first results) \"alice: age=30 score=95\"))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_for_on_hashtable_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"clh\"))\n\
         (tbl (make-hash-table :test 'eq)))\n\
         (puthash 'x 10 tbl)\n\
         (puthash 'y 20 tbl)\n\
         (puthash 'z 30 tbl)\n\
         (with-current-buffer buf\n\
         (cl-loop for k being the hash-keys of tbl\n\
         using (hash-values v)\n\
         do (insert (format \"%s=%d \" k v)))\n\
         (put-text-property 1 6 'source 'hash)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"RESULT: \")\n\
         (put-text-property 1 8 'source 'label)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (src1 (get-text-property 1 'source))\n\
         (src9 (get-text-property 9 'source)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s src1 src9\n\
         (buffer-string)\n\
         (get-text-property 1 'source)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_sum_max_min_with_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"csm\")))\n\
         (with-current-buffer buf\n\
         (dolist (line '(\"10 apples\" \"20 bananas\" \"5 cherries\" \"15 dates\" \"8 elderberries\"))\n\
         (insert line) (insert \"\\n\"))\n\
         (goto-char 1)\n\
         (let ((numbers nil))\n\
         (while (re-search-forward \"^[0-9]+\" nil t)\n\
         (push (string-to-number (match-string 0)) numbers))\n\
         (let ((total (cl-loop for n in numbers sum n))\n\
         (mx (cl-loop for n in numbers maximize n))\n\
         (mn (cl-loop for n in numbers minimize n)))\n\
         (list total mx mn\n\
         (= total 58)\n\
         (= mx 20)\n\
         (= mn 5))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_vector_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((matrix [[1 2 3] [4 5 6] [7 8 9]]))\n\
         (let ((sums (cl-loop for row across matrix\n\
         collect (cl-loop for x across row sum x)))\n\
         (flat (cl-loop for row across matrix\n\
         nconc (cl-loop for x across row collect x))))\n\
         (list sums flat\n\
         (equal sums '(6 15 24))\n\
         (= (length flat) 9)\n\
         (= (cl-loop for x in flat sum x) 45)))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_substring_extraction_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cse\")))\n\
         (with-current-buffer buf\n\
         (insert \"2024-01-15 ERROR main crash\\n2024-01-16 WARN auth timeout\\n2024-01-17 INFO api started\")\n\
         (put-text-property 1 11 'field 'date)\n\
         (put-text-property 12 17 'field 'level)\n\
         (put-text-property 18 22 'field 'module)\n\
         (put-text-property 23 28 'field 'msg)\n\
         (let ((lines (split-string (buffer-string) \"\\n\" t)))\n\
         (let ((parsed (cl-loop for line in lines\n\
         for parts = (split-string line \" \")\n\
         collect (list (first parts)\n\
         (second parts)\n\
         (third parts)\n\
         (fourth parts)))))\n\
         (list parsed\n\
         (= (length parsed) 3)\n\
         (get-text-property 1 'field)\n\
         (get-text-property 12 'field))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_reducing_with_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((words '(\"apple\" \"banana\" \"apple\" \"cherry\" \"banana\" \"apple\" \"date\" \"cherry\")))\n\
         (let ((counts (make-hash-table :test 'equal)))\n\
         (cl-loop for w in words\n\
         do (puthash w (1+ (gethash w counts 0)) counts))\n\
         (let ((sorted (sort (hash-table-keys counts)\n\
         (lambda (a b) (> (gethash a counts) (gethash b counts))))))\n\
         (list sorted\n\
         (= (gethash \"apple\" counts) 3)\n\
         (= (gethash \"banana\" counts) 2)\n\
         (= (gethash \"cherry\" counts) 2)\n\
         (= (gethash \"date\" counts) 1)\n\
         (= (hash-table-count counts) 4)))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_generate_series_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cgs\")))\n\
         (with-current-buffer buf\n\
         (cl-loop for x = 1 then (* x 2)\n\
         while (<= x 1024)\n\
         do (insert (format \"%d \" x)))\n\
         (put-text-property 1 (point-max) 'series 'powers)\n\
         (undo-boundary)\n\
         (let ((nums (cl-loop for x = 1 then (* x 2)\n\
         while (<= x 1024)\n\
         collect x)))\n\
         (list nums\n\
         (= (length nums) 11)\n\
         (= (car (last nums)) 1024)\n\
         (= (car nums) 1)\n\
         (get-text-property 1 'series)\n\
         (buffer-string))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_string_ops_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cso\")))\n\
         (with-current-buffer buf\n\
         (let ((strings '(\"Hello World\" \"Foo Bar\" \"Test Case\" \"Upper Lower\")))\n\
         (cl-loop for s in strings\n\
         for lower = (downcase s)\n\
         for upper = (upcase s)\n\
         do (insert (format \"%s -> %s\\n\" lower upper)))\n\
         (put-text-property 1 12 'transform 'first)\n\
         (put-text-property 13 23 'transform 'second)\n\
         (put-text-property 24 35 'transform 'third)\n\
         (put-text-property 36 50 'transform 'fourth)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"HEADER\\n\")\n\
         (put-text-property 1 7 'transform 'header)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (t1 (get-text-property 1 'transform))\n\
         (t8 (get-text-property 8 'transform)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s t1 t8\n\
         (buffer-string)\n\
         (get-text-property 1 'transform)\n\
         (get-text-property 8 'transform)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
