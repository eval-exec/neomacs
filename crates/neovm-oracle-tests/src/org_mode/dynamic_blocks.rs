use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_custom_dynamic_block_insert_update_all_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (params) (insert (format \"| key | value |\\n| name | %s |\\n| limit | %S |\\n\" (plist-get params :name) (plist-get params :limit)))) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-dynamic-block-alist nil))
    (org-dynamic-block-define
     "probe"
     (lambda (params)
       (insert
        (format "| key | value |\n| name | %s |\n| limit | %S |\n"
                (plist-get params :name)
                (plist-get params :limit)))))
    (with-temp-buffer
      (org-mode)
      (insert "* Blocks\n")
      (org-dynamic-block-insert-dblock "probe")
      (goto-char (point-min))
      (search-forward "#+BEGIN: probe")
      (end-of-line)
      (insert " :limit 3")
      (org-update-dblock)
      (insert "\n#+BEGIN: probe :limit (1 2)\nstale\n#+END:\n")
      (org-update-all-dblocks)
      (list (org-dynamic-block-types)
            (functionp (org-dynamic-block-function "probe"))
            (org-find-dblock "probe")
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_dblock_prepare_nested_content_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((:name \"probe-prepare\" :heading \"A B\" :indentation-column 0 :content \"old line\\n#+begin_src emacs-lisp\\n(+ 1 2)\\n#+end_src\\n\") nil \"#+BEGIN: probe-prepare :heading \\\"A B\\\"\\n- heading :: A B\\n  #+begin_example\\n  example\\n  #+end_example\\n\\n#+END:\\n\\n* After\\n\" ((dynamic-block 1 109) (plain-list 39 101) (example-block 56 100) (headline 109 117)))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (defun org-dblock-write:probe-prepare (params)
    (insert (format "- heading :: %s\n" (plist-get params :heading)))
    (insert "  #+begin_example\n  example\n  #+end_example\n"))
  (with-temp-buffer
    (org-mode)
    (insert "#+BEGIN: probe-prepare :heading \"A B\"\n")
    (insert "old line\n")
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
    (insert "#+END:\n\n")
    (insert "* After\n")
    (goto-char (point-min))
    (let ((prepared (org-prepare-dblock)))
      (goto-char (point-min))
      (org-update-dblock)
      (let ((inside (save-excursion
                      (goto-char (point-max))
                      (search-backward "#+BEGIN: probe")
                      (org-beginning-of-dblock)
                      (org-in-block-p '("probe-prepare")))))
        (list prepared
              inside
              (buffer-substring-no-properties
               (point-min) (point-max))
              (org-element-map (org-element-parse-buffer)
                  '(dynamic-block plain-list example-block headline)
                (lambda (e)
                  (list (org-element-type e)
                        (org-element-property :begin e)
                        (org-element-property :end e)))))))))"##,
        expect,
    );
}

#[test]
fn org_clocktable_dblock_shift_steps_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"Unknown ‘:step’ specification: daysteps\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (with-temp-buffer
    (org-mode)
    (insert "* Project\n")
    (insert "** Alpha\n")
    (insert "CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:00] =>  1:00\n")
    (insert "** Beta\n")
    (insert "CLOCK: [2026-05-28 Thu 11:00]--[2026-05-28 Thu 12:30] =>  1:30\n\n")
    (insert "#+BEGIN: clocktable :scope file :block 2026-05-27 :maxlevel 3 :link nil :step daysteps\n")
    (insert "#+END:\n")
    (goto-char (point-min))
    (search-forward "#+BEGIN: clocktable")
    (beginning-of-line)
    (let ((steps-before
           (mapcar (lambda (pair)
                     (list (format-time-string "%F" (car pair))
                           (format-time-string "%F" (cdr pair))))
                   (org-clocktable-steps
                    '(:block "2026-05-27" :step daysteps)))))
      (org-update-dblock)
      (let ((after-update
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "#+BEGIN: clocktable")
        (beginning-of-line)
        (org-clocktable-shift 'right 1)
        (list steps-before
              after-update
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_columnview_dblock_filters_tblfm_links_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (user-error \"Several field/range formulas try to set @4$6\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "#+COLUMNS: %25ITEM(Task) %TODO(State) %Effort{:} %Owner %Score{+}\n")
    (insert "#+PROPERTY: Owner_ALL Ada Bea Cal\n")
    (insert "* Project :root:\n")
    (insert ":PROPERTIES:\n:Owner: Ada\n:Score: 0\n:Effort: 0:00\n:END:\n")
    (insert "** TODO Alpha :keep:\n")
    (insert ":PROPERTIES:\n:Effort: 1:15\n:Owner: Bea\n:Score: 2\n:END:\n")
    (insert "** DONE Beta :skip:keep:\n")
    (insert ":PROPERTIES:\n:Effort: 0:45\n:Owner: Cal\n:Score: 3\n:END:\n")
    (insert "** TODO Gamma :keep:\n")
    (insert ":PROPERTIES:\n:Effort: 0:30\n:Score: 4\n:END:\n\n")
    (insert "#+BEGIN: columnview :id local :match \"+keep\" ")
    (insert ":exclude-tags (\"skip\") :maxlevel 3 :skip-empty-rows t ")
    (insert ":indent t :hlines 2 :vlines t :link t ")
    (insert ":format \"%25ITEM(Task) %TODO(State) %Effort{:} %Owner %Score{+}\"\n")
    (insert "#+CAPTION: kept caption\n")
    (insert "| stale | data |\n")
    (insert "#+TBLFM: @>$6=vsum(@3..@-1)\n")
    (insert "#+END:\n")
    (goto-char (point-min))
    (search-forward "#+BEGIN: columnview")
    (beginning-of-line)
    (let ((start (point))
          before-table after-table ast)
      (org-update-dblock)
      (goto-char start)
      (setq before-table
            (progn
              (search-forward "|")
              (org-table-to-lisp)))
      (goto-char start)
      (setq ast
            (org-element-map (org-element-parse-buffer)
                '(dynamic-block table keyword)
              (lambda (e)
                (list (org-element-type e)
                      (org-element-property :key e)
                      (org-element-property :value e)
                      (org-element-property :name e)
                      (org-element-property :begin e)
                      (org-element-property :end e)))))
      (search-forward "#+TBLFM:")
      (end-of-line)
      (insert "::@4$6=@3$6+10")
      (goto-char start)
      (search-forward "|")
      (org-table-recalculate 'all t)
      (setq after-table (org-table-to-lisp))
      (list before-table
            after-table
            ast
            org-columns-current-fmt-compiled
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_custom_dblock_nested_table_rewrite_all_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"oracle-matrix\") t (:name \"oracle-matrix\" :label \"alpha\" :rows (1 2 3) :indentation-column 2 :content \"  old alpha\\n\") ((\"oracle-matrix\" \"alpha\" (1 2 3) \"  old alpha\\n\" 2) (\"oracle-matrix\" \"alpha\" (1 2 3) \"- label :: alpha\\n| idx | value | double |\\n|-----+-------+--------|\\n| 1 | 1 | stale |\\n| 2 | 2 | stale |\\n| 3 | 3 | stale |\\n#+TBLFM: $3=$2*2\\n\\n\" 2) (\"oracle-matrix\" \"alpha\" (1 2 3) \"  - label :: alpha\\n  | idx | value | double |\\n  |-----+-------+--------|\\n  | 1 | 1 | stale |\\n  | 2 | 2 | stale |\\n  | 3 | 3 | stale |\\n  #+TBLFM: $3=$2*2\\n  \\n\" 2) (\"oracle-matrix\" \"beta\" (7 8) \"| stale | table |\\n\" 0) (\"oracle-matrix\" \"inserted\" (4 5) \"\\n\" 0)) \"* Blocks\\n  #+BEGIN: oracle-matrix :label \\\"alpha\\\" :rows (1 2 3)\\n  - label :: alpha\\n  | idx | value | double |\\n  |-----+-------+--------|\\n  | 1 | 1 | stale |\\n  | 2 | 2 | stale |\\n  | 3 | 3 | stale |\\n  #+TBLFM: $3=$2*2\\n  \\n  #+END:\\n\\n** Child\\n#+BEGIN: oracle-matrix :label \\\"beta\\\" :rows (7 8)\\n| stale | table |\\n#+END:\\n\\n#+BEGIN: oracle-matrix :label \\\"inserted\\\" :rows (4 5)\\n\\n#+END:\\n\" \"* Blocks\\n  #+BEGIN: oracle-matrix :label \\\"alpha\\\" :rows (1 2 3)\\n  - label :: alpha\\n  | idx | value | double |\\n  |-----+-------+--------|\\n  | 1 | 1 | stale |\\n  | 2 | 2 | stale |\\n  | 3 | 3 | stale |\\n  #+TBLFM: $3=$2*2\\n  \\n  #+END:\\n\\n** Child\\n#+BEGIN: oracle-matrix :label \\\"beta\\\" :rows (7 8)\\n- label :: beta\\n| idx | value | double |\\n|-----+-------+--------|\\n| 1 | 7 | stale |\\n| 2 | 8 | stale |\\n#+TBLFM: $3=$2*2\\n\\n#+END:\\n\\n#+BEGIN: oracle-matrix :label \\\"inserted\\\" :rows (4 5)\\n- label :: inserted\\n| idx | value | double |\\n|-----+-------+--------|\\n| 1 | 4 | stale |\\n| 2 | 5 | stale |\\n#+TBLFM: $3=$2*2\\n\\n#+END:\\n\" (((#(\"idx\" 0 3 (face org-table)) #(\"value\" 0 5 (face org-table)) #(\"double\" 0 6 (face org-table))) hline (#(\"1\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table))) (#(\"2\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table))) (#(\"3\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table)))) ((#(\"idx\" 0 3 (face org-table)) #(\"value\" 0 5 (face org-table)) #(\"double\" 0 6 (face org-table))) hline (#(\"1\" 0 1 (face org-table)) #(\"7\" 0 1 (face org-table)) #(\"14\" 0 2 (face org-table))) (#(\"2\" 0 1 (face org-table)) #(\"8\" 0 1 (face org-table)) #(\"16\" 0 2 (face org-table)))) ((#(\"idx\" 0 3 (face org-table)) #(\"value\" 0 5 (face org-table)) #(\"double\" 0 6 (face org-table))) hline (#(\"1\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"8\" 0 1 (face org-table))) (#(\"2\" 0 1 (face org-table)) #(\"5\" 0 1 (face org-table)) #(\"10\" 0 2 (face org-table))))) ((headline nil 1 648) (dynamic-block nil 10 250) (plain-list nil 64 83) (table nil 83 240) (table-row nil 83 110) (table-row nil 110 137) (table-row nil 137 164) (table-row nil 164 191) (table-row nil 191 218) (headline nil 250 648) (dynamic-block nil 259 450) (plain-list nil 308 324) (table nil 324 442) (table-row nil 324 349) (table-row nil 349 374) (table-row nil 374 399) (table-row nil 399 424) (dynamic-block nil 450 648) (plain-list nil 503 523) (table nil 523 641) (table-row nil 523 548) (table-row nil 548 573) (table-row nil 573 598) (table-row nil 598 623)) \"* Blocks\\n  #+BEGIN: oracle-matrix :label \\\"alpha\\\" :rows (1 2 3)\\n  - label :: alpha\\n  | idx | value | double |\\n  |-----+-------+--------|\\n  |   1 |     1 |      2 |\\n  |   2 |     2 |      4 |\\n  |   3 |     3 |      6 |\\n  #+TBLFM: $3=$2*2\\n  \\n  #+END:\\n\\n** Child\\n#+BEGIN: oracle-matrix :label \\\"beta\\\" :rows (7 8)\\n- label :: beta\\n| idx | value | double |\\n|-----+-------+--------|\\n|   1 |     7 |     14 |\\n|   2 |     8 |     16 |\\n#+TBLFM: $3=$2*2\\n\\n#+END:\\n\\n#+BEGIN: oracle-matrix :label \\\"inserted\\\" :rows (4 5)\\n- label :: inserted\\n| idx | value | double |\\n|-----+-------+--------|\\n|   1 |     4 |      8 |\\n|   2 |     5 |     10 |\\n#+TBLFM: $3=$2*2\\n\\n#+END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-dynamic-block-alist nil)
        (calls nil))
    (defun org-dblock-write:oracle-matrix (params)
      (push (list (plist-get params :name)
                  (plist-get params :label)
                  (plist-get params :rows)
                  (plist-get params :content)
                  (plist-get params :indentation-column))
            calls)
      (let ((label (plist-get params :label))
            (rows (plist-get params :rows)))
        (insert (format "- label :: %s\n" label))
        (insert "| idx | value | double |\n")
        (insert "|-----+-------+--------|\n")
        (cl-loop for row in rows
                 for idx from 1
                 do (insert (format "| %d | %s | stale |\n" idx row)))
        (insert "#+TBLFM: $3=$2*2\n")))
    (org-dynamic-block-define
     "oracle-matrix"
     (lambda ()
       (interactive)
       (org-create-dblock
        '(:name "oracle-matrix" :label "inserted" :rows (4 5)))))
    (with-temp-buffer
      (org-mode)
      (insert "* Blocks\n")
      (insert "  #+BEGIN: oracle-matrix :label \"alpha\" :rows (1 2 3)\n")
      (insert "  old alpha\n")
      (insert "  #+END:\n\n")
      (insert "** Child\n")
      (insert "#+BEGIN: oracle-matrix :label \"beta\" :rows (7 8)\n")
      (insert "| stale | table |\n")
      (insert "#+END:\n\n")
      (goto-char (point-max))
      (org-dynamic-block-insert-dblock "oracle-matrix")
      (let ((types-before (org-dynamic-block-types))
            prepared-alpha after-alpha after-all ast table-summaries)
        (goto-char (point-min))
        (search-forward "alpha")
        (org-beginning-of-dblock)
        (setq prepared-alpha (org-prepare-dblock))
        (org-dblock-write:oracle-matrix prepared-alpha)
        (search-backward "#+BEGIN: oracle-matrix")
        (beginning-of-line)
        (save-window-excursion
          (switch-to-buffer (current-buffer))
          (org-update-dblock))
        (setq after-alpha
              (buffer-substring-no-properties
               (point-min) (point-max)))
        (save-window-excursion
          (switch-to-buffer (current-buffer))
          (org-update-all-dblocks))
        (setq after-all
              (buffer-substring-no-properties
               (point-min) (point-max)))
        (setq table-summaries
              (let (out)
                (goto-char (point-min))
                (while (search-forward "| idx |" nil t)
                  (org-table-align)
                  (org-table-recalculate 'all)
                  (push (org-table-to-lisp) out))
                (nreverse out)))
        (setq ast
              (org-element-map (org-element-parse-buffer)
                  '(dynamic-block plain-list table table-row headline)
                (lambda (e)
                  (list (org-element-type e)
                        (org-element-property :name e)
                        (org-element-property :begin e)
                        (org-element-property :end e)))))
        (list types-before
              (functionp (org-dynamic-block-function "oracle-matrix"))
              prepared-alpha
              (nreverse calls)
              after-alpha
              after-all
              table-summaries
              ast
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_clocktable_properties_shift_recalc_ast_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"Unknown ‘:step’ specification: daysteps\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "#+COLUMNS: %25ITEM %Owner %Effort{:}\n")
    (insert "* Project :root:\n")
    (insert ":PROPERTIES:\n:Owner: Ada\n:Effort: 0:00\n:END:\n")
    (insert "** TODO Alpha :work:\n")
    (insert ":PROPERTIES:\n:Owner: Bea\n:Effort: 1:00\n:END:\n")
    (insert "CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\n")
    (insert "** TODO Beta :home:\n")
    (insert ":PROPERTIES:\n:Owner: Cy\n:Effort: 0:45\n:END:\n")
    (insert "CLOCK: [2026-05-28 Thu 11:15]--[2026-05-28 Thu 12:00] =>  0:45\n")
    (insert "*** TODO Beta child :work:\n")
    (insert ":PROPERTIES:\n:Owner: Dee\n:Effort: 0:30\n:END:\n")
    (insert "CLOCK: [2026-05-29 Fri 14:00]--[2026-05-29 Fri 15:15] =>  1:15\n\n")
    (insert "#+BEGIN: clocktable :scope file :block 2026-05-27 :step daysteps ")
    (insert ":maxlevel 4 :link nil :tags t :properties (\"Owner\" \"Effort\") ")
    (insert ":formula % :compact nil\n")
    (insert "#+CAPTION: Clock rollup\n")
    (insert "| stale | data |\n")
    (insert "#+END:\n")
    (goto-char (point-min))
    (search-forward "#+BEGIN: clocktable")
    (beginning-of-line)
    (let ((steps
           (mapcar (lambda (pair)
                     (list (format-time-string "%F" (car pair))
                           (format-time-string "%F" (cdr pair))))
                   (org-clocktable-steps
                    '(:block "2026-05-27" :step daysteps))))
          before-shift after-right after-left ast tables data)
      (org-update-dblock)
      (setq before-shift
            (buffer-substring-no-properties (point-min) (point-max)))
      (goto-char (point-min))
      (search-forward "#+BEGIN: clocktable")
      (beginning-of-line)
      (setq tables
            (let (out)
              (while (search-forward "| File" nil t)
                (org-table-align)
                (push (org-table-to-lisp) out))
              (nreverse out)))
      (setq data
            (org-clock-get-table-data
             (current-buffer)
             '(:maxlevel 4 :tags t :properties ("Owner" "Effort"))))
      (goto-char (point-min))
      (search-forward "#+BEGIN: clocktable")
      (beginning-of-line)
      (org-clocktable-shift 'right 1)
      (setq after-right
            (buffer-substring-no-properties (point-min) (point-max)))
      (goto-char (point-min))
      (search-forward "#+BEGIN: clocktable")
      (beginning-of-line)
      (org-clocktable-shift 'left 1)
      (setq after-left
            (buffer-substring-no-properties (point-min) (point-max)))
      (setq ast
            (org-element-map (org-element-parse-buffer)
                '(dynamic-block table keyword headline)
              (lambda (e)
                (list (org-element-type e)
                      (org-element-property :block-name e)
                      (org-element-property :key e)
                      (org-element-property :value e)
                      (org-element-property :raw-value e)
                      (org-element-property :begin e)
                      (org-element-property :end e)))))
      (list steps
            (mapcar (lambda (needle)
                      (not (null
                            (string-match-p needle before-shift))))
                    '("Clock rollup" "Alpha" "Beta" "Owner" "Effort"
                      "1:30" "0:45"))
            tables
            data
            (mapcar (lambda (needle)
                      (not (null
                            (string-match-p needle after-right))))
                    '("2026-05-28" "Beta" "0:45"))
            (mapcar (lambda (needle)
                      (not (null
                            (string-match-p needle after-left))))
                    '("2026-05-27" "Alpha" "1:30"))
            ast
            before-shift
            after-right
            after-left))))"##,
        expect,
    );
}

#[test]
fn org_dblock_update_all_errors_indentation_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a dynamic block\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((writes nil)
        (messages nil))
    (defun org-dblock-write:oracle-safe (params)
      (push (list 'safe
                  (plist-get params :name)
                  (plist-get params :label)
                  (plist-get params :content)
                  (plist-get params :indentation-column))
            writes)
      (insert (format "- label :: %s\n" (plist-get params :label)))
      (insert "#+begin_quote\n")
      (insert (format "content=%S\n" (plist-get params :content)))
      (insert "#+end_quote\n"))
    (defun org-dblock-write:oracle-error (params)
      (push (list 'error
                  (plist-get params :name)
                  (plist-get params :reason)
                  (plist-get params :content)
                  (plist-get params :indentation-column))
            writes)
      (error "oracle-error %S" (plist-get params :reason)))
    (with-temp-buffer
      (org-mode)
      (insert "* Dynamic blocks\n")
      (insert "  #+BEGIN: oracle-safe :label \"indented\"\n")
      (insert "  old safe line\n")
      (insert "  #+END:\n\n")
      (insert "#+BEGIN: oracle-missing :label \"missing\"\n")
      (insert "stale missing body\n")
      (insert "#+END:\n\n")
      (insert "#+BEGIN: oracle-error :reason bad-news\n")
      (insert "stale error body\n")
      (insert "#+END:\n\n")
      (insert "#+BEGIN: oracle-safe :label \"after-error\" :items (1 2)\n")
      (insert "- old item\n")
      (insert "#+END:\n")
      (let ((initial (buffer-substring-no-properties
                      (point-min) (point-max)))
            first-update all-update unterminated-error begin-errors
            dblock-positions ast)
        (cl-letf (((symbol-function 'message)
                   (lambda (fmt &rest args)
                     (push (apply #'format fmt args) messages))))
          (goto-char (point-min))
          (search-forward "oracle-safe")
          (org-update-dblock)
          (setq first-update
                (buffer-substring-no-properties
                 (point-min) (point-max)))
          (goto-char (point-min))
          (setq all-update
                (condition-case err
                    (progn
                      (org-update-all-dblocks)
                      (list 'ok
                            (buffer-substring-no-properties
                             (point-min) (point-max))))
                  (error
                   (list 'error (cons (car err) (cdr err))
                         (buffer-substring-no-properties
                          (point-min) (point-max))))))
          (goto-char (point-max))
          (insert "\n#+BEGIN: oracle-safe :label \"unterminated\"\n")
          (insert "unterminated body\n")
          (setq unterminated-error
                (condition-case err
                    (progn (org-update-all-dblocks) 'no-error)
                  (error (cons (car err) (cdr err)))))
          (setq begin-errors
                (mapcar
                 (lambda (needle)
                   (goto-char (point-min))
                   (search-forward needle)
                   (condition-case err
                       (progn (org-beginning-of-dblock)
                              (list needle 'ok (line-number-at-pos)))
                     (error (list needle (cons (car err) (cdr err))))))
                 '("label :: indented" "stale missing" "stale error"
                   "unterminated body" "Dynamic blocks"))))
        (goto-char (point-min))
        (setq dblock-positions
              (let (out)
                (while (re-search-forward org-dblock-start-re nil t)
                  (push (list (match-string 1)
                              (match-string 3)
                              (line-number-at-pos)
                              (current-indentation)
                              (save-excursion
                                (condition-case err
                                    (progn (org-prepare-dblock)
                                           (buffer-substring-no-properties
                                            (point) (line-end-position)))
                                  (error (cons (car err) (cdr err)))))))
                        out))
                (nreverse out)))
        (setq ast
              (org-element-map (org-element-parse-buffer)
                  '(dynamic-block quote-block plain-list headline)
                (lambda (e)
                  (list (org-element-type e)
                        (org-element-property :block-name e)
                        (org-element-property :begin e)
                        (org-element-property :end e)))))
        (list initial
              first-update
              all-update
              unterminated-error
              begin-errors
              dblock-positions
              ast
              (nreverse writes)
              (nreverse messages)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_clocktable_columnview_custom_dblock_update_regen_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Alpha :work:\\n:PROPERTIES:\\n:Effort: 2:00\\n:Owner: Ada\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\\nCLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 11:45] =>  0:45\\n:END:\\n** Sub A1\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 13:00]--[2026-05-27 Wed 14:00] =>  1:00\\n:END:\\n* Beta :home:\\n:PROPERTIES:\\n:Effort: 1:00\\n:Owner: Bob\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 15:00]--[2026-05-27 Wed 15:30] =>  0:30\\n:END:\\n#+BEGIN: clocktable :maxlevel 2 :scope file\\n#+END:\\n\\n#+BEGIN: columnview :hlines 1 :id local\\n#+END:\\n\\n#+BEGIN: clocktable :maxlevel 3 :scope file :block today\\n#+END:\\n\" \"* Alpha :work:\\n:PROPERTIES:\\n:Effort: 2:00\\n:Owner: Ada\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\\nCLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 11:45] =>  0:45\\n:END:\\n** Sub A1\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 13:00]--[2026-05-27 Wed 14:00] =>  1:00\\n:END:\\n* Beta :home:\\n:PROPERTIES:\\n:Effort: 1:00\\n:Owner: Bob\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 15:00]--[2026-05-27 Wed 15:30] =>  0:30\\n:END:\\n#+BEGIN: clocktable :maxlevel 2 :scope file\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     |   Time |      |\\n|--------------+--------+------|\\n| *Total time* | *3:45* |      |\\n|--------------+--------+------|\\n| Alpha        |   3:15 |      |\\n| \\\\_  Sub A1   |        | 1:00 |\\n| Beta         |   0:30 |      |\\n#+END:\\n\\n#+BEGIN: columnview :hlines 1 :id local\\n#+END:\\n\\n#+BEGIN: clocktable :maxlevel 3 :scope file :block today\\n#+END:\\n\" \"* Alpha :work:\\n:PROPERTIES:\\n:Effort: 2:00\\n:Owner: Ada\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\\nCLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 11:45] =>  0:45\\n:END:\\n** Sub A1\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 13:00]--[2026-05-27 Wed 14:00] =>  1:00\\n:END:\\n* Beta :home:\\n:PROPERTIES:\\n:Effort: 1:00\\n:Owner: Bob\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 15:00]--[2026-05-27 Wed 15:30] =>  0:30\\n:END:\\n#+BEGIN: clocktable :maxlevel 2 :scope file\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     |   Time |      |\\n|--------------+--------+------|\\n| *Total time* | *3:45* |      |\\n|--------------+--------+------|\\n| Alpha        |   3:15 |      |\\n| \\\\_  Sub A1   |        | 1:00 |\\n| Beta         |   0:30 |      |\\n#+END:\\n\\n#+BEGIN: columnview :hlines 1 :id local\\n| <25> |      | <3>      |        |\\n| ITEM | TODO | PRIORITY | TAGS   |\\n|------+------+----------+--------|\\n| Beta |      | B        | :home: |\\n#+END:\\n\\n#+BEGIN: clocktable :maxlevel 3 :scope file :block today\\n#+END:\\n\" \"* Alpha :work:\\n:PROPERTIES:\\n:Effort: 2:00\\n:Owner: Ada\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\\nCLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 11:45] =>  0:45\\n:END:\\n** Sub A1\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 13:00]--[2026-05-27 Wed 14:00] =>  1:00\\n:END:\\n* Beta :home:\\n:PROPERTIES:\\n:Effort: 1:00\\n:Owner: Bob\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 15:00]--[2026-05-27 Wed 15:30] =>  0:30\\n:END:\\n#+BEGIN: clocktable :maxlevel 2 :scope file\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     |   Time |      |\\n|--------------+--------+------|\\n| *Total time* | *3:45* |      |\\n|--------------+--------+------|\\n| Alpha        |   3:15 |      |\\n| \\\\_  Sub A1   |        | 1:00 |\\n| Beta         |   0:30 |      |\\n#+END:\\n\\n#+BEGIN: columnview :hlines 1 :id local\\n| <25> |      | <3>      |        |\\n| ITEM | TODO | PRIORITY | TAGS   |\\n|------+------+----------+--------|\\n| Beta |      | B        | :home: |\\n#+END:\\n\\n#+BEGIN: clocktable :maxlevel 3 :scope file :block today\\n#+END:\\n\" \"* Alpha :work:\\n:PROPERTIES:\\n:Effort: 2:00\\n:Owner: Ada\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\\nCLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 11:45] =>  0:45\\n:END:\\n** Sub A1\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 16:00]--[2026-05-27 Wed 16:30] =>  0:30\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 13:00]--[2026-05-27 Wed 14:00] =>  1:00\\n:END:\\n* Beta :home:\\n:PROPERTIES:\\n:Effort: 1:00\\n:Owner: Bob\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-27 Wed 15:00]--[2026-05-27 Wed 15:30] =>  0:30\\n:END:\\n#+BEGIN: clocktable :maxlevel 2 :scope file\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     |   Time |      |\\n|--------------+--------+------|\\n| *Total time* | *4:15* |      |\\n|--------------+--------+------|\\n| Alpha        |   3:45 |      |\\n| \\\\_  Sub A1   |        | 1:30 |\\n| Beta         |   0:30 |      |\\n#+END:\\n\\n#+BEGIN: columnview :hlines 1 :id local\\n| <25> |      | <3>      |        |\\n| ITEM | TODO | PRIORITY | TAGS   |\\n|------+------+----------+--------|\\n| Beta |      | B        | :home: |\\n#+END:\\n\\n#+BEGIN: clocktable :maxlevel 3 :scope file :block today\\n#+END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "* Alpha :work:\n")
    (insert ":PROPERTIES:\n:Effort: 2:00\n:Owner: Ada\n:END:\n")
    (insert ":LOGBOOK:\n")
    (insert "CLOCK: [2026-05-27 Wed 09:00]--[2026-05-27 Wed 10:30] =>  1:30\n")
    (insert "CLOCK: [2026-05-27 Wed 11:00]--[2026-05-27 Wed 11:45] =>  0:45\n")
    (insert ":END:\n")
    (insert "** Sub A1\n")
    (insert ":LOGBOOK:\n")
    (insert "CLOCK: [2026-05-27 Wed 13:00]--[2026-05-27 Wed 14:00] =>  1:00\n")
    (insert ":END:\n")
    (insert "* Beta :home:\n")
    (insert ":PROPERTIES:\n:Effort: 1:00\n:Owner: Bob\n:END:\n")
    (insert ":LOGBOOK:\n")
    (insert "CLOCK: [2026-05-27 Wed 15:00]--[2026-05-27 Wed 15:30] =>  0:30\n")
    (insert ":END:\n")
    (insert "#+BEGIN: clocktable :maxlevel 2 :scope file\n")
    (insert "#+END:\n\n")
    (insert "#+BEGIN: columnview :hlines 1 :id local\n")
    (insert "#+END:\n\n")
    (insert "#+BEGIN: clocktable :maxlevel 3 :scope file :block today\n")
    (insert "#+END:\n")
    (let ((snap (lambda ()
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))
      ;; Initial state
      (let ((initial (funcall snap)))
        ;; Update first clocktable
        (goto-char (point-min))
        (search-forward "clocktable :maxlevel 2")
        (org-dblock-update)
        (let ((after-ct1 (funcall snap)))
          ;; Update columnview
          (goto-char (point-min))
          (search-forward "columnview")
          (org-dblock-update)
          (let ((after-col (funcall snap)))
            ;; Update all dblocks
            (org-dblock-update)
            (let ((after-all (funcall snap)))
              ;; Mutate clock data and re-update
              (goto-char (point-min))
              (search-forward "Sub A1")
              (end-of-line)
              (insert "\n:LOGBOOK:\nCLOCK: [2026-05-27 Wed 16:00]--[2026-05-27 Wed 16:30] =>  0:30\n:END:")
              (goto-char (point-min))
              (search-forward "clocktable :maxlevel 2")
              (org-dblock-update)
              (let ((after-mutate (funcall snap)))
                (list initial
                      after-ct1
                      after-col
                      after-all
                      after-mutate)))))))))"##,
        expect,
    );
}
#[test]
fn org_dblock_clocktable_columnview_multi_update_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Project A\\n** Task A1\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 10:30] =>  1:30\\n:END:\\n** Task A2\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 11:00]--[2026-05-28 Wed 12:00] =>  1:00\\n:END:\\n* Project B\\n** Task B1\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 14:00]--[2026-05-28 Wed 15:30] =>  1:30\\n:END:\\n#+BEGIN: clocktable :maxlevel 2 :scope nil :block 2026-05-28\\n#+END:\\n\\n#+BEGIN: columnview :maxlevel 2\\n#+END:\\n\" \"* Project A\\n** Task A1\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 10:30] =>  1:30\\n:END:\\n** Task A2\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 11:00]--[2026-05-28 Wed 12:00] =>  1:00\\n:END:\\n* Project B\\n** Task B1\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 14:00]--[2026-05-28 Wed 15:30] =>  1:30\\n:END:\\n#+BEGIN: clocktable :maxlevel 2 :scope nil :block 2026-05-28\\n#+CAPTION: Clock summary at [2026-10-27 Tue 05:44], for Thursday, May 28, 2026.\\n| Headline     |   Time |      |\\n|--------------+--------+------|\\n| *Total time* | *4:00* |      |\\n|--------------+--------+------|\\n| Project A    |   2:30 |      |\\n| \\\\_  Task A1  |        | 1:30 |\\n| \\\\_  Task A2  |        | 1:00 |\\n| Project B    |   1:30 |      |\\n| \\\\_  Task B1  |        | 1:30 |\\n#+END:\\n\\n#+BEGIN: columnview :maxlevel 2\\n#+END:\\n\" \"* Project A\\n** Task A1\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 10:30] =>  1:30\\n:END:\\n** Task A2\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 11:00]--[2026-05-28 Wed 12:00] =>  1:00\\n:END:\\n* Project B\\n** Task B1\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 14:00]--[2026-05-28 Wed 15:30] =>  1:30\\n:END:\\n#+BEGIN: clocktable :maxlevel 2 :scope nil :block 2026-05-28\\n#+CAPTION: Clock summary at [2026-10-27 Tue 05:44], for Thursday, May 28, 2026.\\n| Headline     |   Time |      |\\n|--------------+--------+------|\\n| *Total time* | *4:00* |      |\\n|--------------+--------+------|\\n| Project A    |   2:30 |      |\\n| \\\\_  Task A1  |        | 1:30 |\\n| \\\\_  Task A2  |        | 1:00 |\\n| Project B    |   1:30 |      |\\n| \\\\_  Task B1  |        | 1:30 |\\n#+END:\\n\\n#+BEGIN: columnview :maxlevel 2\\n| <25>    |      | <3>      |      |\\n| ITEM    | TODO | PRIORITY | TAGS |\\n|---------+------+----------+------|\\n| Task B1 |      | B        |      |\\n#+END:\\n\" \"* Project A\\n** Task A1\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 10:30] =>  1:30\\n:END:\\n** Task A2\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 11:00]--[2026-05-28 Wed 12:00] =>  1:00\\n:END:\\n* Project B\\n** Task B1\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 16:00]--[2026-05-28 Wed 17:00] =>  1:00\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-05-28 Wed 14:00]--[2026-05-28 Wed 15:30] =>  1:30\\n:END:\\n#+BEGIN: clocktable :maxlevel 2 :scope nil :block 2026-05-28\\n#+CAPTION: Clock summary at [2026-10-27 Tue 05:44], for Thursday, May 28, 2026.\\n| Headline     |   Time |      |\\n|--------------+--------+------|\\n| *Total time* | *5:00* |      |\\n|--------------+--------+------|\\n| Project A    |   2:30 |      |\\n| \\\\_  Task A1  |        | 1:30 |\\n| \\\\_  Task A2  |        | 1:00 |\\n| Project B    |   2:30 |      |\\n| \\\\_  Task B1  |        | 2:30 |\\n#+END:\\n\\n#+BEGIN: columnview :maxlevel 2\\n| <25>    |      | <3>      |      |\\n| ITEM    | TODO | PRIORITY | TAGS |\\n|---------+------+----------+------|\\n| Task B1 |      | B        |      |\\n#+END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (require 'org-colview)
  (with-temp-buffer
    (org-mode)
    (insert "* Project A\n")
    (insert "** Task A1\n")
    (insert ":LOGBOOK:\nCLOCK: [2026-05-28 Wed 09:00]--[2026-05-28 Wed 10:30] =>  1:30\n:END:\n")
    (insert "** Task A2\n")
    (insert ":LOGBOOK:\nCLOCK: [2026-05-28 Wed 11:00]--[2026-05-28 Wed 12:00] =>  1:00\n:END:\n")
    (insert "* Project B\n")
    (insert "** Task B1\n")
    (insert ":LOGBOOK:\nCLOCK: [2026-05-28 Wed 14:00]--[2026-05-28 Wed 15:30] =>  1:30\n:END:\n")
    (insert "#+BEGIN: clocktable :maxlevel 2 :scope nil :block 2026-05-28\n")
    (insert "#+END:\n\n")
    (insert "#+BEGIN: columnview :maxlevel 2\n")
    (insert "#+END:\n")
    (let ((snap (lambda ()
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))
      (let ((initial (funcall snap)))
        ;; Update clocktable
        (goto-char (point-min))
        (search-forward "clocktable :maxlevel 2")
        (org-dblock-update)
        (let ((after-ct (funcall snap)))
          ;; Update columnview
          (goto-char (point-min))
          (search-forward "columnview")
          (org-dblock-update)
          (let ((after-col (funcall snap)))
            ;; Mutate: add clock to Task B1
            (goto-char (point-min))
            (search-forward "Task B1")
            (end-of-line)
            (insert "\n:LOGBOOK:\nCLOCK: [2026-05-28 Wed 16:00]--[2026-05-28 Wed 17:00] =>  1:00\n:END:")
            ;; Re-update clocktable
            (goto-char (point-min))
            (search-forward "clocktable :maxlevel 2")
            (org-dblock-update)
            (let ((after-mutate (funcall snap)))
              (list initial
                    after-ct
                    after-col
                    after-mutate))))))))"##,
        expect,
    );
}
