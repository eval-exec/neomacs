//! Strong integration-deep oracle tests — cross-feature interactions.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Integration: table + formula + export
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_table_formula_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Sales\n| Product | Q1 | Q2 | Total |\n|---------+----+----+-------|\n| A | 100 | 150 | |\n| B | 200 | 250 | |\n| C | 300 | 350 | |\n|---------+----+----+-------|\n| Sum | 600 | 750 | |\n#+TBLFM: $4=$2+$3::@5$2=vsum(@2..@4)::@5$3=vsum(@2..@4)::@5$4=vsum(@2..@4)")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil))
         (data (org-table-to-lisp)))
    (list (plist-get info :title) data)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: headline + property + clock
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_headline_property_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-sum-current-entry)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\n:PROPERTIES:\n:EFFORT: 2:00\n:CATEGORY: work\n:END:\n:LOGBOOK:\nCLOCK: [2026-01-15 10:00]--[2026-01-15 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (let ((todo (org-get-todo-state))
        (effort (org-entry-get nil "EFFORT"))
        (category (org-entry-get nil "CATEGORY"))
        (clocked (org-clock-sum-current-entry)))
    (list todo effort category clocked)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: link + export + attributes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_link_export_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"image.png\" (((#(\"My image\" 0 8 (:parent (#(\"My image\" 0 8 (:parent #7)))))))) (\":width 300px\") nil) (\"other.png\" (((#(\"Other\" 0 5 (:parent (#(\"Other\" 0 5 (:parent #7)))))))) nil (\":width 0.5\\\\textwidth\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: My image\n#+ATTR_HTML: :width 300px\n#+NAME: fig1\n[[file:image.png]]\n\n#+CAPTION: Other\n#+ATTR_LATEX: :width 0.5\\textwidth\n[[file:other.png]]")
  (let* ((tree (org-element-parse-buffer))
         (links (org-element-map tree 'link
                  (lambda (l)
                    (let ((p (org-element-property :parent l)))
                      (list (org-element-property :path l)
                            (org-element-property :caption p)
                            (org-element-property :attr_html p)
                            (org-element-property :attr_latex p)))))))
    links))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: todo + tag + property + planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_todo_tag_property_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Important :work:urgent:\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n:PROPERTIES:\n:EFFORT: 3:00\n:ASSIGNEE: Alice\n:END:")
  (goto-char (point-min))
  (list (org-get-todo-state)
        (org-get-priority (char-after))
        (org-get-tags nil t)
        (org-entry-get nil "SCHEDULED")
        (org-entry-get nil "DEADLINE")
        (org-entry-get nil "EFFORT")
        (org-entry-get nil "ASSIGNEE")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: list + checkbox + statistics + export
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_list_checkbox_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((#(\"Task List\" 0 9 (:parent (#(\"Task List\" 0 9 (:parent #4)))))) \"#+TITLE: Task List\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Task List\n* Project [%]\n- [ ] Task 1\n- [X] Task 2\n- [ ] Task 3\n  - [X] Subtask 3.1\n  - [ ] Subtask 3.2")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil))
         (h (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (list (plist-get info :title) h)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: footnote + link + inline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_footnote_link_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"1\") (\"link\" \"link2\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] with *bold* and /italic/ and [[link][desc]].\n\n[fn:1] Footnote with *markup* and [[link2][desc2]].")
  (let* ((tree (org-element-parse-buffer))
         (footnotes (org-element-map tree 'footnote-reference
                      (lambda (fn) (org-element-property :label fn))))
         (links (org-element-map tree 'link
                  (lambda (l) (org-element-property :path l))))
         (bold (org-element-map tree 'bold
                 (lambda (b) (org-element-property :value b)))))
    (list footnotes links bold)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: block + result + link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_block_result_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((\"calc\" \"emacs-lisp\")) (\"3\") (\"calc\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+NAME: calc\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n\n#+RESULTS: calc\n: 3\n\nSee [[calc][the calculation]].")
  (let* ((tree (org-element-parse-buffer))
         (blocks (org-element-map tree 'src-block
                   (lambda (b)
                     (list (org-element-property :name b)
                           (org-element-property :language b)))))
         (results (org-element-map tree 'fixed-width
                    (lambda (f) (org-element-property :value f))))
         (links (org-element-map tree 'link
                  (lambda (l) (org-element-property :path l)))))
    (list blocks results links)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: drawer + property + visibility
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_drawer_property_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:VAR: val\n:END:\n:LOGBOOK:\n- Note\n:END:\nBody")
  (goto-char (point-min))
  (org-cycle-hide-drawers 'overview)
  (let ((props-hidden (get-char-property (search-forward "VAR") 'invisible))
        (log-hidden (get-char-property (search-forward "Note") 'invisible)))
    (list props-hidden log-hidden)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: sparse tree + property + tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_sparse_tree_property_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Task 1\" \"Task 2\" \"Task 3\" \"WAITING Task 4\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task 1 :work:\n* DONE Task 2 :personal:\n* TODO Task 3 :work:\n* WAITING Task 4")
  (goto-char (point-min))
  (org-match-sparse-tree nil "work+TODO=\"TODO\"")
  (let ((v '()) (h '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((hd (org-get-heading t t t t)))
        (when hd
          (if (get-char-property (point) 'invisible)
              (push hd h) (push hd v))))
      (forward-line))
    (list (nreverse v) (nreverse h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: clock + effort + property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_clock_effort_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-sum-current-entry)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\n:PROPERTIES:\n:EFFORT: 2:00\n:CATEGORY: work\n:END:\n:LOGBOOK:\nCLOCK: [2026-01-15 10:00]--[2026-01-15 11:30] =>  1:30\nCLOCK: [2026-01-16 14:00]--[2026-01-16 15:00] =>  1:00\n:END:")
  (goto-char (point-min))
  (list (org-entry-get nil "EFFORT")
        (org-entry-get nil "CATEGORY")
        (org-clock-sum-current-entry)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: headline + timestamp + planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_headline_timestamp_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Meeting <2026-01-15 Wed>\" (timestamp (:standard-properties [39 nil nil nil 61 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-14 Tue 10:00>\" :year-start 2026 :month-start 1 :day-start 14 :hour-start 10 :minute-start 0 :year-end 2026 :month-end 1 :day-end 14 :hour-end 10 :minute-end 0)) nil ((active 2026 15) (active 2026 16) (active 2026 17)))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Meeting <2026-01-15 Wed>\nSCHEDULED: <2026-01-14 Tue 10:00>\nDEADLINE: <2026-01-16 Thu 17:00>\nBody with <2026-01-17 Fri> date")
  (let* ((tree (org-element-parse-buffer))
         (headline (car (org-element-map tree 'headline (lambda (h) h))))
         (planning (car (org-element-map (org-element-contents headline) 'planning
                         (lambda (p) p))))
         (timestamps (org-element-map tree 'timestamp
                       (lambda (ts)
                         (list (org-element-property :type ts)
                               (org-element-property :year-start ts)
                               (org-element-property :day-start ts))))))
    (list (org-element-property :raw-value headline)
          (org-element-property :scheduled planning)
          (org-element-property :deadline planning)
          timestamps)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: table + list + block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_table_list_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((2) (2) (\"emacs-lisp\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| A | B |\n| 1 | 2 |\n\n- Item 1\n- Item 2\n\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (tables (org-element-map tree 'table
                   (lambda (t) (length (org-element-contents t)))))
         (lists (org-element-map tree 'plain-list
                  (lambda (l) (length (org-element-contents l)))))
         (blocks (org-element-map tree 'src-block
                   (lambda (b) (org-element-property :language b)))))
    (list tables lists blocks)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: export + filter + attribute
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_export_filter_attribute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test\" 0 4 (:parent (#(\"Test\" 0 4 (:parent #4)))))) (\"Heading\") ((\"emacs-lisp\" (\":options [fragile]\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+ATTR_HTML: :id main\n* Heading\n#+ATTR_LATEX: :options [fragile]\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil))
         (headlines (org-element-map tree 'headline
                      (lambda (h) (org-element-property :raw-value h))))
         (blocks (org-element-map tree 'src-block
                   (lambda (b)
                     (list (org-element-property :language b)
                           (org-element-property :attr_latex b))))))
    (list (plist-get info :title) headlines blocks)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: element + deferred + chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_element_deferred_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:type headline :todo \"TODO\" :pri 65 :tags (\"tag\") :var \"val\") (:type headline :todo \"DONE\" :pri 66 :tags (\"newtag\") :var \"newval\" :title \"Changed\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Test :tag:\n:PROPERTIES:\n:VAR: val\n:END:\nBody")
  (goto-char (point-min))
  (let* ((el (org-element-at-point))
         (p1 (list :type (org-element-type el)
                   :todo (org-element-property :todo-keyword el)
                   :pri (org-element-property :priority el)
                   :tags (org-element-property :tags el)
                   :var (org-entry-get nil "VAR"))))
    (org-todo 'right)
    (org-priority 'down)
    (org-set-tags '("newtag"))
    (org-entry-put nil "VAR" "newval")
    (org-edit-headline "Changed")
    (let* ((el2 (org-element-at-point))
           (p2 (list :type (org-element-type el2)
                     :todo (org-element-property :todo-keyword el2)
                     :pri (org-element-property :priority el2)
                     :tags (org-element-property :tags el2)
                     :var (org-entry-get nil "VAR")
                     :title (org-element-property :raw-value el2))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: multi-buffer + shared state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_multi_buffer_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"A1\") (\"B\" \"B1\" \"B2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((r '()))
  (with-temp-buffer
    (org-mode)
    (insert "* A\n** A1\nBodyA")
    (push (org-element-map (org-element-parse-buffer) 'headline
            (lambda (h) (org-element-property :raw-value h))) r))
  (with-temp-buffer
    (org-mode)
    (insert "* B\n** B1\n** B2\nBodyB")
    (push (org-element-map (org-element-parse-buffer) 'headline
            (lambda (h) (org-element-property :raw-value h))) r))
  (nreverse r))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: planning + repeaters + delays
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_planning_repeaters_delays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((cumulate 1 nil nil) (nil nil cumulate 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO W\nSCHEDULED: <2026-01-15 +1w -3d>\n* TODO M\nDEADLINE: <2026-01-20 +1m -1w>")
  (org-element-map (org-element-parse-buffer) 'planning
    (lambda (p)
      (let ((s (org-element-property :scheduled p))
            (d (org-element-property :deadline p)))
        (list (when s (org-element-property :repeater-type s))
              (when s (org-element-property :repeater-value s))
              (when d (org-element-property :repeater-type d))
              (when d (org-element-property :repeater-value d)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: block + switches + parameters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_block_switches_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((src-block \"emacs-lisp\" \"-n\" \":results value :exports both\") (example-block nil \"-n\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp -n :results value :exports both\n(+ 1 2)\n#+END_SRC\n\n#+BEGIN_EXAMPLE -n\nExample\n#+END_EXAMPLE")
  (let* ((tree (org-element-parse-buffer))
         (blocks (org-element-map tree '(src-block example-block)
                   (lambda (b)
                     (list (org-element-type b)
                           (org-element-property :language b)
                           (org-element-property :switches b)
                           (org-element-property :parameters b))))))
    blocks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: headline + all element types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_headline_all_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"TODO\" 65 (\"tag\") \"Title\" (section headline))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Title :tag:\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n:PROPERTIES:\n:VAR: val\n:END:\n:LOGBOOK:\n- Note\n:END:\nBody\n** Sub\n- List\n| tbl |\n#+BEGIN_SRC\n(+ 1)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h))))
         (ch (mapcar 'org-element-type (org-element-contents h))))
    (list (org-element-property :todo-keyword h)
          (org-element-property :priority h)
          (org-element-property :tags h)
          (org-element-property :raw-value h)
          ch)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: export with all options
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_export_all_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test\" 0 4 (:parent (#(\"Test\" 0 4 (:parent #4)))))) nil nil (\"H\") ((\"emacs-lisp\" (\":options [fragile]\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+OPTIONS: toc:nil num:nil\n#+ATTR_HTML: :id main\n* H\n#+ATTR_LATEX: :options [fragile]\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil))
         (hl (org-element-map tree 'headline
               (lambda (h) (org-element-property :raw-value h))))
         (bl (org-element-map tree 'src-block
               (lambda (b)
                 (list (org-element-property :language b)
                       (org-element-property :attr_latex b))))))
    (list (plist-get info :title)
          (plist-get info :with-toc)
          (plist-get info :with-numbers)
          hl bl)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: element hierarchy deep
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_element_hierarchy_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"L1\" 2) (2 \"L2a\" 2) (3 \"L3a\" 0) (3 \"L3b\" 0) (2 \"L2b\" 1) (3 \"L3c\" 2) (4 \"L4a\" 0) (4 \"L4b\" 0) (1 \"L1b\" 1) (2 \"L2c\" 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* L1\n** L2a\n*** L3a\n*** L3b\n** L2b\n*** L3c\n**** L4a\n**** L4b\n* L1b\n** L2c")
  (let* ((tree (org-element-parse-buffer))
         (s (org-element-map tree 'headline
              (lambda (h)
                (list (org-element-property :level h)
                      (org-element-property :raw-value h)
                      (length (org-element-contents h)))))))
    s))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: table complex formulas
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_table_complex_formulas() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range [nil 0 1 2 4] 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| A | 1 | 2 |\n| B | 3 | 4 |\n| C | 5 | 6 |\n|---+---+---|\n| Sum | 9 | 12 |\n#+TBLFM: $4=$2+$3::@5$2=vsum(@2..@4)::@5$3=vsum(@2..@4)")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (org-table-to-lisp))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: list with checkboxes and statistics
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_list_checkboxes_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* Task [66%]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task [%]\n- [X] a\n- [ ] b\n  - [X] b1\n  - [ ] b2\n- [X] c")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (buffer-substring-no-properties (line-beginning-position) (line-end-position)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: footnote with markup and links
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_footnote_markup_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"1\" \"2\") (\"1\" \"2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2]\n\n[fn:1] *bold* /italic/\n[fn:2] [[link][desc]]")
  (let* ((tree (org-element-parse-buffer))
         (fn (org-element-map tree 'footnote-reference
               (lambda (f) (org-element-property :label f))))
         (fd (org-element-map tree 'footnote-definition
               (lambda (d) (org-element-property :label d)))))
    (list fn fd)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: clock with effort and property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_clock_effort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-sum-current-entry)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\n:PROPERTIES:\n:EFFORT: 2:00\n:END:\n:LOGBOOK:\nCLOCK: [2026-01-15 10:00]--[2026-01-15 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (list (org-entry-get nil "EFFORT")
        (org-clock-sum-current-entry)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: link with search and radio
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_link_search_radio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"target\") ((\"file\" \"*heading\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<<<target>>>\nSee target and [[file:test.org::*heading][heading]]")
  (let* ((tree (org-element-parse-buffer))
         (targets (org-element-map tree 'radio-target
                    (lambda (rt) (org-element-property :value rt))))
         (links (org-element-map tree 'link
                  (lambda (l)
                    (list (org-element-property :type l)
                          (org-element-property :search-option l))))))
    (list targets links)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: planning with repeater and delay
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_planning_repeater_delay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (((cumulate 1 week) nil) (nil (cumulate 1 month)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO W\nSCHEDULED: <2026-01-15 Wed +1w -3d>\n* TODO M\nDEADLINE: <2026-01-20 Mon +1m -1w>")
  (let* ((tree (org-element-parse-buffer))
         (plan (org-element-map tree 'planning
                 (lambda (p)
                   (let ((s (org-element-property :scheduled p))
                         (d (org-element-property :deadline p)))
                     (list (when s (list (org-element-property :repeater-type s)
                                         (org-element-property :repeater-value s)
                                         (org-element-property :repeater-unit s)))
                           (when d (list (org-element-property :repeater-type d)
                                         (org-element-property :repeater-value d)
                                         (org-element-property :repeater-unit d)))))))))
    plan))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: block with switches and parameters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_block_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((src-block \"emacs-lisp\" \"-n\" \":results value :exports both\") (example-block nil \"-n\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp -n :results value :exports both\n(+ 1 2)\n#+END_SRC\n\n#+BEGIN_EXAMPLE -n\nExample\n#+END_EXAMPLE")
  (let* ((tree (org-element-parse-buffer))
         (blocks (org-element-map tree '(src-block example-block)
                   (lambda (b)
                     (list (org-element-type b)
                           (org-element-property :language b)
                           (org-element-property :switches b)
                           (org-element-property :parameters b))))))
    blocks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: headline with all elements
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_headline_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"TODO\" 65 (\"tag\") \"Title\" (section headline))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Title :tag:\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n:PROPERTIES:\n:VAR: val\n:END:\n:LOGBOOK:\n- Note\n:END:\nBody\n** Sub\n- List\n| tbl |\n#+BEGIN_SRC\n(+ 1)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h))))
         (ch (mapcar 'org-element-type (org-element-contents h))))
    (list (org-element-property :todo-keyword h)
          (org-element-property :priority h)
          (org-element-property :tags h)
          (org-element-property :raw-value h)
          ch)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: export with options and attributes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_export_opts_attr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"T\" 0 1 (:parent (#(\"T\" 0 1 (:parent #4)))))) nil (\"H\") ((\"emacs-lisp\" (\":options [f]\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n#+OPTIONS: toc:nil\n#+ATTR_HTML: :id m\n* H\n#+ATTR_LATEX: :options [f]\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil))
         (hl (org-element-map tree 'headline
               (lambda (h) (org-element-property :raw-value h))))
         (bl (org-element-map tree 'src-block
               (lambda (b)
                 (list (org-element-property :language b)
                       (org-element-property :attr_latex b))))))
    (list (plist-get info :title)
          (plist-get info :with-toc)
          hl bl)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: element hierarchy
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_element_hier() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* L1\n** L2a\n*** L3a\n*** L3b\n** L2b\n*** L3c\n**** L4a\n**** L4b\n* L1b\n** L2c")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h)
      (list (org-element-property :level h)
            (org-element-property :raw-value h)
            (length (org-element-contents h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: table formula alignment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_table_formula_align() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range [nil 0 1 2 4] 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | 1 | 2 |\n| b | 3 | 4 |\n| c | 5 | 6 |\n|---+---+---|\n| Sum | 9 | 12 |\n#+TBLFM: $4=$2+$3::@5$2=vsum(@2..@4)::@5$3=vsum(@2..@4)")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (org-table-to-lisp))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: list with all features
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_list_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* Task [66%]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task [%]\n- [X] a\n- [ ] b\n  - [X] b1\n  - [ ] b2\n- [X] c")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (buffer-substring-no-properties (line-beginning-position) (line-end-position)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: footnote all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_footnote_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"1\" \"2\") (\"1\" \"2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2]\n\n[fn:1] *bold* /italic/\n[fn:2] [[link][desc]]")
  (let* ((tree (org-element-parse-buffer))
         (fn (org-element-map tree 'footnote-reference
               (lambda (f) (org-element-property :label f))))
         (fd (org-element-map tree 'footnote-definition
               (lambda (d) (org-element-property :label d)))))
    (list fn fd)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: clock all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_clock_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-sum-current-entry)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\n:PROPERTIES:\n:EFFORT: 2:00\n:END:\n:LOGBOOK:\nCLOCK: [2026-01-15 10:00]--[2026-01-15 11:30] =>  1:30\n:END:")
  (goto-char (point-min))
  (list (org-entry-get nil "EFFORT")
        (org-clock-sum-current-entry)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: link all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_link_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"https\" \"//x\" \"https://x\") (\"file\" \"f\" \"file:f\") (\"id\" \"i\" \"id:i\") (\"elisp\" \"(+ 1)\" \"elisp:(+ 1)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "[[https://x][w]] [[file:f][f]] [[id:i][i]] [[elisp:(+ 1)][e]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l)
                      (org-element-property :path l)
                      (org-element-property :raw-link l)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: property all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_property_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"CATEGORY\" . \"???\") (\"B\" . \"2\") (\"A\" . \"1\")) ((\"CATEGORY\" . \"???\") (\"C\" . \"3\") (\"A\" . \"1\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:")
  (goto-char (point-min))
  (let ((p1 (org-entry-properties nil 'standard)))
    (org-entry-put nil "C" "3")
    (org-entry-delete nil "B")
    (list p1 (org-entry-properties nil 'standard))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: tag all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_tag_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"a\" \"b\") (\"c\" \"d\") (\"c\" \"d\" \"e\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H :a:b:")
  (goto-char (point-min))
  (let ((t1 (org-get-tags nil t)))
    (org-set-tags '("c" "d"))
    (let ((t2 (org-get-tags nil t)))
      (org-toggle-tag "e" 'on)
      (list t1 t2 (org-get-tags nil t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: priority all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_priority_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] H\n* TODO H2")
  (goto-char (point-min))
  (let ((p1 (org-get-priority (char-after))))
    (org-priority 'down)
    (let ((p2 (org-get-priority (char-after))))
      (forward-line)
      (org-priority ?B)
      (list p1 p2 (org-get-priority (char-after))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: todo all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_todo_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"TODO\" #(\"DONE\" 0 4 (org-todo-head \"TODO\")) nil #(\"TODO\" 0 4 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "TODO" "PROG" "DONE")))
  (insert "* TODO T")
  (goto-char (point-min))
  (let ((s '()))
    (dotimes (_ 3)
      (push (org-get-todo-state) s)
      (org-todo 'right))
    (push (org-get-todo-state) s)
    (nreverse s)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: visibility all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_visibility_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (let ((s '()))
    (org-set-startup-visibility 'overview)
    (push (get-char-property (search-forward "H2") 'invisible) s)
    (org-set-startup-visibility 'content)
    (push (get-char-property (search-forward "H2") 'invisible) s)
    (org-set-startup-visibility 'all)
    (push (get-char-property (search-forward "H2") 'invisible) s)
    (nreverse s)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: sparse dates
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_sparse_dates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"T1\" \"T2\" \"T3\" \"T4\") (\"T1\" \"T2\" \"T3\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\nSCHEDULED: <2026-01-15>\n* T2\nSCHEDULED: <2026-01-20>\n* T3\nSCHEDULED: <2026-02-01>\n* T4")
  (goto-char (point-min))
  (org-match-sparse-tree nil "SCHEDULED<=\"<2026-01-31>\"")
  (let ((v '()) (h '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((hd (org-get-heading t t t t)))
        (when hd
          (if (get-char-property (point) 'invisible) (push hd h) (push hd v))))
      (forward-line))
    (list (nreverse v) (nreverse h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: outline path
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_outline_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"P\" \"T1\" \"S1\") 4 \"SS1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P\n** T1\n*** S1\n**** SS1\n** T2")
  (goto-char (point-min))
  (search-forward "SS1")
  (list (org-get-outline-path) (org-current-level) (org-get-heading t t t t)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: refile targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_refile_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"P1\" \"P2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P1\n** T1\n* P2\n** T2")
  (mapcar 'car (org-refile-get-targets nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: agenda all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_agenda_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C")
  (org-map-entries
    (lambda () (list (org-get-heading t t t t) (org-get-todo-state)))
    nil 'file))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: colview all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_colview_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %PRIORITY\n* TODO [#A] T")
  (goto-char (point-min))
  (org-columns-get-format))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: entity radio all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_entity_radio_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\alpha \\\\beta\\n<<<t>>>\\nSee t\" \"\\\\alpha \\\\beta\\n<<<t>>>\\nSee t\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\alpha \\beta\n<<<t>>>\nSee t")
  (let ((b (buffer-string)))
    (org-toggle-pretty-entities)
    (list b (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: macro dynamic
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_macro_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Undefined Org macro: g; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: g H $1!\n{{{g(A)}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (list raw (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: structure template
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_structure_template() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-try-structure-completion)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<s")
  (org-try-structure-completion)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: comment fixed
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_comment_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"C\") (\"F\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# C\n: F\nN")
  (let* ((tree (org-element-parse-buffer))
         (c (org-element-map tree 'comment (lambda (c) (org-element-property :value c))))
         (f (org-element-map tree 'fixed-width (lambda (f) (org-element-property :value f)))))
    (list c f)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: pcomplete
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_pcomplete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\agr")
  (length (all-completions "\\ag" (pcomplete-entries))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: property inheritance
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_property_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"2\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+PROPERTY: V 1\n* L1\n:PROPERTIES:\n:V: 2\n:END:\n** L2\n*** L3")
  (goto-char (point-min))
  (search-forward "L3")
  (list (org-entry-get nil "V" 'inherit) (org-entry-get nil "V" nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration: hierarchy
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn int_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* L1\n** L2a\n*** L3a\n*** L3b\n** L2b\n* L1b")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h)
      (list (org-element-property :level h)
            (org-element-property :raw-value h)
            (length (org-element-contents h)))))"##,
        expect,
    );
}
