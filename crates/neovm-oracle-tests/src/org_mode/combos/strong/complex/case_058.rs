//! Strong combo-complex-58 oracle tests — extreme divergence probes:
//! org-babel with :cache and :results drawer, org-element-interpret-
//! data for all element types in sequence, org-map-entries with
//! region scope, org-export with date/author metadata extraction,
//! org-timestamp range calculations (days between), org-table
//! formula with conditional (if) expressions, org-footnote with
//! label collisions, org-cycle with plain lists, org-agenda-list
//! (non-interactive), and org-store-link with custom-id fallback.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo58_babel_cache_and_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (60 (:result-count 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results value :cache yes\n(+ 10 20 30)\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo58_map_entries_with_region_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:full (\"A\" \"A1\" \"A2\" \"B\" \"B1\" \"B2\")) (:region nil) (:tree (\"A\" \"A1\" \"A2\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\n** A2\n* B\n** B1\n** B2\n")
  (let ((r '()))
    ;; full buffer map
    (push (list :full (org-map-entries (lambda () (org-get-heading t t t t)))) r)
    ;; region scope: from A1 to B (inclusive)
    (goto-char (point-min))
    (search-forward "** A1") (beginning-of-line)
    (let ((start (point)))
      (search-forward "* B") (beginning-of-line)
      (let ((end (line-beginning-position)))
        (push (list :region (org-map-entries
                             (lambda () (org-get-heading t t t t))
                             nil 'region)) r)))
    ;; tree scope: only children of A
    (goto-char (point-min))
    (push (list :tree (org-map-entries
                       (lambda () (org-get-heading t t t t))
                       nil 'tree)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo58_timestamp_range_days_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:range-properties (2024 1 1 2024 1 10)) (:type active-range) (:seconds-diff 0) (:days-approx 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Event <2024-01-01 Mon>--<2024-01-10 Wed>\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (ts (car (org-element-map tree 'timestamp #'identity))))
      (when ts
        (let ((y1 (org-element-property :year-start ts))
              (m1 (org-element-property :month-start ts))
              (d1 (org-element-property :day-start ts))
              (y2 (org-element-property :year-end ts))
              (m2 (org-element-property :month-end ts))
              (d2 (org-element-property :day-end ts)))
          (push (list :range-properties (list y1 m1 d1 y2 m2 d2)) r)
          (push (list :type (org-element-property :type ts)) r)
          ;; use org-2ft to compute difference
          (let* ((t1-str (format "<%04d-%02d-%02d>" y1 m1 d1))
                 (t2-str (format "<%04d-%02d-%02d>" y2 m2 d2))
                 (ts1 (org-timestamp-from-string t1-str))
                 (ts2 (org-timestamp-from-string t2-str)))
            (condition-case nil
                (let ((seconds-diff (- (org-2ft ts2) (org-2ft ts1)))
                      (days (/ (- (org-2ft ts2) (org-2ft ts1)) 86400.0)))
                  (push (list :seconds-diff seconds-diff) r)
                  (push (list :days-approx (round days)) r))
              (error (push (list :calc-error t) r)))))))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo58_table_formula_if_conditional() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"Grade\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| Score | Grade |\n|-------+-------|\n")
  (insert "|    95 |       |\n|    72 |       |\n|    45 |       |\n|    60 |       |\n")
  (insert "#+TBLFM: $2=if($1>=90,\"A\",if($1>=80,\"B\",if($1>=70,\"C\",if($1>=60,\"D\",\"F\"))))\n")
  (let ((r '()))
    (goto-char (point-min))
    (org-table-recalculate t) (org-table-align)
    (push (list :after-recalc (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; get grades
    (goto-char (point-min)) (forward-line 1)
    (push (list :grade-95 (org-table-get "Grade" nil)) r)
    (forward-line)
    (push (list :grade-72 (org-table-get "Grade" nil)) r)
    (forward-line)
    (push (list :grade-45 (org-table-get "Grade" nil)) r)
    (forward-line)
    (push (list :grade-60 (org-table-get "Grade" nil)) r)
    ;; to-lisp
    (goto-char (point-min))
    (push (list :to-lisp (org-table-to-lisp)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo58_footnote_label_collisions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Ref[fn:1] and [fn:1] again.\n[fn:1] The definition.\n")
  ;; multiple refs with same label
  (let ((r '()))
    (push (list :init-refs (mapcar (lambda (fr) (org-element-property :label fr))
                                   (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    (push (list :init-defs (mapcar (lambda (fd) (org-element-property :label fd))
                                   (org-element-map (org-element-parse-buffer) 'footnote-definition #'identity))) r)
    ;; normalize
    (org-footnote-normalize 'sort)
    (push (list :after-normalize-refs (mapcar (lambda (fr) (org-element-property :label fr))
                                              (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo58_cycle_with_plain_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:heading headline) (:after-fold-invisible nil) (:after-children-invisible nil) (:after-subtree-invisible nil) (:after-show-headlines 1) (:after-show-items 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (let ((org-cycle-include-plain-lists t))
    (insert "* Heading\n- item a\n- item b\n  - nested i\n  - nested ii\n- item c\n")
    (let ((r '()))
      ;; cycle (folds heading content including lists)
      (goto-char (point-min))
      (push (list :heading (org-element-type (org-element-at-point))) r)
      (org-cycle)  ;; fold
      (push (list :after-fold-invisible (get-char-property (point) 'invisible)) r)
      (org-cycle)  ;; children
      (push (list :after-children-invisible (get-char-property (point) 'invisible)) r)
      (org-cycle)  ;; subtree
      (push (list :after-subtree-invisible (get-char-property (point) 'invisible)) r)
      ;; show all
      (org-show-all)
      (push (list :after-show-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
      (push (list :after-show-items (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo58_agenda_list_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:agenda-fbound t) (:get-day-fbound t) (:agenda-error t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-agenda)
  (insert "* TODO A :work:\nSCHEDULED: <2024-06-01 Sat>\n")
  (insert "* DONE B :home:\nCLOSED: [2024-05-15 Thu]\n")
  (insert "* TODO C :work:\nDEADLINE: <2024-06-15 Sat>\n")
  (let ((r '()))
    ;; org-agenda-list is interactive but we can try org-agenda-get-day-entries
    (condition-case nil
        (progn
          (push (list :agenda-fbound (fboundp 'org-agenda-list)) r)
          (push (list :get-day-fbound (fboundp 'org-agenda-get-day-entries)) r)
          (when (fboundp 'org-agenda-get-day-entries)
            (let* ((date (org-today))
                   (entries (org-agenda-get-day-entries
                             (buffer-file-name) date :todo)))
              (push (list :entry-count (length entries)) r)))
          ;; org-map-entries with todo filter (agenda-like)
          (push (list :todo-entries (org-map-entries
                                     (lambda () (list (org-get-heading t t t t)
                                                      (org-get-todo-state)
                                                      (org-get-tags)))
                                     "TODO=\"TODO\"")) r))
      (error (push (list :agenda-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo58_store_link_custom_id_fallback() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:link-created nil) (:link-has-custom-id nil) (:insert-error t) (:final-links nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ol)
  (require 'org-id)
  (let ((org-id-link-to-org-use-id t))
    (insert "* Target\n:PROPERTIES:\n:CUSTOM_ID: my-target\n:END:\nBody.\n")
    (let ((r '()))
      ;; store link on Target heading
      (goto-char (point-min))
      (let ((link (org-store-link nil)))
        (push (list :link-created (and link (stringp link))) r)
        (push (list :link-has-custom-id
                    (when (stringp link)
                      (or (string-match-p "my-target" link) (string-match-p "id:" link)))) r))
      ;; insert link at end
      (goto-char (point-max))
      (condition-case nil
          (progn (org-insert-link nil (nth 0 (car r)) "Link text")
                 (push (list :inserted t) r))
        (error (push (list :insert-error t) r)))
      (push (list :final-links (mapcar (lambda (l) (org-element-property :type l))
                                       (org-element-map (org-element-parse-buffer) 'link #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo58_export_metadata_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:title (#(\"Test Doc\" 0 8 (:parent (#(\"Test Doc\" 0 8 (:parent #5))))))) (:author (#(\"Alice<br>Bob\" 0 12 (:parent (#(\"Alice<br>Bob\" 0 12 (:parent #5))))))) (:date ((timestamp (:standard-properties [1 nil nil nil 17 0 nil nil nil nil nil nil nil nil #<buffer  *Org parse*> nil nil #2] :type active :range-type nil :raw-value \"<2024-06-15 Sat>\" :year-start 2024 :month-start 6 :day-start 15 :hour-start nil :minute-start nil :year-end 2024 :month-end 6 :day-end 15 :hour-end nil :minute-end nil)))) (:creator \"Emacs\") (:description nil) (:keywords nil) (:backends 5) (:ascii-transcoders nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox)
  (insert "#+TITLE: Test Doc\n#+AUTHOR: Alice<br>Bob\n")
  (insert "#+DATE: <2024-06-15 Sat>\n#+CREATOR: Emacs\n")
  (insert "#+DESCRIPTION: A test document.\n#+KEYWORDS: org, test\n")
  (insert "* Content\nBody.\n")
  (let* ((info (org-export-get-environment))
         (r '()))
    ;; metadata extraction
    (push (list :title (plist-get info :title)) r)
    (push (list :author (plist-get info :author)) r)
    (push (list :date (plist-get info :date)) r)
    (push (list :creator (plist-get info :creator)) r)
    (push (list :description (plist-get info :description)) r)
    (push (list :keywords (plist-get info :keywords)) r)
    ;; backend info
    (push (list :backends (length org-export-registered-backends)) r)
    ;; check transcode-table existence
    (let ((ascii-backend (assq 'ascii org-export-registered-backends)))
      (push (list :ascii-transcoders
                  (when ascii-backend
                    (sort (mapcar #'car (org-export-backend-transcoders ascii-backend))
                          #'string-lessp))) r))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo58_list_to_generic_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:item-count 6) (:list-count 3) (:list-types (unordered unordered ordered)) (:item-structure ((nil nil \"- item one\\n  - neste\") (nil nil \"  - nested 1a\\n\") (nil nil \"  - nested 1b\\n\") (nil nil \"- item two\\n  1. orde\") (nil nil \"  1. ordered a\\n\") (nil nil \"  2. ordered b\\n\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item one\n")
  (insert "  - nested 1a\n")
  (insert "  - nested 1b\n")
  (insert "- item two\n")
  (insert "  1. ordered a\n")
  (insert "  2. ordered b\n")
  (let ((r '()))
    ;; parse the list structure
    (let* ((tree (org-element-parse-buffer))
           (items (org-element-map tree 'item #'identity))
           (plain-lists (org-element-map tree 'plain-list #'identity)))
      (push (list :item-count (length items)) r)
      (push (list :list-count (length plain-lists)) r)
      (push (list :list-types (mapcar (lambda (pl) (org-element-property :type pl)) plain-lists)) r)
      ;; item structure (level, bullet type)
      (push (list :item-structure
                  (mapcar (lambda (i) (list (org-element-property :level i)
                                           (org-element-property :tag i)
                                           (substring-no-properties
                                            (or (buffer-substring-no-properties
                                                 (org-element-property :begin i)
                                                 (min (+ (org-element-property :begin i) 20)
                                                      (org-element-property :end i)))
                                                ""))))
                          items)) r))
    (nreverse r)))"##,
        expect,
    );
}
