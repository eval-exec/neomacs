use expect_test::expect;

use super::ParityBatchCase;

fn setup_configures_new_auctex_buffers_and_expands_document_commands() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-latexmk-expansion"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "draft Ω report.tex" root))
       (default-directory root)
       buffer result)
  (unwind-protect
      (progn
        (neomacs-auctex-latexmk-test--cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\nRelease Ω.\n\\end{document}\n"))
        (setq buffer (find-file-noselect source))
        (with-current-buffer buffer
          (LaTeX-mode)
          (setq-local TeX-master t)
          (let* ((entry (assoc "LatexMk" TeX-command-list))
                 (template (nth 1 entry))
                 (expand
                  (lambda (engine pdf inherit correlate interactive file-error extra)
                    (let ((TeX-engine engine)
                          (TeX-PDF-mode pdf)
                          (auctex-latexmk-inherit-TeX-PDF-mode inherit)
                          (TeX-source-correlate-mode correlate)
                          (TeX-interactive-mode interactive)
                          (TeX-file-line-error file-error)
                          (TeX-command-extra-options extra))
                      (TeX-command-expand template)))))
            (setq result
                  (list
                   :command (copy-tree entry)
                   :counts
                   (list
                    (cl-count "LatexMk" TeX-command-list
                              :key #'car :test #'equal)
                    (cl-count "%(-PDF)" TeX-expand-list
                              :key #'car :test #'equal)
                    (cl-count "\\.aux.bak" LaTeX-clean-intermediate-suffixes
                              :test #'equal))
                   :clean-tail
                   (last LaTeX-clean-intermediate-suffixes 6)
                   :expansions
                   (list
                    (funcall expand 'default t nil nil nil t "-halt-on-error")
                    (funcall expand 'default t t t nil t "-halt-on-error")
                    (funcall expand 'xetex t nil nil t nil "")
                    (funcall expand 'xetex t t nil t nil "")
                    (funcall expand 'luatex t nil nil nil t "--shell-escape")
                    (funcall expand 'ptex nil t nil nil nil "")))))))
    (neomacs-auctex-latexmk-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:command ("LatexMk" "latexmk %(-PDF)%S%(mode) %(file-line-error) %(extraopts) %t" TeX-run-latexmk nil (plain-tex-mode latex-mode doctex-mode) :help "Run LatexMk") :counts (1 1 1) :clean-tail ("\\.alg" "\\.glg" "\\.ist" "\\.fdb_latexmk" "\\.aux.bak" "\\.fls") :expansions ("latexmk  -interaction=nonstopmode  -file-line-error -halt-on-error draft\\ \\Ω\\ report.tex" "latexmk -pdf --synctex=1 -interaction=nonstopmode  -file-line-error -halt-on-error draft\\ \\Ω\\ report.tex" "latexmk -xelatex    draft\\ \\Ω\\ report.tex" "latexmk -pdf -pdflatex=xelatex    draft\\ \\Ω\\ report.tex" "latexmk -lualatex  -interaction=nonstopmode  -file-line-error --shell-escape draft\\ \\Ω\\ report.tex" "latexmk  -interaction=nonstopmode   draft\\ \\Ω\\ report.tex"))"#
    ]];
    ParityBatchCase::value(
        "setup_configures_new_auctex_buffers_and_expands_document_commands",
        elisp_form,
        expect,
    )
}

fn interactive_latexmk_build_passes_encoding_and_creates_real_outputs() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-latexmk-success"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "main.tex" root))
       (arguments-file (expand-file-name "arguments.txt" root))
       (cwd-file (expand-file-name "cwd.txt" root))
       (encoding-file (expand-file-name "encoding.txt" root))
       (ready-file (expand-file-name "ready" root))
       (release-file (expand-file-name "release" root))
       (default-directory root)
       buffer process output messages-start result)
  (unwind-protect
      (progn
        (neomacs-auctex-latexmk-test--cleanup root)
        (make-directory root t)
        (neomacs-auctex-latexmk-test--write-program root 'success)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\n日本語 release Ω.\n\\end{document}\n"))
        (let ((process-environment (copy-sequence process-environment))
              (exec-path (cons (expand-file-name "bin" root) exec-path)))
          (setenv "PATH"
                  (concat (expand-file-name "bin" root)
                          path-separator (getenv "PATH")))
          (setenv "NEOMACS_LATEXMK_ARGUMENTS" arguments-file)
          (setenv "NEOMACS_LATEXMK_CWD" cwd-file)
          (setenv "NEOMACS_LATEXMK_ENCODING" encoding-file)
          (setenv "NEOMACS_LATEXMK_READY" ready-file)
          (setenv "NEOMACS_LATEXMK_RELEASE" release-file)
          (setq buffer (find-file-noselect source))
          (with-current-buffer buffer
            (LaTeX-mode)
            (setq-local TeX-master t)
            (let ((TeX-command-force "LatexMk")
                  (TeX-process-asynchronous t)
                  (TeX-source-correlate-mode t)
                  (TeX-file-line-error t)
                  (TeX-command-extra-options "--halt-on-error"))
              (setq messages-start
                    (with-current-buffer (messages-buffer) (point-max)))
              (call-interactively #'TeX-command-master)
              (setq process (get-process "LatexMk"))))
          (setq output (process-buffer process))
          (let ((initial
                 (list
                  :process (processp process)
                  :live (process-live-p process)
                  :filter (process-filter process)
                  :sentinel (process-sentinel process)
                  :package-sentinel
                  (with-current-buffer output TeX-sentinel-function)
                  :parent-encoding (getenv "LATEXENC"))))
            (with-temp-file release-file (insert "release\n"))
            (neomacs-auctex-latexmk-test--wait process)
            (setq result
                  (list
                   :initial initial
                   :final
                   (list
                    :status (process-status process)
                    :exit (process-exit-status process)
                    :filter (process-filter process)
                    :sentinel (process-sentinel process))
                   :cwd
                   (file-relative-name
                    (string-trim
                     (with-temp-buffer
                       (insert-file-contents cwd-file)
                       (buffer-string)))
                    root)
                   :arguments
                   (neomacs-auctex-latexmk-test--read-lines arguments-file)
                   :encoding
                   (neomacs-auctex-latexmk-test--read-lines encoding-file)
                   :artifacts
                   (mapcar
                    (lambda (relative)
                      (let ((file (expand-file-name relative root)))
                        (list relative
                              (file-exists-p file)
                              (and (file-exists-p file)
                                   (with-temp-buffer
                                     (insert-file-contents-literally file)
                                     (buffer-string))))))
                    '("main.pdf" "main.aux" "main.fdb_latexmk"
                      "main.fls" "main.log"))
                   :output
                   (with-current-buffer output
                     (list
                      :mode major-mode
                      :extension TeX-output-extension
                      :next TeX-command-next
                      :default TeX-command-default
                      :mode-line mode-line-process
                      :transcript
                      (neomacs-auctex-latexmk-test--output-transcript
                       output root)))
                   :messages
                   (neomacs-auctex-latexmk-test--messages
                    messages-start root))))))
    (neomacs-auctex-latexmk-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:initial (:process t :live (run open listen connect stop) :filter TeX-format-filter :sentinel TeX-command-sentinel :package-sentinel Latexmk-sentinel :parent-encoding nil) :final (:status exit :exit 0 :filter TeX-format-filter :sentinel TeX-command-sentinel) :cwd "." :arguments ("--synctex=1" "-interaction=nonstopmode" "-file-line-error" "--halt-on-error" "main.tex") :encoding ("utf8") :artifacts (("main.pdf" t "PDF release \316\251") ("main.aux" t "aux release") ("main.fdb_latexmk" t "database release") ("main.fls" t "file list release") ("main.log" t "log release")) :output (:mode TeX-output-mode :extension "pdf" :next "View" :default "LatexMk" :mode-line " {1}: exit" :transcript "Running `LatexMk' on `main' with ``latexmk --synctex=1 -interaction=nonstopmode  -file-line-error --halt-on-error main.tex''\nlatexmk: applying document rules\nRun number 1 of rule 'bibtex main'\nBibTeX preparation complete\nRule 'bibtex main': finished\nRun number 1 of rule 'pdflatex'\nLatexmk preamble one\nLatexmk preamble two\nLatexmk preamble three\nLatexmk preamble four\nThis is pdfTeX, Version deterministic\nLaTeX2e <2024-11-01>\nOutput written on main.pdf (1 page, 64 bytes).\nTranscript written on main.log.\nLatexmk: All targets are up-to-date\n\nTeX Output finished at <time>") :messages ("Applying style hooks...done" "Type ‘C-c C-l’ to display results of compilation." "LatexMk: successfully formatted {1} page"))"#
    ]];
    ParityBatchCase::value(
        "interactive_latexmk_build_passes_encoding_and_creates_real_outputs",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn up_to_date_document_reports_nothing_to_do_without_rewriting_output() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-latexmk-nothing"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "main.tex" root))
       (pdf (expand-file-name "main.pdf" root))
       (arguments-file (expand-file-name "arguments.txt" root))
       (cwd-file (expand-file-name "cwd.txt" root))
       (encoding-file (expand-file-name "encoding.txt" root))
       (ready-file (expand-file-name "ready" root))
       (release-file (expand-file-name "release" root))
       (default-directory root)
       buffer process output messages-start before result)
  (unwind-protect
      (progn
        (neomacs-auctex-latexmk-test--cleanup root)
        (make-directory root t)
        (neomacs-auctex-latexmk-test--write-program root 'nothing)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\nStable release.\n\\end{document}\n"))
        (with-temp-file pdf (insert "stable PDF bytes"))
        (set-file-times pdf (seconds-to-time 4242))
        (setq before
              (list
               (with-temp-buffer
                 (insert-file-contents-literally pdf)
                 (buffer-string))
               (float-time
                (file-attribute-modification-time (file-attributes pdf)))))
        (let ((process-environment (copy-sequence process-environment))
              (exec-path (cons (expand-file-name "bin" root) exec-path)))
          (setenv "PATH"
                  (concat (expand-file-name "bin" root)
                          path-separator (getenv "PATH")))
          (setenv "NEOMACS_LATEXMK_ARGUMENTS" arguments-file)
          (setenv "NEOMACS_LATEXMK_CWD" cwd-file)
          (setenv "NEOMACS_LATEXMK_ENCODING" encoding-file)
          (setenv "NEOMACS_LATEXMK_READY" ready-file)
          (setenv "NEOMACS_LATEXMK_RELEASE" release-file)
          (setq buffer (find-file-noselect source))
          (with-current-buffer buffer
            (LaTeX-mode)
            (setq-local TeX-master t)
            (let ((TeX-command-force "LatexMk")
                  (TeX-process-asynchronous t))
              (setq messages-start
                    (with-current-buffer (messages-buffer) (point-max)))
              (call-interactively #'TeX-command-master)
              (setq process (get-process "LatexMk"))))
          (setq output (process-buffer process))
          (with-temp-file release-file (insert "release\n"))
          (neomacs-auctex-latexmk-test--wait process)
          (setq result
                (list
                 :status (process-status process)
                 :exit (process-exit-status process)
                 :arguments
                 (neomacs-auctex-latexmk-test--read-lines arguments-file)
                 :encoding
                 (neomacs-auctex-latexmk-test--read-lines encoding-file)
                 :before before
                 :after
                 (list
                  (with-temp-buffer
                    (insert-file-contents-literally pdf)
                    (buffer-string))
                  (float-time
                   (file-attribute-modification-time (file-attributes pdf))))
                 :sidecars
                 (mapcar
                  (lambda (relative)
                    (file-exists-p (expand-file-name relative root)))
                  '("main.aux" "main.fdb_latexmk" "main.fls" "main.log"))
                 :output
                 (with-current-buffer output
                   (list
                    :next TeX-command-next
                    :default TeX-command-default
                    :transcript
                    (neomacs-auctex-latexmk-test--output-transcript
                     output root)))
                 :messages
                 (neomacs-auctex-latexmk-test--messages
                  messages-start root)))))
    (neomacs-auctex-latexmk-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:status exit :exit 0 :arguments ("-interaction=nonstopmode" "-file-line-error" "main.tex") :encoding ("unset") :before ("stable PDF bytes" 4242.0) :after ("stable PDF bytes" 4242.0) :sidecars (nil nil nil nil) :output (:next "View" :default "LatexMk" :transcript "Running `LatexMk' on `main' with ``latexmk  -interaction=nonstopmode  -file-line-error  main.tex''\nlatexmk: inspecting existing targets\nLatexmk: Nothing to do for 'main.tex'.\n\nTeX Output finished at <time>") :messages ("Applying style hooks...done" "Type ‘C-c C-l’ to display results of compilation." "LatexMk: nothing to do"))"#
    ]];
    ParityBatchCase::value(
        "up_to_date_document_reports_nothing_to_do_without_rewriting_output",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn latex_failure_routes_real_error_summary_through_auctex() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-latexmk-latex-failure"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "main.tex" root))
       (arguments-file (expand-file-name "arguments.txt" root))
       (cwd-file (expand-file-name "cwd.txt" root))
       (encoding-file (expand-file-name "encoding.txt" root))
       (ready-file (expand-file-name "ready" root))
       (release-file (expand-file-name "release" root))
       (default-directory root)
       buffer process output messages-start result)
  (unwind-protect
      (progn
        (neomacs-auctex-latexmk-test--cleanup root)
        (make-directory root t)
        (neomacs-auctex-latexmk-test--write-program root 'latex-failure)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\n"
           "\\brokencommand\n"
           "\\end{document}\n"))
        (let ((process-environment (copy-sequence process-environment))
              (exec-path (cons (expand-file-name "bin" root) exec-path)))
          (setenv "PATH"
                  (concat (expand-file-name "bin" root)
                          path-separator (getenv "PATH")))
          (setenv "NEOMACS_LATEXMK_ARGUMENTS" arguments-file)
          (setenv "NEOMACS_LATEXMK_CWD" cwd-file)
          (setenv "NEOMACS_LATEXMK_ENCODING" encoding-file)
          (setenv "NEOMACS_LATEXMK_READY" ready-file)
          (setenv "NEOMACS_LATEXMK_RELEASE" release-file)
          (setq buffer (find-file-noselect source))
          (with-current-buffer buffer
            (LaTeX-mode)
            (setq-local TeX-master t)
            (let ((TeX-command-force "LatexMk")
                  (TeX-process-asynchronous t)
                  (TeX-parse-all-errors nil))
              (setq messages-start
                    (with-current-buffer (messages-buffer) (point-max)))
              (call-interactively #'TeX-command-master)
              (setq process (get-process "LatexMk"))))
          (setq output (process-buffer process))
          (with-temp-file release-file (insert "release\n"))
          (neomacs-auctex-latexmk-test--wait process)
          (setq result
                (list
                 :status (process-status process)
                 :exit (process-exit-status process)
                 :arguments
                 (neomacs-auctex-latexmk-test--read-lines arguments-file)
                 :artifacts
                 (mapcar
                  (lambda (relative)
                    (file-exists-p (expand-file-name relative root)))
                  '("main.pdf" "main.aux" "main.fdb_latexmk"
                    "main.fls" "main.log"))
                 :output
                 (with-current-buffer output
                   (list
                    :next TeX-command-next
                    :default TeX-command-default
                    :transcript
                    (neomacs-auctex-latexmk-test--output-transcript
                     output root)))
                 :messages
                 (neomacs-auctex-latexmk-test--messages
                  messages-start root)))))
    (neomacs-auctex-latexmk-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:status exit :exit 12 :arguments ("-interaction=nonstopmode" "-file-line-error" "main.tex") :artifacts (nil nil nil nil nil) :output (:next "LatexMk" :default "LatexMk" :transcript "Running `LatexMk' on `main' with ``latexmk  -interaction=nonstopmode  -file-line-error  main.tex''\nlatexmk: applying document rules\nRun number 1 of rule 'pdflatex'\nLatexmk preamble one\nLatexmk preamble two\nLatexmk preamble three\nLatexmk preamble four\nThis is pdfTeX, Version deterministic\n! Undefined control sequence.\nl.4 \\brokencommand\nCollected error summary (may duplicate other messages):\n  pdflatex: Command for 'pdflatex' gave return code 1\nLatexmk: Errors, so I did not complete making targets\n\nTeX Output exited abnormally with code 12 at <time>") :messages ("Applying style hooks...done" "Type ‘C-c C-l’ to display results of compilation." "LatexMk errors in ‘*<sandbox>/main output*’. Use C-c ` to display."))"#
    ]];
    ParityBatchCase::value(
        "latex_failure_routes_real_error_summary_through_auctex",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn bibtex_failure_routes_bibliography_diagnostics_through_auctex() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-latexmk-bibtex-failure"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "main.tex" root))
       (arguments-file (expand-file-name "arguments.txt" root))
       (cwd-file (expand-file-name "cwd.txt" root))
       (encoding-file (expand-file-name "encoding.txt" root))
       (ready-file (expand-file-name "ready" root))
       (release-file (expand-file-name "release" root))
       (default-directory root)
       buffer process output messages-start result)
  (unwind-protect
      (progn
        (neomacs-auctex-latexmk-test--cleanup root)
        (make-directory root t)
        (neomacs-auctex-latexmk-test--write-program root 'bibtex-failure)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\n"
           "Missing citation \\cite{missing}.\n"
           "\\bibliography{refs}\n"
           "\\end{document}\n"))
        (let ((process-environment (copy-sequence process-environment))
              (exec-path (cons (expand-file-name "bin" root) exec-path)))
          (setenv "PATH"
                  (concat (expand-file-name "bin" root)
                          path-separator (getenv "PATH")))
          (setenv "NEOMACS_LATEXMK_ARGUMENTS" arguments-file)
          (setenv "NEOMACS_LATEXMK_CWD" cwd-file)
          (setenv "NEOMACS_LATEXMK_ENCODING" encoding-file)
          (setenv "NEOMACS_LATEXMK_READY" ready-file)
          (setenv "NEOMACS_LATEXMK_RELEASE" release-file)
          (setq buffer (find-file-noselect source))
          (with-current-buffer buffer
            (LaTeX-mode)
            (setq-local TeX-master t)
            (let ((TeX-command-force "LatexMk")
                  (TeX-process-asynchronous t))
              (setq messages-start
                    (with-current-buffer (messages-buffer) (point-max)))
              (call-interactively #'TeX-command-master)
              (setq process (get-process "LatexMk"))))
          (setq output (process-buffer process))
          (with-temp-file release-file (insert "release\n"))
          (neomacs-auctex-latexmk-test--wait process)
          (setq result
                (list
                 :status (process-status process)
                 :exit (process-exit-status process)
                 :arguments
                 (neomacs-auctex-latexmk-test--read-lines arguments-file)
                 :artifacts
                 (mapcar
                  (lambda (relative)
                    (file-exists-p (expand-file-name relative root)))
                  '("main.bbl" "main.blg" "main.pdf"))
                 :output
                 (with-current-buffer output
                   (list
                    :next TeX-command-next
                    :default TeX-command-default
                    :transcript
                    (neomacs-auctex-latexmk-test--output-transcript
                     output root)))
                 :messages
                 (neomacs-auctex-latexmk-test--messages
                  messages-start root)))))
    (neomacs-auctex-latexmk-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:status exit :exit 13 :arguments ("-interaction=nonstopmode" "-file-line-error" "main.tex") :artifacts (nil nil nil) :output (:next "LatexMk" :default "LatexMk" :transcript "Running `LatexMk' on `main' with ``latexmk  -interaction=nonstopmode  -file-line-error  main.tex''\nlatexmk: applying bibliography rules\nRun number 1 of rule 'bibtex main'\nRule 'bibtex main': reasons for rerun\nBibTeX setup one\nBibTeX setup two\nBibTeX setup three\nBibTeX setup four\nWarning--I didn't find a database entry for 'missing'\n(There was 1 error message)\nRule 'pdflatex': not run\nCollected error summary (may duplicate other messages):\n  bibtex main: Command for 'bibtex main' gave return code 2\nLatexmk: Errors, so I did not complete making targets\n\nTeX Output exited abnormally with code 13 at <time>") :messages ("Applying style hooks...done" "Type ‘C-c C-l’ to display results of compilation." "BibTeX finished with 1 error message. Type ‘C-c C-l’ to display output."))"#
    ]];
    ParityBatchCase::value(
        "bibtex_failure_routes_bibliography_diagnostics_through_auctex",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn recenter_visits_live_bibtex_rule_and_cleanup_removes_latexmk_sidecars() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-latexmk-recenter-clean"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "main.tex" root))
       (arguments-file (expand-file-name "arguments.txt" root))
       (cwd-file (expand-file-name "cwd.txt" root))
       (encoding-file (expand-file-name "encoding.txt" root))
       (ready-file (expand-file-name "ready" root))
       (release-file (expand-file-name "release" root))
       (default-directory root)
       buffer process output messages-start result)
  (unwind-protect
      (progn
        (neomacs-auctex-latexmk-test--cleanup root)
        (make-directory root t)
        (neomacs-auctex-latexmk-test--write-program root 'recenter)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\nLive citation \\cite{release}.\n"
           "\\bibliography{refs}\n"
           "\\end{document}\n"))
        (dolist (fixture
                 '(("main.aux" . "aux")
                   ("main.aux.bak" . "aux backup")
                   ("main.fdb_latexmk" . "database")
                   ("main.fls" . "file list")
                   ("main.pdf" . "PDF output")
                   ("notes.fls" . "unrelated")))
          (with-temp-file (expand-file-name (car fixture) root)
            (insert (cdr fixture))))
        (let ((process-environment (copy-sequence process-environment))
              (exec-path (cons (expand-file-name "bin" root) exec-path)))
          (setenv "PATH"
                  (concat (expand-file-name "bin" root)
                          path-separator (getenv "PATH")))
          (setenv "NEOMACS_LATEXMK_ARGUMENTS" arguments-file)
          (setenv "NEOMACS_LATEXMK_CWD" cwd-file)
          (setenv "NEOMACS_LATEXMK_ENCODING" encoding-file)
          (setenv "NEOMACS_LATEXMK_READY" ready-file)
          (setenv "NEOMACS_LATEXMK_RELEASE" release-file)
          (setq buffer (find-file-noselect source))
          (switch-to-buffer buffer)
          (with-current-buffer buffer
            (LaTeX-mode)
            (setq-local TeX-master t)
            (let ((TeX-command-force "LatexMk")
                  (TeX-process-asynchronous t))
              (setq messages-start
                    (with-current-buffer (messages-buffer) (point-max)))
              (call-interactively #'TeX-command-master)
              (setq process (get-process "LatexMk"))))
          (setq output (process-buffer process))
          (let ((attempts 0))
            (while (and (< attempts 200)
                        (not (and (file-exists-p ready-file)
                                  (with-current-buffer output
                                    (string-match-p
                                     "Rule 'pdflatex': pending"
                                     (buffer-string))))))
              (accept-process-output process 0.05)
              (setq attempts (1+ attempts)))
            (unless (file-exists-p ready-file)
              (error "latexmk recenter fixture never became ready")))
          (let ((before (current-buffer)) recenter-value point-line)
            (setq recenter-value (TeX-recenter-output-buffer 3))
            (setq point-line
                  (with-current-buffer output
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position))))
            (with-temp-file release-file (insert "release\n"))
            (neomacs-auctex-latexmk-test--wait process)
            (with-current-buffer buffer
              (let ((TeX-clean-confirm nil))
                (TeX-clean nil)))
            (let ((after-intermediate
                   (sort (directory-files root nil "^[^.].*" t)
                         #'string-lessp)))
              (dolist (fixture
                       '(("main.aux.bak" . "aux backup again")
                         ("main.fdb_latexmk" . "database again")
                         ("main.fls" . "file list again")))
                (with-temp-file (expand-file-name (car fixture) root)
                  (insert (cdr fixture))))
              (with-current-buffer buffer
                (let ((TeX-clean-confirm nil))
                  (TeX-clean t)))
              (setq result
                    (list
                     :recenter
                     (list
                      :return
                      (and (bufferp recenter-value)
                           (buffer-name recenter-value))
                      :before (buffer-name before)
                      :after (buffer-name (current-buffer))
                      :point-line point-line
                      :output-live (buffer-live-p output))
                     :process
                     (list (process-status process) (process-exit-status process))
                     :transcript
                     (neomacs-auctex-latexmk-test--output-transcript
                      output root)
                     :after-intermediate after-intermediate
                     :after-output
                     (sort (directory-files root nil "^[^.].*" t)
                           #'string-lessp)
                     :messages
                     (neomacs-auctex-latexmk-test--messages
                      messages-start root)))))))
    (when (and process (process-live-p process))
      (ignore-errors (delete-process process)))
    (neomacs-auctex-latexmk-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:recenter (:return "main.tex" :before "main.tex" :after "main.tex" :point-line "Run number 1 of rule 'bibtex main'" :output-live t) :process (exit 0) :transcript "Running `LatexMk' on `main' with ``latexmk  -interaction=nonstopmode  -file-line-error  main.tex''\nlatexmk: live bibliography build\nRun number 1 of rule 'bibtex main'\nRule 'bibtex main': live diagnostics\nBibTeX diagnostic one\nBibTeX diagnostic two\nRule 'pdflatex': pending\nLatexmk: Nothing to do after inspection\n\nTeX Output finished at <time>" :after-intermediate ("arguments.txt" "bin" "cwd.txt" "encoding.txt" "main.pdf" "main.tex" "notes.fls" "ready" "release") :after-output ("arguments.txt" "bin" "cwd.txt" "encoding.txt" "main.tex" "notes.fls" "ready" "release") :messages ("Applying style hooks...done" "Type ‘C-c C-l’ to display results of compilation." "LatexMk: nothing to do"))"#
    ]];
    ParityBatchCase::value(
        "recenter_visits_live_bibtex_rule_and_cleanup_removes_latexmk_sidecars",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        setup_configures_new_auctex_buffers_and_expands_document_commands(),
        interactive_latexmk_build_passes_encoding_and_creates_real_outputs(),
        up_to_date_document_reports_nothing_to_do_without_rewriting_output(),
        latex_failure_routes_real_error_summary_through_auctex(),
        bibtex_failure_routes_bibliography_diagnostics_through_auctex(),
        recenter_visits_live_bibtex_rule_and_cleanup_removes_latexmk_sidecars(),
    ]
}
