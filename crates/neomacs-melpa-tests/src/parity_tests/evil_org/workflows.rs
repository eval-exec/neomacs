use expect_test::expect;

use super::ParityBatchCase;

fn empty_element_detects_blank_items_and_table_rows() -> ParityBatchCase {
    ParityBatchCase::value(
        "empty_element_detects_blank_items_and_table_rows",
        r####"
(list :empty-item
      (neomacs-evil-org-test-with-buffer
       "- \n"
       (lambda ()
         (goto-char (point-min))
         (and (evil-org--empty-element-p) t)))
      :filled-item
      (neomacs-evil-org-test-with-buffer
       "- hello\n"
       (lambda ()
         (goto-char (point-min))
         (and (evil-org--empty-element-p) t)))
      :empty-table
      (neomacs-evil-org-test-with-buffer
       "|   |   |\n"
       (lambda ()
         (goto-char (point-min))
         (forward-char 2)
         (and (evil-org--empty-element-p) t)))
      :heading-not-empty
      (neomacs-evil-org-test-with-buffer
       "* Head\n"
       (lambda ()
         (goto-char (point-min))
         (and (evil-org--empty-element-p) t))))
"####,
        expect!["OK (:empty-item t :filled-item nil :empty-table t :heading-not-empty nil)"],
    )
}

fn open_below_on_item_inserts_sibling_item() -> ParityBatchCase {
    ParityBatchCase::value(
        "open_below_on_item_inserts_sibling_item",
        r####"
(neomacs-evil-org-test-with-buffer
 "- one\n"
 (lambda ()
   (goto-char (point-min))
   (end-of-line)
   (let ((evil-org-special-o/O '(item table-row)))
     (cl-letf (((symbol-function 'evil-insert)
                (lambda (&rest _) nil)))
       (evil-org-open-below nil)))
   (list :text (buffer-string)
         :lines (length (split-string (buffer-string) "\n" t))
         :has-two-items
         (let ((n 0))
           (save-excursion
             (goto-char (point-min))
             (while (re-search-forward "^[ \t]*-" nil t)
               (cl-incf n)))
           n))))
"####,
        expect![[r#"OK (:text "- one\n- \n" :lines 2 :has-two-items 2)"#]],
    )
}

fn open_above_on_item_inserts_item_before() -> ParityBatchCase {
    ParityBatchCase::value(
        "open_above_on_item_inserts_item_before",
        r####"
(neomacs-evil-org-test-with-buffer
 "- bottom\n"
 (lambda ()
   (goto-char (point-min))
   (let ((evil-org-special-o/O '(item table-row)))
     (cl-letf (((symbol-function 'evil-insert)
                (lambda (&rest _) nil)))
       (evil-org-open-above nil)))
   (list :first-line
         (car (split-string (buffer-string) "\n" t))
         :item-count
         (let ((n 0))
           (save-excursion
             (goto-char (point-min))
             (while (re-search-forward "^[ \t]*-" nil t)
               (cl-incf n)))
           n)
         :contains-bottom
         (and (string-match-p "bottom" (buffer-string)) t))))
"####,
        expect![[r#"OK (:first-line "- " :item-count 2 :contains-bottom t)"#]],
    )
}

fn return_on_empty_item_clears_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "return_on_empty_item_clears_line",
        r####"
(neomacs-evil-org-test-with-buffer
 "- \n"
 (lambda ()
   (goto-char (point-min))
   (end-of-line)
   (evil-org-return nil)
   (list :text (buffer-substring-no-properties (point-min) (point-max))
         :blank (and (string-match-p "\\`[ \t]*\n?\\'" (buffer-string)) t))))
"####,
        expect![[r#"OK (:text "\n" :blank t)"#]],
    )
}

fn key_theme_installs_open_and_insert_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "key_theme_installs_open_and_insert_bindings",
        r####"
(with-temp-buffer
  (org-mode)
  (evil-mode 1)
  (evil-org-mode 1)
  (evil-org-set-key-theme '(navigation insert additional))
  (let ((nmap (evil-get-minor-mode-keymap 'normal 'evil-org-mode))
        (imap (evil-get-minor-mode-keymap 'insert 'evil-org-mode)))
    (list :o (lookup-key nmap (kbd "o"))
          :O (lookup-key nmap (kbd "O"))
          :I (lookup-key nmap (kbd "I"))
          :insert-C-t (lookup-key imap (kbd "C-t"))
          :mode evil-org-mode)))
"####,
        expect![
            "OK (:o evil-org-open-below :O evil-org-open-above :I evil-org-insert-line :insert-C-t org-metaright :mode t)"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        empty_element_detects_blank_items_and_table_rows(),
        open_below_on_item_inserts_sibling_item(),
        open_above_on_item_inserts_item_before(),
        return_on_empty_item_clears_line(),
        key_theme_installs_open_and_insert_bindings(),
    ]
}
