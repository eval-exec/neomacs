use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_plot_options_tsv_quote_script_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument plistp ((:plot-type . 2d) (:with . lines) (:ind . 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-plot)
  (let* ((root (make-temp-file "org-plot-tsv" t))
         (data-file (expand-file-name "plot.dat" root))
         (png-file (expand-file-name "out.png" root)))
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          (insert "#+PLOT: title:\"Demo Plot\" ind:1 deps:(2 3) with:lines\n")
          (insert "#+PLOT: set:\"grid\" set:\"key outside\" line:\"set xlabel 'Day'\" file:\"out.png\"\n")
          (insert "| Day | A | B |\n")
          (insert "|-----+---+---|\n")
          (insert "| <2026-05-27 Wed> | 2 | 3 |\n")
          (insert "| label \"x\" | 5 | 8 |\n")
          (goto-char (point-min))
          (let* ((params (org-plot/collect-options
                          (org-plot/collect-options
                           (copy-sequence org-plot/gnuplot-default-options))))
                 (table (progn
                          (org-plot/goto-nearest-table)
                          (delq 'hline (cdr (org-table-to-lisp)))))
                 (org-plot-timestamp-fmt "%Y/%m/%d")
                 (quoted (mapcar #'org-plot-quote-tsv-field
                                 '("<2026-05-27 Wed>" "label \"x\"" "42")))
                 (params (plist-put params :file png-file)))
            (org-plot/gnuplot-to-data table data-file params)
            (let* ((data (with-temp-buffer
                           (insert-file-contents data-file)
                           (buffer-string)))
                   (script (replace-regexp-in-string
                            (regexp-quote root)
                            "<root>"
                            (org-plot/gnuplot-script table data-file 3 params))))
              (list (plist-get params :title)
                    (plist-get params :ind)
                    (plist-get params :deps)
                    (plist-get params :with)
                    (reverse (plist-get params :set))
                    (plist-get params :line)
                    quoted
                    data
                    script))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_plot_grid_data_map_ticks_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((1 . \"North\") (2 . \"South\") (3 . \"West\")) 0 10 10.0 6 \"0.000000  0.500000  1.000000\\n0.000000  1.500000  1.000000\\n0.000000  1.500000  2.000000\\n0.000000  2.500000  2.000000\\n0.000000  2.500000  3.000000\\n0.000000  3.500000  3.000000\\n\\n1.000000  0.500000  1.000000\\n1.000000  1.500000  1.000000\\n1.000000  1.500000  2.000000\\n1.000000  2.500000  2.000000\\n1.000000  2.500000  3.000000\\n1.000000  3.500000  3.000000\\n\\n1.000000  0.500000  2.000000\\n1.000000  1.500000  2.000000\\n1.000000  1.500000  4.000000\\n1.000000  2.500000  4.000000\\n1.000000  2.500000  9.000000\\n1.000000  3.500000  9.000000\\n\\n2.000000  0.500000  2.000000\\n2.000000  1.500000  2.000000\\n2.000000  1.500000  4.000000\\n2.000000  2.500000  4.000000\\n2.000000  2.500000  9.000000\\n2.000000  3.500000  9.000000\\n\\n2.000000  0.500000  4.000000\\n2.000000  1.500000  4.000000\\n2.000000  1.500000  8.000000\\n2.000000  2.500000  8.000000\\n2.000000  2.500000  27.000000\\n2.000000  3.500000  27.000000\\n\\n3.000000  0.500000  4.000000\\n3.000000  1.500000  4.000000\\n3.000000  1.500000  8.000000\\n3.000000  2.500000  8.000000\\n3.000000  2.500000  27.000000\\n3.000000  3.500000  27.000000\\n\\n\" \"reset\\nset term GNUTERM \\nset pm3d map\\n\\nset datafile separator \\\"\\\\t\\\"\\nset ytics (\\\"North\\\" 1, \\\"South\\\" 2, \\\"West\\\" 3)\\nsplot '<root>/grid.dat' with pm3d title ''\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-plot)
  (let* ((root (make-temp-file "org-plot-grid" t))
         (data-file (expand-file-name "grid.dat" root))
         (table '(("North" "1" "2" "4")
                  ("South" "2" "4" "8")
                  ("West" "3" "9" "27")))
         (params '(:plot-type grid :ind 1 :deps (2 3 4) :map t :with pm3d)))
    (unwind-protect
        (let* ((row-labels
                (org-plot/gnuplot-to-grid-data table data-file params))
               (data (with-temp-buffer
                       (insert-file-contents data-file)
                       (buffer-string)))
               (params (plist-put params :ylabels row-labels))
               (script (replace-regexp-in-string
                        (regexp-quote root)
                        "<root>"
                        (org-plot/gnuplot-script table data-file 4 params)))
               (stats (org--plot/values-stats '(1 2 4 8) 0 10)))
          (list row-labels
                (plist-get stats :min)
                (plist-get stats :max)
                (plist-get stats :nice-range)
                (org--plot/sensible-tick-num table)
                data
                script))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_plot_radar_script_normalized_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t nil t \"reset\\nset term GNUTERM \\n\\nset datafile separator \\\"\\\\t\\\"\\n### spider plot/chart with gnuplot\\n# also known as: radar chart, web chart, star chart, cobweb chart,\\n#                radar plot,  web plot,  star plot,  cobweb plot,  etc. ...\\nset datafile separator ' '\\nset size square\\nunset tics\\nset angles degree\\nset key bmargin center horizontal\\nunset border\\n\\n# Load data and setup\\nload \\\"<tmp-file>\\\"\\n\\n# General settings\\nDataColCount = words($Data[1])-1\\nAxesCount = |$Data|-HeaderLines-1\\nAngleOffset = 90\\nMax = 1\\nd=0.1*Max\\nDirection = -1   # counterclockwise=1, clockwise = -1\\n\\n# Tic settings\\nTicCount = 5\\nTicOffset = 0.1\\nTicValue(axis,i) = real(i)*(word($Settings[axis],3)-word($Settings[axis],2)) \\\\\\n\t  / word($Settings[axis],4)+word($Settings[axis],2)\\nTicLabelPosX(axis,i) = PosX(axis,i/TicCount) + PosY(axis, TicOffset)\\nTicLabelPosY(axis,i) = PosY(axis,i/TicCount) - PosX(axis, TicOffset)\\nTicLen = 0.03\\nTicdX(axis,i) = 0.5*TicLen*cos(alpha(axis)-90)\\nTicdY(axis,i) = 0.5*TicLen*sin(alpha(axis)-90)\\n\\n# Label\\nLabOffset = 0.10\\nLabX(axis) = PosX(axis+1,Max+2*d) + PosY(axis, LabOffset)\\nLabY(axis) = PosY($0+1,Max+2*d)\\n\\n# Functions\\nalpha(axis) = (axis-1)*Direction*360.0/AxesCount+AngleOffset\\nPosX(axis,R) = R*cos(alpha(axis))\\nPosY(axis,R) = R*sin(alpha(axis))\\nScale(axis,value) = real(value-word($Settings[axis],2))/(word($Settings[axis],3)-word($Settings[axis],2))\\n\\n# Spider settings\\nset style arrow 1 dt 1 lw 1.0 @fgal head filled size 0.06,25     # style for axes\\nset style arrow 2 dt 2 lw 0.5 @fgal nohead   # style for weblines\\nset style arrow 3 dt 1 lw 1 @fgal nohead     # style for axis tics\\nset samples AxesCount\\nset isosamples TicCount\\nset urange[1:AxesCount]\\nset vrange[1:TicCount]\\nset style fill transparent solid 0.2\\n\\nset xrange[-Max-4*d:Max+4*d]\\nset yrange[-Max-4*d:Max+4*d]\\nplot \\\\\\n    '+' u (0):(0):(PosX($0,Max+d)):(PosY($0,Max+d)) w vec as 1 not, \\\\\\n    $Data u (LabX($0)): \\\\\\n\t(LabY($0)):1 every ::HeaderLines w labels center enhanced @fgt not, \\\\\\n    for [i=1:DataColCount] $Data u (PosX($0+1,Scale($0+1,column(i+1)))): \\\\\\n\t(PosY($0+1,Scale($0+1,column(i+1)))) every ::HeaderLines w filledcurves lt i title word($Data[1],i+1), \\\\\\n    '++' u (PosX($1,$2/TicCount)):(PosY($1,$2/TicCount)): \\\\\\n\t(PosX($1+1,$2/TicCount)-PosX($1,$2/TicCount)):  \\\\\\n\t(PosY($1+1,$2/TicCount)-PosY($1,$2/TicCount)) w vec as 2 not, \\\\\\n    '++' u (TicLabelPosX(1,$2)):(TicLabelPosY(1,$2)): \\\\\\n\t(sprintf('%g',TicValue(1,$2))) w labels font ',8' @fgat not\\n#    '++' u (PosX($1,$2/TicCount)-TicdX($1,$2/TicCount)): \\\\\\n#        (PosY($1,$2/TicCount)-TicdY($1,$2/TicCount)): \\\\\\n#        (2*TicdX($1,$2/TicCount)):(2*TicdY($1,$2/TicCount)) \\\\\\n#        w vec as 3 not, \\\\\\n### end of code\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-plot)
  (let* ((table '(("Speed" "4" "7")
                  ("Quality" "8" "5")
                  ("Cost" "2" "9")))
         (params '(:plot-type radar
                   :labels ("A" "B")
                   :ymin 0
                   :ymax 10
                   :ticks 5))
         (script (org-plot/gnuplot-script table "radar.dat" 3 params)))
    (list (not (null (string-match-p "spider plot" script)))
          (not (null (string-match-p "Speed" script)))
          (not (null (string-match-p "filledcurves" script)))
          (replace-regexp-in-string
           (concat
            (regexp-quote (file-name-as-directory temporary-file-directory))
            "[^\"\n]+")
           "<tmp-file>"
           script))))"##,
        expect,
    );
}

#[test]
fn org_plot_transpose_preface_time_text_error_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument plistp ((:plot-type . 2d) (:with . lines) (:ind . 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-plot)
  (let* ((root (make-temp-file "org-plot-transpose" t))
         (data-file (expand-file-name "transpose.dat" root))
         (out-file (expand-file-name "plot.svg" root))
         (org-plot/gnuplot-script-preamble
          (lambda (type) (format "# preamble %S" type)))
         (org-plot/gnuplot-term-extra
          (lambda (type) (format "size %d,%d"
                                 (if (eq type '2d) 600 300)
                                 240))))
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          (insert "#+PLOT: title:\"Transposed\" type:2d trans:t ind:1 deps:(2 3)\n")
          (insert "#+PLOT: with:histograms set:\"style fill solid\" timefmt:\"%Y-%m-%d\" file:\"plot.svg\"\n")
          (insert "| Metric | <2026-05-27 Wed> | <2026-05-28 Thu> |\n")
          (insert "|--------+-------------------+-------------------|\n")
          (insert "| Alpha  | 1                 | 2                 |\n")
          (insert "| Beta   | 3                 | 5                 |\n")
          (goto-char (point-min))
          (search-forward "| Metric")
          (let* ((raw-table (org-table-to-lisp))
                 (transposed (let ((tbl raw-table))
                               (setq tbl (apply #'cl-mapcar #'list
                                                (remove 'hline tbl)))
                               (push 'hline (cdr tbl))
                               tbl))
                 (params (copy-sequence org-plot/gnuplot-default-options)))
            (goto-char (point-min))
            (while (re-search-forward "^#\\+PLOT:" nil t)
              (setq params (org-plot/collect-options params)))
            (when (eq (cadr transposed) 'hline)
              (setq params (plist-put params :labels (car transposed)))
              (setq transposed (delq 'hline (cdr transposed))))
            (let* ((num-cols (length (car transposed)))
                   (params (plist-put params :file out-file))
                   (ind-column (mapcar (lambda (row)
                                         (nth (1- (plist-get params :ind))
                                              row))
                                       transposed))
                   (time-detected
                    (cl-every (lambda (el)
                                (string-match org-ts-regexp3 el))
                              ind-column))
                   (text-detected
                    (cl-notevery (lambda (el)
                                   (string-match org-table-number-regexp el))
                                 ind-column)))
              (when time-detected
                (setq params (plist-put params :timeind t)))
              (when text-detected
                (setq params (plist-put params :textind t)))
              (org-plot/gnuplot-to-data transposed data-file params)
              (let ((data (with-temp-buffer
                            (insert-file-contents data-file)
                            (buffer-string)))
                    (preface (org-plot/gnuplot-script
                              transposed data-file num-cols params t))
                    (script (org-plot/gnuplot-script
                             transposed data-file num-cols params))
                    (bad (condition-case err
                             (org-plot/gnuplot-script
                              transposed data-file num-cols
                              '(:plot-type no-such-type))
                           (error (cons (car err) (cdr err))))))
                (list raw-table
                      transposed
                      (plist-get params :labels)
                      (plist-get params :timeind)
                      (plist-get params :textind)
                      data
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>" preface)
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>" script)
                      bad)))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_plot_collect_options_multi_series_line_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Multi Series\" nil lines nil nil nil nil ((\"X\" \"A\" \"B\") hline (\"1\" \"10\" \"100\") (\"2\" \"20\" \"200\") (\"3\" \"30\" \"300\")) \"\\\"X\\\"\t\\\"A\\\"\t\\\"B\\\"\\n1\t10\t100\\n2\t20\t200\\n3\t30\t300\" \"reset\\nset term GNUTERM \\n\\nset title 'Multi Series'\\nset datafile separator \\\"\\\\t\\\"\\nplot '<root>/data.tsv' using 1 with lines title '1',\\\\\\n    '<root>/data.tsv' using 2 with lines title '2',\\\\\\n    '<root>/data.tsv' using 3 with lines title '3'\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-plot)
  (with-temp-buffer
    (org-mode)
    (insert "#+PLOT: title:\"Multi Series\" type:2d with:lines\n")
    (insert "#+PLOT: set:\"grid\" set:\"style data linespoints\"\n")
    (insert "#+PLOT: ind:1 deps:(2 3) transpose:yes\n")
    (insert "| X | A | B |\n")
    (insert "|---+---+---|\n")
    (insert "| 1 | 10 | 100 |\n")
    (insert "| 2 | 20 | 200 |\n")
    (insert "| 3 | 30 | 300 |\n")
    (goto-char (point-min))
    (let* ((opts (org-plot/collect-options '(:include t)))
           (title (plist-get opts :title))
           (type (plist-get opts :type))
           (with (plist-get opts :with))
           (ind (plist-get opts :ind))
           (deps (plist-get opts :deps))
           (set (plist-get opts :set))
           (transpose (plist-get opts :transpose)))
      (goto-char (point-min))
      (search-forward "| X")
      (let* ((table (org-table-to-lisp))
             (num-cols (length (nth 2 table))))
        (let* ((root (make-temp-file "org-plot" t))
               (data-file (expand-file-name "data.tsv" root)))
          (unwind-protect
              (progn
                (org-plot/gnuplot-to-data table data-file opts)
                (let ((tsv (with-temp-buffer
                             (insert-file-contents data-file)
                             (buffer-string))))
                  (let ((script (org-plot/gnuplot-script
                                 table data-file num-cols opts)))
                    (list title type with ind deps set transpose
                          table
                          (replace-regexp-in-string
                           (regexp-quote root) "<root>" tsv)
                          (replace-regexp-in-string
                           (regexp-quote root) "<root>" script)))))
            (delete-directory root t)))))))"##,
        expect,
    );
}

#[test]
fn org_plot_time_series_type_detect_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Time Series\" nil linespoints nil ((\"Date\" \"Value\") hline (\"2026-01-01\" \"10\") (\"2026-02-01\" \"20\") (\"2026-03-01\" \"15\") (\"2026-04-01\" \"25\")) \"\\\"Date\\\"\t\\\"Value\\\"\\n2026-01-01\t10\\n2026-02-01\t20\\n2026-03-01\t15\\n2026-04-01\t25\" \"reset\\nset term GNUTERM \\n\\nset title 'Time Series'\\nset datafile separator \\\"\\\\t\\\"\\nplot '<root>/data.tsv' using 1 with linespoints title '1',\\\\\\n    '<root>/data.tsv' using 2 with linespoints title '2'\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-plot)
  (with-temp-buffer
    (org-mode)
    (insert "#+PLOT: title:\"Time Series\" type:2d with:linespoints\n")
    (insert "#+PLOT: timeind:t\n")
    (insert "| Date | Value |\n")
    (insert "|------+-------|\n")
    (insert "| 2026-01-01 | 10 |\n")
    (insert "| 2026-02-01 | 20 |\n")
    (insert "| 2026-03-01 | 15 |\n")
    (insert "| 2026-04-01 | 25 |\n")
    (goto-char (point-min))
    (let* ((opts (org-plot/collect-options '(:include t)))
           (title (plist-get opts :title))
           (type (plist-get opts :type))
           (with (plist-get opts :with))
           (timeind (plist-get opts :timeind))
           (table (progn
                    (goto-char (point-min))
                    (search-forward "| Date")
                    (org-table-to-lisp)))
           (num-cols (length (nth 2 table))))
      (let* ((root (make-temp-file "org-plot-ts" t))
             (data-file (expand-file-name "data.tsv" root)))
        (unwind-protect
            (progn
              (org-plot/gnuplot-to-data table data-file opts)
              (let ((tsv (with-temp-buffer
                           (insert-file-contents data-file)
                           (buffer-string)))
                    (script (org-plot/gnuplot-script
                             table data-file num-cols opts)))
                (list title type with timeind
                      table
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>" tsv)
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>" script))))
          (delete-directory root t))))))"##,
        expect,
    );
}
