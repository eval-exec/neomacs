use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LEDGER_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'ledger-mode)

(setq ledger-mode-should-check-version nil
      ledger-init-file-name nil)

(defun ledger372-test-context-state ()
  (let ((context (ledger-context-at-point)))
    (list :line-type (ledger-context-line-type context)
          :field (ledger-context-current-field context)
          :fields
          (mapcar (lambda (field)
                    (list (car field) (copy-sequence (cadr field))))
                  (nth 2 context)))))

(defun ledger372-test-buffer-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :mark (mark t)
        :mark-active mark-active
        :modified (buffer-modified-p)
        :mode major-mode
        :thing (save-excursion (ledger-thing-at-point))
        :context (ledger372-test-context-state)))

(defun ledger372-test-mode-state ()
  (list :mode major-mode
        :mode-name mode-name
        :comment-start comment-start
        :completion
        (and (memq #'ledger-complete-at-point completion-at-point-functions) t)
        :after-save
        (cl-count #'ledger-report-redo-after-save after-save-hook :test #'eq)
        :post-command
        (cl-count #'ledger-highlight-xact-under-point post-command-hook :test #'eq)
        :keys
        (mapcar (lambda (key)
                  (list key (lookup-key ledger-mode-map (kbd key))))
                '("C-M-i" "M-p" "M-n" "C-c C-c" "C-c C-f"
                  "C-c C-o C-r"))))

(defun ledger372-test-context-at (needle &optional offset occurrence)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle nil nil (or occurrence 1))
    (goto-char (+ (match-beginning 0) (or offset 0)))
    (ledger372-test-context-state)))

(defun ledger372-test-face-at (needle)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (let ((position (match-beginning 0)))
      (list :face (copy-tree (get-text-property position 'face))
            :font-lock-face
            (copy-tree (get-text-property position 'font-lock-face))))))

(defun ledger372-test-visible-text ()
  (let ((position (point-min)) pieces)
    (while (< position (point-max))
      (unless (get-char-property position 'invisible)
        (push (char-to-string (char-after position)) pieces))
      (setq position (1+ position)))
    (apply #'concat (nreverse pieces))))

(defun ledger372-test-occur-overlays ()
  (mapcar
   (lambda (overlay)
     (list :start (overlay-start overlay)
           :end (overlay-end overlay)
           :custom (overlay-get overlay ledger-occur-overlay-property-name)
           :invisible (overlay-get overlay 'invisible)
           :font-lock-face (overlay-get overlay 'font-lock-face)
           :text (buffer-substring-no-properties
                  (overlay-start overlay) (overlay-end overlay))))
   (sort
    (cl-remove-if-not
     (lambda (overlay)
       (overlay-get overlay ledger-occur-overlay-property-name))
     (overlays-in (point-min) (point-max)))
    (lambda (left right)
      (or (< (overlay-start left) (overlay-start right))
          (and (= (overlay-start left) (overlay-start right))
               (< (overlay-end left) (overlay-end right))))))))

(defun ledger372-test-occur-state ()
  (list :mode ledger-occur-mode
        :regex ledger-occur-current-regex
        :lighter (copy-tree (assq 'ledger-occur-mode minor-mode-alist))
        :visible (ledger372-test-visible-text)
        :overlays (ledger372-test-occur-overlays)))

(defun ledger372-test-file-bytes (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (buffer-string)))

(defun ledger372-test-file-text (file)
  (decode-coding-string (ledger372-test-file-bytes file) 'utf-8))

(defun ledger372-test-normalize-paths (text root script)
  (replace-regexp-in-string
   (regexp-quote script) "[LEDGER]"
   (replace-regexp-in-string (regexp-quote root) "[ROOT]" text t t)
   t t))

(defun ledger372-test-delete-tree (root)
  (when (and (stringp root)
             (file-name-absolute-p root)
             (file-directory-p root)
             (not (file-symlink-p root)))
    (delete-directory root t)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LEDGER_MODE_MELPA_PIN, "ledger-mode.el")
        .expect("prepare pinned ledger-mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn mode_completion_and_navigation_cover_real_journal_structure() -> ParityBatchCase {
    let form = r####"
(with-temp-buffer
  (insert "account Assets:Checking\n"
          "account Expenses:Food:Groceries\n"
          "account Income:Salary\n"
          "payee Café März\n"
          "commodity $\n\n"
          "2024-03-12 * (REL-417) Café März\n"
          "    Assets:Checking              $ -50.25\n"
          "    Expenses:Food:Groceries       $ 50.25\n\n"
          "2024-03-15 Employer\n"
          "    Assets:Checking             $ 2000.00\n"
          "    Income:Salary              $ -2000.00\n\n"
          "2024-03-19 Market\n"
          "    Assets:Che")
  (ledger-mode)
  (font-lock-ensure)
  (let ((before
         (list :mode (ledger372-test-mode-state)
               :transaction (ledger372-test-context-at "Café März" 2 2)
               :posting (ledger372-test-context-at "Expenses:Food" 4 2)
               :faces
               (list :date (ledger372-test-face-at "2024-03-12")
                     :status (ledger372-test-face-at "*")
                     :payee (ledger372-test-face-at "Café März")
                     :account (ledger372-test-face-at "Expenses:Food")
                     :amount (ledger372-test-face-at "$ 50.25"))
               :accounts (ledger-accounts-list)
               :payees (ledger-payees-list))))
    (goto-char (point-max))
    (let ((capf (ledger-complete-at-point))
          (completion-result (completion-at-point)))
      (let ((completed (ledger372-test-buffer-state)))
        (call-interactively (key-binding (kbd "M-p")))
        (let ((previous (ledger372-test-buffer-state)))
          (call-interactively (key-binding (kbd "M-n")))
          (let ((next (ledger372-test-buffer-state)))
            (call-interactively (key-binding (kbd "M-n")))
            (list :before before
                  :capf (list :start (car capf)
                              :end (cadr capf)
                              :exit-function
                              (functionp
                               (plist-get (cdddr capf) :exit-function)))
                  :completion-result completion-result
                  :completed completed
                  :previous previous
                  :next next
                  :next-again (ledger372-test-buffer-state))))))))
"####;
    ParityBatchCase::value(
        "mode_completion_and_navigation_cover_real_journal_structure",
        form,
        expect![[
            r#"OK (:before (:mode (:mode ledger-mode :mode-name "Ledger" :comment-start ";" :completion t :after-save 1 :post-command 1 :keys (("C-M-i" completion-at-point) ("M-p" ledger-navigate-prev-xact-or-directive) ("M-n" ledger-navigate-next-xact-or-directive) ("C-c C-c" ledger-toggle-current) ("C-c C-f" ledger-occur) ("C-c C-o C-r" ledger-report))) :transaction (:line-type xact :field payee :fields ((date "2024-03-12") (status "*") (code "(REL-417)") (payee "Café März"))) :posting (:line-type acct-transaction :field account :fields ((indent "   ") (status nil) (account "Expenses:Food:Groceries") (separator "       ") (commoditized-amount "$ 50.25"))) :faces (:date (:face ledger-font-posting-date-face :font-lock-face nil) :status (:face nil :font-lock-face nil) :payee (:face ledger-font-payee-name-face :font-lock-face nil) :account (:face ledger-font-account-name-face :font-lock-face nil) :amount (:face ledger-font-posting-amount-face :font-lock-face nil)) :accounts ("Assets:Che" "Assets:Checking" "Expenses:Food:Groceries" "Income:Salary") :payees ("Café März" "Employer" "Market")) :capf (:start 353 :end 363 :exit-function t) :completion-result t :completed (:text "account Assets:Checking\naccount Expenses:Food:Groceries\naccount Income:Salary\npayee Café März\ncommodity $\n\n2024-03-12 * (REL-417) Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25\n\n2024-03-15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00\n\n2024-03-19 Market\n    Assets:Checking" :point 368 :line 16 :column 19 :mark nil :mark-active nil :modified t :mode ledger-mode :thing posting :context (:line-type acct-transaction :field account :fields ((indent "   ") (status nil) (account "Assets:Checking")))) :previous (:text "account Assets:Checking\naccount Expenses:Food:Groceries\naccount Income:Salary\npayee Café März\ncommodity $\n\n2024-03-12 * (REL-417) Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25\n\n2024-03-15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00\n\n2024-03-19 Market\n    Assets:Checking" :point 226 :line 11 :column 0 :mark nil :mark-active nil :modified t :mode ledger-mode :thing transaction :context (:line-type xact :field date :fields ((date "2024-03-15") (payee "Employer")))) :next (:text "account Assets:Checking\naccount Expenses:Food:Groceries\naccount Income:Salary\npayee Café März\ncommodity $\n\n2024-03-12 * (REL-417) Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25\n\n2024-03-15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00\n\n2024-03-19 Market\n    Assets:Checking" :point 331 :line 15 :column 0 :mark nil :mark-active nil :modified t :mode ledger-mode :thing transaction :context (:line-type xact :field date :fields ((date "2024-03-19") (payee "Market")))) :next-again (:text "account Assets:Checking\naccount Expenses:Food:Groceries\naccount Income:Salary\npayee Café März\ncommodity $\n\n2024-03-12 * (REL-417) Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25\n\n2024-03-15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00\n\n2024-03-19 Market\n    Assets:Checking" :point 368 :line 16 :column 19 :mark nil :mark-active nil :modified t :mode ledger-mode :thing posting :context (:line-type acct-transaction :field account :fields ((indent "   ") (status nil) (account "Assets:Checking")))))"#
        ]],
    )
}

fn posting_state_dates_rename_and_cleanup_preserve_editing_semantics() -> ParityBatchCase {
    let form = r####"
(with-temp-buffer
  (insert "account Assets:Checking\n"
          "account Expenses:Food\n\n"
          "2024/04/30 Café März\n"
          "    Expenses:Food\n"
          "    Assets:Checking  $ -42.75\n\n\n"
          "2024/04/01 Employer\n"
          "    Assets:Checking  $ 2000\n"
          "    Income:Salary\n")
  (let ((ledger-post-auto-align t))
    (ledger-mode)
    (goto-char (point-min))
    (search-forward "Expenses:Food" nil nil 2)
    (end-of-line)
    (ledger-post-fill)
    (let ((filled (ledger372-test-buffer-state)))
      (ledger-navigate-beginning-of-xact)
      (call-interactively (key-binding (kbd "C-c C-c")))
      (let ((toggled (ledger372-test-buffer-state)))
        (ledger-insert-effective-date "2024/05/02")
        (let ((effective (ledger372-test-buffer-state)))
          (search-backward "02")
          (forward-char 1)
          (ledger-date-up 3)
          (let ((date-up (ledger372-test-buffer-state)))
            (ledger-date-down 3)
            (let ((date-restored (ledger372-test-buffer-state)))
              (ledger-rename-account "Expenses:Food" "Expenses:Dining")
              (let ((renamed (ledger372-test-buffer-state)))
                (ledger-mode-clean-buffer)
                (list :filled filled
                      :toggled toggled
                      :effective effective
                      :date-up date-up
                      :date-restored date-restored
                      :renamed renamed
                      :cleaned (ledger372-test-buffer-state))))))))))
"####;
    ParityBatchCase::value(
        "posting_state_dates_rename_and_cleanup_preserve_editing_semantics",
        form,
        expect![[
            r#"OK (:filled (:text "account Assets:Checking\naccount Expenses:Food\n\n2024/04/30 Café März\n    Expenses:Food                            $ 42.75\n    Assets:Checking                         $ -42.75\n\n\n2024/04/01 Employer\n    Assets:Checking  $ 2000\n    Income:Salary\n" :point 121 :line 5 :column 52 :mark nil :mark-active nil :modified t :mode ledger-mode :thing posting :context (:line-type acct-transaction :field commoditized-amount :fields ((indent "   ") (status nil) (account "Expenses:Food") (separator "                            ") (commoditized-amount "$ 42.75")))) :toggled (:text "account Assets:Checking\naccount Expenses:Food\n\n2024/04/30 * Café März\n    Expenses:Food                            $ 42.75\n    Assets:Checking                         $ -42.75\n\n\n2024/04/01 Employer\n    Assets:Checking  $ 2000\n    Income:Salary\n" :point 58 :line 4 :column 10 :mark nil :mark-active nil :modified t :mode ledger-mode :thing transaction :context (:line-type xact :field date :fields ((date "2024/04/30") (status "*") (payee "Café März")))) :effective (:text "account Assets:Checking\naccount Expenses:Food\n\n2024/04/30=2024/05/02 * Café März\n    Expenses:Food                            $ 42.75\n    Assets:Checking                         $ -42.75\n\n\n2024/04/01 Employer\n    Assets:Checking  $ 2000\n    Income:Salary\n" :point 69 :line 4 :column 21 :mark nil :mark-active nil :modified t :mode ledger-mode :thing transaction :context (:line-type xact :field date :fields ((date "2024/04/30") (status "*") (payee "Café März")))) :date-up (:text "account Assets:Checking\naccount Expenses:Food\n\n2024/04/30=2024/05/05 * Café März\n    Expenses:Food                            $ 42.75\n    Assets:Checking                         $ -42.75\n\n\n2024/04/01 Employer\n    Assets:Checking  $ 2000\n    Income:Salary\n" :point 68 :line 4 :column 20 :mark nil :mark-active nil :modified t :mode ledger-mode :thing transaction :context (:line-type xact :field date :fields ((date "2024/04/30") (status "*") (payee "Café März")))) :date-restored (:text "account Assets:Checking\naccount Expenses:Food\n\n2024/04/30=2024/05/02 * Café März\n    Expenses:Food                            $ 42.75\n    Assets:Checking                         $ -42.75\n\n\n2024/04/01 Employer\n    Assets:Checking  $ 2000\n    Income:Salary\n" :point 68 :line 4 :column 20 :mark nil :mark-active nil :modified t :mode ledger-mode :thing transaction :context (:line-type xact :field date :fields ((date "2024/04/30") (status "*") (payee "Café März")))) :renamed (:text "account Assets:Checking\naccount Expenses:Dining\n\n2024/04/30=2024/05/02 * Café März\n    Expenses:Dining                          $ 42.75\n    Assets:Checking                         $ -42.75\n\n\n2024/04/01 Employer\n    Assets:Checking                           $ 2000\n    Income:Salary\n" :point 70 :line 4 :column 20 :mark nil :mark-active nil :modified t :mode ledger-mode :thing transaction :context (:line-type xact :field date :fields ((date "2024/04/30") (status "*") (payee "Café März")))) :cleaned (:text "account Assets:Checking\naccount Expenses:Dining\n\n2024/04/01 Employer\n    Assets:Checking                           $ 2000\n    Income:Salary\n\n2024/04/30=2024/05/02 * Café März\n    Expenses:Dining                          $ 42.75\n    Assets:Checking                         $ -42.75\n" :point 162 :line 8 :column 20 :mark nil :mark-active nil :modified t :mode ledger-mode :thing transaction :context (:line-type xact :field date :fields ((date "2024/04/30") (status "*") (payee "Café März")))))"#
        ]],
    )
}

fn occur_public_filter_refresh_and_clear_own_exact_overlay_state() -> ParityBatchCase {
    let form = r####"
(with-temp-buffer
  (insert "2024/03/12 Café März\n"
          "    Assets:Checking              $ -50.25\n"
          "    Expenses:Food:Groceries       $ 50.25\n\n"
          "2024/03/15 Employer\n"
          "    Assets:Checking             $ 2000.00\n"
          "    Income:Salary              $ -2000.00\n\n"
          "2024/03/19 Market\n"
          "    Assets:Checking              $ -12.00\n"
          "    Expenses:Food:Groceries       $ 12.00\n")
  (ledger-mode)
  (setq-local ledger-occur-use-face-shown t)
  (ledger-occur "Groceries")
  (let ((groceries (ledger372-test-occur-state)))
    (ledger-occur "Employer")
    (let ((employer (ledger372-test-occur-state)))
      (ledger-occur-refresh)
      (let ((refreshed (ledger372-test-occur-state)))
        (ledger-occur "")
        (list :groceries groceries
              :employer employer
              :refreshed refreshed
              :cleared (ledger372-test-occur-state)
              :text (buffer-substring-no-properties
                     (point-min) (point-max)))))))
"####;
    ParityBatchCase::value(
        "occur_public_filter_refresh_and_clear_own_exact_overlay_state",
        form,
        expect![[
            r#"OK (:groceries (:mode t :regex "Groceries" :lighter (ledger-occur-mode (:eval (format " Ledger-Narrow(%s)" ledger-occur-current-regex))) :visible "2024/03/12 Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25\n\n2024/03/19 Market\n    Assets:Checking              $ -12.00\n    Expenses:Food:Groceries       $ 12.00\n" :overlays ((:start 1 :end 1 :custom t :invisible ledger-occur-hidden :font-lock-face nil :text "") (:start 1 :end 105 :custom t :invisible nil :font-lock-face ledger-occur-xact-face :text "2024/03/12 Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25") (:start 106 :end 211 :custom t :invisible ledger-occur-hidden :font-lock-face nil :text "\n2024/03/15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00\n") (:start 212 :end 313 :custom t :invisible nil :font-lock-face ledger-occur-xact-face :text "2024/03/19 Market\n    Assets:Checking              $ -12.00\n    Expenses:Food:Groceries       $ 12.00") (:start 314 :end 314 :custom t :invisible ledger-occur-hidden :font-lock-face nil :text ""))) :employer (:mode t :regex "Employer" :lighter (ledger-occur-mode (:eval (format " Ledger-Narrow(%s)" ledger-occur-current-regex))) :visible "\n2024/03/15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00\n" :overlays ((:start 1 :end 106 :custom t :invisible ledger-occur-hidden :font-lock-face nil :text "2024/03/12 Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25\n") (:start 107 :end 210 :custom t :invisible nil :font-lock-face ledger-occur-xact-face :text "2024/03/15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00") (:start 211 :end 314 :custom t :invisible ledger-occur-hidden :font-lock-face nil :text "\n2024/03/19 Market\n    Assets:Checking              $ -12.00\n    Expenses:Food:Groceries       $ 12.00\n"))) :refreshed (:mode t :regex "Employer" :lighter (ledger-occur-mode (:eval (format " Ledger-Narrow(%s)" ledger-occur-current-regex))) :visible "\n2024/03/15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00\n" :overlays ((:start 1 :end 106 :custom t :invisible ledger-occur-hidden :font-lock-face nil :text "2024/03/12 Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25\n") (:start 107 :end 210 :custom t :invisible nil :font-lock-face ledger-occur-xact-face :text "2024/03/15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00") (:start 211 :end 314 :custom t :invisible ledger-occur-hidden :font-lock-face nil :text "\n2024/03/19 Market\n    Assets:Checking              $ -12.00\n    Expenses:Food:Groceries       $ 12.00\n"))) :cleared (:mode nil :regex "Employer" :lighter (ledger-occur-mode (:eval (format " Ledger-Narrow(%s)" ledger-occur-current-regex))) :visible "2024/03/12 Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25\n\n2024/03/15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00\n\n2024/03/19 Market\n    Assets:Checking              $ -12.00\n    Expenses:Food:Groceries       $ 12.00\n" :overlays nil) :text "2024/03/12 Café März\n    Assets:Checking              $ -50.25\n    Expenses:Food:Groceries       $ 50.25\n\n2024/03/15 Employer\n    Assets:Checking             $ 2000.00\n    Income:Salary              $ -2000.00\n\n2024/03/19 Market\n    Assets:Checking              $ -12.00\n    Expenses:Food:Groceries       $ 12.00\n")"#
        ]],
    )
}

fn schedule_reports_fixed_upcoming_transactions_and_recovers_from_missing_file() -> ParityBatchCase
{
    let form = r####"
(let* ((root (make-temp-file "ledger372-schedule-" t))
       (schedule-file (expand-file-name "schedule space 界.ledger" root))
       (missing-file (expand-file-name "missing.ledger" root))
       (schedule-buffer "*Ledger372 Schedule*")
       failure-created-buffer visited result)
  (unwind-protect
      (save-window-excursion
        (with-temp-file schedule-file
          (insert "[*/*/15] Rent\n"
                  "    Expenses:Rent      $ 500\n"
                  "    Assets:Checking\n\n"
                  "[*/*/L] Internet\n"
                  "    Expenses:Internet   $ 50\n"
                  "    Assets:Checking\n"))
        (let* ((ledger-schedule-buffer-name schedule-buffer)
               (failure
                (condition-case condition
                    (progn
                      (ledger-schedule-upcoming missing-file 0 20)
                      'unexpected-success)
                  (error
                   (list (car condition)
                         (replace-regexp-in-string
                          (regexp-quote missing-file) "[MISSING]"
                          (error-message-string condition) t t))))))
          (setq failure-created-buffer (and (get-buffer schedule-buffer) t))
          (cl-letf (((symbol-function 'current-time)
                     (lambda () (encode-time 0 0 12 10 4 2024 t))))
            (ledger-schedule-upcoming schedule-file 0 20))
          (setq visited (get-file-buffer schedule-file))
          (let ((buffer (get-buffer schedule-buffer)))
            (setq result
                  (list :failure failure
                        :failure-created-buffer failure-created-buffer
                        :selected (eq (window-buffer (selected-window)) buffer)
                        :count (with-current-buffer buffer
                                 (count-lines (point-min) (point-max)))
                        :schedule
                        (with-current-buffer buffer
                          (list :text (buffer-substring-no-properties
                                       (point-min) (point-max))
                                :mode major-mode
                                :point (point)
                                :modified (buffer-modified-p)
                                :source-buffer-live (buffer-live-p visited)))
                        :source-file
                        (with-current-buffer visited
                          (list :name (file-name-nondirectory buffer-file-name)
                                :mode major-mode
                                :modified (buffer-modified-p)
                                :text (ledger372-test-file-text schedule-file))))))))
    (when (buffer-live-p (get-buffer schedule-buffer))
      (kill-buffer schedule-buffer))
    (when (buffer-live-p visited)
      (with-current-buffer visited (set-buffer-modified-p nil))
      (kill-buffer visited))
    (ledger372-test-delete-tree root))
  result)
"####;
    ParityBatchCase::value(
        "schedule_reports_fixed_upcoming_transactions_and_recovers_from_missing_file",
        form,
        expect![[
            r#"OK (:failure (error "Could not find ledger schedule file at [MISSING]") :failure-created-buffer nil :selected t :count 8 :schedule (:text "2024/04/15 Rent\n    Expenses:Rent      $ 500\n    Assets:Checking\n\n2024/04/30 Internet\n    Expenses:Internet   $ 50\n    Assets:Checking\n\n" :mode ledger-mode :point 137 :modified t :source-buffer-live t) :source-file (:name "schedule space 界.ledger" :mode ledger-mode :modified nil :text "[*/*/15] Rent\n    Expenses:Rent      $ 500\n    Assets:Checking\n\n[*/*/L] Internet\n    Expenses:Internet   $ 50\n    Assets:Checking\n"))"#
        ]],
    )
}

fn owned_report_executes_exact_arguments_redoes_and_visits_source() -> ParityBatchCase {
    let form = r####"
(let* ((root (make-temp-file "ledger372 report 界 " t))
       (journal (expand-file-name "journal space 界.ledger" root))
       (script (expand-file-name "ledger fixture" root))
       (arguments (expand-file-name "arguments.log" root))
       (report-buffer "*Ledger372 Report*")
       source report-window result)
  (unwind-protect
      (save-window-excursion
        (with-temp-file journal
          (insert "2024/03/12 Café März\n"
                  "    Assets:Checking              $ -50.25\n"
                  "    Expenses:Food:Groceries       $ 50.25\n"))
        (with-temp-file script
          (insert "#!/bin/sh\n"
                  "set -eu\n"
                  "printf '%s\\n' '--run--' >>\"$LEDGER372_ARGS\"\n"
                  "printf 'LC_ALL=%s\\n' \"$LC_ALL\" >>\"$LEDGER372_ARGS\"\n"
                  "printf 'PWD=%s\\n' \"$PWD\" >>\"$LEDGER372_ARGS\"\n"
                  "for argument do printf '%s\\n' \"$argument\" >>\"$LEDGER372_ARGS\"; done\n"
                  "file=\n"
                  "while [ \"$#\" -gt 0 ]; do\n"
                  "  if [ \"$1\" = '-f' ]; then shift; file=$1; fi\n"
                  "  shift\n"
                  "done\n"
                  "[ -n \"$file\" ]\n"
                  "printf '%s:1:2024/03/12 Café März  $50.25\\n' \"$file\"\n"))
        (set-file-modes script #o700)
        (unless (and (file-regular-p script)
                     (file-executable-p script)
                     (not (file-symlink-p script)))
          (error "Ledger report fixture is not a direct executable file"))
        (setq source (find-file-noselect journal))
        (with-current-buffer source
          (ledger-mode))
        (let ((ledger-binary-path script)
              (ledger-reports '(("reg" "%(binary) -f %(ledger-file) reg")))
              (ledger-report-buffer-name report-buffer)
              (ledger-report-auto-width nil)
              (ledger-report-use-native-highlighting nil)
              (ledger-report-use-header-line t)
              (ledger-report-resize-window nil)
              (ledger-report-use-strict nil)
              (ledger-report-links-in-register t)
              (ledger-report-links-beginning-of-xact t)
              (ledger-report-auto-refresh nil)
              (shell-file-name "/bin/sh")
              (process-environment
               (list "LC_ALL=C" (concat "LEDGER372_ARGS=" arguments))))
          (switch-to-buffer source)
          (ledger-report "reg" nil)
          (setq report-window (selected-window))
          (let* ((report (get-buffer report-buffer))
                 (initial
                  (with-current-buffer report
                    (goto-char (point-min))
                    (list :text (buffer-substring-no-properties
                                 (point-min) (point-max))
                          :mode major-mode
                          :name ledger-report-name
                          :command ledger-report-cmd
                          :header (list :enabled (and header-line-format t)
                                        :kind (car-safe header-line-format))
                          :source
                          (let ((value (get-text-property (point) 'ledger-source)))
                            (list :file (and value
                                             (file-name-nondirectory (car value)))
                                  :line (cdr value)
                                  :help
                                  (let ((help (get-text-property (point) 'help-echo)))
                                    (and help
                                         (ledger372-test-normalize-paths
                                          help root script)))
                                  :button (and (button-at (point))
                                               (button-type (button-at (point))))
                                  :face (copy-tree
                                         (get-text-property (point) 'face))))))))
            (with-current-buffer report
              (goto-char (point-min))
              (ledger-report-visit-source))
            (let ((visited
                   (with-current-buffer (window-buffer (selected-window))
                     (list :selected-file
                           (file-name-nondirectory buffer-file-name)
                           :same-buffer (eq (current-buffer) source)
                           :line (line-number-at-pos)
                           :point (point)
                           :text (buffer-substring-no-properties
                                  (line-beginning-position) (line-end-position))))))
              (with-current-buffer source
                (ledger-report-redo))
              (let ((redone
                     (with-current-buffer report
                       (goto-char (point-min))
                       (list :text (buffer-substring-no-properties
                                    (point-min) (point-max))
                             :source (copy-tree
                                      (get-text-property (point) 'ledger-source))
                             :modified (buffer-modified-p)))))
                (select-window report-window)
                (ledger-report-quit)
                (setq result
                      (list :initial initial
                            :visited visited
                            :redone
                            (list :text (plist-get redone :text)
                                  :source
                                  (let ((value (plist-get redone :source)))
                                    (and value
                                         (cons (file-name-nondirectory (car value))
                                               (cdr value))))
                                  :modified (plist-get redone :modified))
                            :report-killed (not (buffer-live-p report))
                            :arguments
                            (ledger372-test-normalize-paths
                             (ledger372-test-file-text arguments)
                             root script))))))))
    (when (buffer-live-p (get-buffer report-buffer))
      (kill-buffer report-buffer))
    (when (buffer-live-p source)
      (with-current-buffer source (set-buffer-modified-p nil))
      (kill-buffer source))
    (ledger372-test-delete-tree root))
  result)
"####;
    ParityBatchCase::value(
        "owned_report_executes_exact_arguments_redoes_and_visits_source",
        form,
        expect![[
            r#"OK (:initial (:text "2024/03/12 Café März  $50.25\n" :mode ledger-report-mode :name "reg" :command "%(binary) -f %(ledger-file) reg" :header (:enabled t :kind :eval) :source (:file "journal space 界.ledger" :line 1 :help "mouse-2, RET: Visit [ROOT]/journal space 界.ledger:1" :button ledger-report-register-entry :face (ledger-font-report-clickable-face))) :visited (:selected-file "journal space 界.ledger" :same-buffer t :line 1 :point 1 :text "2024/03/12 Café März") :redone (:text "2024/03/12 Café März  $50.25\n" :source ("journal space 界.ledger" . 1) :modified nil) :report-killed t :arguments "--run--\nLC_ALL=C\nPWD=[ROOT]\n--prepend-format=%(filename):%(beg_line):\n-f\n[ROOT]/journal space 界.ledger\nreg\n--run--\nLC_ALL=C\nPWD=[ROOT]\n--prepend-format=%(filename):%(beg_line):\n-f\n[ROOT]/journal space 界.ledger\nreg\n")"#
        ]],
    )
}

#[test]
fn ledger_mode_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "ledger-mode-package-batch",
        "ledger_mode_parity",
        &[
            mode_completion_and_navigation_cover_real_journal_structure(),
            posting_state_dates_rename_and_cleanup_preserve_editing_semantics(),
            occur_public_filter_refresh_and_clear_own_exact_overlay_state(),
            schedule_reports_fixed_upcoming_transactions_and_recovers_from_missing_file(),
            owned_report_executes_exact_arguments_redoes_and_visits_source(),
        ],
    );
}
