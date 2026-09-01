//! Omega-strict combo tests for org-mode extreme edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-type edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_type_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (plain-text nil nil dummy dummy dummy nil anonymous anonymous nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; plain-text.
   (org-element-type "string")
   ;; nil.
   (org-element-type nil)
   ;; number.
   (org-element-type 1)
   ;; symbol.
   (org-element-type '(dummy))
   ;; with extra args.
   (org-element-type '(dummy nil 'foo))
   (org-element-type '(dummy (:a a :b b) 'foo))
   ;; anonymous node.
   (org-element-type '((dummy)))
   (org-element-type '((dummy)) t)
   (org-element-type '("string") t)
   (org-element-type '(1 2) t)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-type-p edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_type_p_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (foo) (foo bar) nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (org-element-type-p '(foo) 'foo)
   (org-element-type-p '(foo) '(foo))
   (org-element-type-p '(foo) '(foo bar))
   (org-element-type-p '(foo) 'bar)
   (org-element-type-p '(foo) '(bar baz))
   (org-element-type-p "string" 'plain-text)
   (org-element-type-p '((foo)) 'anonymous)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-class edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_class_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (element object element object object element element element object object object object)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Regular elements.
   (org-element-class '(paragraph nil) nil)
   (org-element-class '(target nil) nil)
   ;; Special types.
   (org-element-class '(org-data nil) nil)
   (org-element-class "text" nil)
   (org-element-class '("secondary " "string") nil)
   ;; Pseudo elements.
   (org-element-class '(foo nil) nil)
   (org-element-class '(foo nil) '(center-block nil))
   (org-element-class '(foo nil) '(org-data nil))
   ;; Pseudo objects.
   (org-element-class '(foo nil) '(bold nil))
   (org-element-class '(foo nil) '(paragraph nil))
   (org-element-class '(foo nil) '("secondary"))
   ;; In title secondary string.
   (let* ((datum '(foo nil))
          (headline `(headline (:title (,datum) :secondary (:title)))))
     (org-element-put-property datum :parent headline)
     (org-element-class datum))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-property-raw edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_property_raw_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((nil default nil default) (nil default nil default) (nil default nil default) (nil default nil default))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((results nil))
    ;; No properties.
    (dolist (element `( nil
                        (headline nil)
                        (headline nil (headline))
                        "string"))
      (push (list (org-element-property-raw :begin element)
                  (org-element-property-raw :begin element 'default)
                  (org-element-property-raw :begin1 element)
                  (org-element-property-raw :begin1 element 'default))
            results))
    (nreverse results)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-property-raw non-standard
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_property_raw_non_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((nil default 1 1) (nil default 1 1) (nil default 1 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((results nil))
    (dolist (element `((headline (:begin1 1))
                       (headline (:begin1 1) (headline))
                       ,(propertize "string" :begin1 1)))
      (push (list (org-element-property-raw :begin element)
                  (org-element-property-raw :begin element 'default)
                  (org-element-property-raw :begin1 element)
                  (org-element-property-raw :begin1 element 'default))
            results))
    (nreverse results)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-property-raw standard array
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_property_raw_standard_array() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((test test nil default) (test test nil default))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((results nil))
    (dolist (element `((headline (:standard-properties ,(make-vector 10 'test)))
                       (headline (:standard-properties ,(make-vector 10 'test)) (headline))))
      (push (list (org-element-property-raw :begin element)
                  (org-element-property-raw :begin element 'default)
                  (org-element-property-raw :begin1 element)
                  (org-element-property-raw :begin1 element 'default))
            results))
    (nreverse results)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-property-raw plist
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_property_raw_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((1 1 nil default) (1 1 nil default) (1 1 nil default))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((results nil))
    (dolist (element `((headline (:begin 1))
                       (headline (:begin 1) (headline))
                       ,(propertize "string" :begin 1)))
      (push (list (org-element-property-raw :begin element)
                  (org-element-property-raw :begin element 'default)
                  (org-element-property-raw :begin1 element)
                  (org-element-property-raw :begin1 element 'default))
            results))
    (nreverse results)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-property-raw mixed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_property_raw_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((test test nil default) (test test nil default) (test test nil default))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((results nil))
    (dolist (element `((headline (:standard-properties ,(make-vector 10 'test) :begin 1))
                       (headline (:begin 1 :standard-properties ,(make-vector 10 'test)))
                       (headline (:standard-properties ,(make-vector 10 'test) :begin 1) (headline))))
      (push (list (org-element-property-raw :begin element)
                  (org-element-property-raw :begin element 'default)
                  (org-element-property-raw :begin1 element)
                  (org-element-property-raw :begin1 element 'default))
            results))
    (nreverse results)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-property-raw general
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_property_raw_general() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((test test 1 1) (test test 1 1) (test test 1 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((results nil))
    (dolist (element `((headline (:standard-properties ,(make-vector 10 'test) :begin1 1))
                       (headline (:begin1 1 :standard-properties ,(make-vector 10 'test)))
                       (headline (:standard-properties ,(make-vector 10 'test) :begin1 1) (headline))))
      (push (list (org-element-property-raw :begin element)
                  (org-element-property-raw :begin element 'default)
                  (org-element-property-raw :begin1 element)
                  (org-element-property-raw :begin1 element 'default))
            results))
    (nreverse results)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-property (deferred)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_property_deferred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Resolve :deferred property.
   (let ((el (org-element-create
              'dummy
              `(:deferred ,(org-element-deferred-create
                            t (lambda (el) (org-element-put-property el :foo 'bar) nil))))))
     (list (org-element-property :foo el) (org-element-property :foo2 el)))
   ;; Deferred value.
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (org-element-property :foo el))
   ;; Auto-undefer.
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create t (lambda (_) 'bar))))))
     (list (org-element-property :foo el) (org-element-property-raw :foo el)))
   ;; Force undefer.
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (list (org-element-property :foo el)
           (org-element-property-raw :foo el)
           (org-element-property :foo el nil 'force)
           (org-element-property-raw :foo el)))
   ;; Deferred alias.
   (let ((el (org-element-create
              'dummy `( :foo 1 :bar ,(org-element-deferred-create-alias :foo)))))
     (list (org-element-property :foo el) (org-element-property :bar el)))
   ;; Deferred list.
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create-list
                              (list 1 2 (org-element-deferred-create nil (lambda (_) 3))))))))
     (org-element-property :foo el))
   ;; Deferred with side effects (retry).
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create
                              nil (lambda (el)
                                    (org-element-put-property el :foo 1)
                                    (throw :org-element-deferred-retry nil)))))))
     (org-element-property :foo el))
   ;; Recursive undefer.
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create
                              nil (lambda (el)
                                    (org-element-deferred-create
                                     nil (lambda (_) 1)))))))
     (org-element-property :foo el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-property-2
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_property_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create 'dummy '(:foo bar))))
    (eq (org-element-property :foo el)
        (org-element-property-2 el :foo))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-parent
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create 'dummy '(:parent bar))))
    (eq (org-element-property :parent el)
        (org-element-parent el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-properties-resolve
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_properties_resolve() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create
             'dummy
             `( :foo ,(org-element-deferred-create t (lambda (_) 1))
                :bar ,(org-element-deferred-create nil (lambda (_) 2))
                :deferred ,(org-element-deferred-create
                            t nil (lambda (el)
                                    (org-element-put-property el :baz 3)))))))
    ;; Resolve conditionally.
    (setq el (org-element-properties-resolve el))
    (let ((r1 (list (org-element-property-raw :foo el)
                    (org-element-property-raw :bar el)
                    (org-element-property :bar el)
                    (org-element-property-raw :baz el))))
      ;; Resolve unconditionally.
      (setq el (org-element-properties-resolve el 'force))
      (list r1 (org-element-property-raw :bar el)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-secondary-p edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_secondary_p_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:title :foo nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; In title secondary string.
     (with-temp-buffer (org-mode) (insert "* Headline *object*")
       (goto-char (point-min))
       (org-element-map (org-element-parse-buffer) 'bold
         (lambda (object) (org-element-secondary-p object))
         nil t))
     ;; Manual construction.
     (org-element-secondary-p
      (let* ((el (org-element-create 'dummy '(:secondary (:foo))))
             (child (org-element-create "string" `(:parent ,el))))
        (org-element-put-property el :foo (list child)) child))
     ;; Outside secondary string.
     (with-temp-buffer (org-mode) (insert "Paragraph *object*")
       (goto-char (point-min))
       (org-element-map (org-element-parse-buffer) 'bold
         (lambda (object) (org-element-type (org-element-secondary-p object)))
         nil t))
     ;; Wrong property.
     (eq :foo
         (org-element-secondary-p
          (let* ((el (org-element-create 'dummy '(:secondary (:foo))))
                 (child (org-element-create "string" `(:parent ,el))))
            (org-element-put-property el :bar (list child)) child))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-map plain text
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_map_plain_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Some text \alpha
#+BEGIN_CENTER
Some other text
#+END_CENTER")
      (goto-char (point-min))
      (let ((count 0))
        (org-element-map
            (org-element-parse-buffer) 'plain-text
          (lambda (s) (when (string-match "text" s) (cl-incf count))))
        count))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-map secondary string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_map_secondary_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((bold nil \"bold\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  ;; Applies to secondary strings.
  (org-element-map '("some " (bold nil "bold") "text") 'bold 'identity))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-map enter secondary first
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_map_enter_secondary_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"alpha\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Some \\alpha headline\n\\beta entity.")
      (goto-char (point-min))
      (org-element-property
       :name
       (org-element-map (org-element-parse-buffer) 'entity 'identity nil t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-map no recursion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_map_no_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER\n\\alpha\n#+END_CENTER")
      (goto-char (point-min))
      (org-element-map
          (org-element-parse-buffer) 'entity 'identity nil nil 'center-block))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-map with affiliated
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_map_with_affiliated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"1\" 0 1 (:parent (#(\"1\" 0 1 (:parent #3))))) #(\"a\" 0 1 (:parent (#(\"a\" 0 1 (:parent #3))))) #(\"2\" 0 1 (:parent (#(\"2\" 0 1 (:parent #3))))) #(\"b\" 0 1 (:parent (#(\"b\" 0 1 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CAPTION[a]: 1\n#+CAPTION[b]: 2\nParagraph")
      (goto-char (point-min))
      (org-element-map
          (org-element-at-point) 'plain-text 'identity nil nil nil t))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-map first match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_map_first_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"H1\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n* H2\n* H3\n")
      (goto-char (point-min))
      (org-element-property
       :raw-value
       (org-element-map (org-element-parse-buffer) 'headline #'identity nil t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-map accumulate
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_map_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H2\" \"H3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n* H2\n* H3\n")
      (goto-char (point-min))
      (mapcar (lambda (h) (org-element-property :raw-value h))
              (org-element-map (org-element-parse-buffer) 'headline #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-ast-map types=t
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_ast_map_types_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (plain-text plain-text bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  ;; TYPES = t.
  (org-element-ast-map
   (org-element-create 'anonymous nil "a" "b" (org-element-create 'bold))
   t #'org-element-type))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-ast-map ignore
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_ast_map_ignore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (plain-text plain-text)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  ;; IGNORE.
  (let ((bold (org-element-create 'bold)))
    (org-element-ast-map
     (org-element-create 'anonymous nil "a" "b" bold)
     t #'org-element-type (list bold))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-ast-map fun list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_ast_map_fun_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n* H2")
      (goto-char (point-min))
      (org-element-map
          (org-element-parse-buffer)
          t '(org-element-property :raw-value node)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-ast-map extra secondary
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_ast_map_extra_secondary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((bold bold) (bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Extra secondary properties.
   (org-element-ast-map
    (org-element-create
     'dummy
     `(:foo ,(org-element-create 'bold))
     (org-element-create 'bold))
    'bold #'org-element-type
    nil nil nil '(:foo))
   ;; Without extra secondary - should differ.
   (org-element-ast-map
    (org-element-create
     'dummy
     `(:foo ,(org-element-create 'bold))
     (org-element-create 'bold))
    'bold #'org-element-type)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-ast-map no secondary
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_ast_map_no_secondary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((bold) (bold bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; no-secondary flag.
   (org-element-ast-map
    (org-element-create
     'dummy
     `(:secondary (:foo) :foo ,(org-element-create 'bold))
     (org-element-create 'bold))
    'bold #'org-element-type
    nil nil nil nil 'no-secondary)
   ;; Without no-secondary.
   (org-element-ast-map
    (org-element-create
     'dummy
     `(:secondary (:foo) :foo ,(org-element-create 'bold))
     (org-element-create 'bold))
    'bold #'org-element-type)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Omega: org-element with all org-element-ast-map deferred
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn omega_all_ast_map_deferred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((dummy bold) (dummy plain-text bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; no-undefer.
   (org-element-ast-map
    (org-element-create
     'dummy
     `(:secondary (:foo) :foo ,(org-element-deferred-create nil (lambda (_) "a")))
     (org-element-create 'bold))
    t #'org-element-type
    nil nil nil nil nil 'no-undefer)
   ;; Default (with undefer).
   (org-element-ast-map
    (org-element-create
     'dummy
     `(:secondary (:foo) :foo ,(org-element-deferred-create nil (lambda (_) "a")))
     (org-element-create 'bold))
    t #'org-element-type)))"##,
        expect,
    );
}
