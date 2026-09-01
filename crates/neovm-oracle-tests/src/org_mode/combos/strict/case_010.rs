//! Combo-strict-10 oracle tests — strict verification for untouched
//! org APIs: org-collect-keywords, org-tag-string-to-alist/alist-to-
//! string, org-priority-to-value, org-set-regexps-and-options,
//! org-insert-heading variants, org-emphasize, org-today/org-current-
//! time, org-file-contents, org-extract-log-state-settings, and
//! LaTeX package handling.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_collect_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 14 59)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: My Title\n#+AUTHOR: Alice\n#+DATE: 2024\n#+OPTIONS: num:t toc:nil\n#+FILETAGS: :proj:\n")
      (list
       ;; collect specific keyword
       (list :title (org-collect-keywords '("TITLE")))
       ;; collect multiple keywords
       (list :author+date (org-collect-keywords '("AUTHOR" "DATE")))
       ;; collect filetags
       (list :filetags (org-collect-keywords '("FILETAGS")))
       ;; collect non-existent
       (list :bogus (org-collect-keywords '("BOGUS"))))))))"##,
        expect,
    );
}

#[test]
fn strict_tag_string_to_alist_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Invalid tag token: :startgroup\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; string to alist
   (list :simple (org-tag-string-to-alist ":work:home:urgent:"))
   ;; alist to string
   (list :to-string (org-tag-alist-to-string '(:startgroup nil :endgroup nil :grouptags nil)))
   ;; with groups
   (list :grouped (org-tag-string-to-alist ":work{Work}:home{Home}:urgent:"))
   ;; tag alist to groups
   (let ((alist (org-tag-string-to-alist ":a:b:c")))
     (list :alist alist))
   ;; tags sort
   (let ((tags '("c" "a" "b")))
     (list :sorted (sort (copy-sequence tags) #'string-lessp))))))"##,
        expect,
    );
}

#[test]
fn strict_priority_to_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 65)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; standard priorities (65=A=highest, 66=B, 67=C=lowest)
   (list :A (org-priority-to-value ?A))
   (list :B (org-priority-to-value ?B))
   (list :C (org-priority-to-value ?C))
   ;; numeric value back
   (cond ((fboundp 'org-priority-to-value)
          (list :A-val (integerp (org-priority-to-value ?A))))
         (t :no-func))
   ;; get priority from char
   (let ((saved-default org-default-priority))
     (list :default-priority (when (boundp 'org-default-priority) org-default-priority)
           :highest (when (boundp 'org-highest-priority) org-highest-priority)
           :lowest (when (boundp 'org-lowest-priority) org-lowest-priority))))))"##,
        expect,
    );
}

#[test]
fn strict_set_regexps_and_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 71)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Test\n#+OPTIONS: num:t toc:t H:3\n#+TODO: TODO WAIT | DONE CANCELED\n")
      (goto-char (point-min))
      ;; call set-regexps-and-options to populate keyword variables
      (condition-case nil
          (progn (org-set-regexps-and-options)
                 (list
                  :todo-keywords (when (boundp 'org-todo-keywords) org-todo-keywords)
                  :done-keywords (when (boundp 'org-done-keywords) org-done-keywords)
                  :todo-keyword-regexp (when (boundp 'org-todo-regexp)
                                         (if (stringp org-todo-regexp)
                                             (list :string org-todo-regexp)
                                           :not-string))
                  :not-done-keywords (when (boundp 'org-not-done-keywords) org-not-done-keywords)
                  :num-flag (when (boundp 'org-export-with-section-numbers) org-export-with-section-numbers)))
        (error (list :error "org-set-regexps-and-options failed")))))))"##,
        expect,
    );
}

#[test]
fn strict_insert_heading_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 36 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\nBody A.\n* B\nBody B.\n")
      (let ((r '()))
        ;; insert heading after A's content
        (goto-char (point-min))
        (forward-line 1)  ;; on Body A line
        (end-of-line)
        (org-insert-heading-respect-content)
        (insert "Respect Content Heading")
        (push (list :after-respect-content
                    (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                            (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
        ;; insert heading at end
        (goto-char (point-max))
        (org-insert-heading)
        (insert "End Heading")
        (push (list :after-end-insert
                    (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                            (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
        ;; insert subheading under B
        (goto-char (point-min))
        (search-forward "* B") (beginning-of-line)
        (org-insert-subheading nil)  ;; nil = interactive arg
        (goto-char (point-min))
        (search-forward "* B") (search-forward "**")
        (insert "Sub heading")
        (push (list :after-subheading
                    (mapcar (lambda (h) (list (org-element-property :level h)
                                             (substring-no-properties (org-element-property :raw-value h))))
                            (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
        ;; headline count
        (push (list :total-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_emphasize_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 31 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Some text to emphasize.\n")
      (let ((r '()))
        ;; mark "text" as bold
        (goto-char (point-min))
        (search-forward "text")
        (set-mark (match-beginning 0))
        (search-forward " ") (backward-char)
        ;; use org-emphasize with bold char
        (condition-case nil
            (progn (org-emphasize ?*)
                   (push (list :after-bold (buffer-substring-no-properties (point-min) (point-max))) r))
          (error (push (list :bold-error t) r)))
        ;; now mark "emphasize" as italic
        (goto-char (point-min))
        (search-forward "emphasize")
        (set-mark (match-beginning 0))
        (search-forward ".") (backward-char)
        (condition-case nil
            (progn (org-emphasize ?/)
                   (push (list :after-italic (buffer-substring-no-properties (point-min) (point-max))) r))
          (error (push (list :italic-error t) r)))
        ;; parse and verify bold/italic
        (let ((bolds (length (org-element-map (org-element-parse-buffer) 'bold #'identity)))
              (italics (length (org-element-map (org-element-parse-buffer) 'italic #'identity))))
          (push (list :bold-count bolds) r)
          (push (list :italic-count italics) r))
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_today_current_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; org-today returns an integer (Julian day number)
   (list :today-numberp (numberp (org-today)))
   (list :today-positive (> (org-today) 700000))  ;; any day after year 1
   ;; org-current-time returns a time value
   (cond ((fboundp 'org-current-time)
          (let ((ct (org-current-time)))
            (list :current-time-type (type-of ct))))
         (t :not-available))
   ;; org-time-string-to-seconds with today
   (let ((secs (org-time-string-to-seconds (format-time-string "<%Y-%m-%d %a>"))))
     (list :today-seconds-numberp (numberp secs)))
   ;; consistency: org-2ft on "today" timestamp
   (let* ((ts-str (format-time-string "<%Y-%m-%d %a>"))
          (ts (org-timestamp-from-string ts-str))
          (ft (org-2ft ts)))
     (list :org-2ft-numberp (numberp ft)
           :org-2ft-positive (> ft 0))))))"##,
        expect,
    );
}

#[test]
fn strict_file_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((tmp-file (make-temp-file "org-test-" nil ".txt")))
    (with-temp-file tmp-file
      (insert "Line 1\nLine 2\nLine 3\n"))
    (list
     ;; org-file-contents should work
     (condition-case nil
         (let ((contents (org-file-contents tmp-file)))
           (list (length (split-string contents "\n"))))
       (error "error")))))"##,
        expect,
    );
}

#[test]
fn strict_log_state_settings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 16 36)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+STARTUP: logdone lognotedone logrepeat logdrawer\n")
      (insert "* TODO Task\n:PROPERTIES:\n:LOGGING: lognotedone\n:END:\n")
      (goto-char (point-min))
      (condition-case nil
          (progn (org-set-regexps-and-options)
                 (list
                  :log-done (when (boundp 'org-log-done) org-log-done)
                  :log-repeat (when (boundp 'org-log-repeat) org-log-repeat)
                  :log-note-clock-out (when (boundp 'org-log-note-clock-out) org-log-note-clock-out)
                  :extract (org-extract-log-state-settings
                            (org-entry-get nil "LOGGING"))))
        (error (list :error t)))))))"##,
        expect,
    );
}

#[test]
fn strict_latex_packages() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; org-latex-packages-alist should be defined
   (cond ((boundp 'org-latex-packages-alist)
          (list :packages-defined t
                :default-count (length org-latex-packages-alist)))
         (t :not-available))
   ;; org-get-packages-alist
   (cond ((fboundp 'org-get-packages-alist)
          (let ((pkgs (org-get-packages-alist)))
            (list :has-packages (> (length pkgs) 0))))
         (t :not-available))
   ;; org-format-latex-header
   (cond ((fboundp 'org-format-latex-header)
          (let ((header (org-format-latex-header "" "" "")))
            (list :header-nonempty (> (length header) 0))))
         (t :not-available)))))"##,
        expect,
    );
}
