//! Ported upstream ERT tests from org-mode's test-ox.el (9.7.11) - batch 2.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ── Export: read-attribute ────────────────────────────────────────────

#[test]
fn upstream_ox_read_attribute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:a \"1\" :b \"2\") nil (:a nil :b nil) (:a nil :b nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     ;; Standard.
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer
        (org-mode)
        (insert "#+ATTR_HTML: :a 1 :b 2\nParagraph")
        (goto-char (point-min))
        (org-element-at-point)))
     ;; Empty attribute.
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer
        (org-mode)
        (insert "Paragraph")
        (goto-char (point-min))
        (org-element-at-point)))
     ;; "nil" string.
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer
        (org-mode)
        (insert "#+ATTR_HTML: :a nil :b nil\nParagraph")
        (goto-char (point-min))
        (org-element-at-point)))
     ;; Empty string.
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer
        (org-mode)
        (insert "#+ATTR_HTML: :a :b\nParagraph")
        (goto-char (point-min))
        (org-element-at-point))))))"##,
        expect,
    );
}

// ── Export: define-backend ────────────────────────────────────────────

#[test]
fn upstream_ox_define_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((headline . my-headline-test)) #s(org-export-backend test nil ((headline . my-headline-test)) nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let (org-export-registered-backends)
    (org-export-define-backend 'test '((headline . my-headline-test)))
    (list
     ;; Transcoders.
     (org-export-get-all-transcoders 'test)
     ;; Backend exists.
     (org-export-get-backend 'test))))"##,
        expect,
    );
}

// ── Export: define-derived-backend ────────────────────────────────────

#[test]
fn upstream_ox_define_derived_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (((:headline . test) (:headline . parent)) (parent) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let (org-export-registered-backends)
    (org-export-define-backend 'parent '((:headline . parent)))
    (org-export-define-derived-backend 'test 'parent
      :translate-alist '((:headline . test)))
    (list
     ;; Transcoders: append to parent's.
     (org-export-get-all-transcoders 'test)
     ;; Derived check.
     (org-export-derived-backend-p 'test 'parent)
     (org-export-derived-backend-p 'test 'test))))"##,
        expect,
    );
}

// ── Export: derived-backend-p ─────────────────────────────────────────

#[test]
fn upstream_ox_derived_backend_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((test) (test) (test) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let (org-export-registered-backends)
    (org-export-define-backend 'test '((headline . test)))
    (org-export-define-derived-backend 'test2 'test)
    (org-export-define-derived-backend 'test3 'test2)
    (org-export-define-backend 'other '((headline . other)))
    (list
     ;; Direct match.
     (org-export-derived-backend-p 'test 'test)
     ;; Direct parent.
     (org-export-derived-backend-p 'test2 'test)
     ;; Indirect parent.
     (org-export-derived-backend-p 'test3 'test)
     ;; Not related.
     (org-export-derived-backend-p 'other 'test))))"##,
        expect,
    );
}

// ── Export: get-all-transcoders ───────────────────────────────────────

#[test]
fn upstream_ox_get_all_transcoders() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil ((headline . ignore)) ((section . ignore) (headline . ignore)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   ;; Nil backend.
   (org-export-get-all-transcoders nil)
   ;; Simple.
   (org-export-get-all-transcoders
    (org-export-create-backend
     :transcoders '((headline . ignore))))
   ;; Inherit.
   (let (org-export-registered-backends)
     (org-export-define-backend 'b1 '((headline . ignore)))
     (org-export-get-all-transcoders
      (org-export-create-backend
       :parent 'b1 :transcoders '((section . ignore)))))))"##,
        expect,
    );
}

// ── Export: get-all-options ───────────────────────────────────────────

#[test]
fn upstream_ox_get_all_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (nil ((:key1 value1)) ((:key2 value2) (:key1 value1)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   ;; Nil backend.
   (org-export-get-all-options nil)
   ;; Simple.
   (org-export-get-all-options
    (org-export-create-backend
     :options '((:key1 value1))))
   ;; Inherit.
   (let (org-export-registered-backends)
     (org-export-define-backend 'b1 nil :options-alist '((:key1 value1)))
     (org-export-get-all-options
      (org-export-create-backend
       :parent 'b1 :options '((:key2 value2)))))))"##,
        expect,
    );
}

// ── Export: get-all-filters ───────────────────────────────────────────

#[test]
fn upstream_ox_get_all_filters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil ((:filter-headline . ignore)) ((:filter-section . ignore) (:filter-headline . ignore)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   ;; Nil backend.
   (org-export-get-all-filters nil)
   ;; Simple.
   (org-export-get-all-filters
    (org-export-create-backend
     :filters '((:filter-headline . ignore))))
   ;; Inherit.
   (let (org-export-registered-backends)
     (org-export-define-backend 'b1
       nil :filters-alist '((:filter-headline . ignore)))
     (org-export-get-all-filters
      (org-export-create-backend
       :parent 'b1 :filters '((:filter-section . ignore)))))))"##,
        expect,
    );
}

// ── Export: filter-apply-functions ────────────────────────────────────

#[test]
fn upstream_ox_filter_apply_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"210\" \"20\" \"0\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   ;; Applied in order.
   (org-export-filter-apply-functions
    (list (lambda (value &rest _) (concat "1" value))
          (lambda (value &rest _) (concat "2" value)))
    "0" nil)
   ;; Nil functions skipped.
   (org-export-filter-apply-functions
    (list #'ignore (lambda (value &rest _) (concat "2" value)))
    "0" nil)
   ;; All skipped: return initial.
   (org-export-filter-apply-functions (list #'ignore) "0" nil)
   ;; Empty string short-circuits.
   (org-export-filter-apply-functions
    (list (lambda (_value &rest _) "")
          (lambda (value &rest _) (concat "2" value)))
    "0" nil)))"##,
        expect,
    );
}

// ── Export: comments handling ─────────────────────────────────────────

#[test]
fn upstream_ox_comments_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     ;; Comments between paragraphs.
     (with-temp-buffer
       (org-mode)
       (insert "Para1\n# Comment\n\n# Comment\nPara2")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (info (org-combine-plists
                     (org-export--get-export-attributes)
                     (org-export-get-environment)
                     (org-export--collect-tree-properties
                      tree (org-export-get-environment)))))
         (org-export-data tree info)))
     ;; Comment between same paragraphs.
     (with-temp-buffer
       (org-mode)
       (insert "Para1\n# Comment\nPara2")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (info (org-combine-plists
                     (org-export--get-export-attributes)
                     (org-export-get-environment)
                     (org-export--collect-tree-properties
                      tree (org-export-get-environment)))))
         (org-export-data tree info))))))"##,
        expect,
    );
}

// ── Export: export-block ──────────────────────────────────────────────

#[test]
fn upstream_ox_export_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"BACKEND\" \"Success!\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "#+BEGIN_EXPORT backend\nSuccess!\n#+END_EXPORT")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (blocks (org-element-map tree 'export-block #'identity)))
        (mapcar (lambda (b)
                  (list (org-element-property :type b)
                        (org-element-property :value b)))
                blocks)))))"##,
        expect,
    );
}

// ── Export: export-snippet ────────────────────────────────────────────

#[test]
fn upstream_ox_export_snippet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"html\" \"<b>\") (\"latex\" \"\\\\textbf{bold}\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "@@html:<b>@@ and @@latex:\\textbf{bold}@@")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (snippets (org-element-map tree 'export-snippet #'identity)))
        (mapcar (lambda (s)
                  (list (org-element-property :back-end s)
                        (org-element-property :value s)))
                snippets)))))"##,
        expect,
    );
}

// ── Export: footnote-first-reference-p ────────────────────────────────

#[test]
fn upstream_ox_footnote_first_reference_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "Text[fn:1][fn:1]\n\n[fn:1] Definition")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (ref)
                  (org-export-footnote-first-reference-p ref info))
                (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ── Export: get-footnote-definition ───────────────────────────────────

#[test]
fn upstream_ox_get_footnote_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"A\\n\" 0 1 (:parent (paragraph (:standard-properties [20 20 20 21 21 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (footnote-definition (:standard-properties [13 13 20 21 21 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 21 21 0 nil first-section nil nil nil 1 21 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 21 21 0 nil org-data nil nil nil 3 21 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) (paragraph (:standard-properties [1 1 1 12 13 1 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"Text\" 0 4 (:parent #10)) (footnote-reference (:standard-properties [5 nil nil nil 11 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10] :label \"1\" :type standard)) #(\"\\n\" 0 1 (:parent #10))) #6)] :label \"1\" :pre-blank 0) #3)]) #(\"A\" 0 1 (:parent #3))))) #(\"A\" 0 1 (:parent (footnote-reference (:standard-properties [5 nil 11 12 13 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 13 13 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 13 13 0 nil first-section nil nil nil 1 13 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 13 13 0 nil org-data nil nil nil 3 13 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"Text\" 0 4 (:parent #6)) #3)] :label \"1\" :type inline) #(\"A\" 0 1 (:parent #3))))) #(\"A\" 0 1 (:parent (footnote-reference (:standard-properties [5 nil 10 11 12 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 12 12 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 12 12 0 nil first-section nil nil nil 1 12 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 12 12 0 nil org-data nil nil nil 3 12 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"Text\" 0 4 (:parent #6)) #3)] :label nil :type inline) #(\"A\" 0 1 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     ;; Standard.
     (with-temp-buffer
       (org-mode)
       (insert "Text[fn:1]\n\n[fn:1] A")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (info (org-combine-plists
                     (org-export--get-export-attributes)
                     (org-export-get-environment)
                     (org-export--collect-tree-properties
                      tree (org-export-get-environment)))))
         (org-element-interpret-data
          (org-export-get-footnote-definition
           (org-element-map tree 'footnote-reference #'identity nil t)
           info))))
     ;; Inline definition.
     (with-temp-buffer
       (org-mode)
       (insert "Text[fn:1:A]")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (info (org-combine-plists
                     (org-export--get-export-attributes)
                     (org-export-get-environment)
                     (org-export--collect-tree-properties
                      tree (org-export-get-environment)))))
         (org-element-interpret-data
          (org-export-get-footnote-definition
           (org-element-map tree 'footnote-reference #'identity nil t)
           info))))
     ;; Anonymous definition.
     (with-temp-buffer
       (org-mode)
       (insert "Text[fn::A]")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (info (org-combine-plists
                     (org-export--get-export-attributes)
                     (org-export-get-environment)
                     (org-export--collect-tree-properties
                      tree (org-export-get-environment)))))
         (org-element-interpret-data
          (org-export-get-footnote-definition
           (org-element-map tree 'footnote-reference #'identity nil t)
           info)))))))"##,
        expect,
    );
}

// ── Export: collect-footnote-definitions ──────────────────────────────

#[test]
fn upstream_ox_collect_footnote_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "Text[fn:1] [fn:2]\n\n[fn:1] D1\n[fn:2] D2")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (length (org-export-collect-footnote-definitions info))))))"##,
        expect,
    );
}

// ── Export: get-relative-level ────────────────────────────────────────

#[test]
fn upstream_ox_get_relative_level_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n*** H3\n**** H4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (h)
                  (org-export-get-relative-level h info))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ── Export: numbered-headline-p ──────────────────────────────────────

#[test]
fn upstream_ox_numbered_headline_p_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n*** H3\n* H4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (h)
                  (org-export-numbered-headline-p h info))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ── Export: get-headline-number ──────────────────────────────────────

#[test]
fn upstream_ox_get_headline_number_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1) (1 1) (1 2) (2) (2 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n** H3\n* H4\n** H5")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (h)
                  (org-export-get-headline-number h info))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ── Export: low-level-p ──────────────────────────────────────────────

#[test]
fn upstream_ox_low_level_p_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil)
        (org-export-headline-levels 2))
    (with-temp-buffer
      (org-mode)
      (insert "* H1\n** H2\n*** H3\n**** H4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (mapcar (lambda (h)
                  (org-export-low-level-p h info))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ── Export: get-caption ──────────────────────────────────────────────

#[test]
fn upstream_ox_get_caption_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((#(\"long caption\" 0 12 (:parent (#(\"long caption\" 0 12 (:parent #5)))))) (#(\"short\" 0 5 (:parent (#(\"short\" 0 5 (:parent #5))))))) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     ;; Short and long caption.
     (with-temp-buffer
       (org-mode)
       (insert "#+CAPTION[short]: long caption\n| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
         (list (org-export-get-caption table)
               (org-export-get-caption table t))))
     ;; No caption.
     (with-temp-buffer
       (org-mode)
       (insert "| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
         (org-export-get-caption table))))))"##,
        expect,
    );
}
