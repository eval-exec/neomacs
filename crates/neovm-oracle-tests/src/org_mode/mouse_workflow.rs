use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_mouse_timestamp_options_visibility_mutation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""Date+time [2026-05-27]: OK nil""#]];
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
                  (point-min) (point-max))))))))"##,
        expect,
    );
}
