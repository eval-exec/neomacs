//! GNU-derived semantic coverage for the immutable text-property-any scan.
//! Expected results were captured with GNU Emacs 31.1 in the lane sandbox.

fn assert_gnu_result(form: &str, expected: &str) {
    crate::test_utils::init_test_tracing();
    let actual = crate::test_utils::runtime_startup_eval_one(form);
    assert_eq!(actual, format!("OK {expected}"));
}

#[test]
fn text_property_any_interval_buffer_encodings_and_dense_boundaries() {
    assert_gnu_result(
        r#"(mapcar
 (lambda (entry)
   (with-temp-buffer
     (set-buffer-multibyte (car entry))
     (insert (cadr entry))
     (dotimes (i (1- (point-max)))
       (put-text-property (1+ i) (+ i 2) 'face (if (= (% i 2) 0) 'bold 'italic)))
     (put-text-property 3 5 'cont23-any-p 'hit)
     (list (text-property-any 1 (point-max) 'cont23-any-p 'hit)
           (text-property-any 4 (point-max) 'cont23-any-p 'hit)
           (text-property-any 1 3 'cont23-any-p 'hit)
           (text-property-any 3 3 'cont23-any-p 'hit)
           (text-property-any 1 (point-max) 'cont23-any-p nil)
           (text-property-any 3 (point-max) 'cont23-any-p nil)
           (text-property-any 1 (point-max) 'cont23-any-p 'missing)
           (text-property-not-all 3 5 'cont23-any-p 'hit))))
 (list (list nil "abcdefg")
       (list t "aж😀bcde")
       (list nil (unibyte-string 65 200 66 255 67 68 69))))"#,
        "((3 4 nil nil 1 5 nil nil) (3 4 nil nil 1 5 nil nil) (3 4 nil nil 1 5 nil nil))",
    );
}

#[test]
fn text_property_any_interval_buffer_resolved_precedence() {
    assert_gnu_result(
        r#"(with-temp-buffer
 (insert "abcdefghijkl")
 (let* ((prop (make-symbol "cont23-any-prop"))
        (category (make-symbol "cont23-any-category"))
        (alias-empty (make-symbol "cont23-any-alias-empty"))
        (alias-hit (make-symbol "cont23-any-alias-hit"))
        (char-property-alias-alist (list (list prop alias-empty alias-hit)))
        (default-text-properties (list prop 'default-hit)))
   (put category prop 'category-hit)
   (put-text-property 1 (point-max) 'face 'bold)
   (put-text-property 2 4 prop nil)
   (put-text-property 2 4 'category category)
   (put-text-property 2 4 alias-hit 'alias-hit)
   (put-text-property 5 7 'category category)
   (put-text-property 5 7 alias-hit 'alias-hit)
   (put-text-property 8 10 alias-empty nil)
   (put-text-property 8 10 alias-hit 'alias-hit)
   (list (text-property-any 2 (point-max) prop nil)
         (text-property-any 1 (point-max) prop 'category-hit)
         (text-property-any 1 (point-max) prop 'alias-hit)
         (text-property-any 1 (point-max) prop 'default-hit)
         (text-property-any 2 4 prop 'category-hit)
         (text-property-not-all 2 4 prop nil)
         (progn (put category prop nil)
                (text-property-any 5 7 prop 'alias-hit))
         (text-property-any 11 (point-max) prop 'default-hit))))"#,
        "(2 5 8 1 nil nil 5 11)",
    );
}

#[test]
fn text_property_any_interval_buffer_live_plist_and_trailing_nil() {
    // Keep the live name eligible for the presence summary: adding a wholly
    // unobserved name through plist-put is an existing, separate GNU gap.
    assert_gnu_result(
        r#"(with-temp-buffer
 (insert "abcdefghij")
 (put-text-property 1 2 'cont23-added nil)
 (put-text-property 3 5 'cont23-existing 1)
 (plist-put (text-properties-at 3) 'cont23-added 7)
 (let ((pos 1) (n 0))
   (while (and (< n 20)
               (setq pos (text-property-not-all pos (point-max) 'cont23-added nil)))
     (setq n (1+ n))
     (setq pos (or (text-property-any pos (point-max) 'cont23-added nil) (point-max))))
   (list n pos
         (text-property-any 3 (point-max) 'cont23-added nil)
         (text-property-any 3 (point-max) 'cont23-existing nil)
         (text-property-any 5 (point-max) 'cont23-existing nil)
         (text-property-any 1 3 'cont23-existing 1))))"#,
        "(1 nil 5 5 5 nil)",
    );
}

#[test]
fn text_property_any_interval_buffer_ranges_markers_and_narrowing() {
    assert_gnu_result(
        r#"(with-temp-buffer
 (insert "aж😀bcde")
 (let ((end (copy-marker (point-max) t)))
   (goto-char (point-max))
   (insert "Z")
   (put-text-property 8 9 'cont23-range 'edge)
   (let ((marker-result (text-property-any 1 end 'cont23-range 'edge)))
     (put-text-property 4 6 'cont23-range 'hit)
     (narrow-to-region 3 7)
     (list marker-result
           (text-property-any 3 7 'cont23-range 'hit)
           (text-property-any 3 7 'cont23-range nil)
           (text-property-any 3 3 'cont23-range nil)
           (text-property-any 6 3 'cont23-range 'hit)
           (text-property-any 3 4 'cont23-range 'hit)
           (text-property-any 4 5 'cont23-range 'hit)
           (condition-case err
               (text-property-any 1 7 'cont23-range nil)
             (error (car err)))))))"#,
        "(8 4 3 nil 4 nil 4 args-out-of-range)",
    );
}

#[test]
fn text_property_any_interval_buffer_empty_table_default_and_empty_narrowing() {
    assert_gnu_result(
        r#"(with-temp-buffer
 (insert "abcdef")
 (let ((default-text-properties '(cont23-empty fallback)))
   (list (text-property-any 1 7 'cont23-empty nil)
         (text-property-any 1 7 'cont23-empty 'fallback)
         (text-property-any 2 2 'cont23-empty nil)
         (progn (put-text-property 2 5 'face 'bold)
                (text-property-any 2 5 'cont23-empty 'fallback))
         (progn (narrow-to-region 3 3)
                (text-property-any 3 3 'cont23-empty nil)))) )"#,
        "(1 nil nil 2 nil)",
    );
}

#[test]
fn text_property_any_interval_string_branch_parity() {
    assert_gnu_result(
        r#"(mapcar
 (lambda (s)
   (dotimes (i (length s))
     (put-text-property i (1+ i) 'face (if (= (% i 2) 0) 'bold 'italic) s))
   (put-text-property 2 4 'cont23-string 'hit s)
   (list (text-property-any 0 (length s) 'cont23-string 'hit s)
         (text-property-any 3 (length s) 'cont23-string 'hit s)
         (text-property-any 0 2 'cont23-string 'hit s)
         (text-property-any 2 2 'cont23-string 'hit s)
         (text-property-any 0 (length s) 'cont23-string nil s)
         (text-property-any 2 (length s) 'cont23-string nil s)
         (text-property-any 0 (length s) 'cont23-string 'missing s)))
 (list (copy-sequence "abcdefg")
       (copy-sequence "aж😀bcde")
       (unibyte-string 65 200 66 255 67 68 69)))"#,
        "((2 3 nil nil 0 4 nil) (2 3 nil nil 0 4 nil) (2 3 nil nil 0 4 nil))",
    );
}
