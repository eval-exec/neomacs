use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_table_named_formula_constants_recalc_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Unknown field: Tax\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "#+CONSTANTS: tax=2 fee=3\n")
    (insert "| Item | Net | Tax | Total |\n")
    (insert "|------+-----+-----+-------|\n")
    (insert "| a | 10 |  |  |\n")
    (insert "| b | 20 |  |  |\n")
    (insert "#+TBLFM: $Tax=$Net*$tax::$Total=$Net+$Tax+$fee\n")
    (goto-char (point-min))
    (org-table-recalculate-buffer-tables)
    (goto-char (point-min))
    (search-forward "a")
    (list (org-table-formula-substitute-names "$Net*$tax+$fee")
          (buffer-substring-no-properties
           (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn org_table_iterate_dependency_formula_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable org-table-iterate-max)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "| n | prev | next |\n")
    (insert "|---+------+------|\n")
    (insert "| 1 | 0 |  |\n")
    (insert "| 2 |  |  |\n")
    (insert "| 3 |  |  |\n")
    (insert "#+TBLFM: @3$2=@2$3::@4$2=@3$3::$3=$1+$2\n")
    (goto-char (point-min))
    (let ((org-table-iterate-max 5))
      (org-table-iterate))
    (list org-table-iterate-max
          (buffer-substring-no-properties
           (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn org_table_field_replace_and_formula_rewrite_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\" 2 \" \"| A | B | C |\\n|---+---+---|\\n| 1 | 5 | 6 |\\n| 3 | 4 | 7 |\\n#+TBLFM: $3=$1+$2\\n\" \"| A | C |\\n|---+---|\\n| 1 | 6 |\\n| 3 | 7 |\\n#+TBLFM: $2=$1+$INVALID\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "| A | B | C |\n")
    (insert "|---+---+---|\n")
    (insert "| 1 | 2 |  |\n")
    (insert "| 3 | 4 |  |\n")
    (insert "#+TBLFM: $3=$1+$2\n")
    (goto-char (point-min))
    (search-forward "2")
    (let ((old (org-table-get-field nil "5")))
      (org-table-align)
      (org-table-recalculate-buffer-tables)
      (let ((after-recalc
             (buffer-substring-no-properties
              (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "B")
        (org-table-delete-column)
        (list old
              after-recalc
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_table_remote_named_range_hline_total_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (#(\"2\" 0 1 (face org-table)) (#(\"3\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"7\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"22\" 0 2 (face org-table))) \"#+NAME: rates\\n| Kind | Rate |\\n|------+------|\\n| base |    2 |\\n| rush |    5 |\\n\\n#+NAME: jobs\\n| Job | Hours | Fee | Total |\\n|-----+-------+-----+-------|\\n| A   |     3 |   1 |     7 |\\n| B   |     4 |   2 |    22 |\\n| Sum |       |     |    29 |\\n#+TBLFM: @2$4=$2*remote(rates,@2$2)+$3::@3$4=$2*remote(rates,@3$2)+$3::@4$4=vsum(@2$4..@3$4)\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: rates\n")
    (insert "| Kind | Rate |\n")
    (insert "|------+------|\n")
    (insert "| base | 2 |\n")
    (insert "| rush | 5 |\n\n")
    (insert "#+NAME: jobs\n")
    (insert "| Job | Hours | Fee | Total |\n")
    (insert "|-----+-------+-----+-------|\n")
    (insert "| A | 3 | 1 | |\n")
    (insert "| B | 4 | 2 | |\n")
    (insert "| Sum | | | |\n")
    (insert "#+TBLFM: @2$4=$2*remote(rates,@2$2)+$3::@3$4=$2*remote(rates,@3$2)+$3::@4$4=vsum(@2$4..@3$4)\n")
    (goto-char (point-min))
    (search-forward "#+NAME: jobs")
    (org-table-recalculate-buffer-tables)
    (goto-char (point-min))
    (search-forward "#+NAME: jobs")
    (list (org-table-get-remote-range "rates" "@2$2")
          (org-table-get-remote-range "jobs" "@2$2..@3$4")
          (buffer-substring-no-properties
           (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn org_table_formula_reference_conversion_and_current_field_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Unknown field: Line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Item | Qty | Price | Line |\n")
    (insert "|------+-----+-------+------|\n")
    (insert "| one | 2 | 10 | |\n")
    (insert "| two | 3 | 7 | |\n")
    (insert "| ! | | | Total |\n")
    (insert "#+TBLFM: $Line=$Qty*$Price;%.1f::@>$Line=vsum(@2..@-1);%.2f\n")
    (goto-char (point-min))
    (org-table-recalculate-buffer-tables)
    (goto-char (point-min))
    (search-forward "10")
    (org-table-next-field)
    (org-table-analyze)
    (let ((field-formula (org-table-current-field-formula 'key 'noerror))
          (current-line (org-table-current-line))
          (current-column (org-table-current-column)))
      (list field-formula
            current-line
            current-column
            (org-table-convert-refs-to-rc "B3..D4")
            (org-table-convert-refs-to-an "@3$2..@4$4")
            (org-table-formula-substitute-names "$Line=$Qty*$Price")
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_table_rectangle_cut_paste_sum_wrap_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | Q1 | Q2 | Q3 | Note |\n")
    (insert "|------+----+----+----+------|\n")
    (insert "| a | 1 | 2 | 3 | alpha beta gamma |\n")
    (insert "| b | 4 | 5 | 6 | delta epsilon |\n")
    (insert "| c | 7 | 8 | 9 | zeta eta |\n")
    (goto-char (point-min))
    (search-forward "2")
    (let ((beg (point)))
      (search-forward "8")
      (let* ((clip (org-table-copy-region beg (point) t))
             (after-cut (buffer-substring-no-properties
                         (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "delta")
        (org-table-paste-rectangle)
        (let ((after-paste (buffer-substring-no-properties
                            (point-min) (point-max))))
          (goto-char (point-min))
          (search-forward "Q2")
          (let ((sum (org-table-sum nil nil 3)))
            (goto-char (point-min))
            (search-forward "alpha")
            (search-forward "beta")
            (org-table-wrap-region nil)
            (list clip
                  after-cut
                  after-paste
                  sum
                  (current-kill 0 t)
                  (buffer-substring-no-properties
                   (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_table_create_convert_export_import_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 40 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let* ((root (make-temp-file "org-table-io" t))
         (csv (expand-file-name "data.csv" root))
         (out (expand-file-name "out.tsv" root)))
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          (insert "name,qty,price\nalpha,2,10\nbeta,3,7\n")
          (org-table-convert-region (point-min) (point-max) ",")
          (let ((converted (buffer-substring-no-properties
                            (point-min) (point-max))))
            (goto-char (point-max))
            (insert "\n")
            (org-table-create "2x3")
            (let ((created (buffer-substring-no-properties
                            (point-min) (point-max))))
              (with-temp-file csv
                (insert "x;y;z\n1;2;3\n4;5;6\n"))
              (goto-char (point-max))
              (insert "\n")
              (org-table-import csv ";")
              (org-table-export out "orgtbl-to-tsv")
              (list converted
                    created
                    (org-table-to-lisp
                     (buffer-substring-no-properties
                      (save-excursion
                        (goto-char (point-min))
                        (search-forward "x")
                        (line-beginning-position))
                      (point-max)))
                    (with-temp-buffer
                      (insert-file-contents out)
                      (buffer-substring-no-properties
                       (point-min) (point-max)))
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_table_row_column_cell_motion_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| A | B | C |\n")
    (insert "|---+---+---|\n")
    (insert "| 1 | 2 | 3 |\n")
    (insert "| 4 | 5 | 6 |\n")
    (insert "| 7 | 8 | 9 |\n")
    (goto-char (point-min))
    (search-forward "5")
    (org-table-move-cell-left)
    (org-table-move-cell-up)
    (let ((after-cell (buffer-substring-no-properties
                       (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "7")
      (org-table-move-row-up)
      (goto-char (point-min))
      (search-forward "C")
      (org-table-move-column-left)
      (let ((after-row-col (buffer-substring-no-properties
                            (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "B")
        (org-table-insert-column)
        (org-table-put 1 (org-table-current-column) "Inserted" t)
        (goto-char (point-min))
        (search-forward "4")
        (org-table-insert-row)
        (org-table-put (org-table-current-dline) 1 "new" t)
        (list after-cell
              after-row-col
              (org-table-to-lisp)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_table_column_width_shrink_expand_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | <6> Description | Count |\n")
    (insert "|------+-----------------+-------|\n")
    (insert "| A | alpha beta gamma | 1 |\n")
    (insert "| B | delta epsilon zeta | 2 |\n")
    (goto-char (point-min))
    (search-forward "Description")
    (let ((before (buffer-substring-no-properties
                   (point-min) (point-max))))
      (org-table-toggle-column-width)
      (font-lock-ensure (point-min) (point-max))
      (let ((shrunk
             (mapcar
              (lambda (needle)
                (save-excursion
                  (goto-char (point-min))
                  (search-forward needle)
                  (list needle
                        (get-text-property (match-beginning 0) 'display)
                        (get-text-property (match-beginning 0) 'invisible)
                        (overlays-at (match-beginning 0)))))
              '("Description" "alpha" "delta"))))
        (org-table-expand (point-min) (point-max))
        (let ((expanded
               (mapcar
                (lambda (needle)
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward needle)
                    (list needle
                          (get-text-property (match-beginning 0) 'display)
                          (get-text-property (match-beginning 0)
                                             'invisible))))
                '("Description" "alpha" "delta"))))
          (list before
                shrunk
                expanded
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_table_hline_formula_sort_recalc_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Item | Qty | Price | Total |\n")
    (insert "|------+-----+-------+-------|\n")
    (insert "| A | 2 | 10 | |\n")
    (insert "| B | 3 | 7 | |\n")
    (insert "|------+-----+-------+-------|\n")
    (insert "| Sum | | | |\n")
    (insert "#+TBLFM: @2$4=$2*$3::@3$4=$2*$3::@>$4=vsum(@I..@II)\n")
    (goto-char (point-min))
    (org-table-recalculate 'all)
    (let ((stored-before (org-table-get-stored-formulas))
          (after-first (buffer-substring-no-properties
                        (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "B")
      (beginning-of-line)
      (org-table-insert-row 'below)
      (org-table-put (org-table-current-dline) 1 "C" t)
      (org-table-put (org-table-current-dline) 2 "4" t)
      (org-table-put (org-table-current-dline) 3 "5" t)
      (org-table-insert-hline 'below)
      (goto-char (point-min))
      (search-forward "TBLFM")
      (org-table-eval-formula nil "$4=$2*$3")
      (goto-char (point-min))
      (org-table-recalculate 'all)
      (let ((stored-after (org-table-get-stored-formulas))
            (to-user (org-table-formula-to-user
                      "@2$4=$2*$3::@4$4=$2*$3::@>$4=vsum(@2$4..@-1$4)"))
            (from-user (org-table-formula-from-user
                        "@2$4=$2*$3::@4$4=$2*$3::@>$4=vsum(@2$4..@-1$4)"))
            (first-last (org-table-formula-handle-first/last-rc
                         "@>$4=vsum(@I..@II)")))
        (list stored-before
              after-first
              stored-after
              to-user
              from-user
              first-last
              (sort (copy-sequence stored-after) #'org-table-formula-less-p)
              (org-table-to-lisp)
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_table_sort_transpose_formula_metadata_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | Jan | Feb | Mar | Total |\n")
    (insert "|------+-----+-----+-----+-------|\n")
    (insert "| B | 2 | 3 | 4 | |\n")
    (insert "| A | 5 | 6 | 7 | |\n")
    (insert "|------+-----+-----+-----+-------|\n")
    (insert "| Sum | | | | |\n")
    (insert "#+TBLFM: @2$5=vsum($2..$4)::@3$5=vsum($2..$4)::@>$5=vsum(@2..@-1)\n")
    (goto-char (point-min))
    (org-table-recalculate 'all)
    (let ((after-recalc
           (buffer-substring-no-properties (point-min) (point-max)))
          (formulas-before (org-table-get-stored-formulas)))
      (goto-char (point-min))
      (search-forward "Name")
      (org-table-sort-lines nil ?a)
      (let ((after-sort
             (buffer-substring-no-properties (point-min) (point-max)))
            (field-summary nil))
        (goto-char (point-min))
        (search-forward "6")
        (org-table-analyze)
        (setq field-summary
              (list (org-table-current-dline)
                    (org-table-current-column)
                    (org-table-get-field)
                    (org-table-current-field-formula 'key 'noerror)))
        (goto-char (point-min))
        (search-forward "Name")
        (org-table-transpose-table-at-point)
        (org-table-align)
        (let ((after-transpose
               (buffer-substring-no-properties (point-min) (point-max)))
              (formulas-after (org-table-get-stored-formulas)))
          (list after-recalc
                formulas-before
                after-sort
                field-summary
                after-transpose
                formulas-after
                (org-table-to-lisp)
                (org-table-formula-substitute-names "$Total=vsum($Jan..$Mar)")
                (org-table-convert-refs-to-rc "B3..E4")
                (org-table-convert-refs-to-an "@3$2..@4$5"))))))"##,
        expect,
    );
}

#[test]
fn org_table_marked_duration_lisp_create_columns_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"| Mark | Task  | Estimate | Spent |  Remain | Owner |                 |\\n|------+-------+----------+-------+---------+-------+-----------------|\\n| #    | Alpha |     2:30 |  1:15 |   01:15 | ann   | Alpha:ann:01:15 |\\n|      | Beta  |     1:10 |  0:40 | stale-b | bob   |                 |\\n| #    | Gamma |     3:00 |  2:20 |   00:40 | cy    | Gamma:cy:00:40  |\\n#+TBLFM: $5=$3-$4;U::$7='(concat $2 \\\":\\\" $6 \\\":\\\" $5)\\n\" ((\"$5\" . \"$3-$4;U\") (\"$7\" . \"'(concat $2 \\\":\\\" $6 \\\":\\\" $5)\")) #(\" Gamma:cy:00:40  \" 0 1 (face org-table rear-nonsticky t display (space :relative-width 1)) 1 15 (face org-table) 15 16 (face org-table) 16 17 (face org-table display (space :relative-width 1.001))) \"$7\" (2 3 4 5) \"| Mark | Task  | Estimate | Spent | Remain | Owner |                 |\\n|------+-------+----------+-------+--------+-------+-----------------|\\n| #    | Alpha |     2:30 |  1:15 |  01:15 | ann   | Alpha:ann:01:15 |\\n|      | Beta  |     1:10 |  0:10 |  01:00 | bob   | Beta:bob:01:00  |\\n| #    | Gamma |     3:00 |  2:20 |  00:40 | cy    | Gamma:cy:00:40  |\\n#+TBLFM: $5=$3-$4;U::$7='(concat $2 \\\":\\\" $6 \\\":\\\" $5)\\n\" ((\"$5\" . \"$3-$4;U\") (\"$7\" . \"'(concat $2 \\\":\\\" $6 \\\":\\\" $5)\")) 7 #(\" Beta:bob:01:00  \" 0 1 (face org-table rear-nonsticky t display (space :relative-width 1)) 1 15 (face org-table) 15 16 (face org-table) 16 17 (face org-table display (space :relative-width 1.001))) ((#(\"Mark\" 0 4 (face org-table)) #(\"Task\" 0 4 (face org-table)) #(\"Estimate\" 0 8 (face org-table)) #(\"Spent\" 0 5 (face org-table)) #(\"Remain\" 0 6 (face org-table)) #(\"Owner\" 0 5 (face org-table)) \"\") hline (#(\"#\" 0 1 (face org-formula)) #(\"Alpha\" 0 5 (face org-table)) #(\"2:30\" 0 4 (face org-table)) #(\"1:15\" 0 4 (face org-table)) #(\"01:15\" 0 5 (face org-table)) #(\"ann\" 0 3 (face org-table)) #(\"Alpha:ann:01:15\" 0 15 (face org-table))) (\"\" #(\"Beta\" 0 4 (face org-table)) #(\"1:10\" 0 4 (face org-table)) #(\"0:10\" 0 4 (face org-table)) #(\"01:00\" 0 5 (face org-table)) #(\"bob\" 0 3 (face org-table)) #(\"Beta:bob:01:00\" 0 14 (face org-table))) (#(\"#\" 0 1 (face org-formula)) #(\"Gamma\" 0 5 (face org-table)) #(\"3:00\" 0 4 (face org-table)) #(\"2:20\" 0 4 (face org-table)) #(\"00:40\" 0 5 (face org-table)) #(\"cy\" 0 2 (face org-table)) #(\"Gamma:cy:00:40\" 0 14 (face org-table)))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (let ((org-table-formula-create-columns t))
      (insert "| Mark | Task | Estimate | Spent | Remain | Owner |\n")
      (insert "|------+------|----------+-------+--------+-------|\n")
      (insert "| # | Alpha | 2:30 | 1:15 | stale-a | ann |\n")
      (insert "|   | Beta | 1:10 | 0:40 | stale-b | bob |\n")
      (insert "| # | Gamma | 3:00 | 2:20 | stale-c | cy |\n")
      (insert "#+TBLFM: $5=$3-$4;U::$7='(concat $2 \":\" $6 \":\" $5)\n")
      (goto-char (point-min))
      (org-table-recalculate 'all)
      (let ((after-marked
             (buffer-substring-no-properties (point-min) (point-max)))
            (stored-after-marked (org-table-get-stored-formulas))
            (lisp-summary nil)
            (range-corners nil)
            (field-formula nil))
        (goto-char (point-min))
        (search-forward "Gamma")
        (org-table-goto-column 7)
        (setq lisp-summary (org-table-get-field))
        (setq field-formula
              (org-table-current-field-formula 'key 'noerror))
        (setq range-corners
              (org-table-get-range "@2$3..@4$5" nil nil nil t))
        (goto-char (point-min))
        (search-forward "Beta")
        (org-table-goto-column 4)
        (org-table-get-field nil "0:10")
        (org-table-recalculate)
        (let ((after-current
               (buffer-substring-no-properties (point-min) (point-max)))
              (stored-after-current (org-table-get-stored-formulas)))
          (goto-char (point-min))
          (search-forward "Beta")
          (org-table-goto-column 7)
        (list after-marked
                stored-after-marked
                lisp-summary
                field-formula
                range-corners
                after-current
                stored-after-current
                (org-table-current-column)
                (org-table-get-field)
                (org-table-to-lisp)))))))"##,
        expect,
    );
}

#[test]
fn org_table_structural_edit_formula_metadata_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Unknown field: Total\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "#+CONSTANTS: bonus=2\n")
    (insert "#+NAME: scores\n")
    (insert "| Name | Q1 | Q2 | Extra | Total |\n")
    (insert "|------+----+----+-------+-------|\n")
    (insert "| Ada | 3 | 4 | 1 | |\n")
    (insert "| Bea | 5 | 2 | 0 | |\n")
    (insert "| Cy | 1 | 6 | 3 | |\n")
    (insert "|------+----+----+-------+-------|\n")
    (insert "| Sum | | | | |\n")
    (insert "#+TBLFM: $Total=vsum($Q1..$Extra)+$bonus::@>$Total=vsum(@2..@-1)\n")
    (goto-char (point-min))
    (org-table-recalculate-buffer-tables)
    (let ((after-recalc
           (buffer-substring-no-properties (point-min) (point-max)))
          (stored-before (org-table-get-stored-formulas)))
      (goto-char (point-min))
      (search-forward "Bea")
      (beginning-of-line)
      (org-table-insert-row 'below)
      (org-table-put (org-table-current-dline) 1 "Dee" t)
      (org-table-put (org-table-current-dline) 2 "2" t)
      (org-table-put (org-table-current-dline) 3 "8" t)
      (org-table-put (org-table-current-dline) 4 "4" t)
      (org-table-copy-down 1)
      (let ((after-insert-copy
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "Dee")
        (beginning-of-line)
        (org-table-move-row-up)
        (goto-char (point-min))
        (search-forward "Extra")
        (org-table-move-column-left)
        (let ((after-move
               (buffer-substring-no-properties (point-min) (point-max))))
          (goto-char (point-min))
          (search-forward "Name")
          (org-table-sort-lines nil ?N)
          (goto-char (point-min))
          (search-forward "Ada")
          (org-table-goto-column 5)
          (let ((field-before-delete
                 (list (org-table-current-dline)
                       (org-table-current-column)
                       (org-table-get-field)
                       (org-table-current-field-formula 'key 'noerror)
                       (org-table-formula-substitute-names
                        "$Total=vsum($Q1..$Extra)+$bonus"))))
            (goto-char (point-min))
            (search-forward "Q2")
            (org-table-delete-column)
            (goto-char (point-min))
            (search-forward "Sum")
            (beginning-of-line)
            (org-table-insert-hline 'above)
            (org-table-recalculate-buffer-tables)
            (let ((stored-after (org-table-get-stored-formulas))
                  (after-delete-hline
                   (buffer-substring-no-properties
                    (point-min) (point-max))))
              (goto-char (point-min))
              (search-forward "Ada")
              (org-table-goto-column 4)
              (list after-recalc
                    stored-before
                    after-insert-copy
                    after-move
                    field-before-delete
                    stored-after
                    (org-table-get-field)
                    (org-table-current-field-formula 'key 'noerror)
                    (org-table-get-remote-range "scores" "@2$2..@4$4")
                    (org-table-convert-refs-to-rc "B3..D5")
                    (org-table-convert-refs-to-an "@3$2..@5$4")
                    (org-table-to-lisp)
                    after-delete-hline))))))))"##,
        expect,
    );
}

#[test]
fn org_table_hline_constants_range_property_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "#+CONSTANTS: fee=7 discount=4\n")
    (insert "| Item | Qty | Price | Gross | Adjusted | Net |\n")
    (insert "|------+-----+-------+-------+----------+-----|\n")
    (insert "| A | 2 | 10 | stale | stale | stale |\n")
    (insert "| B | 3 | 12 | stale | stale | stale |\n")
    (insert "|------+-----+-------+-------+----------+-----|\n")
    (insert "| Total |  |  | stale | stale | stale |\n")
    (insert "#+TBLFM: $Gross=$Qty*$Price::$Adjusted=$Gross+$fee::$Net=$Adjusted-$discount::@>$Gross=vsum(@I$Gross..@II$Gross)::@>$Adjusted=vsum(@I$Adjusted..@II$Adjusted)::@>$Net=vsum(@I$Net..@II$Net)\n")
    (goto-char (point-min))
    (org-table-recalculate 'all)
    (let ((after-first
           (buffer-substring-no-properties (point-min) (point-max)))
          (stored-first (org-table-get-stored-formulas))
          (range-values nil)
          (range-corners nil)
          (first-last nil)
          (substituted nil)
          (converted-rc nil)
          (converted-an nil)
          (field-summary nil)
          (prop-summary nil))
      (goto-char (point-min))
      (search-forward "B")
      (org-table-goto-column 5)
      (org-table-analyze)
      (setq range-values (org-table-get-range "@I$Gross..@II$Net"))
      (setq range-corners (org-table-get-range "@I$Gross..@II$Net" nil nil nil t))
      (setq first-last
            (org-table-formula-handle-first/last-rc "@>$>=$<+@<$<"))
      (setq substituted
            (org-table-formula-substitute-names "$Gross+$fee-$discount"))
      (setq converted-rc (org-table-convert-refs-to-rc "D3..F4"))
      (setq converted-an (org-table-convert-refs-to-an "@3$4..@4$6"))
      (setq field-summary
            (list (org-table-current-dline)
                  (org-table-current-column)
                  (org-table-get-field)
                  (org-table-current-field-formula 'key 'noerror)
                  (org-table-get-constant "fee")
                  (org-table-get-constant "missing")))
      (org-table-put-field-property 'org-oracle-marker "adjusted-b")
      (let ((start (progn (skip-chars-backward "^|") (point)))
            (end (progn (skip-chars-forward "^|") (point))))
        (setq prop-summary
              (list (get-text-property start 'org-oracle-marker)
                    (text-property-any start end 'org-oracle-marker "adjusted-b"))))
      (org-table-get-field nil "50")
      (org-table-recalculate)
      (let ((after-edit
             (buffer-substring-no-properties (point-min) (point-max)))
            (stored-edit (org-table-get-stored-formulas)))
        (list after-first
              stored-first
              range-values
              range-corners
              first-last
              substituted
              converted-rc
              converted-an
              field-summary
              prop-summary
              after-edit
              stored-edit
              (org-table-to-lisp))))))"##,
        expect,
    );
}

#[test]
fn org_table_named_remote_debug_structural_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "#+CONSTANTS: scale=3 offset=2\n")
    (insert "#+NAME: lookup\n")
    (insert "| Key | Weight |\n")
    (insert "|-----+--------|\n")
    (insert "| low | 2 |\n")
    (insert "| high | 5 |\n\n")
    (insert "#+NAME: calc\n")
    (insert "| ! | Name | Kind | Raw | Weight | Score | Note |\n")
    (insert "|---+------+------+-----+--------+-------+------|\n")
    (insert "| # | A | low | 4 |  |  | keep |\n")
    (insert "|   | B | high | 3 |  |  | move |\n")
    (insert "| # | C | low | 6 |  |  | keep |\n")
    (insert "|---+------+------+-----+--------+-------+------|\n")
    (insert "| _ | Total | | | | | |\n")
    (insert "#+TBLFM: $Weight=remote(lookup,@2$2)::@3$Weight=remote(lookup,@3$2)::$Score=$Raw*$Weight*$scale+$offset::@>$Score=vsum(@I$Score..@II$Score)\n")
    (goto-char (point-min))
    (search-forward "#+NAME: calc")
    (let ((org-table-formula-debug t)
          (org-table-formula-create-columns t))
      (org-table-recalculate 'all)
      (let ((after-first
             (buffer-substring-no-properties (point-min) (point-max)))
            (stored-first (org-table-get-stored-formulas))
            (debug-formula
             (org-table-formula-substitute-names
              "$Score=$Raw*$Weight*$scale+$offset"))
            field-summary remote-summary range-summary conversions)
        (goto-char (point-min))
        (search-forward "B")
        (org-table-goto-column 6)
        (org-table-analyze)
        (setq field-summary
              (list (org-table-current-dline)
                    (org-table-current-column)
                    (org-table-current-line)
                    (org-table-get-field)
                    (org-table-current-field-formula 'key 'noerror)
                    (get-text-property 0 :orig-formula debug-formula)))
        (setq remote-summary
              (list (org-table-get-remote-range "lookup" "@2$2..@3$2")
                    (org-table-get-remote-range "calc" "@2$Name..@4$Score")
                    (org-table-get-constant "scale")
                    (org-table-get-constant "missing")))
        (setq range-summary
              (list (org-table-get-range "@I$Raw..@II$Score")
                    (org-table-get-range "@I$Raw..@II$Score" nil nil nil t)))
        (setq conversions
              (list (org-table-formula-handle-first/last-rc
                     "@>$Score=vsum(@I$Score..@II$Score)")
                    (org-table-convert-refs-to-rc "D3..F5")
                    (org-table-convert-refs-to-an "@3$4..@5$6")
                    (org-table-formula-to-user
                     "@>$6=vsum(@2$6..@4$6)")
                    (org-table-formula-from-user
                     "@>$Score=vsum(@2$Score..@4$Score)")))
        (goto-char (point-min))
        (search-forward "B")
        (beginning-of-line)
        (org-table-move-row-up)
        (goto-char (point-min))
        (search-forward "Note")
        (org-table-move-column-left)
        (goto-char (point-min))
        (search-forward "C")
        (org-table-goto-column 4)
        (org-table-get-field nil "7")
        (org-table-recalculate 'all)
        (let ((after-edit
               (buffer-substring-no-properties (point-min) (point-max)))
              (stored-edit (org-table-get-stored-formulas)))
          (list after-first
                stored-first
                debug-formula
                field-summary
                remote-summary
                range-summary
                conversions
                after-edit
                stored-edit
                (org-table-to-lisp)))))))"##,
        expect,
    );
}

#[test]
fn org_table_shrink_coordinates_formula_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Unknown field: Total\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| ! | Name | Count | Price | Total | Notes |\n")
    (insert "|---+------+-------+-------+-------+-------|\n")
    (insert "| # | alpha item | 2 | 10.50 | | long note alpha beta gamma |\n")
    (insert "|   | beta item | 3 | 7.25 | | long note beta delta epsilon |\n")
    (insert "| # | gamma item | 4 | 2.00 | | compact |\n")
    (insert "|---+------+-------+-------+-------+-------|\n")
    (insert "| _ | Sum | | | | |\n")
    (insert "#+TBLFM: $Total=$Count*$Price;%.2f::@>$Total=vsum(@I$Total..@II$Total);%.2f\n")
    (goto-char (point-min))
    (org-table-recalculate 'all)
    (org-table-align)
    (let ((after-recalc
           (buffer-substring-no-properties (point-min) (point-max)))
          (stored (org-table-get-stored-formulas))
          (formula-at-alpha
           (save-excursion
             (goto-char (point-min))
             (search-forward "alpha")
             (org-table-goto-column 5)
             (org-table-current-field-formula 'key 'noerror)))
          (coordinate-before nil)
          (coordinate-after nil)
          (shrink-before nil)
          (shrink-after nil)
          (range-overlay nil))
      (org-table-toggle-coordinate-overlays)
      (setq coordinate-before
            (mapcar (lambda (ov)
                      (list (overlay-start ov)
                            (overlay-end ov)
                            (overlay-get ov 'display)
                            (overlay-get ov 'face)
                            (overlay-get ov 'evaporate)))
                    org-table-coordinate-overlays))
      (org-table-toggle-coordinate-overlays)
      (setq coordinate-after
            (and (boundp 'org-table-coordinate-overlays)
                 org-table-coordinate-overlays))
      (goto-char (point-min))
      (search-forward "long note alpha")
      (org-table-toggle-column-width 12)
      (setq shrink-before
            (mapcar (lambda (ov)
                      (list (overlay-start ov)
                            (overlay-end ov)
                            (overlay-get ov 'display)
                            (overlay-get ov 'org-table-column-shrinked)
                            (overlay-get ov 'evaporate)))
                    (overlays-in (point-min) (point-max))))
      (org-table-expand)
      (setq shrink-after
            (mapcar (lambda (ov)
                      (list (overlay-start ov)
                            (overlay-end ov)
                            (overlay-get ov 'org-table-column-shrinked)))
                    (overlays-in (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "alpha item")
      (let ((beg (point)))
        (search-forward "7.25")
        (org-table-highlight-rectangle beg (point))
        (setq range-overlay
              (mapcar (lambda (ov)
                        (list (overlay-start ov)
                              (overlay-end ov)
                              (overlay-get ov 'face)
                              (overlay-get ov 'evaporate)))
                      (overlays-in (point-min) (point-max))))
        (org-table-remove-rectangle-highlight))
      (goto-char (point-min))
      (search-forward "beta item")
      (beginning-of-line)
      (org-table-move-row-down)
      (goto-char (point-min))
      (search-forward "gamma item")
      (org-table-goto-column 3)
      (org-table-get-field nil "5")
      (org-table-recalculate 'all)
      (list after-recalc
            stored
            formula-at-alpha
            coordinate-before
            coordinate-after
            shrink-before
            shrink-after
            range-overlay
            (org-table-get-range "@I$Count..@II$Total")
            (org-table-formula-substitute-names
             "$Total=$Count*$Price")
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_table_rectangle_transpose_sort_shape_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| ! | Item | A | B | Sum | Note |\n")
    (insert "|---+------+---+---+-----+------|\n")
    (insert "| # | zeta | 3 | 4 | | tail-z |\n")
    (insert "| # | alpha | 1 | 9 | | tail-a |\n")
    (insert "| # | beta | 2 | 5 | | tail-b |\n")
    (insert "|---+------+---+---+-----+------|\n")
    (insert "| _ | Total | | | | |\n")
    (insert "#+TBLFM: $Sum=$A+$B::@>$Sum=vsum(@I$Sum..@II$Sum)\n")
    (org-table-recalculate 'all)
    (org-table-align)
    (let ((initial (buffer-substring-no-properties
                    (point-min) (point-max)))
          (initial-lisp (org-table-to-lisp))
          rect-copy after-cut after-paste after-insert after-sort
          after-transpose stored formulas range-summary)
      (goto-char (point-min))
      (search-forward "zeta")
      (let ((beg (point)))
        (search-forward "tail-a")
        (setq rect-copy
              (org-table-copy-region beg (point)))
        (org-table-cut-region beg (point)))
      (setq after-cut
            (buffer-substring-no-properties (point-min) (point-max)))
      (goto-char (point-min))
      (search-forward "beta")
      (org-table-goto-column 6)
      (org-table-paste-rectangle)
      (setq after-paste
            (buffer-substring-no-properties (point-min) (point-max)))
      (goto-char (point-min))
      (search-forward "Item")
      (org-table-insert-column)
      (org-table-get-field nil "Group")
      (goto-char (point-min))
      (search-forward "alpha")
      (org-table-goto-column 2)
      (org-table-get-field nil "A")
      (search-forward "beta")
      (org-table-goto-column 2)
      (org-table-get-field nil "B")
      (goto-char (point-min))
      (search-forward "zeta")
      (org-table-goto-column 2)
      (org-table-get-field nil "Z")
      (goto-char (point-min))
      (search-forward "tail-z")
      (org-table-delete-column)
      (org-table-recalculate 'all)
      (setq after-insert
            (buffer-substring-no-properties (point-min) (point-max)))
      (goto-char (point-min))
      (search-forward "zeta")
      (beginning-of-line)
      (org-table-sort-lines nil ?a)
      (org-table-recalculate 'all)
      (setq after-sort
            (buffer-substring-no-properties (point-min) (point-max)))
      (setq stored (org-table-get-stored-formulas)
            formulas
            (mapcar (lambda (needle)
                      (save-excursion
                        (goto-char (point-min))
                        (search-forward needle)
                        (org-table-goto-column 5)
                        (org-table-current-field-formula 'key 'noerror)))
                    '("alpha" "beta" "zeta"))
            range-summary
            (list (org-table-get-range "@I$A..@II$Sum")
                  (org-table-get-range "@I$A..@II$Sum" nil nil nil t)
                  (org-table-formula-substitute-names "$Sum=$A+$B")
                  (org-table-formula-to-user "$5=$3+$4")))
      (org-table-transpose-table-at-point)
      (setq after-transpose
            (buffer-substring-no-properties (point-min) (point-max)))
      (list initial
            initial-lisp
            rect-copy
            after-cut
            after-paste
            after-insert
            after-sort
            stored
            formulas
            range-summary
            after-transpose
            (org-table-to-lisp)))))"##,
        expect,
    );
}

#[test]
fn org_table_navigation_copydown_formula_edit_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (user-error \"Unknown field: Remain\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (let ((org-table-tab-jumps-over-hlines t)
          (org-table-copy-increment t)
          (org-table-formula-create-columns t))
      (insert "| ! | Task | Day | Seq | Estimate | Done | Remain | Stamp |\n")
      (insert "|---+------+-----+-----+----------+------+--------+-------|\n")
      (insert "| # | Alpha | <2026-05-27 Wed> | item-001 | 2:30 | 1:00 | stale | stale |\n")
      (insert "| # | Beta | <2026-05-28 Thu> | item-002 | 3:15 | 0:45 | stale | stale |\n")
      (insert "|   | Gamma |  |  | 1:10 | 0:20 | stale | stale |\n")
      (insert "|---+------+-----+-----+----------+------+--------+-------|\n")
      (insert "| _ | Total |  |  | stale | stale | stale | stale |\n")
      (insert "#+TBLFM: $Remain=$Estimate-$Done;U::$Stamp='(concat $Task \":\" $Seq \":\" $Remain)::@>$Estimate=vsum(@I$Estimate..@II$Estimate);U::@>$Done=vsum(@I$Done..@II$Done);U::@>$Remain=vsum(@I$Remain..@II$Remain);U\n")
      (goto-char (point-min))
      (org-table-recalculate 'all)
      (org-table-align)
      (let ((initial
             (buffer-substring-no-properties (point-min) (point-max)))
            (stored-initial (org-table-get-stored-formulas))
            nav-summary copy-summary blank-summary formula-summary
            after-copy after-blank after-formula final-summary)
        (goto-char (point-min))
        (search-forward "Beta")
        (org-table-goto-column 8)
        (org-table-next-field)
        (setq nav-summary
              (list (org-table-current-dline)
                    (org-table-current-column)
                    (org-table-get-field)
                    (org-table-current-line)
                    (thing-at-point 'line t)))
        (goto-char (point-min))
        (search-forward "Gamma")
        (org-table-goto-column 3)
        (org-table-copy-down 1)
        (setq copy-summary
              (list (org-table-current-dline)
                    (org-table-current-column)
                    (org-table-get-field)))
        (org-table-goto-column 4)
        (org-table-copy-down 1)
        (setq copy-summary
              (append copy-summary
                      (list (org-table-current-column)
                            (org-table-get-field))))
        (setq after-copy
              (buffer-substring-no-properties (point-min) (point-max)))
        (goto-char (point-min))
        (search-forward "Beta")
        (org-table-goto-column 6)
        (let ((old (org-table-blank-field)))
          (setq blank-summary
                (list old
                      (org-table-get-field)
                      (org-table-current-dline)
                      (org-table-current-column))))
        (org-table-get-field nil "0:55")
        (org-table-recalculate 'all)
        (setq after-blank
              (buffer-substring-no-properties (point-min) (point-max)))
        (goto-char (point-min))
        (search-forward "Alpha")
        (org-table-goto-column 7)
        (org-table-eval-formula '(4) "$5-$6;U")
        (setq formula-summary
              (list (org-table-get-field)
                    (org-table-current-field-formula 'key 'noerror)
                    (org-table-formula-substitute-names
                     "$Remain=$Estimate-$Done")
                    (org-table-formula-to-user "$7=$5-$6")
                    (org-table-formula-from-user "$Remain=$Estimate-$Done")))
        (goto-char (point-min))
        (search-forward "Gamma")
        (org-table-goto-column 8)
        (org-table-get-field nil "manual")
        (org-table-recalculate 'all)
        (setq after-formula
              (buffer-substring-no-properties (point-min) (point-max)))
        (goto-char (point-min))
        (search-forward "Total")
        (org-table-goto-column 5)
        (org-table-next-row)
        (org-table-get-field nil "Delta")
        (org-table-goto-column 5)
        (org-table-get-field nil "4:00")
        (org-table-goto-column 6)
        (org-table-get-field nil "1:30")
        (org-table-goto-column 7)
        (org-table-eval-formula nil "$5-$6;U")
        (org-table-recalculate 'all)
        (setq final-summary
              (list (org-table-current-dline)
                    (org-table-current-column)
                    (org-table-get-field)
                    (org-table-get-range "@I$Estimate..@II$Remain")
                    (org-table-get-range "@I$Estimate..@II$Remain" nil nil nil t)
                    (org-table-get-stored-formulas)
                    (org-table-to-lisp)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))
        (list initial
              stored-initial
              nav-summary
              copy-summary
              after-copy
              blank-summary
              after-blank
              formula-summary
              after-formula
              final-summary)))))"##,
        expect,
    );
}

#[test]
fn org_table_cross_ref_edit_recalc_lisp_dump_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#+NAME: prices\\n| Item     | Unit | Qty | Subtotal |\\n|----------+------+-----+----------|\\n| Apples   |  2.5 |   4 |      10. |\\n| Bananas  |  1.8 |   6 |     10.8 |\\n| Cherries |  5.0 |   2 |      10. |\\n|----------+------+-----+----------|\\n| Total    |      |     |     30.8 |\\n#+TBLFM: @2$4=$2*$3::@3$4=$2*$3::@4$4=$2*$3::@>$4=vsum(@2..@-1)\\n\\n#+NAME: summary\\n| Category | Amount |\\n|----------+--------|\\n| Fruit    |   30.8 |\\n| Tax      |   3.08 |\\n| Grand    |  33.88 |\\n#+TBLFM: @2$2=remote(prices,@>$4)::@3$2=@2$2*0.1;%.2f::@4$2=@2$2+@3$2;%.2f\\n\" ((#(\"Item\" 0 4 (face org-table)) #(\"Unit\" 0 4 (face org-table)) #(\"Qty\" 0 3 (face org-table)) #(\"Subtotal\" 0 8 (face org-table))) hline (#(\"Apples\" 0 6 (face org-table)) #(\"2.5\" 0 3 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"10.\" 0 3 (face org-table))) (#(\"Bananas\" 0 7 (face org-table)) #(\"1.8\" 0 3 (face org-table)) #(\"6\" 0 1 (face org-table)) #(\"10.8\" 0 4 (face org-table))) (#(\"Cherries\" 0 8 (face org-table)) #(\"5.0\" 0 3 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"10.\" 0 3 (face org-table))) hline (#(\"Total\" 0 5 (face org-table)) \"\" \"\" #(\"30.8\" 0 4 (face org-table)))) ((#(\"Category\" 0 8 (face org-table)) #(\"Amount\" 0 6 (face org-table))) hline (#(\"Fruit\" 0 5 (face org-table)) #(\"30.8\" 0 4 (face org-table))) (#(\"Tax\" 0 3 (face org-table)) #(\"3.08\" 0 4 (face org-table))) (#(\"Grand\" 0 5 (face org-table)) #(\"33.88\" 0 5 (face org-table)))) \"#+NAME: prices\\n| Item     | Unit | Qty | Subtotal |\\n|----------+------+-----+----------|\\n| Apples   |  2.5 |   4 |      10. |\\n| Bananas  |  1.8 |  10 |      18. |\\n| Cherries |  5.0 |   2 |      10. |\\n|----------+------+-----+----------|\\n| Total    |      |     |      38. |\\n#+TBLFM: @2$4=$2*$3::@3$4=$2*$3::@4$4=$2*$3::@>$4=vsum(@2..@-1)\\n\\n#+NAME: summary\\n| Category | Amount |\\n|----------+--------|\\n| Fruit    |    38. |\\n| Tax      |   3.80 |\\n| Grand    |  41.80 |\\n#+TBLFM: @2$2=remote(prices,@>$4)::@3$2=@2$2*0.1;%.2f::@4$2=@2$2+@3$2;%.2f\\n\" ((#(\"Item\" 0 4 (face org-table)) #(\"Unit\" 0 4 (face org-table)) #(\"Qty\" 0 3 (face org-table)) #(\"Subtotal\" 0 8 (face org-table))) hline (#(\"Apples\" 0 6 (face org-table)) #(\"2.5\" 0 3 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"10.\" 0 3 (face org-table))) (#(\"Bananas\" 0 7 (face org-table)) #(\"1.8\" 0 3 (face org-table)) #(\"10\" 0 2 (face org-table)) #(\"18.\" 0 3 (face org-table))) (#(\"Cherries\" 0 8 (face org-table)) #(\"5.0\" 0 3 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"10.\" 0 3 (face org-table))) hline (#(\"Total\" 0 5 (face org-table)) \"\" \"\" #(\"38.\" 0 3 (face org-table)))) ((#(\"Category\" 0 8 (face org-table)) #(\"Amount\" 0 6 (face org-table))) hline (#(\"Fruit\" 0 5 (face org-table)) #(\"38.\" 0 3 (face org-table))) (#(\"Tax\" 0 3 (face org-table)) #(\"3.80\" 0 4 (face org-table))) (#(\"Grand\" 0 5 (face org-table)) #(\"41.80\" 0 5 (face org-table)))) (((\"@2$4\" . \"$2*$3\") (\"@3$4\" . \"$2*$3\") (\"@4$4\" . \"$2*$3\") (\"@>$4\" . \"vsum(@2..@-1)\")) ((\"@2$2\" . \"remote(prices,@>$4)\") (\"@3$2\" . \"@2$2*0.1;%.2f\") (\"@4$2\" . \"@2$2+@3$2;%.2f\"))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: prices\n")
    (insert "| Item | Unit | Qty | Subtotal |\n")
    (insert "|------+------+-----+----------|\n")
    (insert "| Apples | 2.5 | 4 | |\n")
    (insert "| Bananas | 1.8 | 6 | |\n")
    (insert "| Cherries | 5.0 | 2 | |\n")
    (insert "|------+------+-----+----------|\n")
    (insert "| Total | | | |\n")
    (insert "#+TBLFM: @2$4=$2*$3::@3$4=$2*$3::@4$4=$2*$3::@>$4=vsum(@2..@-1)\n\n")
    (insert "#+NAME: summary\n")
    (insert "| Category | Amount |\n")
    (insert "|----------+--------|\n")
    (insert "| Fruit | |\n")
    (insert "| Tax | |\n")
    (insert "| Grand | |\n")
    (insert "#+TBLFM: @2$2=remote(prices,@>$4)::@3$2=@2$2*0.1;%.2f::@4$2=@2$2+@3$2;%.2f\n")
    (goto-char (point-min))
    (org-table-recalculate-buffer-tables)
    (let ((prices-after-recalc
           (buffer-substring-no-properties (point-min) (point-max)))
          (prices-lisp
           (progn
             (goto-char (point-min))
             (search-forward "prices")
             (org-table-to-lisp)))
          (summary-lisp
           (progn
             (goto-char (point-min))
             (search-forward "summary")
             (org-table-to-lisp))))
      ;; Edit a cell and recalculate
      (goto-char (point-min))
      (search-forward "Bananas")
      (org-table-goto-column 3)
      (org-table-get-field nil "10")
      (org-table-recalculate-buffer-tables)
      (let ((after-edit
             (buffer-substring-no-properties (point-min) (point-max)))
            (prices-after-edit-lisp
             (progn
               (goto-char (point-min))
               (search-forward "prices")
               (org-table-to-lisp)))
            (summary-after-edit-lisp
             (progn
               (goto-char (point-min))
               (search-forward "summary")
               (org-table-to-lisp)))
            (stored-formulas
             (progn
               (goto-char (point-min))
               (search-forward "prices")
               (list (org-table-get-stored-formulas)
                     (progn
                       (goto-char (point-min))
                       (search-forward "summary")
                       (org-table-get-stored-formulas))))))
        (list prices-after-recalc
              prices-lisp
              summary-lisp
              after-edit
              prices-after-edit-lisp
              summary-after-edit-lisp
              stored-formulas)))))"##,
        expect,
    );
}

#[test]
fn org_table_field_edit_recalc_range_dump_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"| Item   | Price | Qty | Total |\\n|--------+-------+-----+-------|\\n| Apple  |   2.5 |   4 |   10. |\\n| Banana |   1.8 |   6 |  10.8 |\\n| Cherry |   5.0 |   2 |   10. |\\n|--------+-------+-----+-------|\\n| Total  |       |     |  30.8 |\\n#+TBLFM: @2$4=$2*$3::@3$4=$2*$3::@4$4=$2*$3::@>$4=vsum(@2..@-1)\\n\" ((#(\"Item\" 0 4 (face org-table)) #(\"Price\" 0 5 (face org-table)) #(\"Qty\" 0 3 (face org-table)) #(\"Total\" 0 5 (face org-table))) hline (#(\"Apple\" 0 5 (face org-table)) #(\"2.5\" 0 3 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"10.\" 0 3 (face org-table))) (#(\"Banana\" 0 6 (face org-table)) #(\"1.8\" 0 3 (face org-table)) #(\"6\" 0 1 (face org-table)) #(\"10.8\" 0 4 (face org-table))) (#(\"Cherry\" 0 6 (face org-table)) #(\"5.0\" 0 3 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"10.\" 0 3 (face org-table))) hline (#(\"Total\" 0 5 (face org-table)) \"\" \"\" #(\"30.8\" 0 4 (face org-table)))) #(\"   10. \" 0 1 (face org-table rear-nonsticky t display (space :relative-width 1)) 1 3 (face org-table) 3 6 (face org-table) 6 7 (face org-table display (space :relative-width 1.001))) #(\"  10.8 \" 0 1 (face org-table rear-nonsticky t display (space :relative-width 1)) 1 2 (face org-table) 2 6 (face org-table) 6 7 (face org-table display (space :relative-width 1.001))) \"\" \"| Item   | Price | Qty | Total |\\n|--------+-------+-----+-------|\\n| Apple  |   2.5 |   4 |   10. |\\n| Banana |   1.8 |  10 |   18. |\\n| Cherry |   5.0 |   2 |   10. |\\n|--------+-------+-----+-------|\\n| Total  |       |     |   38. |\\n#+TBLFM: @2$4=$2*$3::@3$4=$2*$3::@4$4=$2*$3::@>$4=vsum(@2..@-1)\\n\" #(\"   18. \" 0 1 (face org-table rear-nonsticky t display (space :relative-width 1)) 1 3 (face org-table) 3 6 (face org-table) 6 7 (face org-table display (space :relative-width 1.001))) \"\" ((\"@2$4\" . \"$2*$3\") (\"@3$4\" . \"$2*$3\") (\"@4$4\" . \"$2*$3\") (\"@>$4\" . \"vsum(@2..@-1)\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "| Item | Price | Qty | Total |\n")
    (insert "|------+-------+-----+-------|\n")
    (insert "| Apple | 2.5 | 4 | |\n")
    (insert "| Banana | 1.8 | 6 | |\n")
    (insert "| Cherry | 5.0 | 2 | |\n")
    (insert "|------+-------+-----+-------|\n")
    (insert "| Total | | | |\n")
    (insert "#+TBLFM: @2$4=$2*$3::@3$4=$2*$3::@4$4=$2*$3::@>$4=vsum(@2..@-1)\n")
    ;; Recalculate
    (goto-char (point-min))
    (org-table-recalculate-buffer-tables)
    (let ((after-recalc (buffer-substring-no-properties
                         (point-min) (point-max)))
          (table-lisp (progn
                        (goto-char (point-min))
                        (org-table-to-lisp)))
          ;; Get specific cell values
          (apple-total (progn
                         (goto-char (point-min))
                         (search-forward "Apple")
                         (org-table-goto-column 4)
                         (org-table-get-field)))
          (banana-total (progn
                          (goto-char (point-min))
                          (search-forward "Banana")
                          (org-table-goto-column 4)
                          (org-table-get-field)))
          (grand-total (progn
                         (goto-char (point-min))
                         (search-forward "Total")
                         (forward-line 1)
                         (beginning-of-line)
                         (org-table-goto-column 4)
                         (org-table-get-field))))
      ;; Edit a cell and recalculate
      (goto-char (point-min))
      (search-forward "Banana")
      (org-table-goto-column 3)
      (org-table-get-field nil "10")
      (org-table-recalculate-buffer-tables)
      (let ((after-edit (buffer-substring-no-properties
                         (point-min) (point-max)))
            (banana-after (progn
                            (goto-char (point-min))
                            (search-forward "Banana")
                            (org-table-goto-column 4)
                            (org-table-get-field)))
            (total-after (progn
                           (goto-char (point-min))
                           (search-forward "Total")
                           (forward-line 1)
                           (beginning-of-line)
                           (org-table-goto-column 4)
                           (org-table-get-field)))
            (stored (org-table-get-stored-formulas)))
        (list after-recalc
              table-lisp
              apple-total
              banana-total
              grand-total
              after-edit
              banana-after
              total-after
               stored)))))"##,
        expect,
    );
}

#[test]
fn org_table_sort_region_hline_formula_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil ((#(\"Name\" 0 4 (face org-table)) #(\"Score\" 0 5 (face org-table))) hline (#(\"Alice\" 0 5 (face org-table)) #(\"95\" 0 2 (face org-table))) (#(\"Bob\" 0 3 (face org-table)) #(\"85\" 0 2 (face org-table))) (#(\"Charlie\" 0 7 (face org-table)) #(\"70\" 0 2 (face org-table))) hline (#(\"Avg\" 0 3 (face org-table)) \"\")) ((#(\"Name\" 0 4 (face org-table)) #(\"Score\" 0 5 (face org-table))) hline (#(\"Alice\" 0 5 (face org-table)) #(\"95\" 0 2 (face org-table))) (#(\"Bob\" 0 3 (face org-table)) #(\"85\" 0 2 (face org-table))) (#(\"Charlie\" 0 7 (face org-table)) #(\"70\" 0 2 (face org-table))) hline (#(\"Avg\" 0 3 (face org-table)) \"\")) ((#(\"Name\" 0 4 (face org-table)) #(\"Score\" 0 5 (face org-table))) hline (#(\"Alice\" 0 5 (face org-table)) #(\"95\" 0 2 (face org-table))) (#(\"Bob\" 0 3 (face org-table)) #(\"85\" 0 2 (face org-table))) (#(\"Charlie\" 0 7 (face org-table)) #(\"70\" 0 2 (face org-table))) hline (#(\"Avg\" 0 3 (face org-table)) #(\"83.333333\" 0 9 (face org-table)))) #(\" 83.333333 \" 0 1 (face org-table rear-nonsticky t display (space :relative-width 1)) 1 10 (face org-table) 10 11 (face org-table display (space :relative-width 1.001))) \"| Name    |     Score |\\n|---------+-----------|\\n| Alice   |        95 |\\n| Bob     |        85 |\\n| Charlie |        70 |\\n|---------+-----------|\\n| Avg     | 83.333333 |\\n#+TBLFM: @>$2=vmean(@2..@-1)\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | Score |\n")
    (insert "|------+-------|\n")
    (insert "| Charlie | 70 |\n")
    (insert "| Alice | 95 |\n")
    (insert "| Bob | 85 |\n")
    (insert "|------+-------|\n")
    (insert "| Avg | |\n")
    (insert "#+TBLFM: @>$2=vmean(@2..@-1)\n")
    ;; Initial state
    (let ((initial (buffer-substring-no-properties
                    (point-min) (point-max)))
          (initial-lisp (org-table-to-lisp)))
      ;; Sort by name alphabetically
      (goto-char (point-min))
      (search-forward "Charlie")
      (beginning-of-line)
      (org-table-sort-lines nil ?a)
      (let ((after-sort-name (buffer-substring-no-properties
                              (point-min) (point-max)))
            (sort-name-lisp (org-table-to-lisp)))
        ;; Sort by score numerically descending
        (goto-char (point-min))
        (search-forward "Alice")
        (beginning-of-line)
        (org-table-sort-lines t ?n)
        (let ((after-sort-score (buffer-substring-no-properties
                                 (point-min) (point-max)))
              (sort-score-lisp (org-table-to-lisp)))
          ;; Recalculate
          (goto-char (point-min))
          (org-table-recalculate-buffer-tables)
          (let ((after-recalc (buffer-substring-no-properties
                               (point-min) (point-max)))
                (recalc-lisp (org-table-to-lisp))
                (avg-val (progn
                           (goto-char (point-min))
                           (search-forward "Avg")
                           (org-table-goto-column 2)
                           (org-table-get-field))))
            (list initial-lisp
                  sort-name-lisp
                  sort-score-lisp
                  recalc-lisp
                  avg-val
                  after-recalc)))))))"##,
        expect,
    );
}

#[test]
fn org_table_formula_multi_col_sort_recalc_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil ((#(\"Name\" 0 4 (face org-table)) #(\"Q1\" 0 2 (face org-table)) #(\"Q2\" 0 2 (face org-table)) #(\"Q3\" 0 2 (face org-table)) #(\"Total\" 0 5 (face org-table))) hline (#(\"Eve\" 0 3 (face org-table)) #(\"30\" 0 2 (face org-table)) #(\"40\" 0 2 (face org-table)) #(\"50\" 0 2 (face org-table)) #(\"120\" 0 3 (face org-table))) (#(\"Alice\" 0 5 (face org-table)) #(\"80\" 0 2 (face org-table)) #(\"90\" 0 2 (face org-table)) #(\"70\" 0 2 (face org-table)) #(\"240\" 0 3 (face org-table))) (#(\"Dave\" 0 4 (face org-table)) #(\"50\" 0 2 (face org-table)) #(\"60\" 0 2 (face org-table)) #(\"40\" 0 2 (face org-table)) #(\"150\" 0 3 (face org-table))) (#(\"Bob\" 0 3 (face org-table)) #(\"90\" 0 2 (face org-table)) #(\"85\" 0 2 (face org-table)) #(\"95\" 0 2 (face org-table)) #(\"270\" 0 3 (face org-table))) (#(\"Carol\" 0 5 (face org-table)) #(\"70\" 0 2 (face org-table)) #(\"75\" 0 2 (face org-table)) #(\"80\" 0 2 (face org-table)) #(\"225\" 0 3 (face org-table)))) ((#(\"Name\" 0 4 (face org-table)) #(\"Q1\" 0 2 (face org-table)) #(\"Q2\" 0 2 (face org-table)) #(\"Q3\" 0 2 (face org-table)) #(\"Total\" 0 5 (face org-table))) hline (#(\"Eve\" 0 3 (face org-table)) #(\"30\" 0 2 (face org-table)) #(\"40\" 0 2 (face org-table)) #(\"50\" 0 2 (face org-table)) #(\"120\" 0 3 (face org-table))) (#(\"Alice\" 0 5 (face org-table)) #(\"80\" 0 2 (face org-table)) #(\"90\" 0 2 (face org-table)) #(\"70\" 0 2 (face org-table)) #(\"240\" 0 3 (face org-table))) (#(\"Dave\" 0 4 (face org-table)) #(\"50\" 0 2 (face org-table)) #(\"60\" 0 2 (face org-table)) #(\"40\" 0 2 (face org-table)) #(\"150\" 0 3 (face org-table))) (#(\"Bob\" 0 3 (face org-table)) #(\"95\" 0 2 (face org-table)) #(\"85\" 0 2 (face org-table)) #(\"95\" 0 2 (face org-table)) #(\"275\" 0 3 (face org-table))) (#(\"Carol\" 0 5 (face org-table)) #(\"70\" 0 2 (face org-table)) #(\"75\" 0 2 (face org-table)) #(\"80\" 0 2 (face org-table)) #(\"225\" 0 3 (face org-table)))) #(\"   275 \" 0 1 (face org-table rear-nonsticky t display (space :relative-width 1)) 1 3 (face org-table) 3 6 (face org-table) 6 7 (face org-table display (space :relative-width 1.001))) \"| Name  | Q1 | Q2 | Q3 | Total |\\n|-------+----+----+----+-------|\\n| Eve   | 30 | 40 | 50 |   120 |\\n| Alice | 80 | 90 | 70 |   240 |\\n| Dave  | 50 | 60 | 40 |   150 |\\n| Bob   | 95 | 85 | 95 |   275 |\\n| Carol | 70 | 75 | 80 |   225 |\\n#+TBLFM: $5=vsum($2..$4)\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | Q1 | Q2 | Q3 | Total |\n")
    (insert "|------+----+----+----+-------|\n")
    (insert "| Eve | 30 | 40 | 50 | |\n")
    (insert "| Alice | 80 | 90 | 70 | |\n")
    (insert "| Dave | 50 | 60 | 40 | |\n")
    (insert "| Bob | 90 | 85 | 95 | |\n")
    (insert "| Carol | 70 | 75 | 80 | |\n")
    (insert "#+TBLFM: $5=vsum($2..$4)\n")
    (org-table-recalculate-buffer-tables)
    (let ((initial-lisp (org-table-to-lisp))
          (initial-buf (buffer-substring-no-properties
                        (point-min) (point-max))))
      ;; Sort by Total descending
      (goto-char (point-min))
      (search-forward "Total")
      (beginning-of-line)
      (org-table-sort-lines t ?n)
      (let ((sorted-lisp (org-table-to-lisp))
            (sorted-buf (buffer-substring-no-properties
                         (point-min) (point-max))))
        ;; Edit: change Bob's Q1 to 95
        (goto-char (point-min))
        (search-forward "Bob")
        (org-table-goto-column 2)
        (org-table-get-field nil "95")
        ;; Recalculate
        (org-table-recalculate-buffer-tables)
        (let ((edited-lisp (org-table-to-lisp))
              (edited-buf (buffer-substring-no-properties
                           (point-min) (point-max)))
              (bob-total (progn
                           (goto-char (point-min))
                           (search-forward "Bob")
                           (org-table-goto-column 5)
                           (org-table-get-field))))
          (list initial-lisp
                sorted-lisp
                edited-lisp
                bob-total
                edited-buf))))))"##,
        expect,
    );
}

#[test]
fn org_table_formula_avg_sum_edit_sort_recalc_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 44 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | Math | Sci | Eng | Total | Avg |\n")
    (insert "|------+------+-----+-----+-------+-----|\n")
    (insert "| Eve | 65 | 70 | 60 | | |\n")
    (insert "| Alice | 90 | 95 | 88 | | |\n")
    (insert "| Dave | 72 | 68 | 75 | | |\n")
    (insert "| Bob | 85 | 92 | 90 | | |\n")
    (insert "| Carol | 78 | 80 | 82 | | |\n")
    (insert "#+TBLFM: $5=vsum($2..$4)::$6=vmean($2..$4)\n")
    (org-table-recalculate-buffer-tables)
    (let ((initial-lisp (org-table-to-lisp)))
      ;; Sort by Total descending
      (goto-char (point-min))
      (search-forward "Total")
      (beginning-of-line)
      (org-table-sort-lines t ?n)
      (let ((sorted-lisp (org-table-to-lisp)))
        ;; Edit: change Eve's Math to 88
        (goto-char (point-min))
        (search-forward "Eve")
        (org-table-goto-column 2)
        (org-table-get-field nil "88")
        (org-table-recalculate-buffer-tables)
        (let ((edited-lisp (org-table-to-lisp))
              (eve-total (progn
                           (goto-char (point-min))
                           (search-forward "Eve")
                           (org-table-goto-column 5)
                           (org-table-get-field)))
              (eve-avg (progn
                         (goto-char (point-min))
                         (search-forward "Eve")
                         (org-table-goto-column 6)
                         (org-table-get-field))))
          (list initial-lisp
                sorted-lisp
                edited-lisp
                eve-total
                eve-avg
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_table_formula_multi_col_sort_edit_recalc_v2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 41 48)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | HW1 | HW2 | Mid | Fin | Total | Grade |\n")
    (insert "|------+------+-----+-----+-----+-------+-------|\n")
    (insert "| Eve | 70 | 75 | 65 | 72 | | |\n")
    (insert "| Alice | 95 | 92 | 88 | 90 | | |\n")
    (insert "| Dave | 80 | 78 | 82 | 76 | | |\n")
    (insert "| Bob | 90 | 95 | 92 | 88 | | |\n")
    (insert "| Carol | 85 | 80 | 78 | 82 | | |\n")
    (insert "#+TBLFM: $6=vsum($2..$5)::$7=if($6>=400,\"A\",if($6>=350,\"B\",\"C\"))\n")
    (org-table-recalculate-buffer-tables)
    (let ((initial-lisp (org-table-to-lisp)))
      ;; Sort by Total descending
      (goto-char (point-min))
      (search-forward "Total")
      (beginning-of-line)
      (org-table-sort-lines t ?n)
      (let ((sorted-lisp (org-table-to-lisp)))
        ;; Edit: change Eve's HW1 to 90
        (goto-char (point-min))
        (search-forward "Eve")
        (org-table-goto-column 2)
        (org-table-get-field nil "90")
        (org-table-recalculate-buffer-tables)
        (let ((edited-lisp (org-table-to-lisp))
              (eve-total (progn
                           (goto-char (point-min))
                           (search-forward "Eve")
                           (org-table-goto-column 6)
                           (org-table-get-field)))
              (eve-grade (progn
                           (goto-char (point-min))
                           (search-forward "Eve")
                           (org-table-goto-column 7)
                           (org-table-get-field))))
          (list initial-lisp sorted-lisp edited-lisp
                eve-total eve-grade
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_table_formula_sort_edit_resort_recalc_v3() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 48 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| Name | A | B | C | Total |\n")
    (insert "|------+------+---+---+-------|\n")
    (insert "| Zara | 60 | 70 | 80 | |\n")
    (insert "| Adam | 90 | 85 | 95 | |\n")
    (insert "| Yuki | 75 | 80 | 70 | |\n")
    (insert "| Bill | 88 | 92 | 86 | |\n")
    (insert "| Xena | 70 | 65 | 75 | |\n")
    (insert "#+TBLFM: $5=vsum($2..$4)\n")
    (org-table-recalculate-buffer-tables)
    (let ((initial (org-table-to-lisp)))
      ;; Sort by Name alpha
      (goto-char (point-min))
      (search-forward "Name")
      (beginning-of-line)
      (org-table-sort-lines nil ?a)
      (let ((sorted-alpha (org-table-to-lisp)))
        ;; Sort by Total numeric desc
        (goto-char (point-min))
        (search-forward "Total")
        (beginning-of-line)
        (org-table-sort-lines t ?n)
        (let ((sorted-total (org-table-to-lisp)))
          ;; Edit: change Zara A from 60 to 95
          (goto-char (point-min))
          (search-forward "Zara")
          (org-table-goto-column 2)
          (org-table-get-field nil "95")
          (org-table-recalculate-buffer-tables)
          (let ((after-edit (org-table-to-lisp))
                (zara-total (progn
                              (goto-char (point-min))
                              (search-forward "Zara")
                              (org-table-goto-column 5)
                              (org-table-get-field))))
            ;; Re-sort by Total desc
            (goto-char (point-min))
            (search-forward "Total")
            (beginning-of-line)
            (org-table-sort-lines t ?n)
            (let ((resorted (org-table-to-lisp)))
              (list initial sorted-alpha sorted-total
                    after-edit zara-total resorted
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}

#[test]
fn org_table_formula_string_col_edit_sort_recalc_v4() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 53 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (with-temp-buffer
    (org-mode)
    (insert "| City | Jan | Feb | Mar | Total | Avg |\n")
    (insert "|------+------+-----+-----+-------+-----|\n")
    (insert "| Tokyo | 50 | 55 | 60 | | |\n")
    (insert "| Oslo | 20 | 25 | 30 | | |\n")
    (insert "| Miami | 70 | 75 | 80 | | |\n")
    (insert "| Berlin | 30 | 35 | 40 | | |\n")
    (insert "| Sydney | 60 | 65 | 70 | | |\n")
    (insert "#+TBLFM: $5=vsum($2..$4)::$6=vmean($2..$4)\n")
    (org-table-recalculate-buffer-tables)
    (let ((initial (org-table-to-lisp)))
      ;; Sort by City alpha
      (goto-char (point-min))
      (search-forward "City")
      (beginning-of-line)
      (org-table-sort-lines nil ?a)
      (let ((sorted-alpha (org-table-to-lisp)))
        ;; Sort by Total desc
        (goto-char (point-min))
        (search-forward "Total")
        (beginning-of-line)
        (org-table-sort-lines t ?n)
        (let ((sorted-total (org-table-to-lisp)))
          ;; Edit: change Tokyo Jan from 50 to 80
          (goto-char (point-min))
          (search-forward "Tokyo")
          (org-table-goto-column 2)
          (org-table-get-field nil "80")
          (org-table-recalculate-buffer-tables)
          (let ((after-edit (org-table-to-lisp))
                (tokyo-total (progn
                               (goto-char (point-min))
                               (search-forward "Tokyo")
                               (org-table-goto-column 5)
                               (org-table-get-field)))
                (tokyo-avg (progn
                              (goto-char (point-min))
                              (search-forward "Tokyo")
                              (org-table-goto-column 6)
                              (org-table-get-field))))
            ;; Re-sort by Total desc
            (goto-char (point-min))
            (search-forward "Total")
            (beginning-of-line)
            (org-table-sort-lines t ?n)
            (let ((resorted (org-table-to-lisp)))
              (list initial sorted-alpha sorted-total
                    after-edit tokyo-total tokyo-avg resorted
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))))))))"##,
        expect,
    );
}
