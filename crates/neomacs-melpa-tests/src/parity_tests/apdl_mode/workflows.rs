use expect_test::expect;

use super::ParityBatchCase;

fn apdl_mode_authors_and_navigates_a_structural_analysis_model() -> ParityBatchCase {
    ParityBatchCase::value(
        "apdl_mode_authors_and_navigates_a_structural_analysis_model",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apdl-authoring-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (model (expand-file-name "models/cantilever.mac" root))
       (default-directory root)
       model-buffer
       selected-block
       variable-report
       variable-buttons
       jump-state
       result)
  (unwind-protect
      (progn
        (neomacs-apdl-test-cleanup root)
        (make-directory (file-name-directory model) t)
        (with-temp-file model
          (insert
           "/title,Cantilever production model\n"
           "/prep7\n"
           "youngs=210000 ! MPa\n"
           "poisson_ratio=0.3 ! dimensionless\n"
           "density=7.85e-9 ! tonne/mm3\n"
           "mp,ex,1,youngs\n"
           "mp,prxy,1,poisson_ratio\n"
           "mp,dens,1,density\n"
           "et,1,solid186\n"
           "*get,node_count,node,0,count\n"
           "*if,node_count,gt,0,then\n"
           "f,all,fy,-1000\n"
           "*do,load_step,1,3\n"
           "time,load_step\n"
           "solve\n"
           "*enddo\n"
           "*else\n"
           "/com,No nodes were generated\n"))
        (setq model-buffer (find-file-noselect model))
        (switch-to-buffer model-buffer)
        (setq-local indent-tabs-mode nil)
        (goto-char (point-min))
        (search-forward "density=7.85e-9")
        (end-of-line)
        (insert "\nload_scale=1.25 ! production multiplier")
        (goto-char (point-min))
        (search-forward "youngs=")
        (beginning-of-line)
        (let ((start (point)))
          (forward-line 4)
          (push-mark (point) nil t)
          (goto-char start)
          (setq mark-active t)
          (apdl-align (region-beginning) (region-end))
          (deactivate-mark))
        (goto-char (point-max))
        (apdl-close-block)
        (end-of-line)
        (insert "\nfinish\n")
        (indent-region (point-min) (point-max))
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "*if,node_count")
        (beginning-of-line)
        (apdl-mark-block)
        (setq selected-block
              (buffer-substring-no-properties
               (region-beginning)
               (region-end)))
        (deactivate-mark)
        (save-buffer)
        (apdl-display-variables nil)
        (with-current-buffer "*APDL-variables*"
          (setq variable-report
                (buffer-substring-no-properties
                 (point-min)
                 (point-max)))
          (let ((button (next-button (point-min) t)))
            (while button
              (push
               (list
                (substring-no-properties (button-label button))
                (marker-position (button-get button 'action)))
               variable-buttons)
              (setq button
                    (next-button (button-end button) t))))
          (goto-char (point-min))
          (search-forward "node_count")
          (beginning-of-line)
          (re-search-forward "[[:digit:]]+")
          (push-button (match-beginning 0)))
        (setq jump-state
              (list
               (buffer-name)
               (file-relative-name buffer-file-name root)
               (line-number-at-pos)
               (current-column)
               (buffer-substring-no-properties
                (line-beginning-position)
                (line-end-position))))
        (switch-to-buffer model-buffer)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "solve")
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :point
               (list
                (line-number-at-pos)
                (current-column)
                (current-indentation))
               :variables apdl-user-variables
               :selected-block selected-block
               :variable-report variable-report
               :variable-buttons (nreverse variable-buttons)
               :jump jump-state
               :faces
               (mapcar
                (lambda (token)
                  (list token (neomacs-apdl-test-face-at token)))
                '("/title"
                  "Cantilever production model"
                  "/prep7"
                  "youngs"
                  "load_scale"
                  "solid186"
                  "*if"
                  "solve"
                  "No nodes were generated"))
               :lines (neomacs-apdl-test-lines)
               :modified (buffer-modified-p)
               :disk (neomacs-apdl-test-file-string model))))
    (neomacs-apdl-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:file "models/cantilever.mac" :mode apdl-mode :point (16 9 4) :variables (("youngs" 3) ("poisson_ratio" 4) ("density" 5) ("load_scale" 6) ("node_count" 11) ("load_step" 14)) :selected-block "*if,node_count,gt,0,then\n  f,all,fy,-1000\n  *do,load_step,1,3\n    time,load_step\n    solve\n  *enddo\n*else\n  /com,No nodes were generated\n*endif\n" :variable-report "-*- APDL variables of cantilever.mac click with mouse-2 -*-\n Line | Definition\n    3 | youngs        = 210000       ! MPa\n    4 | poisson_ratio =      0.3     ! dimensionless\n    5 | density       =      7.85e-9 ! tonne/mm3\n    6 | load_scale    =      1.25    ! production multiplier\n   11 | *get,node_count,node,0,count\n   14 | *do,load_step,1,3\n" :variable-buttons (("3" 43) ("4" 78) ("5" 123) ("6" 164) ("11" 288) ("14" 359)) :jump ("cantilever.mac" "models/cantilever.mac" 11 0 "*get,node_count,node,0,count") :faces (("/title" font-lock-keyword-face) ("Cantilever production model" font-lock-doc-face) ("/prep7" font-lock-keyword-face) ("youngs" font-lock-variable-name-face) ("load_scale" font-lock-variable-name-face) ("solid186" font-lock-builtin-face) ("*if" font-lock-keyword-face) ("solve" font-lock-keyword-face) ("No nodes were generated" font-lock-doc-face)) :lines ((1 0 "/title,Cantilever production model") (2 0 "/prep7") (3 0 "youngs        = 210000       ! MPa") (4 0 "poisson_ratio =      0.3     ! dimensionless") (5 0 "density       =      7.85e-9 ! tonne/mm3") (6 0 "load_scale    =      1.25    ! production multiplier") (7 0 "mp,ex,1,youngs") (8 0 "mp,prxy,1,poisson_ratio") (9 0 "mp,dens,1,density") (10 0 "et,1,solid186") (11 0 "*get,node_count,node,0,count") (12 0 "*if,node_count,gt,0,then") (13 2 "  f,all,fy,-1000") (14 2 "  *do,load_step,1,3") (15 4 "    time,load_step") (16 4 "    solve") (17 2 "  *enddo") (18 0 "*else") (19 2 "  /com,No nodes were generated") (20 0 "*endif") (21 0 "finish")) :modified nil :disk "/title,Cantilever production model\n/prep7\nyoungs        = 210000       ! MPa\npoisson_ratio =      0.3     ! dimensionless\ndensity       =      7.85e-9 ! tonne/mm3\nload_scale    =      1.25    ! production multiplier\nmp,ex,1,youngs\nmp,prxy,1,poisson_ratio\nmp,dens,1,density\net,1,solid186\n*get,node_count,node,0,count\n*if,node_count,gt,0,then\n  f,all,fy,-1000\n  *do,load_step,1,3\n    time,load_step\n    solve\n  *enddo\n*else\n  /com,No nodes were generated\n*endif\nfinish\n")"#
        ]],
    )
}

fn apdl_mode_inspects_workbench_mesh_data_and_control_flow() -> ParityBatchCase {
    ParityBatchCase::value(
        "apdl_mode_inspects_workbench_mesh_data_and_control_flow",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apdl-workbench-inspection"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (input (expand-file-name "workbench/ds.dat" root))
       (default-directory root)
       buffer
       hidden-number-blocks
       unhidden-state
       number-boundaries
       outline-state
       selected-branch
       result)
  (unwind-protect
      (progn
        (neomacs-apdl-test-cleanup root)
        (make-directory (file-name-directory input) t)
        (with-temp-file input
          (insert
           "!@ === Workbench mesh ===\n"
           "nblock,3,solid\n"
           "(1i9,3e20.9e3)\n"
           "1,0.0,0.0,0.0\n"
           "2,1.0,0.0,0.0\n"
           "3,1.0,1.0,0.0\n"
           "4,0.0,1.0,0.0\n"
           "5,0.0,0.0,1.0\n"
           "6,1.0,0.0,1.0\n"
           "7,1.0,1.0,1.0\n"
           "8,0.0,1.0,1.0\n"
           "-1\n"
           "!@ === Load selection ===\n"
           "*if,load_case,eq,1,then\n"
           "  nsel,s,loc,x,0\n"
           "  f,all,fy,-1000\n"
           "*else\n"
           "  /com,Load case disabled\n"
           "*endif\n"))
        (setq buffer (find-file-noselect input))
        (switch-to-buffer buffer)
        (setq hidden-number-blocks
              (mapcar
               (lambda (overlay)
                 (list
                  (line-number-at-pos (overlay-start overlay))
                  (line-number-at-pos (overlay-end overlay))
                  (overlay-get overlay 'invisible)
                  (overlay-get overlay 'intangible)
                  (substring-no-properties
                   (overlay-get overlay 'before-string))
                  (substring-no-properties
                   (overlay-get overlay 'after-string))))
               apdl-hide-region-overlays))
        (apdl-unhide-number-blocks)
        (setq unhidden-state
              (list
               apdl-hide-region-overlays
               (seq-count
                (lambda (position)
                  (invisible-p position))
                (number-sequence
                 (point-min)
                 (1- (point-max))))))
        (goto-char (point-min))
        (forward-line 7)
        (end-of-line)
        (let ((start
               (progn
                 (apdl-number-block-start)
                 (list
                  (line-number-at-pos)
                  (current-column)
                  (buffer-substring-no-properties
                   (line-beginning-position)
                   (line-end-position))))))
          (setq number-boundaries
                (list
                 start
                 (progn
                   (apdl-number-block-end)
                   (list
                    (line-number-at-pos)
                    (current-column)
                    (buffer-substring-no-properties
                     (line-beginning-position)
                     (line-end-position)))))))
        (goto-char (point-min))
        (search-forward "!@ === Load selection ===")
        (beginning-of-line)
        (outline-hide-subtree)
        (forward-line 1)
        (setq outline-state
              (list
               (line-number-at-pos)
               (invisible-p (point))
               (buffer-substring-no-properties
                (line-beginning-position)
                (line-end-position))))
        (outline-show-all)
        (goto-char (point-min))
        (search-forward "*if,load_case")
        (beginning-of-line)
        (apdl-mark-block)
        (setq selected-branch
              (buffer-substring-no-properties
               (region-beginning)
               (region-end)))
        (deactivate-mark)
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :outline outline-minor-mode
               :hidden-number-blocks hidden-number-blocks
               :unhidden unhidden-state
               :number-boundaries number-boundaries
               :outline-hidden outline-state
               :selected-branch selected-branch
               :content
               (buffer-substring-no-properties
                (point-min)
                (point-max))
               :modified (buffer-modified-p))))
    (neomacs-apdl-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:file "workbench/ds.dat" :mode apdl-mode :outline t :hidden-number-blocks ((5 10 t t "![ ... hidden" " region ... ]")) :unhidden (nil 0) :number-boundaries ((3 0 "(1i9,3e20.9e3)") (12 2 "-1")) :outline-hidden (14 2 "*if,load_case,eq,1,then") :selected-branch "*if,load_case,eq,1,then\n  nsel,s,loc,x,0\n  f,all,fy,-1000\n*else\n  /com,Load case disabled\n*endif\n" :content "!@ === Workbench mesh ===\nnblock,3,solid\n(1i9,3e20.9e3)\n1,0.0,0.0,0.0\n2,1.0,0.0,0.0\n3,1.0,1.0,0.0\n4,0.0,1.0,0.0\n5,0.0,0.0,1.0\n6,1.0,0.0,1.0\n7,1.0,1.0,1.0\n8,0.0,1.0,1.0\n-1\n!@ === Load selection ===\n*if,load_case,eq,1,then\n  nsel,s,loc,x,0\n  f,all,fy,-1000\n*else\n  /com,Load case disabled\n*endif\n" :modified nil)"#
        ]],
    )
}

fn apdl_mode_completes_documents_and_inserts_a_previewed_code_template() -> ParityBatchCase {
    ParityBatchCase::value(
        "apdl_mode_completes_documents_and_inserts_a_previewed_code_template",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apdl-template-authoring"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (input (expand-file-name "models/load-sweep.inp" root))
       (default-directory root)
       buffer
       parameter-help
       parameter-highlight
       template-prompts
       preview
       result)
  (unwind-protect
      (progn
        (neomacs-apdl-test-cleanup root)
        (make-directory (file-name-directory input) t)
        (with-temp-file input)
        (setq buffer (find-file-noselect input))
        (save-window-excursion
          (switch-to-buffer buffer)
          (insert "/pre")
          (apdl-complete-symbol)
          (insert
           "\n"
           "et,1,solid186\n"
           "mp,ex,1,210000\n"
           "!@ === Load sweep ===\n")
          (goto-char (point-min))
          (search-forward "mp,ex,")
          (apdl-show-command-parameters 1)
          (let* ((text
                  (overlay-get apdl-help-overlay 'before-string))
                 (start
                  (text-property-any
                   0
                   (length text)
                   'face
                   'isearch-fail
                   text))
                 (end
                  (and
                   start
                   (next-single-property-change
                    start
                    'face
                    text
                    (length text)))))
            (setq parameter-help
                  (substring-no-properties text)
                  parameter-highlight
                  (and
                   start
                   (list
                    start
                    end
                    (substring-no-properties text start end)))))
          (goto-char (point-max))
          (cl-letf
              (((symbol-function 'completing-read)
                (lambda
                  (prompt collection predicate require-match
                          initial-input &rest _arguments)
                  (let ((choice "apdl_do"))
                    (unless
                        (and
                         require-match
                         (test-completion
                          choice
                          collection
                          predicate))
                      (error
                       "Template %S is not available"
                       choice))
                    (push
                     (list prompt initial-input choice)
                     template-prompts)
                    choice))))
            (apdl-display-skeleton 1)
            (with-current-buffer "*APDL-skeleton*"
              (setq preview
                    (list
                     :mode major-mode
                     :read-only buffer-read-only
                     :point (point)
                     :banner
                     (substring-no-properties
                      (overlay-get
                       apdl-skeleton-overlay
                       'before-string))
                     :content
                     (buffer-substring-no-properties
                      (point-min)
                      (point-max)))))
            (switch-to-buffer buffer)
            (goto-char (point-max))
            (apdl-display-skeleton 4))
          (font-lock-ensure)
          (save-buffer)
          (goto-char (point-min))
          (search-forward "*cycle")
          (setq result
                (list
                 :file (file-relative-name buffer-file-name root)
                 :mode major-mode
                 :parameter-help parameter-help
                 :parameter-highlight parameter-highlight
                 :template-prompts (nreverse template-prompts)
                 :preview preview
                 :point
                 (list
                  (line-number-at-pos)
                  (current-column)
                  (current-indentation))
                 :faces
                 (mapcar
                  (lambda (token)
                    (list token (neomacs-apdl-test-face-at token)))
                  '("/prep7"
                    "solid186"
                    "Load sweep"
                    "*do"
                    "*cycle"
                    "*enddo"))
                 :lines (neomacs-apdl-test-lines)
                 :modified (buffer-modified-p)
                 :disk (neomacs-apdl-test-file-string input)))))
    (neomacs-apdl-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:file "models/load-sweep.inp" :mode apdl-mode :parameter-help "MP - Defines a linear material property as a constant or a function of temperature.\nMP, Lab, MAT, C0, C1, C2, C3, C4\n--1----2----3---4---5---6---7---\n" :parameter-highlight (91 96 ", MAT") :template-prompts (("Preview template [TAB to complete]: " "apdl-skeleton-" "apdl_do") ("Insert template [TAB to complete]: " "apdl_do" "apdl_do")) :preview (:mode apdl-mode :read-only t :point 1 :banner "-*- APDL template: apdl_do -*-\n" :content "*do,I,1,10,1\n\n*cycle ! continue loop but bypass below commands\nnplot ! this command is not executed *cycle\n*enddo") :point (7 6 0) :faces (("/prep7" font-lock-keyword-face) ("solid186" font-lock-builtin-face) ("Load sweep" font-lock-comment-face) ("*do" font-lock-keyword-face) ("*cycle" font-lock-keyword-face) ("*enddo" font-lock-keyword-face)) :lines ((1 0 "/prep7") (2 0 "et,1,solid186") (3 0 "mp,ex,1,210000") (4 0 "!@ === Load sweep ===") (5 0 "*do,I,1,10,1") (6 2 "  ") (7 0 "*cycle ! continue loop but bypass below commands") (8 2 "  nplot ! this command is not executed *cycle") (9 0 "*enddo")) :modified nil :disk "/prep7\net,1,solid186\nmp,ex,1,210000\n!@ === Load sweep ===\n*do,I,1,10,1\n  \n*cycle ! continue loop but bypass below commands\n  nplot ! this command is not executed *cycle\n*enddo")"#
        ]],
    )
}

fn apdl_mode_runs_a_saved_model_and_opens_real_solver_artifacts() -> ParityBatchCase {
    ParityBatchCase::value(
        "apdl_mode_runs_a_saved_model_and_opens_real_solver_artifacts",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apdl-batch-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (run-directory (expand-file-name "run/" root))
       (input (expand-file-name "beam.mac" run-directory))
       (solver (expand-file-name "tools/mapdl" root))
       (trace (expand-file-name "solver.trace" root))
       (default-directory run-directory)
       model-buffer
       process-state
       process-output
       out-state
       error-state
       stop-message
       confirmations
       result)
  (unwind-protect
      (progn
        (neomacs-apdl-test-cleanup root)
        (make-directory (file-name-directory solver) t)
        (make-directory run-directory t)
        (with-temp-file solver
          (insert
           "#!/bin/sh\n"
           "set -eu\n"
           ": > \"$APDL_TEST_TRACE\"\n"
           "printf 'cwd=%s\\n' \"$PWD\" >> \"$APDL_TEST_TRACE\"\n"
           "output=''\n"
           "input=''\n"
           "while test \"$#\" -gt 0; do\n"
           "  printf 'arg=<%s>\\n' \"$1\" >> \"$APDL_TEST_TRACE\"\n"
           "  case \"$1\" in\n"
           "    -o) shift; output=$1; printf 'arg=<%s>\\n' \"$1\" >> \"$APDL_TEST_TRACE\" ;;\n"
           "    -i) shift; input=$1; printf 'arg=<%s>\\n' \"$1\" >> \"$APDL_TEST_TRACE\" ;;\n"
           "  esac\n"
           "  shift\n"
           "done\n"
           "printf 'MAPDL batch completed\\ninput=%s\\n' \"$input\" > \"$output\"\n"
           "printf 'MAPDL warning: unconverged load step 3\\n' > beam.err\n"
           "printf 'solver stdout: %s\\n' \"$input\"\n"))
        (set-file-modes solver #o755)
        (with-temp-file input
          (insert
           "/filname,beam\n"
           "/prep7\n"
           "et,1,beam188\n"
           "finish\n"))
        (setq model-buffer (find-file-noselect input))
        (switch-to-buffer model-buffer)
        (setq-local apdl-ansys-program solver)
        (setq-local apdl-batch-license "meba")
        (setq-local apdl-no-of-processors 3)
        (setq-local apdl-job "beam")
        (setq-local apdl-license-file "1055@license.example.test")
        (setq-local process-environment
                    (copy-sequence process-environment))
        (setenv "APDL_TEST_TRACE" trace)
        (save-window-excursion
          (cl-letf
              (((symbol-function 'y-or-n-p)
                (lambda (prompt)
                  (push (list :batch prompt) confirmations)
                  t)))
            (apdl-start-batch-run))
          (let ((process (get-process apdl-batch-process)))
            (while
                (process-live-p process)
              (accept-process-output process 0.05))
            (accept-process-output process 0.05)
            (setq process-state
                  (list
                   (process-name process)
                   (process-status process)
                   (process-exit-status process))
                  process-output
                  (with-current-buffer
                      (process-buffer process)
                    (buffer-substring-no-properties
                     (point-min)
                     (point-max))))))
        (switch-to-buffer model-buffer)
        (save-window-excursion
          (apdl-display-out-file nil)
          (setq out-state
                (list
                 (file-name-nondirectory buffer-file-name)
                 buffer-read-only
                 auto-revert-tail-mode
                 (buffer-substring-no-properties
                  (point-min)
                  (point-max)))))
        (switch-to-buffer model-buffer)
        (save-window-excursion
          (apdl-display-error-file nil)
          (setq error-state
                (list
                 (file-name-nondirectory buffer-file-name)
                 buffer-read-only
                 auto-revert-tail-mode
                 (buffer-substring-no-properties
                  (point-min)
                  (point-max)))))
        (switch-to-buffer model-buffer)
        (save-window-excursion
          (cl-letf
              (((symbol-function 'yes-or-no-p)
                (lambda (prompt)
                  (push (list :stop prompt) confirmations)
                  t)))
            (setq stop-message (apdl-abort-file 1))))
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :process process-state
               :process-output process-output
               :confirmations (nreverse confirmations)
               :trace (neomacs-apdl-test-file-string trace)
               :out out-state
               :error error-state
               :out-disk
               (neomacs-apdl-test-file-string
                (expand-file-name "beam.out" run-directory))
               :error-disk
               (neomacs-apdl-test-file-string
                (expand-file-name "beam.err" run-directory))
               :stop-message stop-message
               :stop-disk
               (neomacs-apdl-test-file-string
                (expand-file-name "beam.abt" run-directory))
               :model-disk
               (neomacs-apdl-test-file-string input))))
    (setenv "APDL_TEST_TRACE" nil)
    (neomacs-apdl-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:file "run/beam.mac" :mode apdl-mode :process ("MAPDL-Batch" exit 0) :process-output "solver stdout: [ORACLE-SANDBOX]/apdl-batch-workflow/run/beam.mac\n\nProcess MAPDL-Batch finished\n" :confirmations ((:batch "Start batch run: [ORACLE-SANDBOX]/apdl-batch-workflow/tools/mapdl, input file: [ORACLE-SANDBOX]/apdl-batch-workflow/run/beam.mac, license: meba, job: beam in [ORACLE-SANDBOX]/apdl-batch-workflow/run/, lic server: 1055@license.example.test ") (:stop "Write stop file \"[ORACLE-SANDBOX]/apdl-batch-workflow/run/beam.abt\"? ")) :trace "cwd=[ORACLE-SANDBOX]/apdl-batch-workflow/run\narg=<-p>\narg=<meba>\narg=<-lch>\narg=<[ORACLE-SANDBOX]/apdl-batch-workflow/run/>\narg=<-smp>\narg=<-np>\narg=<3>\narg=<-j>\narg=<beam>\narg=<-s>\narg=<noread>\narg=<-l en-us>\narg=<-b>\narg=<-i>\narg=<[ORACLE-SANDBOX]/apdl-batch-workflow/run/beam.mac>\narg=<-o>\narg=<beam.out>\n" :out ("beam.out" t t "MAPDL batch completed\ninput=[ORACLE-SANDBOX]/apdl-batch-workflow/run/beam.mac\n") :error ("beam.err" t t "MAPDL warning: unconverged load step 3\n") :out-disk "MAPDL batch completed\ninput=[ORACLE-SANDBOX]/apdl-batch-workflow/run/beam.mac\n" :error-disk "MAPDL warning: unconverged load step 3\n" :stop-message "Wrote MAPDL stop file beam.abt in [ORACLE-SANDBOX]/apdl-batch-workflow/run/." :stop-disk "nonlinear\n" :model-disk "/filname,beam\n/prep7\net,1,beam188\nfinish\n")"#
        ]],
    )
}

fn apdl_mode_routes_contextual_commands_elements_and_topics_to_exact_help_pages() -> ParityBatchCase
{
    ParityBatchCase::value(
        "apdl_mode_routes_contextual_commands_elements_and_topics_to_exact_help_pages",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apdl-help-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (model (expand-file-name "models/help-tour.mac" root))
       (local-help
        (file-name-as-directory
         (expand-file-name "ansys-help/" root)))
       (default-directory root)
       buffer
       routes
       prompts
       result)
  (unwind-protect
      (progn
        (neomacs-apdl-test-cleanup root)
        (make-directory (file-name-directory model) t)
        (make-directory local-help t)
        (with-temp-file model
          (insert
           "/prep7\n"
           "et,1,solid186\n"
           "radius = acos(-1)\n"
           "solve\n"))
        (setq buffer (find-file-noselect model))
        (switch-to-buffer buffer)
        (setq-local apdl-current-ansys-version "v251")
        (setq-local apdl-ansys-help-path nil)
        (cl-letf
            (((symbol-function 'browse-url)
              (lambda (url &rest _arguments)
                (push (list :online url) routes)
                url))
             ((symbol-function 'browse-url-of-file)
              (lambda (file &rest _arguments)
                (push (list :local file) routes)
                file)))
          (dolist
              (token '("solid186" "acos" "solve"))
            (goto-char (point-min))
            (search-forward token)
            (backward-char 2)
            (apdl-browse-apdl-help nil))
          (cl-letf
              (((symbol-function 'completing-read)
                (lambda
                  (prompt collection &optional predicate _require-match
                          &rest _arguments)
                  (let
                      ((choice
                        "\"PARAMETRIC DESIGN LANGUAGE GUIDE\""))
                    (unless
                        (test-completion
                         choice
                         collection
                         predicate)
                      (error
                       "Help topic %S is not available"
                       choice))
                    (push (list prompt choice) prompts)
                    choice))))
            (apdl-browse-apdl-help 1))
          (setq-local apdl-ansys-help-path local-help)
          (goto-char (point-min))
          (search-forward "solid186")
          (backward-char 2)
          (apdl-browse-apdl-help nil))
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :prompts (nreverse prompts)
               :routes (nreverse routes)
               :point
               (list
                (line-number-at-pos)
                (current-column)
                (buffer-substring-no-properties
                 (line-beginning-position)
                 (line-end-position)))
               :modified (buffer-modified-p)
               :disk (neomacs-apdl-test-file-string model))))
    (neomacs-apdl-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:file "models/help-tour.mac" :mode apdl-mode :prompts (("Browse help for keyword [TAB to complete]: " "\"PARAMETRIC DESIGN LANGUAGE GUIDE\"")) :routes ((:online "https://ansyshelp.ansys.com/public//Views/Secured/corp/v251/en/ans_elem/Hlp_E_SOLID186.html") (:online "https://ansyshelp.ansys.com/public//Views/Secured/corp/v251/en/ans_apdl/Hlp_P_APDL3_9.html") (:online "https://ansyshelp.ansys.com/public//Views/Secured/corp/v251/en/ans_cmd/Hlp_C_SOLVE.html") (:online "https://ansyshelp.ansys.com/public//Views/Secured/corp/v251/en/ans_apdl/Hlp_P_APDLTOC.html") (:local "[ORACLE-SANDBOX]/apdl-help-workflow/ansys-help/ans_elem/Hlp_E_SOLID186.html")) :point (2 11 "et,1,solid186") :modified nil :disk "/prep7\net,1,solid186\nradius = acos(-1)\nsolve\n")"#
        ]],
    )
}

fn apdl_mode_repairs_an_incomplete_model_and_preserves_existing_content() -> ParityBatchCase {
    ParityBatchCase::value(
        "apdl_mode_repairs_an_incomplete_model_and_preserves_existing_content",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apdl-repair-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (model (expand-file-name "models/repaired.mac" root))
       (default-directory root)
       buffer
       failed-close
       format-after-split
       condensed-command
       result)
  (unwind-protect
      (progn
        (neomacs-apdl-test-cleanup root)
        (make-directory (file-name-directory model) t)
        (with-temp-file model
          (insert
           "jobname = 'production beam model'\n"
           "*vwrite,node_id,displacement,label\n"
           "(I8,E16.8,A8)\n"
           "n,1,0,0,0 $ n,2,1,0,0 $ n,3,2,0,0 ! beam nodes\n"
           "*if,load_case,eq,1,then\n"
           "solve\n"))
        (setq buffer (find-file-noselect model))
        (switch-to-buffer buffer)
        (setq-local indent-tabs-mode nil)
        (goto-char (point-min))
        (let ((before
               (buffer-substring-no-properties
                (point-min)
                (point-max))))
          (setq failed-close
                (list
                 (apdl-close-block)
                 (current-message)
                 (point)
                 (equal
                  before
                  (buffer-substring-no-properties
                   (point-min)
                   (point-max)))
                 (buffer-modified-p))))
        (goto-char (point-min))
        (search-forward "(I8")
        (apdl-indent-format-line)
        (setq format-after-split
              (buffer-substring-no-properties
               (line-beginning-position -1)
               (line-beginning-position 2)))
        (goto-char (point-min))
        (search-forward "n,2,1,0,0")
        (let ((end
               (save-excursion
                 (apdl-command-end)
                 (point))))
          (apdl-command-start)
          (setq condensed-command
                (list
                 (line-number-at-pos)
                 (current-column)
                 (buffer-substring-no-properties
                  (point)
                  end))))
        (goto-char (point-max))
        (apdl-close-block)
        (end-of-line)
        (insert "\nfinish\n")
        (indent-region (point-min) (point-max))
        (font-lock-ensure)
        (save-buffer)
        (goto-char (point-min))
        (search-forward "*endif")
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :failed-close failed-close
               :format-after-split format-after-split
               :condensed-command condensed-command
               :point
               (list
                (line-number-at-pos)
                (current-column)
                (current-indentation))
               :faces
               (mapcar
                (lambda (token)
                  (list token (neomacs-apdl-test-face-at token)))
                '("production beam model"
                  "*vwrite"
                  "&"
                  "beam nodes"
                  "*if"
                  "solve"
                  "*endif"))
               :lines (neomacs-apdl-test-lines)
               :modified (buffer-modified-p)
               :disk (neomacs-apdl-test-file-string model))))
    (neomacs-apdl-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:file "models/repaired.mac" :mode apdl-mode :failed-close ("Cannot find a proper block command to close" nil 1 t nil) :format-after-split "*vwrite,node_id,displacement,label\n(I8 &\n    ,E16.8,A8)\n" :condensed-command (5 12 "n,2,1,0,0 $ n,3,2,0,0 ! beam nodes") :point (8 6 0) :faces (("production beam model" font-lock-string-face) ("*vwrite" font-lock-keyword-face) ("&" font-lock-type-face) ("beam nodes" font-lock-comment-face) ("*if" font-lock-keyword-face) ("solve" font-lock-keyword-face) ("*endif" font-lock-keyword-face)) :lines ((1 0 "jobname = 'production beam model'") (2 0 "*vwrite,node_id,displacement,label") (3 0 "(I8 &") (4 4 "    ,E16.8,A8)") (5 0 "n,1,0,0,0 $ n,2,1,0,0 $ n,3,2,0,0 ! beam nodes") (6 0 "*if,load_case,eq,1,then") (7 2 "  solve") (8 0 "*endif") (9 0 "finish")) :modified nil :disk "jobname = 'production beam model'\n*vwrite,node_id,displacement,label\n(I8 &\n    ,E16.8,A8)\nn,1,0,0,0 $ n,2,1,0,0 $ n,3,2,0,0 ! beam nodes\n*if,load_case,eq,1,then\n  solve\n*endif\nfinish\n")"#
        ]],
    )
}

fn apdl_mode_drives_an_interactive_solver_then_falls_back_to_copying_code() -> ParityBatchCase {
    ParityBatchCase::value(
        "apdl_mode_drives_an_interactive_solver_then_falls_back_to_copying_code",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apdl-interactive-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (run-directory (expand-file-name "run/" root))
       (model (expand-file-name "interactive.mac" run-directory))
       (solver (expand-file-name "tools/mapdl-interactive" root))
       (trace (expand-file-name "interactive.trace" root))
       (default-directory run-directory)
       model-buffer
       confirmations
       query-prompt
       sent-line-state
       sent-region-state
       process-state
       process-output
       fallback-state
       result)
  (unwind-protect
      (progn
        (neomacs-apdl-test-cleanup root)
        (make-directory (file-name-directory solver) t)
        (make-directory run-directory t)
        (with-temp-file solver
          (insert
           "#!/bin/sh\n"
           "set -eu\n"
           "trace=\"${0%/*}/../interactive.trace\"\n"
           ": > \"$trace\"\n"
           "printf 'argv=<%s>\\n' \"$*\" >> \"$trace\"\n"
           "printf 'BEGIN:\\n'\n"
           "while IFS= read -r command; do\n"
           "  printf 'command=<%s>\\n' \"$command\" >> \"$trace\"\n"
           "  printf 'accepted: %s\\n' \"$command\"\n"
           "  case \"$command\" in *'/exit,all'*) exit 0 ;; esac\n"
           "done\n"))
        (set-file-modes solver #o755)
        (with-temp-file model
          (insert
           "/prep7\n"
           "et,1,beam188\n"
           "mp,ex,1,210000\n"
           "solve\n"))
        (setq model-buffer (find-file-noselect model))
        (switch-to-buffer model-buffer)
        (setq-local apdl-ansys-program solver)
        (setq-local apdl-current-ansys-version "v251")
        (setq-local apdl-license "meba")
        (setq-local apdl-no-of-processors 4)
        (setq-local apdl-job "interactive-beam")
        (setq-local apdl-license-file "1055@license.example.test")
        (save-window-excursion
          (cl-letf
              (((symbol-function 'y-or-n-p)
                (lambda (prompt)
                  (push (list :start prompt) confirmations)
                  t))
               ((symbol-function 'display-buffer)
                (lambda (buffer &rest action)
                  (ignore action)
                  (get-buffer buffer)))
               ((symbol-function 'other-window)
                (lambda (count &optional all-frames)
                  (ignore count all-frames))))
            (apdl-start-ansys)))
        (switch-to-buffer model-buffer)
        (goto-char (point-min))
        (cl-letf
            (((symbol-function 'display-buffer-other-frame)
              (lambda (buffer &rest action)
                (ignore action)
                (get-buffer buffer))))
          (apdl-send-to-apdl-and-proceed 1))
        (setq sent-line-state
              (list
               (line-number-at-pos)
               (current-column)
               (buffer-substring-no-properties
                (line-beginning-position)
                (line-end-position))))
        (cl-letf
            (((symbol-function 'completing-read)
              (lambda
                (prompt collection predicate _require-match
                        &rest _arguments)
                (let ((choice "*STATUS"))
                  (unless
                      (test-completion
                       choice
                       collection
                       predicate)
                    (error
                     "APDL query %S is not available"
                     choice))
                  (setq query-prompt (list prompt choice))
                  choice)))
             ((symbol-function 'display-buffer)
              (lambda (buffer &rest action)
                (ignore action)
                (get-buffer buffer))))
          (apdl-query-apdl-command nil))
        (switch-to-buffer model-buffer)
        (goto-char (point-min))
        (forward-line 1)
        (let ((start (point)))
          (forward-line 2)
          (push-mark (point) nil t)
          (goto-char start)
          (setq mark-active t)
          (cl-letf
              (((symbol-function 'display-buffer-other-frame)
                (lambda (buffer &rest action)
                  (ignore action)
                  (get-buffer buffer))))
            (apdl-send-to-ansys 1)))
        (setq sent-region-state
              (list
               (line-number-at-pos)
               (current-column)
               mark-active))
        (let ((process (get-process apdl-process-name)))
          (accept-process-output process 0.2)
          (cl-letf
              (((symbol-function 'yes-or-no-p)
                (lambda (prompt)
                  (push (list :exit prompt) confirmations)
                  t)))
            (apdl-exit-ansys))
          (while
              (process-live-p process)
            (accept-process-output process 0.05))
          (accept-process-output process 0.05)
          (setq process-state
                (list
                 (process-name process)
                 (process-status process)
                 (process-exit-status process))
                process-output
                (with-current-buffer
                    (process-buffer process)
                  (buffer-substring-no-properties
                   (point-min)
                   (point-max)))))
        (switch-to-buffer model-buffer)
        (goto-char (point-min))
        (let ((kill-ring nil)
              (kill-ring-yank-pointer nil))
          (apdl-send-to-apdl-and-proceed 4)
          (setq fallback-state
                (list
                 (current-kill 0 t)
                 (current-message)
                 (line-number-at-pos)
                 (current-column))))
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :confirmations (nreverse confirmations)
               :query-prompt query-prompt
               :sent-line sent-line-state
               :sent-region sent-region-state
               :process process-state
               :process-output process-output
               :trace (neomacs-apdl-test-file-string trace)
               :fallback fallback-state
               :modified (buffer-modified-p)
               :disk (neomacs-apdl-test-file-string model))))
    (neomacs-apdl-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:file "run/interactive.mac" :mode apdl-mode :confirmations ((:start "Start run?  (version: v251, license type: meba, No of processors: 4, job: interactive-beam in [ORACLE-SANDBOX]/apdl-interactive-workflow/run/, server: 1055@license.example.test") (:exit "Do you want to exit the Ansys run?")) :query-prompt ("Send to interpreter: " "*STATUS") :sent-line (2 0 "et,1,beam188") :sent-region (2 0 t) :process ("MAPDL" exit 0) :process-output "BEGIN:\naccepted: /prep7\naccepted: *STATUS\naccepted: /prep7\naccepted: et,1,beam188\naccepted: mp,ex,1,210000\naccepted: solve\naccepted: finish $ /exit,all\n\nProcess MAPDL finished\n" :trace "argv=<-np 4 -p meba -j interactive-beam>\ncommand=</prep7>\ncommand=<*STATUS>\ncommand=</prep7>\ncommand=<et,1,beam188>\ncommand=<mp,ex,1,210000>\ncommand=<solve>\ncommand=<finish $ /exit,all>\n" :fallback ("/prep7\n" nil 1 0) :modified nil :disk "/prep7\net,1,beam188\nmp,ex,1,210000\nsolve\n")"#
        ]],
    )
}

fn apdl_mode_filters_real_license_manager_output_for_operators() -> ParityBatchCase {
    ParityBatchCase::value(
        "apdl_mode_filters_real_license_manager_output_for_operators",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apdl-license-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (tool (expand-file-name "tools/lmutil" root))
       (trace (expand-file-name "lmutil.trace" root))
       (default-directory root)
       license-report
       down-report
       license-face
       user-report
       user-buffer
       result)
  (unwind-protect
      (progn
        (neomacs-apdl-test-cleanup root)
        (make-directory (file-name-directory tool) t)
        (with-temp-file tool
          (insert
           "#!/bin/sh\n"
           "set -eu\n"
           "printf 'argv=<%s>\\n' \"$*\" >> \"$APDL_LICENSE_TRACE\"\n"
           "cat <<'EOF'\n"
           "lmutil - Copyright Ansys\n"
           "License server status: 1055@license.example.test\n"
           "\n"
           "Users of meba: (Total of 5 licenses issued; Total of 2 licenses in use)\n"
           "  \"meba\" v2025.1, vendor: ansyslmd, expiry: permanent\n"
           "    exec workstation /dev/pts/1 (v2025.1) (license/1055 123), start Mon\n"
           "    teammate node /dev/pts/2 (v2025.1) (license/1055 456), start Mon\n"
           "Users of preppost: (Total of 4 licenses issued; Total of 1 license in use)\n"
           "  \"preppost\" v2025.1, vendor: ansyslmd, expiry: permanent\n"
           "    exec workstation /dev/pts/1 (v2025.1) (license/1055 789), start Mon\n"
           "Users of ansys: (Total of 3 licenses issued; Total of 0 licenses in use)\n"
           "  \"ansys\" v2025.1, vendor: ansyslmd, expiry: permanent\n"
           "EOF\n"))
        (set-file-modes tool #o755)
        (setq apdl-lmutil-program tool
              apdl-license-file "1055@license.example.test"
              apdl-license "meba"
              apdl-username "exec"
              process-environment (copy-sequence process-environment))
        (setenv "APDL_LICENSE_TRACE" trace)
        (cl-letf
            (((symbol-function 'current-time-string)
              (lambda () "Mon Jan 15 10:30:00 2024")))
          (apdl-license-status nil)
          (with-current-buffer "*APDL-licenses*"
            (setq license-report
                  (buffer-substring-no-properties
                   (point-min)
                   (point-max)))
            (goto-char (point-min))
            (search-forward "meba:")
            (setq license-face
                  (get-text-property
                   (match-beginning 0)
                   'face))
            (call-interactively (key-binding (kbd "d")))
            (setq down-report
                  (buffer-substring-no-properties
                   (point-min)
                   (point-max)))
            (call-interactively (key-binding (kbd "u"))))
          (with-current-buffer "*User-licenses*"
            (setq user-report
                  (buffer-substring-no-properties
                   (point-min)
                   (point-max))
                  user-buffer
                  (buffer-name))))
        (setq result
              (list
               :license-report license-report
               :down-report down-report
               :license-face license-face
               :user-report user-report
               :user-buffer user-buffer
               :trace (neomacs-apdl-test-file-string trace))))
    (setenv "APDL_LICENSE_TRACE" nil)
    (neomacs-apdl-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:license-report " -*- License status, type h or ? for help -*-\nlmutil - Copyright Ansys\nLicense server status: 1055@license.example.test\nmeba: (5 licenses issued; 2 licenses in use)\n    exec workstation /dev/pts/1 (v2025.1) (license/1055 123), start Mon\n    teammate node /dev/pts/2 (v2025.1) (license/1055 456), start Mon\npreppost: (4 licenses issued; 1 license in use)\n    exec workstation /dev/pts/1 (v2025.1) (license/1055 789), start Mon\nansys: (3 licenses issued; 0 licenses in use)\n\nMon Jan 15 10:30:00 2024\n" :down-report " -*- License status, type h or ? for help -*-\nmeba:ANSYS Mechanical Batch (5 licenses issued; 2 licenses in use)\npreppost:ANSYS Mechanical PrepPost (4 licenses issued; 1 license in use)\nansys:ANSYS Mechanical (3 licenses issued; 0 licenses in use)\n\nMon Jan 15 10:30:00 2024\n" :license-face font-lock-warning-face :user-report " -*- User license status type h or ? for help -*-\nmeba: (5 licenses issued; 2 licenses in use)\n    exec workstation /dev/pts/1 (v2025.1) (license/1055 123), start Mon\npreppost: (4 licenses issued; 1 license in use)\n    exec workstation /dev/pts/1 (v2025.1) (license/1055 789), start Mon\n\nMon Jan 15 10:30:00 2024\n" :user-buffer "*User-licenses*" :trace "argv=<lmstat -c  1055@license.example.test -a>\nargv=<lmstat -c  1055@license.example.test -a>\nargv=<lmstat -c  1055@license.example.test -a>\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        apdl_mode_authors_and_navigates_a_structural_analysis_model(),
        apdl_mode_inspects_workbench_mesh_data_and_control_flow(),
        apdl_mode_completes_documents_and_inserts_a_previewed_code_template(),
        apdl_mode_runs_a_saved_model_and_opens_real_solver_artifacts(),
        apdl_mode_routes_contextual_commands_elements_and_topics_to_exact_help_pages(),
        apdl_mode_repairs_an_incomplete_model_and_preserves_existing_content(),
        apdl_mode_drives_an_interactive_solver_then_falls_back_to_copying_code(),
        apdl_mode_filters_real_license_manager_output_for_operators(),
    ]
}
