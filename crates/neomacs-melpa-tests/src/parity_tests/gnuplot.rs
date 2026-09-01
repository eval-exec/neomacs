use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GNUPLOT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'gnuplot)
(require 'gnuplot-context)

(defun neomacs-gnuplot-test-face (text occurrence)
  "Return TEXT's face at OCCURRENCE in the current buffer."
  (save-excursion
    (goto-char (point-min))
    (dotimes (_ occurrence)
      (search-forward text))
    (get-text-property (match-beginning 0) 'face)))

(defun neomacs-gnuplot-test-syntax-at (text occurrence)
  "Return stable syntax state inside TEXT at OCCURRENCE."
  (save-excursion
    (goto-char (point-min))
    (dotimes (_ occurrence)
      (search-forward text))
    (let ((state (syntax-ppss (1+ (match-beginning 0)))))
      (list :string (and (nth 3 state) t)
            :comment (and (nth 4 state) t)))))

(defun neomacs-gnuplot-test-command-at (text)
  "Return the complete Gnuplot command containing TEXT."
  (save-excursion
    (goto-char (point-min))
    (search-forward text)
    (buffer-substring-no-properties
     (gnuplot--point-at-beginning-of-command)
     (gnuplot--point-at-end-of-command))))

(defun neomacs-gnuplot-test-completion (input)
  "Return contextual completion data after INPUT."
  (with-temp-buffer
    (gnuplot-mode)
    (insert input)
    (goto-char (point-max))
    (let* ((data (gnuplot-context-completion-at-point))
           (begin (nth 0 data))
           (end (nth 1 data))
           (table (nth 2 data))
           (prefix (buffer-substring-no-properties begin end))
           (candidates (sort (delete-dups (all-completions prefix table))
                             #'string<)))
      (list :range (list begin end)
            :prefix prefix
            :candidates candidates))))

(defun neomacs-gnuplot-test-context (input marker)
  "Return ElDoc and Info context at MARKER in INPUT."
  (with-temp-buffer
    (gnuplot-mode)
    (insert input)
    (goto-char (point-min))
    (search-forward marker)
    (let ((eldoc (gnuplot-context-eldoc-function)))
      (list :eldoc eldoc :info gnuplot-context--info-at-point))))

(defun neomacs-gnuplot-test-parse-command (command)
  "Parse COMMAND and return its unmatched token identifiers."
  (with-temp-buffer
    (gnuplot-mode)
    (insert command)
    (goto-char (point-max))
    (let* ((tokens (gnuplot-context--tokenize))
           (result
            (gnuplot-context--match-pattern
             gnuplot-context--compiled-grammar tokens nil)))
      (mapcar
       (lambda (remaining)
         (mapcar #'gnuplot-context--token-id remaining))
       result))))
"###;

fn package_and_mode_contract_configure_a_real_gnuplot_script_buffer() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'gnuplot package-alist))))
  (with-temp-buffer
    (gnuplot-mode)
    (list
     :package
     (list :name (package-desc-name descriptor)
           :version (package-version-join (package-desc-version descriptor))
           :requirements (package-desc-reqs descriptor)
           :features
           (mapcar (lambda (feature) (and (featurep feature) t))
                   '(gnuplot gnuplot-context gnuplot-eldoc)))
     :mode
     (list major-mode mode-name comment-start comment-end comment-column
           (eq indent-line-function #'gnuplot-indent-line)
           (eq syntax-propertize-function #'gnuplot--syntax-propertize)
           (and gnuplot-context-sensitive-mode t)
           (and (memq #'gnuplot-context-completion-at-point
                      completion-at-point-functions)
                t)
           (and (memq #'gnuplot-context-eldoc-function
                      eldoc-documentation-functions)
                t))
     :bindings
     (mapcar (lambda (key) (lookup-key gnuplot-mode-map (kbd key)))
             '("C-c C-l" "C-c C-v" "C-c C-r" "C-c C-b"
               "C-c C-n" "M-TAB" "}"))
     :recognition
     (list (cdr (assoc "\\.gp\\'" auto-mode-alist))
           (cdr (assoc "gnuplot" interpreter-mode-alist))))))
"###;
    let expected = expect![[
        r##"OK (:package (:name gnuplot :version "20260623.1111" :requirements ((emacs (29 1)) (compat (31))) :features (t t t)) :mode (gnuplot-mode "Gnuplot" "# " "" 32 t t t t t) :bindings (gnuplot-send-line-to-gnuplot gnuplot-send-line-and-forward gnuplot-send-region-to-gnuplot gnuplot-send-buffer-to-gnuplot gnuplot-negate-option completion-at-point gnuplot-electric-insert) :recognition (gnuplot-mode gnuplot-mode))"##
    ]];
    ParityBatchCase::value(
        "package_and_mode_contract_configure_a_real_gnuplot_script_buffer",
        elisp_form,
        expected,
    )
}

fn production_script_syntax_distinguishes_quoted_hashes_semicolons_and_continued_comments()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gnuplot-mode)
  (insert "set title \"Revenue #1; Q4\"\n"
          "set output 'team''s-report.png'\n"
          "datafile = \"sales\\\"2026.csv\" # deployment note \\\n"
          " continued for operators\n"
          "plot datafile using 1:2 with linespoints \\\n"
          " title \"Gross; margin\"\n")
  (syntax-propertize (point-max))
  (font-lock-ensure)
  (list
   :syntax
   (list
    :title (neomacs-gnuplot-test-syntax-at "Revenue" 1)
    :quoted-hash (neomacs-gnuplot-test-syntax-at "#1" 1)
    :quoted-semicolon (neomacs-gnuplot-test-syntax-at "; Q4" 1)
    :single-quote (neomacs-gnuplot-test-syntax-at "team" 1)
    :comment (neomacs-gnuplot-test-syntax-at "deployment" 1)
    :continued-comment (neomacs-gnuplot-test-syntax-at "operators" 1)
    :plot-title (neomacs-gnuplot-test-syntax-at "Gross" 1))
   :faces
   (mapcar (lambda (probe)
             (neomacs-gnuplot-test-face (car probe) (cdr probe)))
           '(("set" . 1) ("title" . 1) ("plot" . 1)
             ("using" . 1) ("linespoints" . 1) ("datafile" . 1)))
   :comments
   (list comment-start comment-end comment-start-skip)))
"###;
    let expected = expect![[
        r##"OK (:syntax (:title (:string t :comment nil) :quoted-hash (:string t :comment nil) :quoted-semicolon (:string t :comment nil) :single-quote (:string t :comment nil) :comment (:string nil :comment t) :continued-comment (:string nil :comment t) :plot-title (:string t :comment nil)) :faces (font-lock-constant-face font-lock-type-face font-lock-keyword-face font-lock-type-face font-lock-function-name-face font-lock-variable-name-face) :comments ("# " "" "#[ \11]*"))"##
    ]];
    ParityBatchCase::value(
        "production_script_syntax_distinguishes_quoted_hashes_semicolons_and_continued_comments",
        elisp_form,
        expected,
    )
}

fn nested_blocks_and_continued_plot_commands_indent_as_an_editable_script() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gnuplot-mode)
  (setq-local gnuplot-basic-offset 4)
  (insert "do for [run=1:3] {\n"
          "plot run*x, \\\n"
          "cos(x) title sprintf('run %d', run)\n"
          "if (run > 1) {\n"
          "set grid\n"
          "}\n"
          "}\n")
  (indent-region (point-min) (point-max))
  (let ((first-pass
         (buffer-substring-no-properties (point-min) (point-max)))
        (columns
         (save-excursion
           (goto-char (point-min))
           (let (result)
             (while (not (eobp))
               (push (current-indentation) result)
               (forward-line 1))
             (nreverse result)))))
    (goto-char (point-min))
    (search-forward "set grid")
    (beginning-of-line)
    (delete-horizontal-space)
    (gnuplot-indent-line)
    (list :text first-pass
          :columns columns
          :reindent-column (current-indentation)
          :point (point))))
"###;
    let expected = expect![[
        r#"OK (:text "do for [run=1:3] {\n    plot run*x, \\\n\11 cos(x) title sprintf('run %d', run)\n    if (run > 1) {\n\11set grid\n    }\n}\n" :columns (0 4 9 4 8 4 0) :reindent-column 8 :point 96)"#
    ]];
    ParityBatchCase::value(
        "nested_blocks_and_continued_plot_commands_indent_as_an_editable_script",
        elisp_form,
        expected,
    )
}

fn command_navigation_honors_semicolons_inside_strings_comments_and_continuations()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gnuplot-mode)
  (insert "set title \"Q1; Q2\"; set grid; plot sin(x), \\\n"
          " cos(x) title 'a;b' # comment; not a command\n"
          "unset key\n")
  (syntax-propertize (point-max))
  (list
   :title (neomacs-gnuplot-test-command-at "Q1")
   :grid (neomacs-gnuplot-test-command-at "grid")
   :plot (neomacs-gnuplot-test-command-at "cos(x)")
   :comment-semicolon (neomacs-gnuplot-test-command-at "not a command")
   :unset (neomacs-gnuplot-test-command-at "unset key")
   :continuation
   (save-excursion
     (goto-char (point-min))
     (search-forward "cos(x)")
     (list (gnuplot--point-at-beginning-of-continuation)
           (gnuplot--point-at-end-of-continuation)))))
"###;
    let expected = expect![[
        r#"OK (:title "set title \"Q1; Q2\"" :grid "set grid" :plot "plot sin(x), \\\n cos(x) title 'a;b' # comment; not a command" :comment-semicolon "plot sin(x), \\\n cos(x) title 'a;b' # comment; not a command" :unset "unset key" :continuation (1 90))"#
    ]];
    ParityBatchCase::value(
        "command_navigation_honors_semicolons_inside_strings_comments_and_continuations",
        elisp_form,
        expected,
    )
}

fn option_toggle_edits_only_the_selected_negatable_command() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gnuplot-mode)
  (insert "set grid; set xrange [0:10]; unset key\n"
          "set title 'Quarterly dashboard'\n")
  (goto-char (point-min))
  (search-forward "grid")
  (gnuplot-negate-option)
  (let ((grid-off
         (buffer-substring-no-properties (point-min) (point-max))))
    (gnuplot-negate-option)
    (let ((grid-on
           (buffer-substring-no-properties (point-min) (point-max))))
      (search-forward "xrange")
      (let (notice)
        (cl-letf (((symbol-function 'message)
                   (lambda (format-string &rest arguments)
                     (setq notice (apply #'format format-string arguments)))))
          (gnuplot-negate-option))
        (search-forward "key")
        (gnuplot-negate-option)
        (search-forward "title")
        (gnuplot-negate-option)
        (list :grid-off grid-off
              :grid-on grid-on
              :final (buffer-substring-no-properties (point-min) (point-max))
              :non-negatable-message notice
              :point (point))))))
"###;
    let expected = expect![[
        r#"OK (:grid-off "unset grid; set xrange [0:10]; unset key\nset title 'Quarterly dashboard'\n" :grid-on "set grid; set xrange [0:10]; unset key\nset title 'Quarterly dashboard'\n" :final "set grid; set xrange [0:10]; set key\nunset title 'Quarterly dashboard'\n" :non-negatable-message "There is not a negatable set option on this line" :point 49)"#
    ]];
    ParityBatchCase::value(
        "option_toggle_edits_only_the_selected_negatable_command",
        elisp_form,
        expected,
    )
}

fn contextual_completion_proposes_only_valid_plot_styles_and_set_options() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :plot-style
 (neomacs-gnuplot-test-completion
  "plot 'metrics.csv' using 1:2 with ")
 :set-x
 (neomacs-gnuplot-test-completion "set x")
 :plot-modifier
 (neomacs-gnuplot-test-completion
  "plot for [region=1:4] 'metrics.csv' using 1:region "))
"###;
    let expected = expect![[
        r#"OK (:plot-style (:range (35 35) :prefix "" :candidates ("boxerrorbars" "boxes" "boxxyerrorbars" "candlesticks" "circles" "dots" "errorbars" "errorlines" "filledcurves" "financebars" "fsteps" "histeps" "histograms" "image" "impulses" "labels" "lines" "linespoints" "pm3d" "points" "rgbalpha" "rgbimage" "steps" "vectors" "xerrorbars" "xerrorlines" "xyerrorbars" "xyerrorlines" "yerrorbars" "yerrorlines")) :set-x (:range (5 6) :prefix "x" :candidates ("x2data" "x2dtics" "x2label" "x2mtics" "x2range" "x2tics" "x2zeroaxis" "xdata" "xdtics" "xlabel" "xmtics" "xrange" "xtics" "xyplane" "xzeroaxis")) :plot-modifier (:range (52 52) :prefix "" :candidates ("axes" "binary" "eq" "every" "fillstyle" "index" "linecolor" "linestyle" "linetype" "linewidth" "matrix" "ne" "noautoscale" "nocontours" "nohidden3d" "nonuniform" "nosurface" "notitle" "pointinterval" "pointsize" "pointtype" "smooth" "thru" "title" "using" "volatile" "with")))"#
    ]];
    ParityBatchCase::value(
        "contextual_completion_proposes_only_valid_plot_styles_and_set_options",
        elisp_form,
        expected,
    )
}

fn context_parser_accepts_a_dashboard_script_and_rejects_trailing_garbage() -> ParityBatchCase {
    let elisp_form = r###"
(mapcar
 (lambda (command)
   (list command (neomacs-gnuplot-test-parse-command command)))
 '("set datafile separator ','"
   "set key top right outside rmargin"
   "set xlabel 'Release date'"
   "set ylabel 'Latency (ms)'"
   "plot for [col=2:4] 'metrics.csv' using 1:col with linespoints title columnheader(col)"
   "plot 'metrics.csv' using 1:2 with lines trailing-garbage"))
"###;
    let expected = expect![[
        r#"OK (("set datafile separator ','" (nil)) ("set key top right outside rmargin" (nil)) ("set xlabel 'Release date'" (nil)) ("set ylabel 'Latency (ms)'" (nil)) ("plot for [col=2:4] 'metrics.csv' using 1:col with linespoints title columnheader(col)" (nil)) ("plot 'metrics.csv' using 1:2 with lines trailing-garbage" (("trailing" "-" "garbage"))))"#
    ]];
    ParityBatchCase::value(
        "context_parser_accepts_a_dashboard_script_and_rejects_trailing_garbage",
        elisp_form,
        expected,
    )
}

fn eldoc_and_info_follow_the_plot_clause_at_the_cursor() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :using-lines
 (neomacs-gnuplot-test-context
  "plot 'metrics.csv' with lines using 1:2 title 'latency'" "using 1:2")
 :using-errorbars
 (neomacs-gnuplot-test-context
  "plot 'metrics.csv' with yerrorbars using 1:2:3" "using 1:2:3")
 :xlabel
 (neomacs-gnuplot-test-context "set xlabel 'Release date'" "xlabel")
 :terminal
 (neomacs-gnuplot-test-context "set terminal svg size 900,500" "terminal"))
"###;
    let expected = expect![[
        r#"OK (:using-lines (:eldoc "using y | x:y {'format'}" :info "using") :using-errorbars (:eldoc "using x:y:ydelta | x:y:ylow:yhigh {'format'}" :info "using") :xlabel (:eldoc "set xlabel {\"<label>\"} {offset <offset>} {font \"<font>{,<size>}\"} [more ...]" :info "xlabel") :terminal (:eldoc "set terminal {<terminal-type> | push | pop} [more ...]" :info "terminal"))"#
    ]];
    ParityBatchCase::value(
        "eldoc_and_info_follow_the_plot_clause_at_the_cursor",
        elisp_form,
        expected,
    )
}

fn line_region_and_buffer_dispatch_preserve_complete_commands_and_workflow_position()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gnuplot-mode)
  (insert "set terminal svg\n"
          "\n"
          "# Revenue dashboard\n"
          "plot 'sales.csv' using 1:2 with lines, \\\n"
          "     '' using 1:3 with points\n"
          "set title 'Quarterly revenue'\n")
  (let (sent)
    (cl-letf (((symbol-function 'gnuplot-send-string-to-gnuplot)
               (lambda (string kind)
                 (push (list kind string) sent))))
      (goto-char (point-min))
      (gnuplot-send-line-and-forward 2)
      (let ((after-forward
             (list :point (point)
                   :line (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position)))))
        (goto-char (point-min))
        (search-forward "plot")
        (gnuplot-send-line-to-gnuplot)
        (goto-char (point-min))
        (search-forward "set title")
        (gnuplot-send-region-to-gnuplot
         (line-beginning-position) (line-end-position))
        (gnuplot-send-buffer-to-gnuplot)
        (list :after-forward after-forward
              :sent (nreverse sent)
              :buffer
              (buffer-substring-no-properties (point-min) (point-max)))))))
"###;
    let expected = expect![[
        r#"OK (:after-forward (:point 110 :line "set title 'Quarterly revenue'") :sent ((line "set terminal svg\n") (line "plot 'sales.csv' using 1:2 with lines, \\\n     '' using 1:3 with points\n") (line "plot 'sales.csv' using 1:2 with lines, \\\n     '' using 1:3 with points\n") (region "set title 'Quarterly revenue'\n") (buffer "set terminal svg\n\n# Revenue dashboard\nplot 'sales.csv' using 1:2 with lines, \\\n     '' using 1:3 with points\nset title 'Quarterly revenue'\n")) :buffer "set terminal svg\n\n# Revenue dashboard\nplot 'sales.csv' using 1:2 with lines, \\\n     '' using 1:3 with points\nset title 'Quarterly revenue'\n")"#
    ]];
    ParityBatchCase::value(
        "line_region_and_buffer_dispatch_preserve_complete_commands_and_workflow_position",
        elisp_form,
        expected,
    )
}

#[test]
fn gnuplot_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(GNUPLOT_MELPA_PIN, "gnuplot.el")
            .expect("prepare revision-pinned Gnuplot below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "gnuplot-package-batch",
        "Gnuplot",
        &[
            package_and_mode_contract_configure_a_real_gnuplot_script_buffer(),
            production_script_syntax_distinguishes_quoted_hashes_semicolons_and_continued_comments(
            ),
            nested_blocks_and_continued_plot_commands_indent_as_an_editable_script(),
            command_navigation_honors_semicolons_inside_strings_comments_and_continuations(),
            option_toggle_edits_only_the_selected_negatable_command(),
            contextual_completion_proposes_only_valid_plot_styles_and_set_options(),
            context_parser_accepts_a_dashboard_script_and_rejects_trailing_garbage(),
            eldoc_and_info_follow_the_plot_clause_at_the_cursor(),
            line_region_and_buffer_dispatch_preserve_complete_commands_and_workflow_position(),
        ],
    );
}
