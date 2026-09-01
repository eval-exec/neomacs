//! Divergence tests: buffer-local + defvar + setq + closure + mark combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_buffer_local_var_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 16 54)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-bl-chain-val 0)
  (setq-local 'test-bl-chain-val 100)
  (let ((fns (list
    (lambda () (setq test-bl-chain-val (+ test-bl-chain-val 1)))
    (lambda () (setq test-bl-chain-val (* test-bl-chain-val 2)))
    (lambda () (setq test-bl-chain-val (- test-bl-chain-val 10)))
    (lambda () (push test-bl-chain-val test-bl-chain-history)))))
    (setq-local 'test-bl-chain-history nil)
    (dolist (fn fns) (funcall fn))
    (dolist (fn fns) (funcall fn))
    (list test-bl-chain-val
          (= test-bl-chain-val 404)
          test-bl-chain-history
          (= (length test-bl-chain-history) 2)
          (equal test-bl-chain-history '(404 192)))) #"#,
        expect,
    );
}

#[test]
fn divergence_marker_set_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"AAAA-XXXX-CCCC-DDDD-EEEE\" 0 3 (sec a) 10 13 (sec c) 15 18 (sec d) 20 23 (sec e)) #(\"AAAA-BBBB-CCCC-DDDD-EEEE\" 0 3 (sec a) 5 8 (sec b) 10 13 (sec c) 15 18 (sec d) 20 23 (sec e)) t 6 t 6 t 16 t 16 t 21 t a t b t c t all)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (put-text-property 1 4 'sec 'a)
  (put-text-property 6 9 'sec 'b)
  (put-text-property 11 14 'sec 'c)
  (put-text-property 16 19 'sec 'd)
  (put-text-property 21 24 'sec 'e)
  (let ((m1 (set-marker (make-marker) 1))
        (m2 (set-marker (make-marker) 6))
        (m3 (set-marker (make-marker) 11))
        (m4 (set-marker (make-marker) 16))
        (m5 (set-marker (make-marker) 21))
        (ov (make-overlay 1 24)))
    (overlay-put ov 'scope 'all)
    (undo-boundary)
    (set-marker m1 6)
    (set-marker m3 16)
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "BBBB" nil t)
    (replace-match "XXXX")
    (let ((s (buffer-string)))
      (primitive-undo 1 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "AAAA-BBBB-CCCC-DDDD-EEEE")
            (marker-position m1) (= (marker-position m1) 6)
            (marker-position m2) (= (marker-position m2) 6)
            (marker-position m3) (= (marker-position m3) 16)
            (marker-position m4) (= (marker-position m4) 16)
            (marker-position m5) (= (marker-position m5) 21)
            (get-text-property 1 'sec) (eq (get-text-property 1 'sec) 'a)
            (get-text-property 6 'sec) (eq (get-text-property 6 'sec) 'b)
            (get-text-property 11 'sec) (eq (get-text-property 11 'sec) 'c)
            (overlay-get ov 'scope))))) "#,
        expect,
    );
}

#[test]
fn divergence_mark_ring_with_edits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (4 nil #(\"ALPHA-QQBETA-SIGMA-DELTA-EPSILON\" 0 4 (greek a) 8 11 (greek b) 19 23 (greek d) 25 31 (greek e)) #(\"ALPHA-BETA-GAMMA-DELTA-EPSILON\" 0 4 (greek a) 6 9 (greek b) 11 15 (greek g) 17 21 (greek d) 23 29 (greek e)) t t a t b t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ALPHA-BETA-GAMMA-DELTA-EPSILON")
  (put-text-property 1 5 'greek 'a)
  (put-text-property 7 10 'greek 'b)
  (put-text-property 12 16 'greek 'g)
  (put-text-property 18 22 'greek 'd)
  (put-text-property 24 30 'greek 'e)
  (let ((ov (make-overlay 1 30))
        (m (copy-marker 7 t)))
    (overlay-put ov 'chain t)
    (push-mark 1 t t)
    (push-mark 7 t t)
    (push-mark 12 t t)
    (push-mark 18 t t)
    (push-mark 24 t t)
    (let ((ring-len (length mark-ring)))
      (undo-boundary)
      (goto-char 7)
      (insert "QQ")
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "GAMMA" nil t)
      (replace-match "SIGMA")
      (let ((s (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list ring-len (= ring-len 5)
              s
              (buffer-string)
              (string= (buffer-string) "ALPHA-BETA-GAMMA-DELTA-EPSILON")
              (= (marker-position m) 7)
              (get-text-property 1 'greek) (eq (get-text-property 1 'greek) 'a)
              (get-text-property 7 'greek) (eq (get-text-property 7 'greek) 'b)
              (overlay-get ov 'chain)))))) "#,
        expect,
    );
}

#[test]
fn deficiency_dotimes_with_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((result nil))
    (dotimes (i 10)
      (with-temp-buffer
        (insert (format "item-%d" i))
        (put-text-property 1 4 'idx i)
        (push (cons i (buffer-string)) result)))
    (setq result (nreverse result))
    (list (= (length result) 10)
          (equal (car result) '(0 . "item-0"))
          (equal (car (last result)) '(9 . "item-9"))
          (cl-every (lambda (p) (stringp (cdr p))) result)))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_over_hash_with_edits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 t 2 t 1 t 5 t 5 t 3 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal))
        (counter 0))
    (let ((inc-fn (lambda (key)
                    (setq counter (+ counter 1))
                    (puthash key (+ (gethash key ht 0) 1) ht)))
          (get-fn (lambda (key) (gethash key ht 0)))
          (sum-fn (lambda ()
                    (let ((s 0))
                      (maphash (lambda (_k v) (setq s (+ s v))) ht)
                      s))))
      (dotimes (i 5)
        (funcall inc-fn (format "key-%d" (mod i 3))))
      (list (funcall get-fn "key-0") (= (funcall get-fn "key-0") 2)
            (funcall get-fn "key-1") (= (funcall get-fn "key-1") 2)
            (funcall get-fn "key-2") (= (funcall get-fn "key-2") 1)
            (funcall sum-fn) (= (funcall sum-fn) 5)
            counter (= counter 5)
            (hash-table-count ht) (= (hash-table-count ht) 3))))) "#,
        expect,
    );
}

#[test]
fn divergence_recursive_edit_sim() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 28 44)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "LEVEL1-LEVEL2-LEVEL3-LEVEL4")
  (put-text-property 1 6 'depth 1)
  (put-text-property 8 13 'depth 2)
  (put-text-property 15 20 'depth 3)
  (put-text-property 22 27 'depth 4)
  (let ((m (copy-marker 8 t))
        (ov (make-overlay 1 27)))
    (overlay-put ov 'nested t)
    (undo-boundary)
    (narrow-to-region 8 20)
    (goto-char 8)
    (insert "INNER")
    (let ((inner (buffer-string)))
      (undo-boundary)
      (widen)
      (goto-char 1)
      (re-search-forward "LEVEL1" nil t)
      (replace-match "MODIFIED")
      (let ((outer (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list inner outer
              (buffer-string)
              (string= (buffer-string) "LEVEL1-LEVEL2-LEVEL3-LEVEL4")
              (= (marker-position m) 8)
              (get-text-property 1 'depth) (= (get-text-property 1 'depth) 1)
              (get-text-property 8 'depth) (= (get-text-property 8 'depth) 2)
              (overlay-get ov 'nested))))))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_temp_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((results
          (cl-loop for i from 1 to 10
                   collect (with-temp-buffer
                             (insert (make-string i ?X))
                             (put-text-property 1 i 'len i)
                             (list (buffer-string)
                                   (= (get-text-property 1 'len) i)
                                   (= (buffer-size) i))))))
    (list (= (length results) 10)
          (equal (car results) '("X" t t))
          (cl-every (lambda (r) (and (listp r) (= (length r) 3))) results)))) "#,
        expect,
    );
}

#[test]
fn divergence_dynamic_binding_vs_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-dyn-var nil)
  (let ((closures nil)
        (test-dyn-var "outer"))
    (dolist (item '("a" "b" "c" "d" "e"))
      (let ((captured item))
        (push (lambda () (list captured test-dyn-var)) closures)))
    (setq closures (nreverse closures))
    (let ((results (mapcar #'funcall closures)))
      (list (= (length results) 5)
            (equal (car results) '("a" "outer"))
            (equal (nth 4 results) '("e" "outer"))
            (cl-every (lambda (r) (equal (cadr r) "outer")) results))))) "#,
        expect,
    );
}

#[test]
fn deficiency_backquote_splice_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Result: alpha-beta-gamma-delta-epsilon-beta-gamma-delta\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((items '("alpha" "beta" "gamma" "delta")))
    (insert (format "Result: %s"
                    (mapconcat #'identity
                               `(,@items "epsilon" ,@(cdr items))
                               "-")))
    (list (buffer-string)
          (stringp (buffer-string))
          (> (buffer-size) 10)))) "#,
        expect,
    );
}

#[test]
fn deficiency_mapconcat_propertize_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"one two three four five\" 0 3 (word t) 4 7 (word t) 8 13 (word t) 14 18 (word t) 19 23 (word t)) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let* ((words '("one" "two" "three" "four" "five"))
         (prop-words (mapcar (lambda (w) (propertize w 'word t)) words))
         (result (mapconcat #'identity prop-words " ")))
    (insert result)
    (let ((all-word t))
      (dotimes (i (buffer-size))
        (unless (or (eq (get-text-property (+ i 1) 'word) t)
                    (eq (char-after (+ i 1)) ? ))
          (setq all-word nil)))
      (list (buffer-string)
            (string= (buffer-string) "one two three four five")
            all-word
            (= (buffer-size) 23))))) "#,
        expect,
    );
}
