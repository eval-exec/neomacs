//! Tera-strict combo tests for org-mode extreme edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-create combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_create_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (paragraph 3 (#(\"body\" 0 4 (:parent (section nil #(\"body\" 0 4 (:parent #4)))))) (1 section) \"foo\" 1 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Simple element.
   (org-element-type (org-element-create 'paragraph))
   ;; With properties.
   (org-element-property :level (org-element-create 'headline '(:level 3)))
   ;; With contents.
   (org-element-contents (org-element-create 'section nil "body"))
   ;; With properties and contents.
   (let ((el (org-element-create 'headline '(:level 1) (org-element-create 'section nil "text"))))
     (list (org-element-property :level el)
           (org-element-type (car (org-element-contents el)))))
   ;; String creation.
   (org-element-create "foo")
   ;; Plain-text with properties.
   (get-text-property 0 :a (org-element-create 'plain-text '(:a 1) "foo"))
   ;; Children.
   (let ((children '("a" "b" (org-element-create 'foo))))
     (equal (cddr (apply #'org-element-create 'bar nil children))
            children))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-copy combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_copy_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold 2 nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Preserve type.
   (org-element-type (org-element-copy (org-element-create 'bold nil "text")))
   ;; Preserve properties.
   (org-element-property :level (org-element-copy (org-element-create 'headline '(:level 2))))
   ;; No parent on copy.
   (org-element-property :parent (org-element-copy (org-element-create 'paragraph nil "text")))
   ;; Copy nil.
   (org-element-copy nil)
   ;; Copy secondary string.
   (equal '("text") (org-element-copy '("text")))
   ;; Not eq.
   (eq '("text") (org-element-copy '("text")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-set combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_set_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (bar nil #(\"test\" 0 4 (:parent (#(\"test\" 0 4 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Set with keep-props.
   (org-element-property
    :foo
    (org-element-set
     (org-element-create 'dummy '(:foo bar))
     (org-element-create 'dummy '(:foo2 bar2))
     '(:foo)))
   ;; Set without keep-props.
   (org-element-property
    :foo
    (org-element-set
     (org-element-create 'dummy '(:foo bar))
     (org-element-create 'dummy '(:foo2 bar2))))
   ;; Set string with element.
   (let ((parent (org-element-create 'anonymous nil "test"))
         (str "old"))
     (org-element-set str "new")
     (car (org-element-contents parent)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-adopt combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_adopt_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((paragraph paragraph) section section)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((parent (org-element-create 'section nil))
         (child1 (org-element-create 'paragraph nil "p1"))
         (child2 (org-element-create 'paragraph nil "p2")))
    (org-element-adopt parent child1 child2)
    (list
     ;; Contents after adopt.
     (mapcar #'org-element-type (org-element-contents parent))
     ;; Parent set on children.
     (org-element-type (org-element-property :parent child1))
     (org-element-type (org-element-property :parent child2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-extract combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_extract_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((paragraph) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((parent (org-element-create 'section nil))
         (child1 (org-element-create 'paragraph nil "p1"))
         (child2 (org-element-create 'paragraph nil "p2")))
    (org-element-adopt parent child1 child2)
    (org-element-extract child1)
    (list
     ;; Contents after extract.
     (mapcar #'org-element-type (org-element-contents parent))
     ;; Extracted child has no parent.
     (org-element-property :parent child1))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-insert-before combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_insert_before_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (paragraph paragraph paragraph)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((parent (org-element-create 'section nil))
         (child1 (org-element-create 'paragraph nil "p1"))
         (child2 (org-element-create 'paragraph nil "p2"))
         (new-child (org-element-create 'paragraph nil "new")))
    (org-element-adopt parent child1 child2)
    (org-element-insert-before new-child child2)
    (mapcar #'org-element-type (org-element-contents parent))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-swap-A-B combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_swap_a_b_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((parent (org-element-create 'section nil))
         (child1 (org-element-create 'paragraph nil "p1"))
         (child2 (org-element-create 'paragraph nil "p2"))
         (child3 (org-element-create 'paragraph nil "p3")))
    (org-element-adopt parent child1 child2 child3)
    (org-element-swap-A-B child1 child3)
    (mapcar #'org-element-type (org-element-contents parent))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-uniq combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_uniq_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-uniq)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((el1 (org-element-create 'paragraph nil "p1"))
         (el2 (org-element-create 'paragraph nil "p2"))
         (el3 (org-element-create 'headline '(:level 1)))
         (list (list el1 el2 el3 el1 el2 el3 el1)))
    (list
     ;; Length before uniq.
     (length list)
     ;; Length after uniq.
     (length (org-element-uniq list))
     ;; Order preserved.
     (mapcar #'org-element-type (org-element-uniq list)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-lineage combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_lineage_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((paragraph center-block section headline headline org-data) (bold paragraph center-block section headline headline org-data) (nil :standard-properties section) (nil :standard-properties paragraph) (nil :standard-properties plain-text))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n#+BEGIN_CENTER\n*bold*\n#+END_CENTER")
      (goto-char (point-min))
      (search-forward "bold")
      (let* ((tree (org-element-parse-buffer))
             (bold (car (org-element-map tree 'bold #'identity))))
        (list
         ;; Full lineage.
         (mapcar #'org-element-type (org-element-lineage bold))
         ;; With self.
         (mapcar #'org-element-type (org-element-lineage bold nil t))
         ;; Filtered: headlines only.
         (mapcar #'org-element-type (org-element-lineage bold 'headline))
         ;; Filtered: blocks only.
         (mapcar #'org-element-type (org-element-lineage bold 'center-block))
         ;; Filtered: with self.
         (mapcar #'org-element-type (org-element-lineage bold '(bold paragraph) t)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-lineage-map combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_lineage_map_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((paragraph center-block section headline headline org-data) (bold paragraph center-block section headline headline org-data) (\"H2\" \"H1\") bold)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n#+BEGIN_CENTER\n*bold*\n#+END_CENTER")
      (goto-char (point-min))
      (search-forward "bold")
      (let* ((tree (org-element-parse-buffer))
             (bold (car (org-element-map tree 'bold #'identity))))
        (list
         ;; Full lineage map.
         (org-element-lineage-map bold #'org-element-type)
         ;; With self.
         (org-element-lineage-map bold #'org-element-type nil t)
         ;; FUN as form.
         (org-element-lineage-map
          bold '(org-element-property :raw-value node))
         ;; FIRST-MATCH.
         (org-element-lineage-map
          bold #'org-element-type nil t t))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-property-inherited combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_property_inherited_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 (1 2 3) (\"p\") (\"c\") (\"gc\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((grandchild (org-element-create 'grandchild '(:shared 3 :own-gc "gc")))
         (child (org-element-create 'child '(:shared 2 :own-c "c") grandchild))
         (parent (org-element-create 'parent '(:shared 1 :own-p "p") child)))
    (list
     ;; Without self.
     (org-element-property-inherited :shared grandchild)
     ;; With self.
     (org-element-property-inherited :shared grandchild 'with-self)
     ;; Accumulate.
     (org-element-property-inherited :shared grandchild 'with-self 'accumulate)
     ;; Only parent has :own-p.
     (org-element-property-inherited :own-p grandchild 'with-self 'accumulate)
     ;; Only child has :own-c.
     (org-element-property-inherited :own-c grandchild 'with-self 'accumulate)
     ;; Only grandchild has :own-gc.
     (org-element-property-inherited :own-gc grandchild 'with-self 'accumulate))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-normalize-contents combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_normalize_contents_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((paragraph nil \"Two spaces\\n Three spaces\") (paragraph nil #(\"  Two spaces\\nNo space\" 0 2 (org-ind 2))) (paragraph nil \"Two spaces\\n\\n\\nTwo spaces\") (paragraph nil \"No space\\nTwo spaces\\n Three spaces\") (paragraph nil \"1 space\" (line-break) \" 2 spaces\") (verse-block nil \"line 1\\n\\nline 2\") (paragraph nil \" Two spaces \" (bold nil \" and\\nOne space\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Remove common indentation.
   (org-element-normalize-contents
    '(paragraph nil "  Two spaces\n   Three spaces"))
   ;; No common indentation.
   (org-element-normalize-contents
    '(paragraph nil "  Two spaces\nNo space"))
   ;; Ignore blank lines.
   (org-element-normalize-contents
    '(paragraph nil "  Two spaces\n\n \n  Two spaces"))
   ;; With argument.
   (org-element-normalize-contents
    '(paragraph nil "No space\n  Two spaces\n   Three spaces") t)
   ;; Line break corner case.
   (org-element-normalize-contents
    '(paragraph nil " 1 space" (line-break) "  2 spaces"))
   ;; Verse block.
   (org-element-normalize-contents
    '(verse-block nil "  line 1\n\n  line 2"))
   ;; Recursive objects.
   (org-element-normalize-contents
    '(paragraph nil "  Two spaces " (bold nil " and\n One space")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-secondary-p combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_secondary_p_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:title :foo nil)""#]];
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
        (org-element-put-property el :foo (list child))
        child))
     ;; Outside secondary string.
     (with-temp-buffer (org-mode) (insert "Paragraph *object*")
       (goto-char (point-min))
       (org-element-map (org-element-parse-buffer) 'bold
         (lambda (object) (org-element-type (org-element-secondary-p object)))
         nil t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-map combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_map_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nPara *bold* /italic/.\n** H2\nMore text.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Map with t (all types).
         (length (org-element-map tree t #'identity))
         ;; Map single type.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         ;; Map list of types.
         (mapcar #'org-element-type
                 (org-element-map tree '(bold italic) #'identity))
         ;; Map with first-match.
         (org-element-property :raw-value
           (org-element-map tree 'headline #'identity nil t))
         ;; Map with no-recursion.
         (length (org-element-map tree 'bold #'identity nil nil 'paragraph))
         ;; Map with affiliated.
         (length (org-element-map tree 'bold #'identity nil nil nil nil t))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-ast-map combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_ast_map_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((plain-text plain-text bold) (plain-text plain-text) (bold bold) (bold) (dummy bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; TYPES = t.
   (org-element-ast-map
    (org-element-create 'anonymous nil "a" "b" (org-element-create 'bold))
    t #'org-element-type)
   ;; IGNORE.
   (let ((bold (org-element-create 'bold)))
     (org-element-ast-map
      (org-element-create 'anonymous nil "a" "b" bold)
      t #'org-element-type (list bold)))
   ;; Extra secondary properties.
   (org-element-ast-map
    (org-element-create
     'dummy
     `(:foo ,(org-element-create 'bold))
     (org-element-create 'bold))
    'bold #'org-element-type
    nil nil nil '(:foo))
   ;; No secondary.
   (org-element-ast-map
    (org-element-create
     'dummy
     `(:secondary (:foo) :foo ,(org-element-create 'bold))
     (org-element-create 'bold))
    'bold #'org-element-type
    nil nil nil nil 'no-secondary)
   ;; Deferred values.
   (org-element-ast-map
    (org-element-create
     'dummy
     `(:secondary (:foo) :foo ,(org-element-deferred-create nil (lambda (_) "a")))
     (org-element-create 'bold))
    t #'org-element-type
    nil nil nil nil nil 'no-undefer)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-properties-mapc combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_properties_mapc_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create
             'dummy
             `( :foo ,(org-element-deferred-create t (lambda (_) 1))
                :bar 2))))
    (list
     ;; Find deferred property.
     (catch :found
       (org-element-properties-mapc
        (lambda (_ val _)
          (when (org-element-deferred-p val)
            (throw :found t)))
        el))
     ;; After undefer, find resolved value.
     (catch :found
       (org-element-properties-mapc
        (lambda (prop val _)
          (when (and (eq prop :foo) (eq 1 val))
            (throw :found t)))
        el 'undefer)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-properties-map combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_properties_map_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3) (1 2 nil) (1 2 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create 'dummy '(:foo 1 :bar 2 :baz 3))))
    (list
     ;; Single argument.
     (org-element-properties-map #'identity el)
     ;; Two arguments.
     (org-element-properties-map
      (lambda (prop val) (unless (eq prop :baz) val)) el)
     ;; Three arguments.
     (org-element-properties-map
      (lambda (prop val node)
        (if (eq prop :baz)
            (1+ (org-element-property-raw :baz node))
          val))
      el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-deferred combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_deferred_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-deferred-force-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Deferred-p checks.
   (org-element-deferred-p (org-element-deferred-create t (lambda (_) 1)))
   (org-element-deferred-p (org-element-deferred-create nil (lambda (_) 1)))
   (org-element-deferred-p '(not-deferred))
   ;; Force-p checks.
   (org-element-deferred-force-p (org-element-deferred-create t (lambda (_) 1)))
   (org-element-deferred-force-p (org-element-deferred-create nil (lambda (_) 1)))
   ;; Function access.
   (functionp (org-element-deferred-get-function
               (org-element-deferred-create nil (lambda (_) 1))))
   ;; Create alias.
   (let ((el (org-element-create 'dummy '(:foo 1 :bar ,(org-element-deferred-create-alias :foo)))))
     (list (org-element-property :foo el)
           (org-element-property :bar el)))
   ;; Create list.
   (let ((el (org-element-create 'dummy '(:foo ,(org-element-deferred-create-list
                                                  (list 1 2 (org-element-deferred-create nil (lambda (_) 3))))))))
     (org-element-property :foo el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-property-raw setter combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_property_raw_setter_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 baz 3.14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create 'dummy '(:foo 1))))
    ;; Set via setf.
    (setf (org-element-property-raw :foo el) 42)
    (setf (org-element-property-raw :bar el) 'baz)
    (setf (org-element-property-raw :num el) 3.14)
    (list (org-element-property-raw :foo el)
          (org-element-property-raw :bar el)
          (org-element-property-raw :num el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-at-point combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_at_point_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (paragraph paragraph paragraph headline drawer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; At beginning of buffer.
     (with-temp-buffer (org-mode) (insert "Para")
       (goto-char (point-min))
       (org-element-type (org-element-at-point)))
     ;; At end of buffer.
     (with-temp-buffer (org-mode) (insert "Para")
       (goto-char (point-max))
       (org-element-type (org-element-at-point)))
     ;; Between two paragraphs.
     (with-temp-buffer (org-mode) (insert "P1\n\nP2")
       (goto-char (point-min)) (forward-line 2)
       (org-element-type (org-element-at-point)))
     ;; At blank line after headline.
     (with-temp-buffer (org-mode) (insert "* H\n\nP")
       (goto-char (point-min)) (forward-line 1)
       (org-element-type (org-element-at-point)))
     ;; At drawer boundary.
     (with-temp-buffer (org-mode) (insert ":DRAWER:\ncontents\n:END:")
       (goto-char (point-min)) (forward-line 2)
       (org-element-type (org-element-at-point))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-context combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_context_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Para\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Para with *bold* and /italic/ and _underline_ and =code=.\n")
      (goto-char (point-min))
      (list
       ;; On bold.
       (search-forward "bold")
       (org-element-type (org-element-context))
       ;; On italic.
       (search-forward "italic")
       (org-element-type (org-element-context))
       ;; On underline.
       (search-forward "underline")
       (org-element-type (org-element-context))
       ;; On code.
       (search-forward "code")
       (org-element-type (org-element-context))
       ;; On plain text.
       (search-forward "Para")
       (org-element-type (org-element-context))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tera: org-element with all org-element-interpret-data combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tera_all_org_element_interpret_data_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Simple paragraph.\\n\" \"* H1\\nBody 1\\n** H2\\nBody 2\\n\" \"| a | b |\\n| c | d |\\n\" \"- Item 1\\n- Item 2\\n  - Sub\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Paragraph.
     (with-temp-buffer (org-mode) (insert "Simple paragraph.\n")
       (goto-char (point-min))
       (substring-no-properties
        (org-element-interpret-data (org-element-parse-buffer))))
     ;; Headlines.
     (with-temp-buffer (org-mode) (insert "* H1\nBody 1\n** H2\nBody 2\n")
       (goto-char (point-min))
       (substring-no-properties
        (org-element-interpret-data (org-element-parse-buffer))))
     ;; Table.
     (with-temp-buffer (org-mode) (insert "| a | b |\n| c | d |\n")
       (goto-char (point-min))
       (substring-no-properties
        (org-element-interpret-data (org-element-parse-buffer))))
     ;; List.
     (with-temp-buffer (org-mode) (insert "- Item 1\n- Item 2\n  - Sub\n")
       (goto-char (point-min))
       (substring-no-properties
        (org-element-interpret-data (org-element-parse-buffer)))))))"##,
        expect,
    );
}
