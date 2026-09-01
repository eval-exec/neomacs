//! Strong combo-complex-74 oracle tests — absolute final probes:
//! org-element-interpret-data serial string (full roundtrip),
//! org-babel with org-babel-map-src-blocks, org-agenda with
//! org-agenda-check-type, org-export with org-export-insert-
//! image-links, org-clock with org-clock-timestamps-down/up,
//! org-goto-interface, org-babel with :results replace cycle,
//! and org-cycle with org-cycle-content-optimization.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo74_element_interpret_serial_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:s1-has-TODO 2) (:s1-has-star 0) (:s2-length t) (:stable t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-element)
  ;; build, interpret as serial string, reparse
  (let* ((data (org-element-create 'org-data nil
                 (org-element-create 'headline '(:level 1 :raw-value "Roundtrip" :todo-keyword "TODO")
                   (org-element-create 'section nil
                     (org-element-create 'paragraph nil
                       (org-element-create 'bold nil "B")
                       " and "
                       (org-element-create 'italic nil "I"))))))
         (s1 (substring-no-properties (org-element-interpret-data data)))
         (r '()))
    (push (list :s1-has-TODO (string-match-p "TODO" s1)) r)
    (push (list :s1-has-star (string-match-p "\\`\\*" s1)) r)
    ;; reparse s1 as serial and interpret again
    (let* ((data2 (with-temp-buffer (org-mode) (insert s1) (goto-char (point-min))
                    (org-element-parse-buffer)))
           (s2 (substring-no-properties (org-element-interpret-data data2))))
      (push (list :s2-length (> (length s2) 0)) r)
      (push (list :stable (string= s1 s2)) r))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo74_babel_map_src_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:map-src-fbound t) (:src-langs (\"emacs-lisp\" \"emacs-lisp\" \"sh\")) (:src-block-count 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-core)
  (insert "#+begin_src emacs-lisp\n1\n#+end_src\n\n")
  (insert "#+begin_src emacs-lisp\n2\n#+end_src\n\n")
  (insert "#+begin_src sh\necho 3\n#+end_src\n")
  (let ((r '()))
    (push (list :map-src-fbound (fboundp 'org-babel-map-src-blocks)) r)
    ;; org-babel-map-src-blocks
    (let ((langs '()))
      (condition-case nil
          (when (fboundp 'org-babel-map-src-blocks)
            (org-babel-map-src-blocks nil
              (push (nth 0 (org-babel-get-src-block-info)) langs)))
        (error nil))
      (push (list :src-langs (nreverse langs)) r))
    (push (list :src-block-count (length (org-element-map (org-element-parse-buffer) 'src-block #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo74_agenda_check_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:check-type-fbound t :agenda-type-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda)
  (list
   :check-type-fbound (fboundp 'org-agenda-check-type)
   :agenda-type-fbound (boundp 'org-agenda-type)
   ))"##,
        expect,
    );
}

#[test]
fn combo74_export_insert_image_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:image-links-fbound t :data-fbound t :string-as-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox)
  (list
   :image-links-fbound (fboundp 'org-export-insert-image-links)
   :data-fbound (fboundp 'org-export-data)
   :string-as-fbound (fboundp 'org-export-string-as)
   ))"##,
        expect,
    );
}

#[test]
fn combo74_clock_timestamps_adjust() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:ts-up-fbound t) (:ts-down-fbound t) (:after-up \"CLOCK: [2024-01-01 Mon 10:00]--[2024-01-01 Mon 11:00] =>  1:00\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock)
  (insert "CLOCK: [2024-01-01 Mon 10:00]--[2024-01-01 Mon 11:00] =>  1:00\n")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :ts-up-fbound (fboundp 'org-clock-timestamps-up)) r)
    (push (list :ts-down-fbound (fboundp 'org-clock-timestamps-down)) r)
    ;; try adjusting up
    (condition-case nil
        (progn (org-clock-timestamps-up 1)
               (push (list :after-up (buffer-string)) r))
      (error (push (list :up-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo74_org_goto_interface() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:goto-fbound t :location-fbound t :ret-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-goto)
  (list
   :goto-fbound (fboundp 'org-goto)
   :location-fbound (fboundp 'org-goto-location)
   :ret-fbound (fboundp 'org-goto-ret)
   ))"##,
        expect,
    );
}

#[test]
fn combo74_babel_results_replace_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (42 (:after1 \"#+begin_src emacs-lisp :results value replace\\n42\\n#+end_src\\n\\n#+RESULTS:\\n: 42\\n\") 99 (:after2 \"#+begin_src emacs-lisp :results value replace\\n99\\n#+end_src\\n\\n#+RESULTS:\\n: 99\\n\") (:result-count 0))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results value replace\n42\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src")
      ;; execute once
      (push (org-babel-execute-src-block) r)
      (push (list :after1 (buffer-string)) r)
      ;; change the value in the block
      (goto-char (point-min))
      (search-forward "42") (replace-match "99")
      ;; execute again (should replace old result)
      (goto-char (point-min)) (search-forward "#+begin_src")
      (push (org-babel-execute-src-block) r)
      (push (list :after2 (buffer-string)) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo74_org_comment_block_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:comment-count 1) (:para-count 1) (:comment-value \"Hidden content *bold*.\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
  (insert "#+BEGIN_COMMENT\nHidden content *bold*.\n#+END_COMMENT\n\nVisible.\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (comments (org-element-map tree 'comment-block #'identity))
           (paragraphs (org-element-map tree 'paragraph #'identity)))
      (push (list :comment-count (length comments)) r)
      (push (list :para-count (length paragraphs)) r)
      ;; comment block value
      (when (car comments)
        (push (list :comment-value (org-element-property :value (car comments))) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo74_org_subscript_superscript_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:sub-count 3) (:sup-count 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
  (let ((org-use-sub-superscripts t))
    (insert "H_{2}O and x^{2} and a_b and a^{b}_{c}.\n")
    (goto-char (point-min))
    (let* ((tree (org-element-parse-buffer))
           (subs (org-element-map tree 'subscript #'identity))
           (sups (org-element-map tree 'superscript #'identity))
           (r '()))
      (push (list :sub-count (length subs)) r)
      (push (list :sup-count (length sups)) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo74_org_hierarchy_full_extract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument plistp [10 10 16 23 23 0 (:title) section nil nil nil 18 21 3 #<killed buffer> nil nil (headline (:standard-properties [5 5 10 29 29 0 (:title) section nil nil nil 12 27 2 #<killed buffer> nil nil (headline (:standard-properties [1 1 5 34 34 0 (:title) first-section nil nil nil 7 32 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 38 38 0 nil org-data nil nil nil 3 38 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #5 (headline (:standard-properties [34 34 nil nil 38 0 (:title) nil nil nil nil nil nil 1 #<killed buffer> nil nil #8] :pre-blank 0 :raw-value \"G\" :title (#(\"G\" 0 1 (:parent (headline (:standard-properties [34 34 nil nil 38 0 (:title) nil nil nil nil nil nil 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 38 38 0 nil org-data nil nil nil 3 38 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) (headline (:standard-properties [1 1 5 34 34 0 (:title) first-section nil nil nil 7 32 1 #<killed buffer> nil nil #17] :pre-blank 0 :raw-value \"A\" :title (#(\"A\" 0 1 (:parent #18))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [5 5 10 29 29 0 (:title) section nil nil nil 12 27 2 #<killed buffer> nil nil #18] :pre-blank 0 :raw-value \"B\" :title (#(\"B\" 0 1 (:parent #19))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [10 10 16 23 23 0 (:title) section nil nil nil 18 21 3 #<killed buffer> nil nil #19] :pre-blank 0 :raw-value \"C\" :title (#(\"C\" 0 1 (:parent #20))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [16 16 nil nil 23 0 (:title) section nil nil nil nil nil 4 #<killed buffer> nil nil #20] :pre-blank 0 :raw-value \"D\" :title (#(\"D\" 0 1 (:parent #21))) :level 4 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) (headline (:standard-properties [23 23 nil nil 29 0 (:title) nil nil nil nil nil nil 3 #<killed buffer> nil nil #19] :pre-blank 0 :raw-value \"E\" :title (#(\"E\" 0 1 (:parent #20))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) (headline (:standard-properties [29 29 nil nil 34 0 (:title) nil nil nil nil nil nil 2 #<killed buffer> nil nil #18] :pre-blank 0 :raw-value \"F\" :title (#(\"F\" 0 1 (:parent #19))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) #14)] :pre-blank 0 :raw-value \"G\" :title (#(\"G\" 0 1 (:parent #14))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"A\" :title (#(\"A\" 0 1 (:parent (headline (:standard-properties [1 1 5 34 34 0 (:title) first-section nil nil nil 7 32 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 38 38 0 nil org-data nil nil nil 3 38 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #10 (headline (:standard-properties [34 34 nil nil 38 0 (:title) nil nil nil nil nil nil 1 #<killed buffer> nil nil #13] :pre-blank 0 :raw-value \"G\" :title (#(\"G\" 0 1 (:parent #14))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"A\" :title (#(\"A\" 0 1 (:parent #10))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [5 5 10 29 29 0 (:title) section nil nil nil 12 27 2 #<killed buffer> nil nil #10] :pre-blank 0 :raw-value \"B\" :title (#(\"B\" 0 1 (:parent #11))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [10 10 16 23 23 0 (:title) section nil nil nil 18 21 3 #<killed buffer> nil nil #11] :pre-blank 0 :raw-value \"C\" :title (#(\"C\" 0 1 (:parent #12))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [16 16 nil nil 23 0 (:title) section nil nil nil nil nil 4 #<killed buffer> nil nil #12] :pre-blank 0 :raw-value \"D\" :title (#(\"D\" 0 1 (:parent #13))) :level 4 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) (headline (:standard-properties [23 23 nil nil 29 0 (:title) nil nil nil nil nil nil 3 #<killed buffer> nil nil #11] :pre-blank 0 :raw-value \"E\" :title (#(\"E\" 0 1 (:parent #12))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) (headline (:standard-properties [29 29 nil nil 34 0 (:title) nil nil nil nil nil nil 2 #<killed buffer> nil nil #10] :pre-blank 0 :raw-value \"F\" :title (#(\"F\" 0 1 (:parent #11))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) #2 (headline (:standard-properties [29 29 nil nil 34 0 (:title) nil nil nil nil nil nil 2 #<killed buffer> nil nil #5] :pre-blank 0 :raw-value \"F\" :title (#(\"F\" 0 1 (:parent (headline (:standard-properties [29 29 nil nil 34 0 (:title) nil nil nil nil nil nil 2 #<killed buffer> nil nil (headline (:standard-properties [1 1 5 34 34 0 (:title) first-section nil nil nil 7 32 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 38 38 0 nil org-data nil nil nil 3 38 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #14 (headline (:standard-properties [34 34 nil nil 38 0 (:title) nil nil nil nil nil nil 1 #<killed buffer> nil nil #17] :pre-blank 0 :raw-value \"G\" :title (#(\"G\" 0 1 (:parent #18))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"A\" :title (#(\"A\" 0 1 (:parent #14))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [5 5 10 29 29 0 (:title) section nil nil nil 12 27 2 #<killed buffer> nil nil #14] :pre-blank 0 :raw-value \"B\" :title (#(\"B\" 0 1 (:parent #15))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [10 10 16 23 23 0 (:title) section nil nil nil 18 21 3 #<killed buffer> nil nil #15] :pre-blank 0 :raw-value \"C\" :title (#(\"C\" 0 1 (:parent #16))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [16 16 nil nil 23 0 (:title) section nil nil nil nil nil 4 #<killed buffer> nil nil #16] :pre-blank 0 :raw-value \"D\" :title (#(\"D\" 0 1 (:parent #17))) :level 4 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) (headline (:standard-properties [23 23 nil nil 29 0 (:title) nil nil nil nil nil nil 3 #<killed buffer> nil nil #15] :pre-blank 0 :raw-value \"E\" :title (#(\"E\" 0 1 (:parent #16))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) #11)] :pre-blank 0 :raw-value \"F\" :title (#(\"F\" 0 1 (:parent #11))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"B\" :title (#(\"B\" 0 1 (:parent (headline (:standard-properties [5 5 10 29 29 0 (:title) section nil nil nil 12 27 2 #<killed buffer> nil nil (headline (:standard-properties [1 1 5 34 34 0 (:title) first-section nil nil nil 7 32 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 38 38 0 nil org-data nil nil nil 3 38 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #10 (headline (:standard-properties [34 34 nil nil 38 0 (:title) nil nil nil nil nil nil 1 #<killed buffer> nil nil #13] :pre-blank 0 :raw-value \"G\" :title (#(\"G\" 0 1 (:parent #14))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"A\" :title (#(\"A\" 0 1 (:parent #10))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) #7 (headline (:standard-properties [29 29 nil nil 34 0 (:title) nil nil nil nil nil nil 2 #<killed buffer> nil nil #10] :pre-blank 0 :raw-value \"F\" :title (#(\"F\" 0 1 (:parent #11))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"B\" :title (#(\"B\" 0 1 (:parent #7))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [10 10 16 23 23 0 (:title) section nil nil nil 18 21 3 #<killed buffer> nil nil #7] :pre-blank 0 :raw-value \"C\" :title (#(\"C\" 0 1 (:parent #8))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [16 16 nil nil 23 0 (:title) section nil nil nil nil nil 4 #<killed buffer> nil nil #8] :pre-blank 0 :raw-value \"D\" :title (#(\"D\" 0 1 (:parent #9))) :level 4 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) (headline (:standard-properties [23 23 nil nil 29 0 (:title) nil nil nil nil nil nil 3 #<killed buffer> nil nil #7] :pre-blank 0 :raw-value \"E\" :title (#(\"E\" 0 1 (:parent #8))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties #1 :pre-blank 0 :raw-value \"C\" :title (#(\"C\" 0 1 (:parent (headline (:standard-properties [10 10 16 23 23 0 (:title) section nil nil nil 18 21 3 #<killed buffer> nil nil (headline (:standard-properties [5 5 10 29 29 0 (:title) section nil nil nil 12 27 2 #<killed buffer> nil nil (headline (:standard-properties [1 1 5 34 34 0 (:title) first-section nil nil nil 7 32 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 38 38 0 nil org-data nil nil nil 3 38 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #14 (headline (:standard-properties [34 34 nil nil 38 0 (:title) nil nil nil nil nil nil 1 #<killed buffer> nil nil #17] :pre-blank 0 :raw-value \"G\" :title (#(\"G\" 0 1 (:parent #18))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"A\" :title (#(\"A\" 0 1 (:parent #14))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) #11 (headline (:standard-properties [29 29 nil nil 34 0 (:title) nil nil nil nil nil nil 2 #<killed buffer> nil nil #14] :pre-blank 0 :raw-value \"F\" :title (#(\"F\" 0 1 (:parent #15))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"B\" :title (#(\"B\" 0 1 (:parent #11))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) #8 (headline (:standard-properties [23 23 nil nil 29 0 (:title) nil nil nil nil nil nil 3 #<killed buffer> nil nil #11] :pre-blank 0 :raw-value \"E\" :title (#(\"E\" 0 1 (:parent #12))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"C\" :title (#(\"C\" 0 1 (:parent #8))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [16 16 nil nil 23 0 (:title) section nil nil nil nil nil 4 #<killed buffer> nil nil #8] :pre-blank 0 :raw-value \"D\" :title (#(\"D\" 0 1 (:parent #9))) :level 4 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [16 16 nil nil 23 0 (:title) section nil nil nil nil nil 4 #<killed buffer> nil nil #3] :pre-blank 0 :raw-value \"D\" :title (#(\"D\" 0 1 (:parent (headline (:standard-properties [16 16 nil nil 23 0 (:title) section nil nil nil nil nil 4 #<killed buffer> nil nil (headline (:standard-properties [10 10 16 23 23 0 (:title) section nil nil nil 18 21 3 #<killed buffer> nil nil (headline (:standard-properties [5 5 10 29 29 0 (:title) section nil nil nil 12 27 2 #<killed buffer> nil nil (headline (:standard-properties [1 1 5 34 34 0 (:title) first-section nil nil nil 7 32 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 38 38 0 nil org-data nil nil nil 3 38 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #18 (headline (:standard-properties [34 34 nil nil 38 0 (:title) nil nil nil nil nil nil 1 #<killed buffer> nil nil #21] :pre-blank 0 :raw-value \"G\" :title (#(\"G\" 0 1 (:parent #22))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"A\" :title (#(\"A\" 0 1 (:parent #18))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) #15 (headline (:standard-properties [29 29 nil nil 34 0 (:title) nil nil nil nil nil nil 2 #<killed buffer> nil nil #18] :pre-blank 0 :raw-value \"F\" :title (#(\"F\" 0 1 (:parent #19))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"B\" :title (#(\"B\" 0 1 (:parent #15))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) #12 (headline (:standard-properties [23 23 nil nil 29 0 (:title) nil nil nil nil nil nil 3 #<killed buffer> nil nil #15] :pre-blank 0 :raw-value \"E\" :title (#(\"E\" 0 1 (:parent #16))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"C\" :title (#(\"C\" 0 1 (:parent #12))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) #9)] :pre-blank 0 :raw-value \"D\" :title (#(\"D\" 0 1 (:parent #9))) :level 4 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))))) :level 4 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) (headline (:standard-properties [23 23 nil nil 29 0 (:title) nil nil nil nil nil nil 3 #<killed buffer> nil nil #2] :pre-blank 0 :raw-value \"E\" :title (#(\"E\" 0 1 (:parent (headline (:standard-properties [23 23 nil nil 29 0 (:title) nil nil nil nil nil nil 3 #<killed buffer> nil nil (headline (:standard-properties [5 5 10 29 29 0 (:title) section nil nil nil 12 27 2 #<killed buffer> nil nil (headline (:standard-properties [1 1 5 34 34 0 (:title) first-section nil nil nil 7 32 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 38 38 0 nil org-data nil nil nil 3 38 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #14 (headline (:standard-properties [34 34 nil nil 38 0 (:title) nil nil nil nil nil nil 1 #<killed buffer> nil nil #17] :pre-blank 0 :raw-value \"G\" :title (#(\"G\" 0 1 (:parent #18))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"A\" :title (#(\"A\" 0 1 (:parent #14))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) #11 (headline (:standard-properties [29 29 nil nil 34 0 (:title) nil nil nil nil nil nil 2 #<killed buffer> nil nil #14] :pre-blank 0 :raw-value \"F\" :title (#(\"F\" 0 1 (:parent #15))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))] :pre-blank 0 :raw-value \"B\" :title (#(\"B\" 0 1 (:parent #11))) :level 2 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [10 10 16 23 23 0 (:title) section nil nil nil 18 21 3 #<killed buffer> nil nil #11] :pre-blank 0 :raw-value \"C\" :title (#(\"C\" 0 1 (:parent #12))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (headline (:standard-properties [16 16 nil nil 23 0 (:title) section nil nil nil nil nil 4 #<killed buffer> nil nil #12] :pre-blank 0 :raw-value \"D\" :title (#(\"D\" 0 1 (:parent #13))) :level 4 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))) #8)] :pre-blank 0 :raw-value \"E\" :title (#(\"E\" 0 1 (:parent #8))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil))))) :level 3 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil)))])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
  (insert "* A\n** B\n*** C\n**** D\n*** E\n** F\n* G\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (all-headlines (org-element-map tree 'headline #'identity)))
      (push (list :count (length all-headlines)) r)
      (push (list :levels (mapcar (lambda (h) (org-element-property :level h)) all-headlines)) r)
      ;; lineage of deepest
      (let ((deepest (car (org-element-map tree 'headline
                            (lambda (h) (when (= (org-element-property :level h) 4) h))))))
        (when deepest
          (push (list :deepest-lineage (mapcar (lambda (el) (org-element-property :raw-value el))
                                               (org-element-lineage deepest 'headline))) r))))
    (nreverse r)))"##,
        expect,
    );
}
