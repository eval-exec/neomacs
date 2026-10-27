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

    let expect = expect_test::expect![[
        r#""OK (\"- [ ] Parent [0/0]\\n- [ ] A\\n  - [ ] B\\n    - [ ] B child\\n- [ ] C\\n\" ((0 nil (1) nil 20) (19 nil (2) nil 28) (27 nil (3) (36) 52) (51 nil (4) nil 60)) (unordered (\"[ ] Parent [0/0]\") (\"[ ] A\") (\"[ ] B\" (unordered (\"[ ] B child\"))) (\"[ ] C\")) \"- [ ] Parent [0/0]\\n- [ ] A\\n- [ ] B\\n  - [ ] B child\\n- [ ] C\\n\")""#
    ]];
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
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_ordered_alpha_list_sort_renumber_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"a. Alpha\\n   a. child two\\n   b. child one\\nb. Beta\\nc. Gamma\\n\" ((\"a. Alpha\\n   a. child one\" (1) ordered) (\"\\n   b. child two\\n\" (2) ordered) (\"\\n   b. child two\\nb. Beta\\n\" (3) ordered)) (ordered (\"Alpha\" (ordered (\"child one\") (\"child two\"))) (\"Beta\") (\"Gamma\")) \"a. Alpha\\n   a. child one\\n   b. child two\\nb. Beta\\nc. Gamma\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (let ((org-list-allow-alphabetical t))
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
    ;; GNU sorts numeric bullets; restore alphabetic numbering afterwards.
    (org-cycle-list-bullet "1.")
    (org-sort-list nil ?a)
    (org-cycle-list-bullet "a.")
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
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_insert_delete_move_description_items_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"- term A :: first\\n  continuation A\\n- term inserted :: new\\n- term B :: second\\n- term C :: third\\n\" \"- term A :: first\\n  continuation A\\n- term B :: second\\n- term inserted :: new\\n- term C :: third\\n\" (descriptive (\"term A :: first\\ncontinuation A\") (\"term B :: second\") (\"term inserted :: new\")) \"- term A :: first\\n  continuation A\\n- term B :: second\\n- term inserted :: new\\n\")""#
    ]];
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
    (insert "term inserted")
    (search-forward " :: ")
    (insert "new")
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
              (save-excursion
                (goto-char (point-min))
                (org-list-to-lisp))
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
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
fn org_list_struct_write_visibility_apply_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((0 \"- \" \"[ ]\" nil 2) (33 \"- \" \"[ ]\" 1 4) (76 \"- \" \"[X]\" 1 4) (100 \"- \" \"[ ]\" nil 2) (123 \"- \" nil nil 2)) \"- [X] Alpha\\n  continuation alpha\\n  - [X] Alpha child one\\n    child one body\\n  - [X] Alpha child two\\n- Beta\\n  beta body\\n- Gamma\\n\" ((0 \"- [X] Alpha\" \"[X]\" nil (34 77) 2) (100 \"\\n  continuation alpha\\n  - [X] Alpha child one\\n    child one body\\n  - [X] Alpha child two\\n\" nil nil nil 2) (119 \"\\n  continuation alpha\\n  - [X] Alpha child one\\n    child one body\\n  - [X] Alpha child two\\n- Beta\\n  beta body\\n\" nil nil nil 2)) (\"- [X] Alpha\" \"- Beta\" \"- Gamma\") ((\"continuation\" nil) (\"Alpha child one\" nil) (\"child one body\" 2) (\"Alpha child two\" nil) (\"Beta\" nil)) ((org-babel-hide-result . t) (org-hide-block . t) (org-fold-outline . \"...\") (org-hide-block . \"...\") (org-hide-drawer . \"...\") (org-link) (outline . t) t) ((\"continuation\" nil) (\"Alpha child one\" nil) (\"child one body\" nil) (\"Alpha child two\" nil) (\"Beta\" nil)) (unordered (\"[X] Alpha\\ncontinuation alpha\" (unordered (\"[X] Alpha child one\\nchild one body\") (\"[X] Alpha child two\"))) (\"Beta\\nbeta body\") (\"Gamma\")) \"- [X] Alpha\\n  continuation alpha\\n  - [X] Alpha child one\\n    child one body\\n  - [X] Alpha child two\\n- Beta\\n  beta body\\n- Gamma\\n\")""#
    ]];
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
           (items (mapcar #'car struct))
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
              (save-excursion
                (goto-char (point-min))
                (org-list-to-lisp))
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_list_nested_counter_checkbox_repair_cycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((after-checkbox ((14 \"1. [X] Alpha [2/2]\" nil (34 54) (1) ordered nil) (73 \"2. [ ] Beta\" nil (86 105) (2) ordered nil) (123 \"3. [X] Gamma\" nil nil (3) ordered nil)) (ordered (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[ ] Beta\" (ordered (\"Beta child b\") (\"Beta child a\"))) (\"[X] Gamma\")) \"* Tasks [2/3]\\n1. [X] Alpha [2/2]\\n   - [X] Alpha done\\n   - [X] Alpha todo\\n2. [ ] Beta\\n   a. Beta child b\\n   b. Beta child a\\n3. [X] Gamma\\n\") (after-child-move ((14 \"1. [X] Alpha [2/2]\" nil (34 54) (1) ordered nil) (73 \"2. [ ] Beta\" nil (86 105) (2) ordered nil) (123 \"3. [X] Gamma\" nil nil (3) ordered nil)) (ordered (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[ ] Beta\" (ordered (\"Beta child a\") (\"Beta child b\"))) (\"[X] Gamma\")) \"* Tasks [2/3]\\n1. [X] Alpha [2/2]\\n   - [X] Alpha done\\n   - [X] Alpha todo\\n2. [ ] Beta\\n   a. Beta child a\\n   b. Beta child b\\n3. [X] Gamma\\n\") (after-indent-gamma ((14 \"1. [X] Alpha [2/2]\" nil (34 54) (1) ordered nil) (73 \"2. [X] Beta\" nil (86 105 124) (2) ordered nil)) (ordered (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[X] Beta\" (ordered (\"Beta child a\") (\"Beta child b\") (\"[X] Gamma\")))) \"* Tasks [2/2]\\n1. [X] Alpha [2/2]\\n   - [X] Alpha done\\n   - [X] Alpha todo\\n2. [X] Beta\\n   a. Beta child a\\n   b. Beta child b\\n   c. [X] Gamma\\n\") (after-outdent-repair ((14 \"1. [X] Alpha [2/2]\" nil (34 54) (1) ordered nil) (73 \"2. [X] Beta\" nil (86 105) (2) ordered nil) (123 \"3. [X] Gamma\" nil nil (3) ordered nil)) (ordered (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[X] Beta\" (ordered (\"Beta child a\") (\"Beta child b\"))) (\"[X] Gamma\")) \"* Tasks [3/3]\\n1. [X] Alpha [2/2]\\n   - [X] Alpha done\\n   - [X] Alpha todo\\n2. [X] Beta\\n   a. Beta child a\\n   b. Beta child b\\n3. [X] Gamma\\n\") (after-list-cycle ((14 \"1. [X] Alpha [2/2]\" nil (34 54) (1) ordered nil) (73 \"2. [X] Beta\" nil (86 105) (2) ordered nil) (123 \"3. [X] Gamma\" nil nil (3) ordered nil)) (ordered (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[X] Beta\" (ordered (\"Beta child a\") (\"Beta child b\"))) (\"[X] Gamma\")) \"* Tasks [3/3]\\n1. [X] Alpha [2/2]\\n   - [X] Alpha done\\n   - [X] Alpha todo\\n2. [X] Beta\\n   a. Beta child a\\n   b. Beta child b\\n3. [X] Gamma\\n\") (after-bullet-cycle ((14 \"1) [X] Alpha [2/2]\" nil (34 54) (1) ordered nil) (73 \"2) [X] Beta\" nil (86 105) (2) ordered nil) (123 \"3) [X] Gamma\" nil nil (3) ordered nil)) (ordered (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[X] Beta\" (ordered (\"Beta child a\") (\"Beta child b\"))) (\"[X] Gamma\")) \"* Tasks [3/3]\\n1) [X] Alpha [2/2]\\n   - [X] Alpha done\\n   - [X] Alpha todo\\n2) [X] Beta\\n   a. Beta child a\\n   b. Beta child b\\n3) [X] Gamma\\n\")) \"<ordered>\\n<item>[on]Alpha [2/2]\\n<unordered>\\n<item>[on]Alpha done</item>\\n|\\n<item>[on]Alpha todo</item>\\n</unordered></item>\\n|\\n<item>[on]Beta\\n<ordered>\\n<item>Beta child a</item>\\n|\\n<item>Beta child b</item>\\n</ordered></item>\\n|\\n<item>[on]Gamma</item>\\n</ordered>\" \"* Tasks [3/3]\\n1) [X] Alpha [2/2]\\n   - [X] Alpha done\\n   - [X] Alpha todo\\n2) [X] Beta\\n   a. Beta child a\\n   b. Beta child b\\n3) [X] Gamma\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-list)
  (require 'ox-org)
  (require 'org-cycle)
  (require 'org-fold)
  (with-temp-buffer
    (let ((org-cycle-include-plain-lists 'integrate)
          (org-list-allow-alphabetical t))
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
               (let* ((struct (save-excursion
                                (goto-char (point-min))
                                (forward-line 1)
                                (org-list-struct)))
                      (prevs (org-list-prevs-alist struct))
                      (parents (org-list-parents-alist struct))
                      (items (org-list-get-all-items (caar struct) struct prevs)))
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
                       (save-excursion
                         (goto-char (caar struct))
                         (org-list-to-lisp))
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
        (forward-line 1)
        (org-cycle-list-bullet ?+)
        (push (funcall snapshot 'after-bullet-cycle) states)
        (list (nreverse states)
              (org-list-to-generic
               (save-excursion
                 (goto-char (point-min))
                 (forward-line 1)
                 (org-list-to-lisp))
               (list :backend 'org
                     :raw t
                     :ostart "<ordered>"
                     :oend "</ordered>"
                     :ustart "<unordered>"
                     :uend "</unordered>"
                     :istart "<item>"
                     :iend "</item>"
                     :isep "|"
                     :cbon "[on]"
                     :cboff "[off]"
                     :cbtrans "[mixed]"
                     :ifmt (lambda (_type contents) contents)))
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_list_checkbox_table_fold_element_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((initial \"* TODO Sprint [0/4] [0%] :work:\\n- [ ] Implement API\\n  - [X] schema\\n  - [ ] endpoint\\n- [-] Review docs [1/2]\\n  1. [X] intro\\n  2. [ ] examples\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |        |\\n| Docs |        2 |    1 |        |\\n| Ship |        1 |    0 |        |\\n| Sum  |          |      |        |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((32 \"- [ ] Implement API\" nil (53 68) (1) unordered) (84 \"- [-] Review docs [1/2]\" nil (109 124) (2) unordered) (141 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 457 nil \"TODO\" \"Sprint [0/4] [0%]\") (item 33 85 off nil nil) (item 53 68 on nil nil) (item 68 85 off nil nil) (item 85 142 trans nil nil) (item 109 124 on nil nil) (item 124 142 off nil nil) (item 142 161 off nil nil) (table 162 457 nil nil nil) (table-row 162 198 nil nil nil) (table-cell 163 170 nil nil nil) (table-cell 170 181 nil nil nil) (table-cell 181 188 nil nil nil) (table-cell 188 197 nil nil nil) (table-row 198 234 nil nil nil) (table-row 234 270 nil nil nil) (table-cell 235 242 nil nil nil) (table-cell 242 253 nil nil nil) (table-cell 253 260 nil nil nil) (table-cell 260 269 nil nil nil) (table-row 270 306 nil nil nil) (table-cell 271 278 nil nil nil) (table-cell 278 289 nil nil nil) (table-cell 289 296 nil nil nil) (table-cell 296 305 nil nil nil) (table-row 306 342 nil nil nil) (table-cell 307 314 nil nil nil) (table-cell 314 325 nil nil nil) (table-cell 325 332 nil nil nil) (table-cell 332 341 nil nil nil) (table-row 342 378 nil nil nil) (table-cell 343 350 nil nil nil) (table-cell 350 361 nil nil nil) (table-cell 361 368 nil nil nil) (table-cell 368 377 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 2 nil paragraph) (\"schema\" 3 nil paragraph) (\"endpoint\" 4 nil paragraph) (\"Review docs\" 5 nil paragraph) (\"intro\" 6 nil paragraph) (\"examples\" 7 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((\"Task\" \"Estimate\" \"Done\" \"Weight\") hline (\"API\" \"3\" \"0\" \"\") (\"Docs\" \"2\" \"1\" \"\") (\"Ship\" \"1\" \"0\" \"\") (\"Sum\" \"\" \"\" \"\"))) (after-checkboxes \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |        |\\n| Docs |        2 |    1 |        |\\n| Ship |        1 |    0 |        |\\n| Sum  |          |      |        |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Implement API\" nil (99 114) (1) unordered) (130 \"- [X] Review docs [2/2]\" nil (155 170) (2) unordered) (187 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 503 nil \"TODO\" \"Sprint [6/7] [85%]\") (item 79 131 on nil nil) (item 99 114 on nil nil) (item 114 131 on nil nil) (item 131 188 on nil nil) (item 155 170 on nil nil) (item 170 188 on nil nil) (item 188 207 off nil nil) (table 208 503 nil nil nil) (table-row 208 244 nil nil nil) (table-cell 209 216 nil nil nil) (table-cell 216 227 nil nil nil) (table-cell 227 234 nil nil nil) (table-cell 234 243 nil nil nil) (table-row 244 280 nil nil nil) (table-row 280 316 nil nil nil) (table-cell 281 288 nil nil nil) (table-cell 288 299 nil nil nil) (table-cell 299 306 nil nil nil) (table-cell 306 315 nil nil nil) (table-row 316 352 nil nil nil) (table-cell 317 324 nil nil nil) (table-cell 324 335 nil nil nil) (table-cell 335 342 nil nil nil) (table-cell 342 351 nil nil nil) (table-row 352 388 nil nil nil) (table-cell 353 360 nil nil nil) (table-cell 360 371 nil nil nil) (table-cell 371 378 nil nil nil) (table-cell 378 387 nil nil nil) (table-row 388 424 nil nil nil) (table-cell 389 396 nil nil nil) (table-cell 396 407 nil nil nil) (table-cell 407 414 nil nil nil) (table-cell 414 423 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 2 nil paragraph) (\"schema\" 3 nil paragraph) (\"endpoint\" 4 nil paragraph) (\"Review docs\" 5 nil paragraph) (\"intro\" 6 nil paragraph) (\"examples\" 7 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((\"Task\" \"Estimate\" \"Done\" \"Weight\") hline (\"API\" \"3\" \"0\" \"\") (\"Docs\" \"2\" \"1\" \"\") (\"Ship\" \"1\" \"0\" \"\") (\"Sum\" \"\" \"\" \"\"))) (after-move-docs \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |        |\\n| Docs |        2 |    1 |        |\\n| Ship |        1 |    0 |        |\\n| Sum  |          |      |        |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Review docs [2/2]\" nil (103 118) (1) unordered) (135 \"- [X] Implement API\" nil (156 171) (2) unordered) (187 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 503 nil \"TODO\" \"Sprint [6/7] [85%]\") (item 79 136 on nil nil) (item 103 118 on nil nil) (item 118 136 on nil nil) (item 136 188 on nil nil) (item 156 171 on nil nil) (item 171 188 on nil nil) (item 188 207 off nil nil) (table 208 503 nil nil nil) (table-row 208 244 nil nil nil) (table-cell 209 216 nil nil nil) (table-cell 216 227 nil nil nil) (table-cell 227 234 nil nil nil) (table-cell 234 243 nil nil nil) (table-row 244 280 nil nil nil) (table-row 280 316 nil nil nil) (table-cell 281 288 nil nil nil) (table-cell 288 299 nil nil nil) (table-cell 299 306 nil nil nil) (table-cell 306 315 nil nil nil) (table-row 316 352 nil nil nil) (table-cell 317 324 nil nil nil) (table-cell 324 335 nil nil nil) (table-cell 335 342 nil nil nil) (table-cell 342 351 nil nil nil) (table-row 352 388 nil nil nil) (table-cell 353 360 nil nil nil) (table-cell 360 371 nil nil nil) (table-cell 371 378 nil nil nil) (table-cell 378 387 nil nil nil) (table-row 388 424 nil nil nil) (table-cell 389 396 nil nil nil) (table-cell 396 407 nil nil nil) (table-cell 407 414 nil nil nil) (table-cell 414 423 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 5 nil paragraph) (\"schema\" 6 nil paragraph) (\"endpoint\" 7 nil paragraph) (\"Review docs\" 2 nil paragraph) (\"intro\" 3 nil paragraph) (\"examples\" 4 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((\"Task\" \"Estimate\" \"Done\" \"Weight\") hline (\"API\" \"3\" \"0\" \"\") (\"Docs\" \"2\" \"1\" \"\") (\"Ship\" \"1\" \"0\" \"\") (\"Sum\" \"\" \"\" \"\"))) (after-indent-ship \"* TODO Sprint [5/7] [71%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [-] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n  - [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |        |\\n| Docs |        2 |    1 |        |\\n| Ship |        1 |    0 |        |\\n| Sum  |          |      |        |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Review docs [2/2]\" nil (103 118) (1) unordered) (135 \"- [-] Implement API\" nil (156 171 188) (2) unordered)) ((headline 1 505 nil \"TODO\" \"Sprint [5/7] [71%]\") (item 79 136 on nil nil) (item 103 118 on nil nil) (item 118 136 on nil nil) (item 136 209 trans nil nil) (item 156 171 on nil nil) (item 171 188 on nil nil) (item 188 209 off nil nil) (table 210 505 nil nil nil) (table-row 210 246 nil nil nil) (table-cell 211 218 nil nil nil) (table-cell 218 229 nil nil nil) (table-cell 229 236 nil nil nil) (table-cell 236 245 nil nil nil) (table-row 246 282 nil nil nil) (table-row 282 318 nil nil nil) (table-cell 283 290 nil nil nil) (table-cell 290 301 nil nil nil) (table-cell 301 308 nil nil nil) (table-cell 308 317 nil nil nil) (table-row 318 354 nil nil nil) (table-cell 319 326 nil nil nil) (table-cell 326 337 nil nil nil) (table-cell 337 344 nil nil nil) (table-cell 344 353 nil nil nil) (table-row 354 390 nil nil nil) (table-cell 355 362 nil nil nil) (table-cell 362 373 nil nil nil) (table-cell 373 380 nil nil nil) (table-cell 380 389 nil nil nil) (table-row 390 426 nil nil nil) (table-cell 391 398 nil nil nil) (table-cell 398 409 nil nil nil) (table-cell 409 416 nil nil nil) (table-cell 416 425 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 5 nil paragraph) (\"schema\" 6 nil paragraph) (\"endpoint\" 7 nil paragraph) (\"Review docs\" 2 nil paragraph) (\"intro\" 3 nil paragraph) (\"examples\" 4 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((\"Task\" \"Estimate\" \"Done\" \"Weight\") hline (\"API\" \"3\" \"0\" \"\") (\"Docs\" \"2\" \"1\" \"\") (\"Ship\" \"1\" \"0\" \"\") (\"Sum\" \"\" \"\" \"\"))) (after-repair \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |        |\\n| Docs |        2 |    1 |        |\\n| Ship |        1 |    0 |        |\\n| Sum  |          |      |        |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Review docs [2/2]\" nil (103 118) (1) unordered) (135 \"- [X] Implement API\" nil (156 171) (2) unordered) (187 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 503 nil \"TODO\" \"Sprint [6/7] [85%]\") (item 79 136 on nil nil) (item 103 118 on nil nil) (item 118 136 on nil nil) (item 136 188 on nil nil) (item 156 171 on nil nil) (item 171 188 on nil nil) (item 188 207 off nil nil) (table 208 503 nil nil nil) (table-row 208 244 nil nil nil) (table-cell 209 216 nil nil nil) (table-cell 216 227 nil nil nil) (table-cell 227 234 nil nil nil) (table-cell 234 243 nil nil nil) (table-row 244 280 nil nil nil) (table-row 280 316 nil nil nil) (table-cell 281 288 nil nil nil) (table-cell 288 299 nil nil nil) (table-cell 299 306 nil nil nil) (table-cell 306 315 nil nil nil) (table-row 316 352 nil nil nil) (table-cell 317 324 nil nil nil) (table-cell 324 335 nil nil nil) (table-cell 335 342 nil nil nil) (table-cell 342 351 nil nil nil) (table-row 352 388 nil nil nil) (table-cell 353 360 nil nil nil) (table-cell 360 371 nil nil nil) (table-cell 371 378 nil nil nil) (table-cell 378 387 nil nil nil) (table-row 388 424 nil nil nil) (table-cell 389 396 nil nil nil) (table-cell 396 407 nil nil nil) (table-cell 407 414 nil nil nil) (table-cell 414 423 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 5 nil paragraph) (\"schema\" 6 nil paragraph) (\"endpoint\" 7 nil paragraph) (\"Review docs\" 2 nil paragraph) (\"intro\" 3 nil paragraph) (\"examples\" 4 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((\"Task\" \"Estimate\" \"Done\" \"Weight\") hline (\"API\" \"3\" \"0\" \"\") (\"Docs\" \"2\" \"1\" \"\") (\"Ship\" \"1\" \"0\" \"\") (\"Sum\" \"\" \"\" \"\"))) (after-table \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |      6 |\\n| Docs |        2 |    1 |      5 |\\n| Ship |        1 |    0 |      2 |\\n| Sum  |          |      |     13 |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Review docs [2/2]\" nil (103 118) (1) unordered) (135 \"- [X] Implement API\" nil (156 171) (2) unordered) (187 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 503 nil \"TODO\" \"Sprint [6/7] [85%]\") (item 79 136 on nil nil) (item 103 118 on nil nil) (item 118 136 on nil nil) (item 136 188 on nil nil) (item 156 171 on nil nil) (item 171 188 on nil nil) (item 188 207 off nil nil) (table 208 503 nil nil nil) (table-row 208 244 nil nil nil) (table-cell 209 216 nil nil nil) (table-cell 216 227 nil nil nil) (table-cell 227 234 nil nil nil) (table-cell 234 243 nil nil nil) (table-row 244 280 nil nil nil) (table-row 280 316 nil nil nil) (table-cell 281 288 nil nil nil) (table-cell 288 299 nil nil nil) (table-cell 299 306 nil nil nil) (table-cell 306 315 nil nil nil) (table-row 316 352 nil nil nil) (table-cell 317 324 nil nil nil) (table-cell 324 335 nil nil nil) (table-cell 335 342 nil nil nil) (table-cell 342 351 nil nil nil) (table-row 352 388 nil nil nil) (table-cell 353 360 nil nil nil) (table-cell 360 371 nil nil nil) (table-cell 371 378 nil nil nil) (table-cell 378 387 nil nil nil) (table-row 388 424 nil nil nil) (table-cell 389 396 nil nil nil) (table-cell 396 407 nil nil nil) (table-cell 407 414 nil nil nil) (table-cell 414 423 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 5 nil paragraph) (\"schema\" 6 nil paragraph) (\"endpoint\" 7 nil paragraph) (\"Review docs\" 2 nil paragraph) (\"intro\" 3 nil paragraph) (\"examples\" 4 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((#(\"Task\" 0 4 (face org-table)) #(\"Estimate\" 0 8 (face org-table)) #(\"Done\" 0 4 (face org-table)) #(\"Weight\" 0 6 (face org-table))) hline (#(\"API\" 0 3 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table))) (#(\"Docs\" 0 4 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"5\" 0 1 (face org-table))) (#(\"Ship\" 0 4 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table))) (#(\"Sum\" 0 3 (face org-table)) \"\" \"\" #(\"13\" 0 2 (face org-table))))) (headline-cycle \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |      6 |\\n| Docs |        2 |    1 |      5 |\\n| Ship |        1 |    0 |      2 |\\n| Sum  |          |      |     13 |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Review docs [2/2]\" nil (103 118) (1) unordered) (135 \"- [X] Implement API\" nil (156 171) (2) unordered) (187 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 503 nil \"TODO\" \"Sprint [6/7] [85%]\") (item 79 136 on nil nil) (item 103 118 on nil nil) (item 118 136 on nil nil) (item 136 188 on nil nil) (item 156 171 on nil nil) (item 171 188 on nil nil) (item 188 207 off nil nil) (table 208 503 nil nil nil) (table-row 208 244 nil nil nil) (table-cell 209 216 nil nil nil) (table-cell 216 227 nil nil nil) (table-cell 227 234 nil nil nil) (table-cell 234 243 nil nil nil) (table-row 244 280 nil nil nil) (table-row 280 316 nil nil nil) (table-cell 281 288 nil nil nil) (table-cell 288 299 nil nil nil) (table-cell 299 306 nil nil nil) (table-cell 306 315 nil nil nil) (table-row 316 352 nil nil nil) (table-cell 317 324 nil nil nil) (table-cell 324 335 nil nil nil) (table-cell 335 342 nil nil nil) (table-cell 342 351 nil nil nil) (table-row 352 388 nil nil nil) (table-cell 353 360 nil nil nil) (table-cell 360 371 nil nil nil) (table-cell 371 378 nil nil nil) (table-cell 378 387 nil nil nil) (table-row 388 424 nil nil nil) (table-cell 389 396 nil nil nil) (table-cell 396 407 nil nil nil) (table-cell 407 414 nil nil nil) (table-cell 414 423 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 5 2 paragraph) (\"schema\" 6 2 paragraph) (\"endpoint\" 7 2 paragraph) (\"Review docs\" 2 2 paragraph) (\"intro\" 3 2 paragraph) (\"examples\" 4 2 paragraph) (\"Ship release\" 8 2 paragraph) (\"Task\" 10 2 table-row) (\"Sum\" 15 2 table-row)) ((#(\"Task\" 0 4 (face org-table)) #(\"Estimate\" 0 8 (face org-table)) #(\"Done\" 0 4 (face org-table)) #(\"Weight\" 0 6 (face org-table))) hline (#(\"API\" 0 3 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table))) (#(\"Docs\" 0 4 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"5\" 0 1 (face org-table))) (#(\"Ship\" 0 4 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table))) (#(\"Sum\" 0 3 (face org-table)) \"\" \"\" #(\"13\" 0 2 (face org-table))))) (headline-cycle \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |      6 |\\n| Docs |        2 |    1 |      5 |\\n| Ship |        1 |    0 |      2 |\\n| Sum  |          |      |     13 |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Review docs [2/2]\" nil (103 118) (1) unordered) (135 \"- [X] Implement API\" nil (156 171) (2) unordered) (187 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 503 nil \"TODO\" \"Sprint [6/7] [85%]\") (item 79 136 on nil nil) (item 103 118 on nil nil) (item 118 136 on nil nil) (item 136 188 on nil nil) (item 156 171 on nil nil) (item 171 188 on nil nil) (item 188 207 off nil nil) (table 208 503 nil nil nil) (table-row 208 244 nil nil nil) (table-cell 209 216 nil nil nil) (table-cell 216 227 nil nil nil) (table-cell 227 234 nil nil nil) (table-cell 234 243 nil nil nil) (table-row 244 280 nil nil nil) (table-row 280 316 nil nil nil) (table-cell 281 288 nil nil nil) (table-cell 288 299 nil nil nil) (table-cell 299 306 nil nil nil) (table-cell 306 315 nil nil nil) (table-row 316 352 nil nil nil) (table-cell 317 324 nil nil nil) (table-cell 324 335 nil nil nil) (table-cell 335 342 nil nil nil) (table-cell 342 351 nil nil nil) (table-row 352 388 nil nil nil) (table-cell 353 360 nil nil nil) (table-cell 360 371 nil nil nil) (table-cell 371 378 nil nil nil) (table-cell 378 387 nil nil nil) (table-row 388 424 nil nil nil) (table-cell 389 396 nil nil nil) (table-cell 396 407 nil nil nil) (table-cell 407 414 nil nil nil) (table-cell 414 423 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 5 2 paragraph) (\"schema\" 6 2 paragraph) (\"endpoint\" 7 nil paragraph) (\"Review docs\" 2 nil paragraph) (\"intro\" 3 2 paragraph) (\"examples\" 4 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((#(\"Task\" 0 4 (face org-table)) #(\"Estimate\" 0 8 (face org-table)) #(\"Done\" 0 4 (face org-table)) #(\"Weight\" 0 6 (face org-table))) hline (#(\"API\" 0 3 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table))) (#(\"Docs\" 0 4 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"5\" 0 1 (face org-table))) (#(\"Ship\" 0 4 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table))) (#(\"Sum\" 0 3 (face org-table)) \"\" \"\" #(\"13\" 0 2 (face org-table))))) (headline-cycle \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |      6 |\\n| Docs |        2 |    1 |      5 |\\n| Ship |        1 |    0 |      2 |\\n| Sum  |          |      |     13 |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Review docs [2/2]\" nil (103 118) (1) unordered) (135 \"- [X] Implement API\" nil (156 171) (2) unordered) (187 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 503 nil \"TODO\" \"Sprint [6/7] [85%]\") (item 79 136 on nil nil) (item 103 118 on nil nil) (item 118 136 on nil nil) (item 136 188 on nil nil) (item 156 171 on nil nil) (item 171 188 on nil nil) (item 188 207 off nil nil) (table 208 503 nil nil nil) (table-row 208 244 nil nil nil) (table-cell 209 216 nil nil nil) (table-cell 216 227 nil nil nil) (table-cell 227 234 nil nil nil) (table-cell 234 243 nil nil nil) (table-row 244 280 nil nil nil) (table-row 280 316 nil nil nil) (table-cell 281 288 nil nil nil) (table-cell 288 299 nil nil nil) (table-cell 299 306 nil nil nil) (table-cell 306 315 nil nil nil) (table-row 316 352 nil nil nil) (table-cell 317 324 nil nil nil) (table-cell 324 335 nil nil nil) (table-cell 335 342 nil nil nil) (table-cell 342 351 nil nil nil) (table-row 352 388 nil nil nil) (table-cell 353 360 nil nil nil) (table-cell 360 371 nil nil nil) (table-cell 371 378 nil nil nil) (table-cell 378 387 nil nil nil) (table-row 388 424 nil nil nil) (table-cell 389 396 nil nil nil) (table-cell 396 407 nil nil nil) (table-cell 407 414 nil nil nil) (table-cell 414 423 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 5 nil paragraph) (\"schema\" 6 nil paragraph) (\"endpoint\" 7 nil paragraph) (\"Review docs\" 2 nil paragraph) (\"intro\" 3 nil paragraph) (\"examples\" 4 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((#(\"Task\" 0 4 (face org-table)) #(\"Estimate\" 0 8 (face org-table)) #(\"Done\" 0 4 (face org-table)) #(\"Weight\" 0 6 (face org-table))) hline (#(\"API\" 0 3 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table))) (#(\"Docs\" 0 4 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"5\" 0 1 (face org-table))) (#(\"Ship\" 0 4 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table))) (#(\"Sum\" 0 3 (face org-table)) \"\" \"\" #(\"13\" 0 2 (face org-table))))) (context-examples \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |      6 |\\n| Docs |        2 |    1 |      5 |\\n| Ship |        1 |    0 |      2 |\\n| Sum  |          |      |     13 |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Review docs [2/2]\" nil (103 118) (1) unordered) (135 \"- [X] Implement API\" nil (156 171) (2) unordered) (187 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 503 nil \"TODO\" \"Sprint [6/7] [85%]\") (item 79 136 on nil nil) (item 103 118 on nil nil) (item 118 136 on nil nil) (item 136 188 on nil nil) (item 156 171 on nil nil) (item 171 188 on nil nil) (item 188 207 off nil nil) (table 208 503 nil nil nil) (table-row 208 244 nil nil nil) (table-cell 209 216 nil nil nil) (table-cell 216 227 nil nil nil) (table-cell 227 234 nil nil nil) (table-cell 234 243 nil nil nil) (table-row 244 280 nil nil nil) (table-row 280 316 nil nil nil) (table-cell 281 288 nil nil nil) (table-cell 288 299 nil nil nil) (table-cell 299 306 nil nil nil) (table-cell 306 315 nil nil nil) (table-row 316 352 nil nil nil) (table-cell 317 324 nil nil nil) (table-cell 324 335 nil nil nil) (table-cell 335 342 nil nil nil) (table-cell 342 351 nil nil nil) (table-row 352 388 nil nil nil) (table-cell 353 360 nil nil nil) (table-cell 360 371 nil nil nil) (table-cell 371 378 nil nil nil) (table-cell 378 387 nil nil nil) (table-row 388 424 nil nil nil) (table-cell 389 396 nil nil nil) (table-cell 396 407 nil nil nil) (table-cell 407 414 nil nil nil) (table-cell 414 423 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 5 nil paragraph) (\"schema\" 6 nil paragraph) (\"endpoint\" 7 nil paragraph) (\"Review docs\" 2 nil paragraph) (\"intro\" 3 nil paragraph) (\"examples\" 4 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((#(\"Task\" 0 4 (face org-table)) #(\"Estimate\" 0 8 (face org-table)) #(\"Done\" 0 4 (face org-table)) #(\"Weight\" 0 6 (face org-table))) hline (#(\"API\" 0 3 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table))) (#(\"Docs\" 0 4 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"5\" 0 1 (face org-table))) (#(\"Ship\" 0 4 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table))) (#(\"Sum\" 0 3 (face org-table)) \"\" \"\" #(\"13\" 0 2 (face org-table))))) (final \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |      6 |\\n| Docs |        2 |    1 |      5 |\\n| Ship |        1 |    0 |      2 |\\n| Sum  |          |      |     13 |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\" ((78 \"- [X] Review docs [2/2]\" nil (103 118) (1) unordered) (135 \"- [X] Implement API\" nil (156 171) (2) unordered) (187 \"- [ ] Ship release\" nil nil (3) unordered)) ((headline 1 503 nil \"TODO\" \"Sprint [6/7] [85%]\") (item 79 136 on nil nil) (item 103 118 on nil nil) (item 118 136 on nil nil) (item 136 188 on nil nil) (item 156 171 on nil nil) (item 171 188 on nil nil) (item 188 207 off nil nil) (table 208 503 nil nil nil) (table-row 208 244 nil nil nil) (table-cell 209 216 nil nil nil) (table-cell 216 227 nil nil nil) (table-cell 227 234 nil nil nil) (table-cell 234 243 nil nil nil) (table-row 244 280 nil nil nil) (table-row 280 316 nil nil nil) (table-cell 281 288 nil nil nil) (table-cell 288 299 nil nil nil) (table-cell 299 306 nil nil nil) (table-cell 306 315 nil nil nil) (table-row 316 352 nil nil nil) (table-cell 317 324 nil nil nil) (table-cell 324 335 nil nil nil) (table-cell 335 342 nil nil nil) (table-cell 342 351 nil nil nil) (table-row 352 388 nil nil nil) (table-cell 353 360 nil nil nil) (table-cell 360 371 nil nil nil) (table-cell 371 378 nil nil nil) (table-cell 378 387 nil nil nil) (table-row 388 424 nil nil nil) (table-cell 389 396 nil nil nil) (table-cell 396 407 nil nil nil) (table-cell 407 414 nil nil nil) (table-cell 414 423 nil nil nil)) ((\"Sprint\" 1 nil headline) (\"Implement API\" 5 nil paragraph) (\"schema\" 6 nil paragraph) (\"endpoint\" 7 nil paragraph) (\"Review docs\" 2 nil paragraph) (\"intro\" 3 nil paragraph) (\"examples\" 4 nil paragraph) (\"Ship release\" 8 nil paragraph) (\"Task\" 10 nil table-row) (\"Sum\" 15 nil table-row)) ((#(\"Task\" 0 4 (face org-table)) #(\"Estimate\" 0 8 (face org-table)) #(\"Done\" 0 4 (face org-table)) #(\"Weight\" 0 6 (face org-table))) hline (#(\"API\" 0 3 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table))) (#(\"Docs\" 0 4 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"5\" 0 1 (face org-table))) (#(\"Ship\" 0 4 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"0\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table))) (#(\"Sum\" 0 3 (face org-table)) \"\" \"\" #(\"13\" 0 2 (face org-table)))))) (unordered (\"[X] Review docs [2/2]\" (ordered (\"[X] intro\") (\"[X] examples\"))) (\"[X] Implement API\" (unordered (\"[X] schema\") (\"[X] endpoint\"))) (\"[ ] Ship release\")) \"* TODO Sprint [6/7] [85%]                                              :work:\\n- [X] Review docs [2/2]\\n  1. [X] intro\\n  2. [X] examples\\n- [X] Implement API\\n  - [X] schema\\n  - [X] endpoint\\n- [ ] Ship release\\n\\n| Task | Estimate | Done | Weight |\\n|------+----------+------+--------|\\n| API  |        3 |    0 |      6 |\\n| Docs |        2 |    1 |      5 |\\n| Ship |        1 |    0 |      2 |\\n| Sum  |          |      |     13 |\\n#+TBLFM: @2$4=$2*2+@2$3::@3$4=$2*2+@3$3::@4$4=$2*2+@4$3::@5$4=vsum(@2$4..@4$4)\\n\")""#
    ]];
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
                        (org-list-get-all-items (caar struct) struct prevs))
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
                (save-excursion
                  (goto-char (point-min))
                  (search-forward "- [")
                  (org-list-to-lisp))
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_list_make_subtree_checkbox_counter_roundtrip_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((ordered (\"[@c] [X] Alpha :: first line\\ncontinuation alpha\" (descriptive (\"[ ] Alpha child :: child def\\nchild body\"))) (\"[-] Beta [1/2] :: second line\" (ordered (\"[X] Beta done\") (\"[ ] Beta todo\")))) \"* Parent [0/2]\\nIntro paragraph\\na. [@c] [X] Alpha :: first line\\n   continuation alpha\\n   - [ ] Alpha child :: child def\\n     child body\\nd. [-] Beta [1/2] :: second line\\n   1) [X] Beta done\\n   2) [ ] Beta todo\\n* Tail\\n\" ((1 nil \"Parent [0/0]\" nil 1 194) (2 \"DONE\" \"Alpha :: first line\" nil 32 122) (3 \"TODO\" \"Alpha child child def\" nil 79 122) (2 \"TODO\" \"Beta [0/0] :: second line\" nil 122 194) (3 \"DONE\" \"Beta done\" nil 156 175) (3 \"TODO\" \"Beta todo\" nil 175 194) (1 nil \"Tail\" nil 194 201)) nil \"* Parent [0/0]\\nIntro paragraph\\n** DONE Alpha :: first line\\ncontinuation alpha\\n*** TODO  Alpha child child def\\nchild body\\n** TODO Beta [0/0] :: second line\\n*** DONE Beta done\\n*** TODO Beta todo\\n* Tail\\n\" \"*** DONE Alpha :: first line\\ncontinuation alpha\\nTODO  Alpha child child def\\nchild body\\nTODO Beta [1/2] :: second line\\nDONE Beta done\\nTODO Beta todo\" \"* Parent [0/0]\\nIntro paragraph\\n*** DONE Alpha :: first line\\n continuation alpha\\n**** TODO  Alpha child child def\\n child body\\n* TODO Beta [0/0] :: second line\\n** DONE Beta done\\n** TODO Beta todo\\n* Tail\\n\")""#
    ]];
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
                  after-level-edits)))))))"##,
        expect,
    );
}

#[test]
fn org_list_checkbox_dependency_sort_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((initial ok (initial-state t ((26 \"- [ ] Zebra [0/2]\" \"[ ]\" nil (45 67) (1) unordered nil) (88 \"- [-] Alpha [1/2]\" \"[-]\" nil (107 126) (2) unordered nil) (144 \"- [ ] Mango\" \"[ ]\" nil nil (3) unordered nil) (156 \"- [X] Done item\" \"[X]\" nil nil (4) unordered nil)) (unordered (\"[ ] Zebra [0/2]\" (unordered (\"[ ] Zebra child b\") (\"[X] Zebra child a\"))) (\"[-] Alpha [1/2]\" (unordered (\"[X] Alpha done\") (\"[ ] Alpha todo\"))) (\"[ ] Mango\") (\"[X] Done item\")) \"* TODO Project [0/4] [0%]\\n- [ ] Zebra [0/2]\\n  - [ ] Zebra child b\\n  - [X] Zebra child a\\n- [-] Alpha [1/2]\\n  - [X] Alpha done\\n  - [ ] Alpha todo\\n- [ ] Mango\\n- [X] Done item\\n\")) (after-checks nil ((28 \"- [X] Zebra [2/2]\" \"[X]\" nil (47 69) (1) unordered nil) (90 \"- [X] Alpha [2/2]\" \"[X]\" nil (109 128) (2) unordered nil) (146 \"- [X] Mango\" \"[X]\" nil nil (3) unordered nil) (158 \"- [X] Done item\" \"[X]\" nil nil (4) unordered nil)) (unordered (\"[X] Zebra [2/2]\" (unordered (\"[X] Zebra child b\") (\"[X] Zebra child a\"))) (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[X] Mango\") (\"[X] Done item\")) \"* TODO Project [4/4] [100%]\\n- [X] Zebra [2/2]\\n  - [X] Zebra child b\\n  - [X] Zebra child a\\n- [X] Alpha [2/2]\\n  - [X] Alpha done\\n  - [X] Alpha todo\\n- [X] Mango\\n- [X] Done item\\n\") (after-move-indent nil ((28 \"- [X] Zebra [2/2]\" \"[X]\" nil (47 69) (1) unordered nil) (90 \"- [X] Alpha [3/3]\" \"[X]\" nil (109 128 147) (2) unordered nil) (160 \"- [X] Done item\" \"[X]\" nil nil (3) unordered nil)) (unordered (\"[X] Zebra [2/2]\" (unordered (\"[X] Zebra child a\") (\"[X] Zebra child b\"))) (\"[X] Alpha [3/3]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\") (\"[X] Mango\"))) (\"[X] Done item\")) \"* TODO Project [3/3] [100%]\\n- [X] Zebra [2/2]\\n  - [X] Zebra child a\\n  - [X] Zebra child b\\n- [X] Alpha [3/3]\\n  - [X] Alpha done\\n  - [X] Alpha todo\\n  - [X] Mango\\n- [X] Done item\\n\") (after-sort-repair nil ((28 \"- [X] Alpha [2/2]\" \"[X]\" nil (47 66) (1) unordered nil) (84 \"- [X] Done item\" \"[X]\" nil nil (2) unordered nil) (100 \"- [X] Mango\" \"[X]\" nil nil (3) unordered nil) (112 \"- [X] Zebra [2/2]\" \"[X]\" nil (131 153) (4) unordered nil)) (unordered (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[X] Done item\") (\"[X] Mango\") (\"[X] Zebra [2/2]\" (unordered (\"[X] Zebra child a\") (\"[X] Zebra child b\")))) \"* TODO Project [4/4] [100%]\\n- [X] Alpha [2/2]\\n  - [X] Alpha done\\n  - [X] Alpha todo\\n- [X] Done item\\n- [X] Mango\\n- [X] Zebra [2/2]\\n  - [X] Zebra child a\\n  - [X] Zebra child b\\n\") (after-cycle nil ((28 \"- [X] Alpha [2/2]\" \"[X]\" nil (47 66) (1) unordered nil) (84 \"- [X] Done item\" \"[X]\" nil nil (2) unordered nil) (100 \"- [X] Mango\" \"[X]\" nil nil (3) unordered nil) (112 \"- [X] Zebra [2/2]\" \"[X]\" nil (131 153) (4) unordered nil)) (unordered (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[X] Done item\") (\"[X] Mango\") (\"[X] Zebra [2/2]\" (unordered (\"[X] Zebra child a\") (\"[X] Zebra child b\")))) \"* TODO Project [4/4] [100%]\\n- [X] Alpha [2/2]\\n  - [X] Alpha done\\n  - [X] Alpha todo\\n- [X] Done item\\n- [X] Mango\\n- [X] Zebra [2/2]\\n  - [X] Zebra child a\\n  - [X] Zebra child b\\n\") (done-attempt ok #(\"DONE\" 0 4 (org-todo-head \"TODO\")) (after-done nil ((28 \"- [X] Alpha [2/2]\" \"[X]\" nil (47 66) (1) unordered nil) (84 \"- [X] Done item\" \"[X]\" nil nil (2) unordered nil) (100 \"- [X] Mango\" \"[X]\" nil nil (3) unordered nil) (112 \"- [X] Zebra [2/2]\" \"[X]\" nil (131 153) (4) unordered nil)) (unordered (\"[X] Alpha [2/2]\" (unordered (\"[X] Alpha done\") (\"[X] Alpha todo\"))) (\"[X] Done item\") (\"[X] Mango\") (\"[X] Zebra [2/2]\" (unordered (\"[X] Zebra child a\") (\"[X] Zebra child b\")))) \"* DONE Project [4/4] [100%]\\n- [X] Alpha [2/2]\\n  - [X] Alpha done\\n  - [X] Alpha todo\\n- [X] Done item\\n- [X] Mango\\n- [X] Zebra [2/2]\\n  - [X] Zebra child a\\n  - [X] Zebra child b\\n\"))) ((headline \"DONE\" \"Project [4/4] [100%]\") (item on nil nil) (item on nil nil) (item on nil nil) (item on nil nil) (item on nil nil) (item on nil nil) (item on nil nil) (item on nil nil)) \"* DONE Project [4/4] [100%]\\n- [X] Alpha [2/2]\\n  - [X] Alpha done\\n  - [X] Alpha todo\\n- [X] Done item\\n- [X] Mango\\n- [X] Zebra [2/2]\\n  - [X] Zebra child a\\n  - [X] Zebra child b\\n\")""#
    ]];
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
               (let* ((struct (save-excursion
                                (goto-char (point-min))
                                (search-forward "Zebra")
                                (org-list-struct)))
                      (prevs (org-list-prevs-alist struct))
                      (parents (org-list-parents-alist struct))
                      (items (org-list-get-all-items (caar struct) struct prevs)))
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
                       (save-excursion
                         (goto-char (caar struct))
                         (org-list-to-lisp))
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
        (goto-char (point-min))
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
               (point-min) (point-max)))))))"##,
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

    let expect = expect_test::expect![[
        r#""OK ((\"* Tasks [0/4]\\n- [-] Item A\\n  - [X] Sub A1\\n  - [ ] Sub A2\\n- [ ] Item B\\n  - [ ] Sub B1\\n- [ ] Item C\\n- [ ] Item D\\n\" ((trans nil) (on nil) (off nil) (off nil) (off nil) (off nil) (off nil))) (\"* Tasks [1/4]\\n- [X] Item A\\n  - [X] Sub A1\\n  - [X] Sub A2\\n- [ ] Item B\\n  - [ ] Sub B1\\n- [ ] Item C\\n- [ ] Item D\\n\" ((on nil) (on nil) (on nil) (off nil) (off nil) (off nil) (off nil))) (\"* Tasks [1/4]\\n- [X] Item A\\n  - [X] Sub A1\\n  - [X] Sub A2\\n- [ ] Item B\\n  - [ ] Sub B1\\n- [ ] Item C\\n- [ ] Item D\\n\" ((on nil) (on nil) (on nil) (off nil) (off nil) (off nil) (off nil))) (\"* Tasks [1/4]\\n- [X] Item A\\n  - [X] Sub A1\\n  - [X] Sub A2\\n- [ ] Item B\\n  - [ ] Sub B1\\n- [ ] Item C\\n- [ ] Item D\\n\" ((on nil) (on nil) (on nil) (off nil) (off nil) (off nil) (off nil))) (\"* Tasks [1/4]\\n- [X] Item A\\n  - [X] Sub A1\\n  - [X] Sub A2\\n- [ ] Item B\\n  - [ ] Sub B1\\n- [ ] Item C\\n- [ ] Item D\\n\" ((on nil) (on nil) (on nil) (off nil) (off nil) (off nil) (off nil))) (\"* Tasks [1/4]\\n- [X] Item A\\n  - [X] Sub A1\\n  - [X] Sub A2\\n- [ ] Item B\\n  - [ ] Sub B1\\n- [ ] Item C\\n- [ ] Item D\\n\" ((on nil) (on nil) (on nil) (off nil) (off nil) (off nil) (off nil))))""#
    ]];
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
                          after-stats))))))))))"##,
        expect,
    );
}

#[test]
fn org_list_structure_indent_outdent_renumber_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"1. First\\n   - Child A\\n   - Child B\\n     - Grandchild\\n2. Second\\n   - [X] Checked\\n   - [ ] Unchecked\\n3. Third\\n\" ((\"1. \" nil nil 1) (\"- \" nil nil 10) (\"- \" nil nil 23) (\"- \" nil nil 36) (\"2. \" nil nil 54) (\"- \" on nil 64) (\"- \" off nil 81) (\"3. \" nil nil 100))) (\"1. First\\n   - Child A\\n     - Child B\\n     - Grandchild\\n2. Second\\n   - [X] Checked\\n   - [ ] Unchecked\\n3. Third\\n\" ((\"1. \" nil nil 1) (\"- \" nil nil 10) (\"- \" nil nil 23) (\"- \" nil nil 38) (\"2. \" nil nil 56) (\"- \" on nil 66) (\"- \" off nil 83) (\"3. \" nil nil 102))) (\"1. First\\n   - Child A\\n   - Child B\\n     - Grandchild\\n2. Second\\n   - [X] Checked\\n   - [ ] Unchecked\\n3. Third\\n\" ((\"1. \" nil nil 1) (\"- \" nil nil 10) (\"- \" nil nil 23) (\"- \" nil nil 36) (\"2. \" nil nil 54) (\"- \" on nil 64) (\"- \" off nil 81) (\"3. \" nil nil 100))) (\"1. First\\n   - Child A\\n   - Child B\\n     - Grandchild\\n2. Second\\n   - [X] Checked\\n   - [X] Unchecked\\n3. Third\\n\" ((\"1. \" nil nil 1) (\"- \" nil nil 10) (\"- \" nil nil 23) (\"- \" nil nil 36) (\"2. \" nil nil 54) (\"- \" on nil 64) (\"- \" on nil 81) (\"3. \" nil nil 100))) (\"1. First\\n   - Child A\\n   - Child B\\n     - Grandchild\\n2. Second\\n   - [X] Checked\\n   - [X] Unchecked\\n3. Third\\n\" ((\"1. \" nil nil 1) (\"- \" nil nil 10) (\"- \" nil nil 23) (\"- \" nil nil 36) (\"2. \" nil nil 54) (\"- \" on nil 64) (\"- \" on nil 81) (\"3. \" nil nil 100))) (\"1. Third\\n2. Second\\n   - [X] Checked\\n   - [X] Unchecked\\n3. First\\n   - Child A\\n   - Child B\\n     - Grandchild\\n\" ((\"1. \" nil nil 1) (\"2. \" nil nil 10) (\"- \" on nil 20) (\"- \" on nil 37) (\"3. \" nil nil 56) (\"- \" nil nil 65) (\"- \" nil nil 78) (\"- \" nil nil 91))))""#
    ]];
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
                      after-sort)))))))))"##,
        expect,
    );
}

#[test]
fn org_list_checkbox_toggle_update_sort_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"- [ ] Item alpha\\n- [ ] Item beta\\n- [X] Item gamma\\n- [ ] Item delta\\n  - [ ] Sub A\\n  - [X] Sub B\\n\" \"- [X] Item alpha\\n- [ ] Item beta\\n- [X] Item gamma\\n- [-] Item delta\\n  - [ ] Sub A\\n  - [X] Sub B\\n\" \"- [X] Item alpha\\n- [ ] Item beta\\n- [X] Item gamma\\n- [-] Item delta\\n  - [ ] Sub A\\n  - [X] Sub B\\n\" \"- [X] Item gamma\\n- [-] Item delta\\n  - [ ] Sub A\\n  - [X] Sub B\\n- [ ] Item beta\\n- [X] Item alpha\\n\" \"- [X] Item gamma\\n- [-] Item delta\\n  - [ ] Sub A\\n  - [X] Sub B\\n- [ ] Item beta\\n- [X] Item alpha\\n- [ ] Item epsilon\\n\" \"- [X] Item gamma\\n- [-] Item delta\\n  - [X] Sub A\\n  - [X] Sub B\\n- [X] Item beta\\n- [X] Item alpha\\n- [X] Item epsilon\\n\")""#
    ]];
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
                        after-replace))))))))))"##,
        expect,
    );
}
