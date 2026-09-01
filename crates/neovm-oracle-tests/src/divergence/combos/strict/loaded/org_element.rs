//! Strict combo oracle probes, batch 47: org-mode / org-element parsing via
//! assert_oracle_parity_with_load. org is the largest commonly-used library;
//! org-element-parse-buffer is intricate. These attempt to load org/org.el
//! and org/org-element.el and parse a small document.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_i4_org_mode_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (org-mode t org-mode)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (org-mode)
  (list major-mode
        (boundp 'org-element--cache)
        (derived-mode-p 'org-mode)))
"##,
        &["org/org.el"],
        expect,
    );
}

#[test]
fn div_i4_org_element_parse_headings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"Heading 1\" \"Sub heading\") 2 28)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "* Heading 1\n** Sub heading\nSome text.\n")
  (org-mode)
  (let ((parsed (org-element-parse-buffer)))
    (list (org-element-map parsed 'headline
            (lambda (h) (org-element-property :raw-value h)))
          (length (org-element-map parsed 'headline 'identity))
          (org-element-map parsed 'paragraph
            (lambda (p) (org-element-property :begin p)) nil t))))
"##,
        &["org/org.el", "org/org-element.el"],
        expect,
    );
}

#[test]
fn div_i4_org_element_parse_link_and_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"//example.com\") 2 (plain-list (:standard-properties [44 44 44 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) (section (:standard-properties [1 1 1 66 66 0 nil first-section nil nil nil 1 66 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 66 66 0 nil org-data nil nil nil 3 66 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #4)]) (paragraph (:standard-properties [1 1 1 44 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil #4]) #(\"Text with \" 0 10 (:parent (paragraph (:standard-properties [1 1 1 44 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 66 66 0 nil first-section nil nil nil 1 66 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 66 66 0 nil org-data nil nil nil 3 66 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8 (plain-list (:standard-properties [44 44 44 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #11] :type unordered) (item (:standard-properties [44 44 46 55 55 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #12] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [46 46 46 55 55 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #13]) #(\"item one\\n\" 0 9 (:parent #14)))) (item (:standard-properties [55 55 57 66 66 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #12] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [57 57 57 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #13]) #(\"item two\\n\" 0 9 (:parent #14))))))]) #(\"Text with \" 0 10 (:parent #8)) (link (:standard-properties [11 nil 34 40 42 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #8] :type \"https\" :type-explicit-p t :path \"//example.com\" :format bracket :raw-link \"https://example.com\" :application nil :search-option nil) #(\"a link\" 0 6 (:parent #9))) #(\".\\n\" 0 2 (:parent #8))))) (link (:standard-properties [11 nil 34 40 42 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #5] :type \"https\" :type-explicit-p t :path \"//example.com\" :format bracket :raw-link \"https://example.com\" :application nil :search-option nil) #(\"a link\" 0 6 (:parent (link (:standard-properties [11 nil 34 40 42 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 44 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 66 66 0 nil first-section nil nil nil 1 66 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 66 66 0 nil org-data nil nil nil 3 66 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #15)]) #12 (plain-list (:standard-properties [44 44 44 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #15] :type unordered) (item (:standard-properties [44 44 46 55 55 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #16] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [46 46 46 55 55 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #17]) #(\"item one\\n\" 0 9 (:parent #18)))) (item (:standard-properties [55 55 57 66 66 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #16] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [57 57 57 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #17]) #(\"item two\\n\" 0 9 (:parent #18))))))]) #(\"Text with \" 0 10 (:parent #12)) #9 #(\".\\n\" 0 2 (:parent #12)))] :type \"https\" :type-explicit-p t :path \"//example.com\" :format bracket :raw-link \"https://example.com\" :application nil :search-option nil) #(\"a link\" 0 6 (:parent #9)))))) #(\".\\n\" 0 2 (:parent (paragraph (:standard-properties [1 1 1 44 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 66 66 0 nil first-section nil nil nil 1 66 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 66 66 0 nil org-data nil nil nil 3 66 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #11)]) #8 (plain-list (:standard-properties [44 44 44 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #11] :type unordered) (item (:standard-properties [44 44 46 55 55 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #12] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [46 46 46 55 55 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #13]) #(\"item one\\n\" 0 9 (:parent #14)))) (item (:standard-properties [55 55 57 66 66 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #12] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [57 57 57 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #13]) #(\"item two\\n\" 0 9 (:parent #14))))))]) #(\"Text with \" 0 10 (:parent #8)) (link (:standard-properties [11 nil 34 40 42 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #8] :type \"https\" :type-explicit-p t :path \"//example.com\" :format bracket :raw-link \"https://example.com\" :application nil :search-option nil) #(\"a link\" 0 6 (:parent #9))) #(\".\\n\" 0 2 (:parent #8)))))) #1)] :type unordered) (item (:standard-properties [44 44 46 55 55 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #1] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [46 46 46 55 55 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2]) #(\"item one\\n\" 0 9 (:parent (paragraph (:standard-properties [46 46 46 55 55 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (item (:standard-properties [44 44 46 55 55 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) (plain-list (:standard-properties [44 44 44 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) (section (:standard-properties [1 1 1 66 66 0 nil first-section nil nil nil 1 66 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 66 66 0 nil org-data nil nil nil 3 66 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #15)]) (paragraph (:standard-properties [1 1 1 44 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil #15]) #(\"Text with \" 0 10 (:parent #16)) (link (:standard-properties [11 nil 34 40 42 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #16] :type \"https\" :type-explicit-p t :path \"//example.com\" :format bracket :raw-link \"https://example.com\" :application nil :search-option nil) #(\"a link\" 0 6 (:parent #17))) #(\".\\n\" 0 2 (:parent #16))) #12)] :type unordered) #9 (item (:standard-properties [55 55 57 66 66 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #12] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [57 57 57 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #13]) #(\"item two\\n\" 0 9 (:parent #14)))))] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) #6)]) #(\"item one\\n\" 0 9 (:parent #6))))))) (item (:standard-properties [55 55 57 66 66 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #1] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [57 57 57 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #2]) #(\"item two\\n\" 0 9 (:parent (paragraph (:standard-properties [57 57 57 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (item (:standard-properties [55 55 57 66 66 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) (plain-list (:standard-properties [44 44 44 66 66 0 nil nil nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) (section (:standard-properties [1 1 1 66 66 0 nil first-section nil nil nil 1 66 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 66 66 0 nil org-data nil nil nil 3 66 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #15)]) (paragraph (:standard-properties [1 1 1 44 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil #15]) #(\"Text with \" 0 10 (:parent #16)) (link (:standard-properties [11 nil 34 40 42 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #16] :type \"https\" :type-explicit-p t :path \"//example.com\" :format bracket :raw-link \"https://example.com\" :application nil :search-option nil) #(\"a link\" 0 6 (:parent #17))) #(\".\\n\" 0 2 (:parent #16))) #12)] :type unordered) (item (:standard-properties [44 44 46 55 55 0 (:tag) item nil nil nil nil nil nil #<killed buffer> nil ((44 0 \"- \" nil nil nil 55) (55 0 \"- \" nil nil nil 66)) #12] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) (paragraph (:standard-properties [46 46 46 55 55 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #13]) #(\"item one\\n\" 0 9 (:parent #14)))) #9)] :bullet \"- \" :checkbox nil :counter nil :pre-blank 0 :tag nil) #6)]) #(\"item two\\n\" 0 9 (:parent #6)))))))))""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "Text with [[https://example.com][a link]].\n- item one\n- item two\n")
  (org-mode)
  (let ((parsed (org-element-parse-buffer)))
    (list (org-element-map parsed 'link
            (lambda (l) (org-element-property :path l)))
          (length (org-element-map parsed 'item 'identity))
          (org-element-map parsed 'plain-list 'identity nil t))))
"##,
        &["org/org.el", "org/org-element.el"],
        expect,
    );
}

#[test]
fn div_i4_org_element_property_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"my-id\"""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "* Heading\n:PROPERTIES:\n:CUSTOM_ID: my-id\n:END:\nBody.\n")
  (org-mode)
  (let ((parsed (org-element-parse-buffer)))
    (org-element-map parsed 'headline
      (lambda (h) (org-element-property :CUSTOM_ID h)) nil t)))
"##,
        &["org/org.el", "org/org-element.el"],
        expect,
    );
}
