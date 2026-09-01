use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_checkbox_statistics_nested_ctrl_c_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Project [2/3] [66%]\\n- [X] One\\n- [X] Two [2/2]\\n  - [X] Two A\\n  - [X] Two B\\n- [ ] Three\\n\" ((on \"One\") (on \"Two [2/2]\") (on \"Two A\") (on \"Two B\") (off \"Three\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "* Project [0/3] [0%]\n")
    (insert "- [ ] One\n")
    (insert "- [-] Two [1/2]\n")
    (insert "  - [X] Two A\n")
    (insert "  - [ ] Two B\n")
    (insert "- [ ] Three\n")
    (goto-char (point-min))
    (search-forward "One")
    (org-ctrl-c-ctrl-c)
    (search-forward "Two B")
    (org-ctrl-c-ctrl-c)
    (goto-char (point-min))
    (org-update-checkbox-count t)
    (list
     (buffer-substring-no-properties (point-min) (point-max))
     (org-element-map (org-element-parse-buffer) 'item
       (lambda (item)
         (list (org-element-property :checkbox item)
               (buffer-substring-no-properties
                (org-element-property :contents-begin item)
                (save-excursion
                  (goto-char (org-element-property :contents-begin item))
                  (line-end-position)))))))))"##,
        expect,
    );
}

#[test]
fn org_list_move_sort_cycle_bullet_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"- apple\\n  - child b\\n  - child a\\n- zebra\\n- mango\\n\" \"- apple\\n  - child b\\n  - child a\\n- mango\\n- zebra\\n\" \"1) apple\\n   - child b\\n   - child a\\n2) mango\\n3) zebra\\n\" (ordered (\"apple\" (unordered (\"child b\") (\"child a\"))) (\"mango\") (\"zebra\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "- zebra\n")
    (insert "- apple\n")
    (insert "  - child b\n")
    (insert "  - child a\n")
    (insert "- mango\n")
    (goto-char (point-min))
    (search-forward "apple")
    (beginning-of-line)
    (org-move-item-up)
    (let ((after-move
           (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (org-sort-list nil ?a)
      (let ((after-sort
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (org-cycle-list-bullet ?+)
        (list after-move
              after-sort
              (buffer-substring-no-properties (point-min) (point-max))
              (org-list-to-lisp))))))"##,
        expect,
    );
}

#[test]
fn org_list_to_generic_html_org_delete_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((ordered (\"[X] Alpha :: definition line\\ncontinuation\") (\"[ ] Beta\" (ordered (\"nested one\") (\"nested two\")))) t t \"1. [X] Alpha :: definition line\\n  continuation\\n1. [ ] Beta\\n  1. nested one\\n  1. nested two\" t \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "1. [X] Alpha :: definition line\n")
    (insert "   continuation\n")
    (insert "2. [ ] Beta\n")
    (insert "   1. nested one\n")
    (insert "   2. nested two\n")
    (goto-char (point-min))
    (let* ((as-lisp (org-list-to-lisp))
           (html (org-list-to-html as-lisp))
           (org (org-list-to-org as-lisp))
           (texinfo (org-list-to-texinfo as-lisp)))
      (org-list-to-lisp t)
      (list as-lisp
            (not (null (string-match-p "<ol" html)))
            (not (null (string-match-p "definition line" html)))
            org
            (not (null (string-match-p "@enumerate" texinfo)))
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_list_indent_outdent_checkbox_repair_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "- [ ] Parent [0/3]\n")
    (insert "- [X] A\n")
    (insert "- [ ] B\n")
    (insert "  - [ ] B child\n")
    (insert "- [ ] C\n")
    (goto-char (point-min))
    (search-forward "B")
    (beginning-of-line)
    (org-indent-item-tree)
    (let ((after-indent
           (buffer-substring-no-properties (point-min) (point-max))))
      (org-toggle-checkbox)
      (org-outdent-item-tree)
      (goto-char (point-min))
      (org-update-checkbox-count t)
      (org-list-repair)
      (let* ((struct (org-list-struct))
             (parents (org-list-parents-alist struct))
             (prevs (org-list-prevs-alist struct))
             (items (mapcar
                     (lambda (item)
                       (list (- item (point-min))
                             (org-list-get-parent item struct parents)
                             (org-list-get-item-number item struct prevs parents)
                             (org-list-get-children item struct parents)
                             (org-list-get-item-end item struct)))
                     (org-list-get-all-items (point-min) struct prevs))))
        (list after-indent
              items
              (org-list-to-lisp)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_insert_delete_move_description_items_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "- term A :: first\n")
    (insert "  continuation A\n")
    (insert "- term B :: second\n")
    (insert "- term C :: third\n")
    (goto-char (point-min))
    (search-forward "term B")
    (beginning-of-line)
    (org-insert-item)
    (insert "term inserted :: new")
    (let ((after-insert
           (buffer-substring-no-properties (point-min) (point-max))))
      (org-move-item-down)
      (let ((after-move
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "term C")
        (beginning-of-line)
        (let* ((struct (org-list-struct))
               (item (point)))
          (org-list-delete-item item struct))
        (list after-insert
              after-move
              (org-list-to-lisp)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_ordered_alpha_list_sort_renumber_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "a. Gamma\n")
    (insert "b. Alpha\n")
    (insert "   a. child two\n")
    (insert "   b. child one\n")
    (insert "c. Beta\n")
    (goto-char (point-min))
    (search-forward "Alpha")
    (beginning-of-line)
    (org-sort-list nil ?a)
    (let ((after-sort
           (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "child one")
      (beginning-of-line)
      (org-move-item-up)
      (let* ((struct (org-list-struct))
             (prevs (org-list-prevs-alist struct))
             (parents (org-list-parents-alist struct))
             (summary
              (mapcar
               (lambda (item)
                 (list (buffer-substring-no-properties
                        item (line-end-position))
                       (org-list-get-item-number item struct prevs parents)
                       (org-list-get-list-type item struct prevs)))
               (org-list-get-all-items (point-min) struct prevs))))
        (list after-sort
              summary
              (org-list-to-lisp)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_list_descriptive_generic_roundtrip_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Unknown :backend value\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "- [X] Term *A* :: First line\n")
    (insert "  continuation with =code=\n")
    (insert "  1. [@3] child three\n")
    (insert "  2. [ ] child off\n")
    (insert "- [-] Term B :: second line\n")
    (goto-char (point-min))
    (let* ((parsed (org-list-to-lisp))
           (generic
            (org-list-to-generic
             parsed
             (list :backend 'org
                   :raw t
                   :dstart (lambda (depth) (format "<dl depth=%d>" depth))
                   :dend "</dl>"
                   :ostart (lambda (depth) (format "<ol depth=%d>" depth))
                   :oend "</ol>"
                   :dtstart "<dt>"
                   :dtend "</dt>"
                   :ddstart "<dd>"
                   :ddend "</dd>"
                   :istart (lambda (type depth)
                             (format "<item type=%S depth=%d>" type depth))
                   :icount (lambda (type depth count)
                             (format "<item type=%S depth=%d count=%d>"
                                     type depth count))
                   :iend "</item>"
                   :isep "|"
                   :cbon "{X}"
                   :cboff "{ }"
                   :cbtrans "{-}"
                   :ifmt (lambda (type contents)
                           (format "[%S]%s" type contents)))))
           (html (org-list-to-html parsed '(:raw t)))
           (org (org-list-to-org parsed))
           (subtree (org-list-to-subtree parsed 2)))
      (org-list-to-lisp t)
      (insert org)
      (list parsed
            generic
            (list (string-match-p "<dl" html)
                  (string-match-p "Term" html)
                  (string-match-p "child three" html))
            org
            subtree
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_list_checkbox_table_fold_element_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-element)
  (require 'org-fold)
  (require 'org-list)
  (require 'org-table)
  (with-temp-buffer
    (let ((org-cycle-include-plain-lists 'integrate)
          (org-checkbox-hierarchical-statistics nil)
          (org-list-allow-alphabetical t))
      (org-mode)
      (insert "* TODO Sprint [0/4] [0%] :work:\n")
      (insert "- [ ] Implement API\n")
      (insert "  - [X] schema\n")
      (insert "  - [ ] endpoint\n")
      (insert "- [-] Review docs [1/2]\n")
      (insert "  1. [X] intro\n")
      (insert "  2. [ ] examples\n")
      (insert "- [ ] Ship release\n")
      (insert "\n| Task | Estimate | Done | Weight |\n")
      (insert "|------+----------+------+--------|\n")
      (insert "| API  |        3 |    0 |        |\n")
      (insert "| Docs |        2 |    1 |        |\n")
      (insert "| Ship |        1 |    0 |        |\n")
      (insert "| Sum  |          |      |        |\n")
      (insert "#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\n")
      (let ((snapshot
             (lambda (label)
               (let* ((struct (save-excursion
                                (goto-char (point-min))
                                (search-forward "- [")
                                (org-list-struct)))
                      (prevs (org-list-prevs-alist struct))
                      (parents (org-list-parents-alist struct)))
                 (list label
                       (buffer-substring-no-properties
                        (point-min) (point-max))
                       (mapcar
                        (lambda (item)
                          (save-excursion
                            (goto-char item)
                            (list (- item (point-min))
                                  (buffer-substring-no-properties
                                   (line-beginning-position)
                                   (line-end-position))
                                  (org-list-get-parent item struct parents)
                                  (org-list-get-children item struct parents)
                                  (org-list-get-item-number
                                   item struct prevs parents)
                                  (org-list-get-list-type item struct prevs))))
                        (org-list-get-all-items (point-min) struct prevs))
                       (org-element-map (org-element-parse-buffer)
                           '(headline item table table-row table-cell)
                         (lambda (el)
                           (list (org-element-type el)
                                 (org-element-property :begin el)
                                 (org-element-property :end el)
                                 (org-element-property :checkbox el)
                                 (org-element-property :todo-keyword el)
                                 (org-element-property :raw-value el))))
                       (mapcar
                        (lambda (needle)
                          (save-excursion
                            (goto-char (point-min))
                            (search-forward needle)
                            (list needle
                                  (line-number-at-pos)
                                  (invisible-p (point))
                                  (org-element-type
                                   (org-element-at-point)))))
                        '("Sprint" "Implement API" "schema" "endpoint"
                          "Review docs" "intro" "examples" "Ship release"
                          "Task" "Sum"))
                       (save-excursion
                         (goto-char (point-min))
                         (search-forward "| Task")
                         (org-table-to-lisp)))))))
        (let (states)
          (push (funcall snapshot 'initial) states)
          (goto-char (point-min))
          (search-forward "Implement API")
          (org-toggle-checkbox)
          (goto-char (point-min))
          (search-forward "endpoint")
          (org-toggle-checkbox)
          (goto-char (point-min))
          (search-forward "examples")
          (org-toggle-checkbox)
          (goto-char (point-min))
          (org-update-checkbox-count t)
          (push (funcall snapshot 'after-checkboxes) states)
          (goto-char (point-min))
          (search-forward "Review docs")
          (beginning-of-line)
          (org-move-item-up)
          (push (funcall snapshot 'after-move-docs) states)
          (goto-char (point-min))
          (search-forward "Ship release")
          (beginning-of-line)
          (org-indent-item-tree)
          (push (funcall snapshot 'after-indent-ship) states)
          (org-outdent-item-tree)
          (org-list-repair)
          (org-update-checkbox-count t)
          (push (funcall snapshot 'after-repair) states)
          (goto-char (point-min))
          (search-forward "| Task")
          (org-table-recalculate 'all)
          (push (funcall snapshot 'after-table) states)
          (goto-char (point-min))
          (search-forward "Sprint")
          (beginning-of-line)
          (dotimes (_ 3)
            (org-cycle)
            (push (funcall snapshot 'headline-cycle) states))
          (goto-char (point-min))
          (search-forward "examples")
          (org-fold-show-context 'default)
          (push (funcall snapshot 'context-examples) states)
          (org-fold-show-all)
          (push (funcall snapshot 'final) states)
          (list (nreverse states)
                (org-list-to-lisp)
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_list_nested_counter_checkbox_repair_cycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (require 'org-cycle)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-cycle-include-plain-lists 'integrate))
      (org-mode)
      (insert "* Tasks [0/3]\n")
      (insert "1. [ ] Alpha [1/2]\n")
      (insert "   - [X] Alpha done\n")
      (insert "   - [ ] Alpha todo\n")
      (insert "2. [ ] Beta\n")
      (insert "   a. Beta child b\n")
      (insert "   b. Beta child a\n")
      (insert "3. [X] Gamma\n")
      (let ((snapshot
             (lambda (label)
               (let* ((struct (org-list-struct))
                      (prevs (org-list-prevs-alist struct))
                      (parents (org-list-parents-alist struct))
                      (items (org-list-get-all-items (point-min) struct prevs)))
                 (list label
                       (mapcar
                        (lambda (item)
                          (save-excursion
                            (goto-char item)
                            (list (- item (point-min))
                                  (buffer-substring-no-properties
                                   item (line-end-position))
                                  (org-list-get-parent item struct parents)
                                  (org-list-get-children item struct parents)
                                  (org-list-get-item-number
                                   item struct prevs parents)
                                  (org-list-get-list-type item struct prevs)
                                  (invisible-p item))))
                        items)
                       (org-list-to-lisp)
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))
            states)
        (goto-char (point-min))
        (search-forward "Alpha todo")
        (org-toggle-checkbox)
        (goto-char (point-min))
        (org-update-checkbox-count t)
        (push (funcall snapshot 'after-checkbox) states)
        (goto-char (point-min))
        (search-forward "Beta child a")
        (beginning-of-line)
        (org-move-item-up)
        (push (funcall snapshot 'after-child-move) states)
        (goto-char (point-min))
        (search-forward "Gamma")
        (beginning-of-line)
        (org-indent-item-tree)
        (push (funcall snapshot 'after-indent-gamma) states)
        (org-outdent-item-tree)
        (org-list-repair)
        (push (funcall snapshot 'after-outdent-repair) states)
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-cycle)
        (push (funcall snapshot 'after-list-cycle) states)
        (org-fold-show-all)
        (goto-char (point-min))
        (org-cycle-list-bullet ?+)
        (push (funcall snapshot 'after-bullet-cycle) states)
        (list (nreverse states)
              (org-list-to-generic
               (org-list-to-lisp)
               (list :backend 'org
                     :raw t
                     :ostart "<ordered>"
                     :oend "</ordered>"
                     :ulstart "<unordered>"
                     :ulend "</unordered>"
                     :istart "<item>"
                     :iend "</item>"
                     :isep "|"
                     :cbon "[on]"
                     :cboff "[off]"
                     :cbtrans "[mixed]"
                     :ifmt (lambda (_type contents) contents)))
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_list_struct_write_visibility_apply_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (require 'org-fold)
  (with-temp-buffer
    (org-mode)
    (insert "- [ ] Alpha\n")
    (insert "  continuation alpha\n")
    (insert "  - [ ] Alpha child one\n")
    (insert "    child one body\n")
    (insert "  - [X] Alpha child two\n")
    (insert "- [ ] Beta\n")
    (insert "  beta body\n")
    (insert "- Gamma\n")
    (goto-char (point-min))
    (let* ((old-struct (org-list-struct))
           (struct (copy-tree old-struct))
           (prevs (org-list-prevs-alist struct))
           (parents (org-list-parents-alist struct))
           (items (org-list-get-all-items (point-min) struct prevs))
           (first (nth 0 items))
           (child-one (nth 1 items))
           (beta (nth 3 items))
           (before
            (mapcar (lambda (item)
                      (list (- item (point-min))
                            (org-list-get-bullet item struct)
                            (org-list-get-checkbox item struct)
                            (org-list-get-parent item struct parents)
                            (org-list-item-body-column item)))
                    items)))
      (org-list-set-checkbox first struct "[X]")
      (org-list-set-checkbox child-one struct "[X]")
      (org-list-set-checkbox beta struct nil)
      (org-list-write-struct struct parents old-struct)
      (let* ((after-write
              (buffer-substring-no-properties (point-min) (point-max)))
             (written-struct (org-list-struct))
             (written-pre (org-list-prevs-alist written-struct))
             (written-parents (org-list-parents-alist written-struct))
             (written-items
              (org-list-get-all-items (point-min) written-struct written-pre))
             (after-summary
              (mapcar
               (lambda (item)
                 (list (- item (point-min))
                       (buffer-substring-no-properties
                        item (line-end-position))
                       (org-list-get-checkbox item written-struct)
                       (org-list-get-parent item written-struct
                                            written-parents)
                       (org-list-get-children item written-struct
                                              written-parents)
                       (org-list-item-body-column item)))
               written-items))
             (applied
              (progn
                (goto-char (point-min))
                (org-apply-on-list
                 (lambda (acc)
                   (cons (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                         acc))
                 nil)))
             folded children-state subtree-state)
        (goto-char (point-min))
        (org-list-set-item-visibility (car written-items) written-struct
                                      'children)
        (setq children-state
              (mapcar (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (list needle (invisible-p (line-beginning-position)))))
                      '("continuation" "Alpha child one" "child one body"
                        "Alpha child two" "Beta")))
        (setq folded buffer-invisibility-spec)
        (org-list-set-item-visibility (car written-items) written-struct
                                      'subtree)
        (setq subtree-state
              (mapcar (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (list needle (invisible-p (line-beginning-position)))))
                      '("continuation" "Alpha child one" "child one body"
                        "Alpha child two" "Beta")))
        (list before
              after-write
              after-summary
              applied
              children-state
              folded
              subtree-state
              (org-list-to-lisp)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_list_make_subtree_checkbox_counter_roundtrip_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (let ((org-list-allow-alphabetical t)
          (org-adapt-indentation t))
      (org-mode)
      (insert "* Parent [0/2]\n")
      (insert "Intro paragraph\n")
      (insert "a. [@c] [X] Alpha :: first line\n")
      (insert "   continuation alpha\n")
      (insert "   - [ ] Alpha child :: child def\n")
      (insert "     child body\n")
      (insert "d. [-] Beta [1/2] :: second line\n")
      (insert "   1) [X] Beta done\n")
      (insert "   2) [ ] Beta todo\n")
      (insert "* Tail\n")
      (let ((before-list (save-excursion
                           (goto-char (point-min))
                           (search-forward "Alpha")
                           (beginning-of-line)
                           (org-list-to-lisp)))
            (before-buffer
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-list-make-subtree)
        (goto-char (point-min))
        (org-update-checkbox-count t)
        (let* ((tree (org-element-parse-buffer))
               (heads
                (org-element-map tree 'headline
                  (lambda (h)
                    (list (org-element-property :level h)
                          (org-element-property :todo-keyword h)
                          (org-element-property :raw-value h)
                          (org-element-property :priority h)
                          (org-element-property :begin h)
                          (org-element-property :end h)))))
               (items
                (org-element-map tree 'item
                  (lambda (item)
                    (list (org-element-property :checkbox item)
                          (org-element-property :counter item)
                          (org-element-property :tag item)))))
               (converted
                (buffer-substring-no-properties (point-min) (point-max))))
          (goto-char (point-min))
          (search-forward "Alpha")
          (org-demote-subtree)
          (goto-char (point-min))
          (search-forward "Beta")
          (org-promote-subtree)
          (org-update-checkbox-count t)
          (let ((after-level-edits
                 (buffer-substring-no-properties (point-min) (point-max)))
                (roundtrip-list
                 (org-list-to-subtree before-list 3
                                      '(:raw t :istart "" :iend ""))))
            (list before-list
                  before-buffer
                  heads
                  items
                  converted
                  roundtrip-list
                  after-level-edits))))))"##,
        expect,
    );
}

#[test]
fn org_list_checkbox_dependency_sort_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (require 'org-cycle)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-enforce-todo-checkbox-dependencies t)
          (org-todo-keywords '((sequence "TODO" "NEXT" "|" "DONE")))
          (org-cycle-include-plain-lists 'integrate))
      (org-mode)
      (insert "* TODO Project [0/4] [0%]\n")
      (insert "- [ ] Zebra [0/2]\n")
      (insert "  - [ ] Zebra child b\n")
      (insert "  - [X] Zebra child a\n")
      (insert "- [-] Alpha [1/2]\n")
      (insert "  - [X] Alpha done\n")
      (insert "  - [ ] Alpha todo\n")
      (insert "- [ ] Mango\n")
      (insert "- [X] Done item\n")
      (let ((snapshot
             (lambda (label)
               (let* ((struct (org-list-struct))
                      (prevs (org-list-prevs-alist struct))
                      (parents (org-list-parents-alist struct))
                      (items (org-list-get-all-items (point-min) struct prevs)))
                 (list label
                       (condition-case err
                           (save-excursion
                             (goto-char (point-min))
                             (org-entry-blocked-p))
                         (error (cons (car err) (cdr err))))
                       (mapcar
                        (lambda (item)
                          (save-excursion
                            (goto-char item)
                            (list (- item (point-min))
                                  (buffer-substring-no-properties
                                   item (line-end-position))
                                  (org-list-get-checkbox item struct)
                                  (org-list-get-parent item struct parents)
                                  (org-list-get-children item struct parents)
                                  (org-list-get-item-number
                                   item struct prevs parents)
                                  (org-list-get-list-type item struct prevs)
                                  (invisible-p item))))
                        items)
                       (org-list-to-lisp)
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))
            states)
        (goto-char (point-min))
        (let ((blocked-attempt
               (condition-case err
                   (progn (org-todo "DONE") 'ok)
                 (error (cons (car err) (cdr err))))))
          (push (list 'initial blocked-attempt
                      (funcall snapshot 'initial-state))
                states))
        (goto-char (point-min))
        (search-forward "Alpha todo")
        (org-toggle-checkbox)
        (search-forward "Zebra child b")
        (org-toggle-checkbox)
        (goto-char (point-min))
        (search-forward "Mango")
        (org-toggle-checkbox)
        (goto-char (point-min))
        (org-update-checkbox-count t)
        (push (funcall snapshot 'after-checks) states)
        (goto-char (point-min))
        (search-forward "Zebra child a")
        (beginning-of-line)
        (org-move-item-up)
        (goto-char (point-min))
        (search-forward "Mango")
        (beginning-of-line)
        (org-indent-item-tree)
        (push (funcall snapshot 'after-move-indent) states)
        (org-outdent-item-tree)
        (goto-char (point-min))
        (search-forward "Zebra")
        (beginning-of-line)
        (org-sort-list nil ?a)
        (org-list-repair)
        (goto-char (point-min))
        (org-update-checkbox-count t)
        (push (funcall snapshot 'after-sort-repair) states)
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-cycle)
        (push (funcall snapshot 'after-cycle) states)
        (org-fold-show-all)
        (goto-char (point-min))
        (let ((done-attempt
               (condition-case err
                   (progn (org-todo "DONE") 'ok)
                 (error (cons (car err) (cdr err))))))
          (push (list 'done-attempt done-attempt
                      (org-get-todo-state)
                      (funcall snapshot 'after-done))
                states))
        (list (nreverse states)
              (org-element-map (org-element-parse-buffer)
                  '(headline item)
                (lambda (el)
                  (pcase (org-element-type el)
                    ('headline
                     (list 'headline
                           (org-element-property :todo-keyword el)
                           (org-element-property :raw-value el)))
                    ('item
                     (list 'item
                           (org-element-property :checkbox el)
                           (org-element-property :counter el)
                           (org-element-property :tag el))))))
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_list_send_item_struct_navigation_kill_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (let ((org-list-use-circular-motion t)
          (org-list-allow-alphabetical t)
          (org-list-demote-modify-bullet
           '(("1." . "-")
             ("1)" . "+")
             ("A." . "1.")
             ("a)" . "-")
             ("-" . "a)"))))
      (org-mode)
      (setq kill-ring nil)
      (insert "* Plan [0/3]\n")
      (insert "1. [ ] Alpha [1/2]\n")
      (insert "   a) [X] Alpha child one\n")
      (insert "      child one body\n")
      (insert "   b) [ ] Alpha child two\n")
      (insert "      child two body\n")
      (insert "2. [X] Beta :: definition\n")
      (insert "   - [ ] Beta child\n")
      (insert "     beta body\n")
      (insert "3. [ ] Gamma\n")
      (insert "   gamma body\n")
      (let ((snapshot
             (lambda (label)
               (let* ((struct (save-excursion
                                (goto-char (point-min))
                                (search-forward "Alpha")
                                (beginning-of-line)
                                (org-list-struct)))
                      (prevs (org-list-prevs-alist struct))
                      (parents (org-list-parents-alist struct))
                      (items (org-list-get-all-items
                              (org-list-get-top-point struct)
                              struct prevs)))
                 (list label
                       (mapcar
                        (lambda (item)
                          (save-excursion
                            (goto-char item)
                            (list (- item (point-min))
                                  (buffer-substring-no-properties
                                   item (line-end-position))
                                  (org-list-get-bullet item struct)
                                  (org-list-get-checkbox item struct)
                                  (org-list-get-counter item struct)
                                  (org-list-get-parent item struct parents)
                                  (org-list-get-children item struct parents)
                                  (org-list-get-prev-item item struct prevs)
                                  (org-list-get-next-item item struct prevs)
                                  (org-list-get-list-type item struct prevs)
                                  (org-list-item-body-column item)
                                  (- (org-list-get-item-end item struct)
                                     (point-min)))))
                        items)
                       (org-list-to-lisp)
                       (buffer-substring-no-properties
                        (point-min) (point-max))))))
            states)
        (push (funcall snapshot 'initial) states)
        (goto-char (point-min))
        (search-forward "Beta ::")
        (beginning-of-line)
        (let* ((struct (org-list-struct))
               (item (line-beginning-position))
               (moved (org-list-send-item item 'begin struct)))
          (org-list-write-struct moved (org-list-parents-alist moved)))
        (goto-char (point-min))
        (org-update-checkbox-count t)
        (push (funcall snapshot 'after-send-begin) states)
        (goto-char (point-min))
        (search-forward "Alpha child two")
        (beginning-of-line)
        (let* ((struct (org-list-struct))
               (item (line-beginning-position))
               (moved (org-list-send-item item "1" struct)))
          (org-list-write-struct moved (org-list-parents-alist moved)))
        (push (funcall snapshot 'after-send-child-first) states)
        (goto-char (point-min))
        (search-forward "Gamma")
        (beginning-of-line)
        (let* ((struct (org-list-struct))
               (item (line-beginning-position))
               (deleted (org-list-send-item item 'kill struct)))
          (org-list-write-struct deleted (org-list-parents-alist deleted)))
        (goto-char (point-min))
        (org-update-checkbox-count t)
        (push (funcall snapshot 'after-kill-gamma) states)
        (goto-char (point-min))
        (search-forward "Alpha child one")
        (beginning-of-line)
        (let* ((old (org-list-struct))
               (parents (org-list-parents-alist old))
               (out-parents
                (org-list-struct-outdent
                 (line-beginning-position)
                 (save-excursion (forward-line 1) (point))
                 old parents)))
          (org-list-write-struct old out-parents old))
        (push (funcall snapshot 'after-struct-outdent) states)
        (goto-char (point-min))
        (search-forward "Alpha child one")
        (beginning-of-line)
        (let* ((old (org-list-struct))
               (parents (org-list-parents-alist old))
               (prevs (org-list-prevs-alist old))
               (in-parents
                (org-list-struct-indent
                 (line-beginning-position)
                 (save-excursion (forward-line 1) (point))
                 old parents prevs)))
          (org-list-write-struct old in-parents old))
        (push (funcall snapshot 'after-struct-indent) states)
        (goto-char (point-min))
        (search-forward "Beta ::")
        (beginning-of-line)
        (let ((nav nil))
          (org-previous-item)
          (push (buffer-substring-no-properties
                 (line-beginning-position) (line-end-position))
                nav)
          (org-next-item)
          (push (buffer-substring-no-properties
                 (line-beginning-position) (line-end-position))
                nav)
          (goto-char (point-min))
          (search-forward "Alpha child one")
          (beginning-of-line)
          (push (condition-case err
                    (progn
                      (let ((org-list-use-circular-motion nil))
                        (org-previous-item))
                      'ok)
                  (error (cons (car err) (cdr err))))
                nav)
          (list (nreverse states)
                (nreverse nav)
                (current-kill 0 t)
                (org-element-map (org-element-parse-buffer) 'item
                  (lambda (item)
                    (list (org-element-property :checkbox item)
                          (org-element-property :counter item)
                          (org-element-property :tag item)
                          (org-element-property :begin item)
                          (org-element-property :end item))))
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_checkbox_toggle_counter_stats_toggle_cycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 59 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "* Tasks [0/4]\n")
    (insert "- [ ] Item A\n")
    (insert "  - [ ] Sub A1\n")
    (insert "  - [ ] Sub A2\n")
    (insert "- [ ] Item B\n")
    (insert "  - [ ] Sub B1\n")
    (insert "- [ ] Item C\n")
    (insert "- [ ] Item D\n")
    (let ((snap (lambda ()
                  (list (buffer-substring-no-properties
                         (point-min) (point-max))
                        (org-element-map (org-element-parse-buffer) 'item
                          (lambda (item)
                            (list (org-element-property :checkbox item)
                                  (org-element-property :counter item))))))))
      ;; Toggle Sub A1
      (goto-char (point-min))
      (search-forward "Sub A1")
      (beginning-of-line)
      (org-toggle-checkbox)
      (let ((after-a1 (funcall snap)))
        ;; Toggle Sub A2
        (goto-char (point-min))
        (search-forward "Sub A2")
        (beginning-of-line)
        (org-toggle-checkbox)
        (let ((after-a2 (funcall snap)))
          ;; Update counters
          (goto-char (point-min))
          (org-update-checkbox-count)
          (let ((after-count (funcall snap)))
            ;; Toggle Item B
            (goto-char (point-min))
            (search-forward "Item B")
            (beginning-of-line)
            (org-toggle-checkbox)
            (let ((after-b (funcall snap)))
              ;; Cycle checkbox states on Item C
              (goto-char (point-min))
              (search-forward "Item C")
              (beginning-of-line)
              (org-toggle-checkbox)
              (org-toggle-checkbox)
              (let ((after-c-cycle (funcall snap)))
                ;; Stats update
                (goto-char (point-min))
                (org-update-statistics-cookies t)
                (let ((after-stats (funcall snap)))
                  (list after-a1
                        after-a2
                        after-count
                        after-b
                        after-c-cycle
                          after-stats)))))))))))"##,
        expect,
    );
}

#[test]
fn org_list_structure_indent_outdent_renumber_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 56 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "1. First\n")
    (insert "   - Child A\n")
    (insert "   - Child B\n")
    (insert "     - Grandchild\n")
    (insert "2. Second\n")
    (insert "   - [X] Checked\n")
    (insert "   - [ ] Unchecked\n")
    (insert "3. Third\n")
    ;; Parse initial structure
    (let* ((snap (lambda ()
                   (list (buffer-substring-no-properties
                          (point-min) (point-max))
                         (org-element-map (org-element-parse-buffer) 'item
                           (lambda (item)
                             (list (org-element-property :bullet item)
                                   (org-element-property :checkbox item)
                                   (org-element-property :counter item)
                                   (org-element-property :begin item)))))))
           (initial (funcall snap)))
      ;; Indent Child B under Child A
      (goto-char (point-min))
      (search-forward "Child B")
      (beginning-of-line)
      (org-indent-item)
      (let ((after-indent (funcall snap)))
        ;; Outdent back
        (goto-char (point-min))
        (search-forward "Child B")
        (beginning-of-line)
        (org-outdent-item)
        (let ((after-outdent (funcall snap)))
          ;; Toggle checkbox
          (goto-char (point-min))
          (search-forward "Unchecked")
          (beginning-of-line)
          (org-toggle-checkbox)
          (let ((after-toggle (funcall snap)))
            ;; Update counter
            (goto-char (point-min))
            (org-update-checkbox-count)
            (let ((after-count (funcall snap)))
              ;; Sort list
              (goto-char (point-min))
              (org-sort-list nil ?A)
              (let ((after-sort (funcall snap)))
                (list initial
                      after-indent
                      after-outdent
                      after-toggle
                      after-count
                      after-sort))))))))))"##,
        expect,
    );
}

#[test]
fn org_list_checkbox_toggle_update_sort_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (with-temp-buffer
    (org-mode)
    (insert "- [ ] Item alpha\n")
    (insert "- [ ] Item beta\n")
    (insert "- [X] Item gamma\n")
    (insert "- [ ] Item delta\n")
    (insert "  - [ ] Sub A\n")
    (insert "  - [X] Sub B\n")
    (let ((snap (lambda ()
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))
      (let ((initial (funcall snap)))
        ;; Toggle checkbox on alpha
        (goto-char (point-min))
        (search-forward "alpha")
        (beginning-of-line)
        (org-toggle-checkbox)
        (let ((after-toggle (funcall snap)))
          ;; Update counter
          (goto-char (point-min))
          (org-update-checkbox-count)
          (let ((after-count (funcall snap)))
            ;; Sort list
            (goto-char (point-min))
            (org-sort-list nil ?A)
            (let ((after-sort (funcall snap)))
              ;; Edit: add new item
              (goto-char (point-max))
              (insert "- [ ] Item epsilon\n")
              (let ((after-edit (funcall snap)))
                ;; Re-toggle all
                (goto-char (point-min))
                (while (re-search-forward "\\[ \\]" nil t)
                  (replace-match "[X]"))
                (let ((after-replace (funcall snap)))
                  (list initial
                        after-toggle
                        after-count
                        after-sort
                        after-edit
                        after-replace))))))))))))"##,
        expect,
    );
}
