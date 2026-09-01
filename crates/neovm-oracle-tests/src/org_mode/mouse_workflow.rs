use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_mouse_insert_menu_priority_checkbox_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-mouse)
  (with-temp-buffer
    (let ((org-priority-lowest ?D)
          (org-priority-default ?B)
          (org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE")))
          menu-after-actions replace-menu)
      (org-mode)
      (insert "* TODO [#A] Alpha :work:urgent:\n")
      (insert "- item one\n")
      (insert "- [ ] item two\n")
      (insert "Middle line\n")
      (insert "\n")
      (let ((line-states nil)
            (menus nil))
        (goto-char (point-min))
        (search-forward "Alpha")
        (push (list 'headline-middle
                    (org-mouse-line-position)
                    (org-mouse-get-priority)
                    (org-mouse-get-priority t))
              line-states)
        (org-mouse-end-headline)
        (push (list 'headline-end
                    (point)
                    (buffer-substring-no-properties
                     (line-beginning-position) (point)))
              line-states)
        (goto-char (point-min))
        (search-forward "item one")
        (beginning-of-line)
        (push (list 'item-begin (org-mouse-line-position))
              line-states)
        (org-mouse-insert-checkbox)
        (goto-char (point-min))
        (search-forward "item two")
        (beginning-of-line)
        (org-mouse-for-each-item 'org-mouse-insert-checkbox)
        (goto-char (point-max))
        (org-mouse-insert-heading)
        (insert "Inserted heading")
        (goto-char (point-min))
        (search-forward "Middle")
        (org-mouse-insert-item "dropped text")
        (goto-char (point-max))
        (org-mouse-insert-item "tail text")
        (goto-char (point-min))
        (search-forward "TODO")
        (push (list 'todo-menu
                    (mapcar (lambda (item)
                              (and (vectorp item)
                                   (list (aref item 0)
                                         (aref item 2)
                                         (aref item 4))))
                            (org-mouse-todo-menu "TODO")))
              menus)
        (goto-char (point-min))
        (search-forward ":work:")
        (push (list 'tag-menu
                    (mapcar (lambda (item)
                              (cond ((vectorp item)
                                     (list (aref item 0)
                                           (aref item 2)
                                           (aref item 4)))
                                    (t item)))
                            (org-mouse-tag-menu)))
              menus)
        (goto-char (point-min))
        (search-forward "[#A]")
        (setq replace-menu
              (mapcar (lambda (item)
                        (cond ((vectorp item)
                               (list (aref item 0)
                                     (aref item 2)
                                     (aref item 4)))
                              (t item)))
                      (org-mouse-keyword-replace-menu
                       (org-mouse-priority-list) 1 "Priority %s" t)))
        (funcall (aref (nth 2 (org-mouse-keyword-replace-menu
                               (org-mouse-priority-list) 1
                               "Priority %s" t))
                       1))
        (setq menu-after-actions
              (buffer-substring-no-properties (point-min) (point-max)))
        (list (nreverse line-states)
              (nreverse menus)
              replace-menu
              menu-after-actions
              (org-mouse-clip-text "abcdefghijklmnopqrstuvwxyz" 12)
              (org-mouse-agenda-type 'todo-tree)
              (org-mouse-agenda-type 'unknown)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_mouse_timestamp_options_visibility_mutation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-mouse)
  (require 'org-cycle)
  (with-temp-buffer
    (let ((org-priority-lowest ?C)
          (org-priority-default ?B)
          (org-startup-folded nil)
          (org-todo-keywords '((sequence "TODO" "NEXT" "|" "DONE")))
          (current-time-fn (lambda () (encode-time 0 0 12 27 5 2026))))
      (cl-letf (((symbol-function 'current-time) current-time-fn))
        (org-mode)
        (insert "* TODO [#A] Alpha :work:\n")
        (insert "SCHEDULED: <2026-05-27 Wed>\n")
        (insert "#+OPTIONS: toc:nil num:t author:nil\n")
        (insert "** NEXT Child\nChild body\n")
        (insert "*** Grand\nGrand body\n")
        (insert "* Tail\nTail body\n")
        (let (states)
          (goto-char (point-min))
          (search-forward "SCHEDULED")
          (org-mouse-delete-timestamp)
          (push (list 'after-delete-timestamp
                      (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position)))
                states)
          (goto-char (point-min))
          (search-forward "Alpha")
          (end-of-line)
          (insert "\n")
          (org-mouse-timestamp-today 2 'day)
          (push (list 'after-insert-shifted-date
                      (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position)))
                states)
          (goto-char (point-min))
          (search-forward "[#A]")
          (org-mouse-set-priority "C")
          (push (list 'after-priority
                      (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position))
                      (org-mouse-get-priority t))
                states)
          (goto-char (point-min))
          (search-forward "TODO")
          (let ((todo-menu (org-mouse-keyword-replace-menu
                            '("TODO" "NEXT" "DONE") 0 "State %s")))
            (funcall (aref (nth 1 todo-menu) 1))
            (push (list 'after-todo-menu
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position)))
                  states))
          (goto-char (point-min))
          (re-search-forward "#\\+OPTIONS: \\(.*\\)")
          (let ((option-menu
                 (org-mouse-list-options-menu '("author:nil" "num:t"
                                                "toc:nil" "todo:nil"))))
            (funcall (aref (nth 3 option-menu) 1))
            (push (list 'after-option-toggle
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position))
                        (mapcar (lambda (item)
                                  (list (aref item 0)
                                        (aref item 3)))
                                option-menu))
                  states))
          (goto-char (point-min))
          (search-forward "NEXT")
          (let ((none-menu
                 (org-mouse-keyword-replace-menu
                  '("TODO" "NEXT" "DONE") 0 nil t)))
            (funcall (aref (car (last none-menu)) 1))
            (push (list 'after-remove-keyword
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position)))
                  states))
          (org-mouse-show-headlines)
          (push (list 'after-headlines
                      (mapcar (lambda (needle)
                                (save-excursion
                                  (goto-char (point-min))
                                  (search-forward needle)
                                  (not (null (invisible-p (point))))))
                              '("Child body" "Grand" "Tail body")))
                states)
          (org-mouse-show-overview)
          (push (list 'after-overview
                      (mapcar (lambda (needle)
                                (save-excursion
                                  (goto-char (point-min))
                                  (search-forward needle)
                                  (not (null (invisible-p (point))))))
                              '("Alpha" "Child" "Tail" "Tail body")))
                states)
          (list (nreverse states)
                (org-mouse-clip-text "short" 12)
                (org-mouse-clip-text "0123456789abcdef" 10)
                 (buffer-substring-no-properties
                  (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_mouse_context_menu_move_drag_link_open_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function org-mouse-move-subtree-down)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-mouse)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Alpha :work:\n")
    (insert "Body with [[https://example.org][link]].\n")
    (insert "** Child\nchild body\n")
    (insert "* Beta :home:\n")
    (insert "Beta body.\n")
    (insert "* Gamma\n")
    (insert "Gamma body.\n")
    (let ((snap (lambda ()
                  (list (buffer-substring-no-properties
                         (point-min) (point-max))
                        (org-element-map (org-element-parse-buffer) 'headline
                          (lambda (h)
                            (list (org-element-property :raw-value h)
                                  (org-get-tags nil t))))))))
      ;; Show context menu items
      (let ((context-items
             (condition-case nil
                 (progn
                   (goto-char (point-min))
                   (search-forward "Alpha")
                   (beginning-of-line)
                   (org-mouse-popup-context-menu))
               (error 'error))))
        ;; Move subtree down
        (goto-char (point-min))
        (search-forward "Alpha")
        (beginning-of-line)
        (org-mouse-move-subtree-down)
        (let ((after-move (funcall snap)))
          ;; Move subtree up
          (goto-char (point-min))
          (search-forward "Gamma")
          (beginning-of-line)
          (org-mouse-move-subtree-up)
          (let ((after-move-up (funcall snap)))
            ;; Toggle tag
            (goto-char (point-min))
            (search-forward "Beta")
            (beginning-of-line)
            (org-mouse-toggle-tag "urgent")
            (let ((after-tag (funcall snap)))
              (list context-items
                    after-move
                    after-move-up
                    after-tag)))))))))"##,
        expect,
    );
}
