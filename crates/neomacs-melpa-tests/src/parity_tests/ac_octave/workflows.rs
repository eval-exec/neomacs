use expect_test::expect;

use super::ParityBatchCase;

/// The installation instructions verbatim: open an Octave file, call
/// `ac-octave-setup' from `octave-mode-hook', and complete.  Setup has to put
/// `ac-source-octave' in front of `ac-sources' and adding it twice must not
/// duplicate it.  Completing has to start the session with the documented
/// arguments, re-enter the file's directory, ask Octave
/// `completion_matches ("comm");' for exactly the symbol before point, and turn
/// Octave's unsorted, duplicated answer into a sorted deduplicated list carrying
/// the source's own faces, arity symbol and documentation function.
fn ac_octave_setup_completes_a_symbol_from_the_inferior_octave_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_octave_setup_completes_a_symbol_from_the_inferior_octave_process",
        r##"(progn
  (aco-test-write "project/analysis.m" "function r = analysis(x)\n  r = comm\nend\n")
  (aco-test-start-octave
   '(("completion_matches (\"comm\");"
      "common_size\ncommutation_matrix\ncommandwindow\ncommon_size\ncomma")))
  (aco-test-session
   (aco-test-settle)
   (aco-test-visit "project/analysis.m")
   (ac-octave-setup)
   (goto-char (point-min))
   (search-forward "r = comm")
   (let ((candidates (aco-test-complete)))
     (list :major-mode major-mode
           :ac-sources ac-sources
           :source ac-source-octave
           :complete-list ac-octave-complete-list
           :candidates candidates
           :properties (text-properties-at 0 (car ac-candidates))
           :prefix (list ac-prefix ac-point (point))
           :completed (substring-no-properties (ac-complete))
           :buffer (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :modified (buffer-modified-p)
           :octave-log (aco-test-log-lines)))))"##,
        expect![[
            r#"OK (:major-mode octave-mode :ac-sources (ac-source-octave ac-source-words-in-same-mode-buffers) :source ((candidates . ac-octave-candidate) (document . ac-octave-documentation) (candidate-face . ac-octave-candidate-face) (selection-face . ac-octave-selection-face) (init . ac-octave-init) (requires . 0) (cache) (symbol . "f")) :complete-list ("comma" "commandwindow" "common_size" "commutation_matrix") :candidates ("comma" "common_size" "commandwindow" "commutation_matrix") :properties (symbol "f" document ac-octave-documentation popup-face ac-octave-candidate-face selection-face ac-octave-selection-face) :prefix ("comm" 32 36) :completed "comma" :buffer "function r = analysis(x)\n  r = comma\nend\n" :point 37 :modified t :octave-log ("ARGV -i --no-line-editing --no-gui" "CMD PS2" "CMD disp (getenv ('OCTAVE_SRCDIR'))" "CMD more off;" "CMD PS1 ('octave> ');" "CMD disp (pwd ())" "CMD " "CMD cd <sandbox>/project/;" "CMD completion_matches (\"comm\");"))"#
        ]],
    )
}

fn ac_octave_documents_the_selected_candidate_with_octave_help() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_octave_documents_the_selected_candidate_with_octave_help",
        r##"(progn
  (aco-test-write "project/analysis.m" "function r = analysis(x)\n  r = comm\nend\n")
  (aco-test-start-octave
   '(("completion_matches (\"comm\");"
      "common_size\ncommutation_matrix\ncommandwindow\ncommon_size\ncomma")
     ("help common_size;"
      "'common_size' is a function from the file /usr/share/octave/9.2.0/m/general/common_size.m\n\n -- [ERR, Y1, ...] = common_size (X1, X2, ...)\n     Determine if all input arguments are either scalar or of\n     common size.  Return ERR = 0 on success.\n\n     See also: size, size_equal, numel, ndims.")))
  (aco-test-session
   (aco-test-settle)
   (aco-test-visit "project/analysis.m")
   (goto-char (point-min))
   (search-forward "r = comm")
   (let ((candidates (aco-test-complete)))
     (list :candidates candidates
           :menu-live (ac-menu-live-p)
           :selected-first (substring-no-properties (ac-selected-candidate))
           :selected-second (progn (ac-next)
                                   (substring-no-properties (ac-selected-candidate)))
           :documentation (popup-item-documentation (ac-selected-candidate))
           :digested inferior-octave-output-list
           :completed (substring-no-properties (ac-complete))
           :buffer (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :octave-log (aco-test-log-lines)))))"##,
        expect![[
            r#"OK (:candidates ("comma" "common_size" "commandwindow" "commutation_matrix") :menu-live t :selected-first "comma" :selected-second "common_size" :documentation "'common_size' is a function from the file /usr/share/octave/9.2.0/m/general/common_size.m\n\n -- [ERR, Y1, ...] = common_size (X1, X2, ...)\n     Determine if all input arguments are either scalar or of\n     common size.  Return ERR = 0 on success.\n\n     See also: size, size_equal, numel, ndims." :digested ("'common_size' is a function from the file /usr/share/octave/9.2.0/m/general/common_size.m" "" " -- [ERR, Y1, ...] = common_size (X1, X2, ...)" "     Determine if all input arguments are either scalar or of" "     common size.  Return ERR = 0 on success." "" "     See also: size, size_equal, numel, ndims.") :completed "common_size" :buffer "function r = analysis(x)\n  r = common_size\nend\n" :point 43 :octave-log ("ARGV -i --no-line-editing --no-gui" "CMD PS2" "CMD disp (getenv ('OCTAVE_SRCDIR'))" "CMD more off;" "CMD PS1 ('octave> ');" "CMD disp (pwd ())" "CMD " "CMD cd <sandbox>/project/;" "CMD completion_matches (\"comm\");" "CMD help common_size;"))"#
        ]],
    )
    .fresh_process()
}

fn ac_octave_re_enters_each_buffers_directory_before_completing() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_octave_re_enters_each_buffers_directory_before_completing",
        r##"(progn
  (aco-test-write "project/stats/summary.m" "function s = summary(x)\n  s = mean\nend\n")
  (aco-test-write "project/plots/figure.m" "function figure_helper()\n  plot\nend\n")
  (aco-test-start-octave
   '(("completion_matches (\"mean\");" "mean\nmeansq\nmean_helper")
     ("completion_matches (\"plot\");" "plot\nplot3\nplotmatrix\nplot_helper")))
  (aco-test-session
   (aco-test-settle)
   (cl-flet ((complete-in (file word)
               (aco-test-visit file)
               (goto-char (point-min))
               (search-forward word)
               (let ((candidates (aco-test-complete)))
                 (list (buffer-name)
                       default-directory
                       candidates
                       ac-octave-complete-list))))
     (let ((stats (complete-in "project/stats/summary.m" "s = mean"))
           (plots (complete-in "project/plots/figure.m" "  plot")))
       (list :stats stats
             :plots plots
             :one-process (list (process-name inferior-octave-process)
                                (process-status inferior-octave-process))
             :octave-log (aco-test-log-lines))))))"##,
        expect![[
            r#"OK (:stats ("summary.m" "[ORACLE-SANDBOX]/project/stats/" ("mean" "meansq" "mean_helper") ("mean" "mean_helper" "meansq")) :plots ("figure.m" "[ORACLE-SANDBOX]/project/plots/" ("plot3" "plot" "plotmatrix" "plot_helper") ("plot" "plot3" "plot_helper" "plotmatrix")) :one-process ("Inferior Octave" run) :octave-log ("ARGV -i --no-line-editing --no-gui" "CMD PS2" "CMD disp (getenv ('OCTAVE_SRCDIR'))" "CMD more off;" "CMD PS1 ('octave> ');" "CMD disp (pwd ())" "CMD " "CMD cd <sandbox>/project/stats/;" "CMD completion_matches (\"mean\");" "CMD cd <sandbox>/project/plots/;" "CMD completion_matches (\"plot\");"))"#
        ]],
    )
    .fresh_process()
}

fn ac_octave_offers_nothing_when_octave_has_no_matching_symbol() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_octave_offers_nothing_when_octave_has_no_matching_symbol",
        r##"(progn
  (aco-test-write "project/analysis.m" "function r = analysis(x)\n  r = zzz\nend\n")
  (aco-test-start-octave
   '(("completion_matches (\"zzz\");" "")
     ("help zzz;" "")))
  (aco-test-session
   (aco-test-settle)
   (aco-test-visit "project/analysis.m")
   (goto-char (point-min))
   (search-forward "r = zzz")
   (let ((candidates (aco-test-complete)))
     (list :candidates candidates
           :complete-list ac-octave-complete-list
           :digested inferior-octave-output-list
           :selected (ac-selected-candidate)
           :completed (ac-complete)
           :documentation (ac-octave-documentation "zzz")
           :buffer (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :modified (buffer-modified-p)
           :octave-log (aco-test-log-lines)))))"##,
        expect![[
            r#"OK (:candidates nil :complete-list nil :digested nil :selected nil :completed nil :documentation "" :buffer "function r = analysis(x)\n  r = zzz\nend\n" :point 35 :modified nil :octave-log ("ARGV -i --no-line-editing --no-gui" "CMD PS2" "CMD disp (getenv ('OCTAVE_SRCDIR'))" "CMD more off;" "CMD PS1 ('octave> ');" "CMD disp (pwd ())" "CMD " "CMD cd <sandbox>/project/;" "CMD completion_matches (\"zzz\");" "CMD help zzz;"))"#
        ]],
    )
    .fresh_process()
}

fn ac_octave_reports_a_dead_session_while_documentation_stays_silent() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_octave_reports_a_dead_session_while_documentation_stays_silent",
        r##"(progn
  (aco-test-write "project/analysis.m" "function r = analysis(x)\n  r = comm\nend\n")
  (aco-test-start-octave
   '(("completion_matches (\"comm\");" "commandwindow\ncomma\ncommon_size")))
  (aco-test-session
   (aco-test-settle)
   (aco-test-visit "project/analysis.m")
   (goto-char (point-min))
   (search-forward "r = comm")
   (let ((candidates (aco-test-complete)))
     (delete-process inferior-octave-process)
     (list :candidates candidates
           :live (inferior-octave-process-live-p)
           :complete (condition-case error
                         (ac-octave-do-complete)
                       (error (list :signalled error)))
           :documentation (ac-octave-documentation "comma")
           :cached-update (progn (ac-update t)
                                 (mapcar #'substring-no-properties ac-candidates))
           :buffer (progn (ac-stop)
                          (buffer-substring-no-properties (point-min) (point-max)))
           :point (point)
           :octave-log (aco-test-log-lines)))))"##,
        expect![[
            r#"OK (:candidates ("comma" "common_size" "commandwindow") :live nil :complete (:signalled (error #("No inferior octave process running. Type M-x run-octave" 41 55 (font-lock-face help-key-binding face help-key-binding)))) :documentation nil :cached-update ("comma" "common_size" "commandwindow") :buffer "function r = analysis(x)\n  r = comm\nend\n" :point 36 :octave-log ("ARGV -i --no-line-editing --no-gui" "CMD PS2" "CMD disp (getenv ('OCTAVE_SRCDIR'))" "CMD more off;" "CMD PS1 ('octave> ');" "CMD disp (pwd ())" "CMD " "CMD cd <sandbox>/project/;" "CMD completion_matches (\"comm\");"))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ac_octave_setup_completes_a_symbol_from_the_inferior_octave_process(),
        ac_octave_documents_the_selected_candidate_with_octave_help(),
        ac_octave_re_enters_each_buffers_directory_before_completing(),
        ac_octave_offers_nothing_when_octave_has_no_matching_symbol(),
        ac_octave_reports_a_dead_session_while_documentation_stays_silent(),
    ]
}
