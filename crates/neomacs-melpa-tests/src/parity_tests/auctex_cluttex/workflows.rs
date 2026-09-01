use expect_test::expect;

use super::ParityBatchCase;

fn mode_is_buffer_local_idempotent_and_preserves_later_user_choice() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-cluttex-mode-lifecycle"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (first-file (expand-file-name "paper.tex" root))
       (second-file (expand-file-name "appendix.tex" root))
       first second result)
  (unwind-protect
      (progn
        (neomacs-auctex-cluttex-test--cleanup root)
        (make-directory root t)
        (with-temp-file first-file
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\nRelease Ω report.\n\\end{document}\n"))
        (with-temp-file second-file
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\nAppendix.\n\\end{document}\n"))
        (setq first (find-file-noselect first-file)
              second (find-file-noselect second-file))
        (with-current-buffer first
          (LaTeX-mode)
          (let ((original-default TeX-command-default)
                (original-commands (copy-tree TeX-command-list))
                (original-expansions (copy-tree TeX-expand-list-builtin)))
            (auctex-cluttex-mode 1)
            (let ((enabled
                   (list
                    :mode auctex-cluttex-mode
                    :local (local-variable-p 'auctex-cluttex-mode)
                    :default TeX-command-default
                    :command-count
                    (cl-count "ClutTeX" (neomacs-auctex-cluttex-test--command-names)
                              :test #'equal)
                    :command-neighbors
                    (let* ((names (neomacs-auctex-cluttex-test--command-names))
                           (position (cl-position "ClutTeX" names :test #'equal)))
                      (list (nth (1- position) names)
                            (nth position names)
                            (nth (1+ position) names)))
                    :command (copy-tree (assoc "ClutTeX" TeX-command-list))
                    :expansion-prefix
                    (cl-subseq
                     (neomacs-auctex-cluttex-test--expansion-keys) 0 5)
                    :content
                    (buffer-substring-no-properties (point-min) (point-max)))))
              (auctex-cluttex-mode 1)
              (let ((reenabled
                     (list
                      :mode auctex-cluttex-mode
                      :default TeX-command-default
                      :command-count
                      (cl-count "ClutTeX"
                                (neomacs-auctex-cluttex-test--command-names)
                                :test #'equal)
                      :engine-count
                      (cl-count "%(cluttexengine)"
                                (neomacs-auctex-cluttex-test--expansion-keys)
                                :test #'equal)
                      :bib-count
                      (cl-count "%(cluttexbib)"
                                (neomacs-auctex-cluttex-test--expansion-keys)
                                :test #'equal)
                      :index-count
                      (cl-count "%(cluttexindex)"
                                (neomacs-auctex-cluttex-test--expansion-keys)
                                :test #'equal))))
                (with-current-buffer second
                  (LaTeX-mode)
                  (setq result
                        (list
                         :enabled enabled
                         :reenabled reenabled
                         :second
                         (list
                          :mode auctex-cluttex-mode
                          :local (local-variable-p 'auctex-cluttex-mode)
                          :default TeX-command-default
                          :command-count
                          (cl-count
                           "ClutTeX"
                           (neomacs-auctex-cluttex-test--command-names)
                           :test #'equal)))))
                (with-current-buffer first
                  (auctex-cluttex-mode 0)
                  (setq result
                        (append
                         result
                         (list
                          :disabled
                          (list
                           :mode auctex-cluttex-mode
                           :default TeX-command-default
                           :commands-restored
                           (equal TeX-command-list original-commands)
                           :expansions-restored
                           (equal TeX-expand-list-builtin original-expansions)))))
                  (auctex-cluttex-mode 1)
                  (setq TeX-command-default "View")
                  (auctex-cluttex-mode 0)
                  (setq result
                        (append
                         result
                         (list
                          :later-user-choice
                          (list
                           :mode auctex-cluttex-mode
                           :default TeX-command-default
                           :command-count
                           (cl-count
                            "ClutTeX"
                            (neomacs-auctex-cluttex-test--command-names)
                            :test #'equal)
                           :original-default original-default))))))))))
    (neomacs-auctex-cluttex-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r####"OK (:enabled (:mode t :local t :default "ClutTeX" :command-count 1 :command-neighbors ("Clean All" "ClutTeX" "Other") :command ("ClutTeX" "cluttex -e %(cluttexengine) %(cluttexbib) %(cluttexindex) %S %t" auctex-cluttex--TeX-run-ClutTeX nil (plain-tex-mode latex-mode) :help "Run ClutTeX") :expansion-prefix ("%(cluttexengine)" "%(cluttexbib)" "%(cluttexindex)" "%q" "%V") :content "\\documentclass{article}\n\\begin{document}\nRelease Ω report.\n\\end{document}\n") :reenabled (:mode t :default "ClutTeX" :command-count 1 :engine-count 1 :bib-count 1 :index-count 1) :second (:mode nil :local nil :default "LaTeX" :command-count 0) :disabled (:mode nil :default "LaTeX" :commands-restored t :expansions-restored t) :later-user-choice (:mode nil :default "View" :command-count 0 :original-default "LaTeX"))"####
    ]];
    ParityBatchCase::value(
        "mode_is_buffer_local_idempotent_and_preserves_later_user_choice",
        elisp_form,
        expect,
    )
}

fn expands_real_latex_and_plain_tex_projects_for_each_supported_engine() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-cluttex-command-expansion"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (latex-file (expand-file-name "main.tex" root))
       (plain-file (expand-file-name "plain.tex" root))
       (bib-file (expand-file-name "refs.bib" root))
       (default-directory root)
       latex-buffer plain-buffer latex-state plain-state)
  (unwind-protect
      (progn
        (neomacs-auctex-cluttex-test--cleanup root)
        (make-directory root t)
        (with-temp-file latex-file
          (insert
           "\\documentclass{article}\n"
           "\\usepackage{makeidx}\n"
           "\\makeindex\n"
           "\\begin{document}\n"
           "Release Ω cites \\cite{stable}.\\index{release!stable}\n"
           "\\bibliographystyle{plain}\n"
           "\\bibliography{refs}\n"
           "\\end{document}\n"))
        (with-temp-file plain-file
          (insert "\\input plain\nRelease Ω.\n\\bye\n"))
        (with-temp-file bib-file
          (insert
           "@book{stable,\n"
           "  author = {Ada Lovelace},\n"
           "  title = {Release Engineering},\n"
           "  year = {1843}\n"
           "}\n"))
        (setq latex-buffer (find-file-noselect latex-file))
        (with-current-buffer latex-buffer
          (let ((TeX-parse-self t))
            (LaTeX-mode)
            (TeX-auto-apply))
          (auctex-cluttex-mode 1)
          (TeX-source-correlate-mode 1)
          (let ((template (nth 1 (assoc "ClutTeX" TeX-command-list))))
            (setq latex-state
                  (list
                   :mode major-mode
                   :bibliographies (copy-tree (LaTeX-bibliography-list))
                   :index-entries (copy-tree (LaTeX-index-entry-list))
                   :template template
                   :commands
                   (mapcar
                    (lambda (configuration)
                      (let ((TeX-engine (nth 0 configuration))
                            (LaTeX-using-Biber (nth 1 configuration)))
                        (list
                         TeX-engine
                         LaTeX-using-Biber
                         (TeX-command-expand template))))
                    '((default nil) (uptex nil) (ptex nil)
                      (xetex t) (luatex nil)))))))
        (setq plain-buffer (find-file-noselect plain-file))
        (with-current-buffer plain-buffer
          (plain-TeX-mode)
          (auctex-cluttex-mode 1)
          (setq plain-state
                (list
                 :mode major-mode
                 :default
                 (let ((TeX-engine 'default))
                   (TeX-command-expand
                    (nth 1 (assoc "ClutTeX" TeX-command-list))))
                 :uptex
                 (let ((TeX-engine 'uptex))
                   (TeX-command-expand
                    (nth 1 (assoc "ClutTeX" TeX-command-list)))))))
        (list
         :latex latex-state
         :plain plain-state
         :source
         (with-current-buffer latex-buffer
           (buffer-substring-no-properties (point-min) (point-max)))))
    (when (bound-and-true-p TeX-source-correlate-mode)
      (TeX-source-correlate-mode 0))
    (neomacs-auctex-cluttex-test--cleanup root)))
"####;
    let expect = expect![[
        r####"OK (:latex (:mode LaTeX-mode :bibliographies (("refs")) :index-entries (("release!stable")) :template "cluttex -e %(cluttexengine) %(cluttexbib) %(cluttexindex) %S %t" :commands ((default nil "cluttex -e pdflatex --bibtex=bibtex --makeindex=makeindex --synctex=1 main.tex") (uptex nil "cluttex -e uplatex --bibtex=upbibtex --makeindex=upmendex --synctex=1 main.tex") (ptex nil "cluttex -e platex --bibtex=pbibtex --makeindex=mendex --synctex=1 main.tex") (xetex t "cluttex -e xelatex --biber --makeindex=upmendex --synctex=1 main.tex") (luatex nil "cluttex -e lualatex --bibtex=bibtex --makeindex=upmendex --synctex=1 main.tex"))) :plain (:mode plain-TeX-mode :default "cluttex -e pdftex   --synctex=1 plain.tex" :uptex "cluttex -e uptex   --synctex=1 plain.tex") :source "\\documentclass{article}\n\\usepackage{makeidx}\n\\makeindex\n\\begin{document}\nRelease Ω cites \\cite{stable}.\\index{release!stable}\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n")"####
    ]];
    ParityBatchCase::value(
        "expands_real_latex_and_plain_tex_projects_for_each_supported_engine",
        elisp_form,
        expect,
    )
}

fn interactive_synchronous_command_builds_a_real_document_and_colorizes_output() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-cluttex-synchronous-success"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "main.tex" root))
       (bibliography (expand-file-name "refs.bib" root))
       (arguments-file (expand-file-name "arguments.txt" root))
       (cwd-file (expand-file-name "cwd.txt" root))
       (default-directory root)
       buffer output dispatch messages-start result)
  (unwind-protect
      (progn
        (neomacs-auctex-cluttex-test--cleanup root)
        (make-directory root t)
        (neomacs-auctex-cluttex-test--write-program root 0)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\usepackage{makeidx}\n"
           "\\makeindex\n"
           "\\begin{document}\n"
           "Release Ω cites \\cite{stable}.\\index{release!stable}\n"
           "\\bibliographystyle{plain}\n"
           "\\bibliography{refs}\n"
           "\\end{document}\n"))
        (with-temp-file bibliography
          (insert
           "@book{stable, author={Ada Lovelace}, title={Release}, year={1843}}\n"))
        (let ((process-environment (copy-sequence process-environment))
              (exec-path (cons (expand-file-name "bin" root) exec-path)))
          (setenv "PATH"
                  (concat (expand-file-name "bin" root)
                          path-separator (getenv "PATH")))
          (setenv "NEOMACS_CLUTTEX_ARGUMENTS" arguments-file)
          (setenv "NEOMACS_CLUTTEX_CWD" cwd-file)
          (setq buffer (find-file-noselect source))
          (with-current-buffer buffer
            (let ((TeX-parse-self t))
              (LaTeX-mode)
              (TeX-auto-apply))
            (setq-local TeX-master t)
            (auctex-cluttex-mode 1)
            (TeX-source-correlate-mode 1)
            (setq messages-start
                  (with-current-buffer (messages-buffer) (point-max)))
            (let ((TeX-command-force "ClutTeX")
                  (TeX-process-asynchronous nil))
              (setq dispatch (call-interactively #'TeX-command-master)))
            (setq output (TeX-process-buffer "main"))
            (setq result
                  (list
                   :dispatch dispatch
                   :current
                   (list :is-output-buffer (eq (current-buffer) output)
                         :mode major-mode
                         :command-default TeX-command-default)
                   :source
                   (with-current-buffer buffer
                     (list
                      :buffer (buffer-name)
                      :mode major-mode
                      :command-default TeX-command-default
                      :bibliographies (copy-tree (LaTeX-bibliography-list))
                      :index-entries (copy-tree (LaTeX-index-entry-list))))
                   :cwd
                   (file-relative-name
                    (string-trim
                     (with-temp-buffer
                       (insert-file-contents cwd-file)
                       (buffer-string)))
                    root)
                   :arguments
                   (with-temp-buffer
                     (insert-file-contents arguments-file)
                     (split-string (buffer-string) "\n" t))
                   :artifacts
                   (list
                    (file-exists-p (expand-file-name "main.pdf" root))
                    (file-exists-p (expand-file-name "main.synctex.gz" root))
                    (file-exists-p (expand-file-name "main.log" root)))
                   :output
                   (list
                    :running
                    (neomacs-auctex-cluttex-test--output-line-state
                     output "Running `ClutTeX'")
                    :colored
                    (neomacs-auctex-cluttex-test--output-line-properties
                     output "compiled release Ω")
                    :plain
                    (neomacs-auctex-cluttex-test--output-line-state
                     output "artifact ready")
                    :raw-escape
                    (with-current-buffer output
                      (and (string-match-p
                            (regexp-quote "\e[")
                            (buffer-substring-no-properties
                             (point-min) (point-max)))
                           t))
                    :extension
                    (with-current-buffer output TeX-output-extension)
                    :next
                    (with-current-buffer output TeX-command-next)
                    :mode-line-process
                    (with-current-buffer output mode-line-process)
                    :package-sentinel
                    (with-current-buffer output TeX-sentinel-function))
                   :messages
                   (neomacs-auctex-cluttex-test--messages
                    "^ClutTeX finished successfully\\.$"
                    messages-start))))))
    (when (bound-and-true-p TeX-source-correlate-mode)
      (TeX-source-correlate-mode 0))
    (neomacs-auctex-cluttex-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r####"OK (:dispatch nil :current (:is-output-buffer t :mode TeX-output-mode :command-default "ClutTeX") :source (:buffer "main.tex" :mode LaTeX-mode :command-default "ClutTeX" :bibliographies (("refs")) :index-entries (("release!stable"))) :cwd "." :arguments ("-e" "pdflatex" "--bibtex=bibtex" "--makeindex=makeindex" "--synctex=1" "main.tex") :artifacts (t t nil) :output (:running (:line "Running `ClutTeX' on `main' with ``cluttex -e pdflatex --bibtex=bibtex --makeindex=makeindex --synctex=1 main.tex''" :face nil) :colored (:line "ClutTeX compiled release Ω" :face nil :font-lock-face nil :overlay-faces ((:foreground "green3"))) :plain (:line "artifact ready" :face nil) :raw-escape nil :extension "pdf" :next "View" :mode-line-process ": exit" :package-sentinel auctex-cluttex--TeX-ClutTeX-sentinel) :messages ("ClutTeX finished successfully."))"####
    ]];
    ParityBatchCase::value(
        "interactive_synchronous_command_builds_a_real_document_and_colorizes_output",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn async_command_runs_a_real_local_process_and_colorizes_its_output() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-cluttex-async-success"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "main.tex" root))
       (arguments-file (expand-file-name "arguments.txt" root))
       (cwd-file (expand-file-name "cwd.txt" root))
       (default-directory root)
       buffer process output messages-start result)
  (unwind-protect
      (progn
        (neomacs-auctex-cluttex-test--cleanup root)
        (make-directory root t)
        (neomacs-auctex-cluttex-test--write-program root 0)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\nRelease Ω.\n\\end{document}\n"))
        (let ((process-environment (copy-sequence process-environment))
              (exec-path (cons (expand-file-name "bin" root) exec-path)))
          (setenv "PATH"
                  (concat (expand-file-name "bin" root)
                          path-separator (getenv "PATH")))
          (setenv "NEOMACS_CLUTTEX_ARGUMENTS" arguments-file)
          (setenv "NEOMACS_CLUTTEX_CWD" cwd-file)
          (setq buffer (find-file-noselect source))
          (setq messages-start
                (with-current-buffer (messages-buffer) (point-max)))
          (with-current-buffer buffer
            (let ((TeX-process-asynchronous t))
              (LaTeX-mode)
              (auctex-cluttex-mode 1)
              (TeX-source-correlate-mode 1)
              (setq process (TeX-command "ClutTeX" #'TeX-master-file 0))))
          (setq output (process-buffer process))
          (let ((initial
                 (list
                  :process (processp process)
                  :live (process-live-p process)
                  :filter (process-filter process)
                  :sentinel (process-sentinel process)
                  :package-sentinel
                  (with-current-buffer output TeX-sentinel-function))))
            (let ((attempts 0))
              (while (and (< attempts 100) (process-live-p process))
                (accept-process-output process 0.1)
                (setq attempts (1+ attempts)))
              (accept-process-output process 0.1))
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
                   (with-temp-buffer
                     (insert-file-contents arguments-file)
                     (split-string (buffer-string) "\n" t))
                   :artifacts
                   (list
                    (file-exists-p (expand-file-name "main.pdf" root))
                    (file-exists-p (expand-file-name "main.synctex.gz" root))
                    (file-exists-p (expand-file-name "main.log" root)))
                   :output
                   (list
                    :running
                    (neomacs-auctex-cluttex-test--output-line-state
                     output "Running `ClutTeX'")
                    :colored
                    (neomacs-auctex-cluttex-test--output-line-state
                     output "compiled release Ω")
                    :plain
                    (neomacs-auctex-cluttex-test--output-line-state
                     output "artifact ready")
                    :raw-escape
                    (with-current-buffer output
                      (and (string-match-p
                            (regexp-quote "\e[")
                            (buffer-substring-no-properties
                             (point-min) (point-max)))
                           t))
                    :extension
                    (with-current-buffer output TeX-output-extension)
                    :next
                    (with-current-buffer output TeX-command-next))
                   :messages
                   (neomacs-auctex-cluttex-test--messages
                    "^ClutTeX finished successfully\\.$"
                    messages-start))))))
    (when (bound-and-true-p TeX-source-correlate-mode)
      (TeX-source-correlate-mode 0))
    (neomacs-auctex-cluttex-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r####"OK (:initial (:process t :live (run open listen connect stop) :filter auctex-cluttex--TeX-ClutTeX-filter :sentinel TeX-command-sentinel :package-sentinel auctex-cluttex--TeX-ClutTeX-sentinel) :final (:status exit :exit 0 :filter auctex-cluttex--TeX-ClutTeX-filter :sentinel TeX-command-sentinel) :cwd "." :arguments ("-e" "pdflatex" "--synctex=1" "main.tex") :artifacts (t t nil) :output (:running (:line "Running `ClutTeX' on `main' with ``cluttex -e pdflatex   --synctex=1 main.tex''" :face nil) :colored (:line "ClutTeX compiled release Ω" :face (:foreground "green3")) :plain (:line "artifact ready" :face nil) :raw-escape nil :extension "pdf" :next "View") :messages ("ClutTeX finished successfully."))"####
    ]];
    ParityBatchCase::value(
        "async_command_runs_a_real_local_process_and_colorizes_its_output",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn async_failure_preserves_artifact_state_and_reports_the_public_error() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-cluttex-async-failure"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "main.tex" root))
       (arguments-file (expand-file-name "arguments.txt" root))
       (cwd-file (expand-file-name "cwd.txt" root))
       (default-directory root)
       buffer process output messages-start result)
  (unwind-protect
      (progn
        (neomacs-auctex-cluttex-test--cleanup root)
        (make-directory root t)
        (neomacs-auctex-cluttex-test--write-program root 7)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\nBroken \\cite{missing}.\n\\end{document}\n"))
        (let ((process-environment (copy-sequence process-environment))
              (exec-path (cons (expand-file-name "bin" root) exec-path)))
          (setenv "PATH"
                  (concat (expand-file-name "bin" root)
                          path-separator (getenv "PATH")))
          (setenv "NEOMACS_CLUTTEX_ARGUMENTS" arguments-file)
          (setenv "NEOMACS_CLUTTEX_CWD" cwd-file)
          (setq buffer (find-file-noselect source))
          (setq messages-start
                (with-current-buffer (messages-buffer) (point-max)))
          (with-current-buffer buffer
            (let ((TeX-process-asynchronous t))
              (LaTeX-mode)
              (auctex-cluttex-mode 1)
              (setq process (TeX-command "ClutTeX" #'TeX-master-file 0))))
          (setq output (process-buffer process))
          (let ((attempts 0))
            (while (and (< attempts 100) (process-live-p process))
              (accept-process-output process 0.1)
              (setq attempts (1+ attempts)))
            (accept-process-output process 0.1))
          (setq result
                (list
                 :status (process-status process)
                 :exit (process-exit-status process)
                 :cwd
                 (file-relative-name
                  (string-trim
                   (with-temp-buffer
                     (insert-file-contents cwd-file)
                     (buffer-string)))
                  root)
                 :arguments
                 (with-temp-buffer
                   (insert-file-contents arguments-file)
                   (split-string (buffer-string) "\n" t))
                 :artifacts
                 (list
                  (file-exists-p (expand-file-name "main.pdf" root))
                  (file-exists-p (expand-file-name "main.synctex.gz" root))
                  (file-exists-p (expand-file-name "main.log" root)))
                 :colored
                 (neomacs-auctex-cluttex-test--output-line-state
                  output "rejected broken citation Ω")
                 :abnormal
                 (with-current-buffer output
                   (save-excursion
                     (goto-char (point-min))
                     (search-forward "TeX Output exited abnormally")
                     (replace-regexp-in-string
                      " at .*$" " at <time>"
                      (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position)))))
                 :raw-escape
                 (with-current-buffer output
                   (and (string-match-p
                         (regexp-quote "\e[")
                         (buffer-substring-no-properties
                          (point-min) (point-max)))
                        t))
                 :extension (with-current-buffer output TeX-output-extension)
                 :next (with-current-buffer output TeX-command-next)
                 :still-compiling (and (memq process compilation-in-progress) t)
                 :messages
                 (neomacs-auctex-cluttex-test--messages
                  "^ClutTeX failed\\..*$"
                  messages-start)))))
    (neomacs-auctex-cluttex-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r####"OK (:status exit :exit 7 :cwd "." :arguments ("-e" "pdflatex" "main.tex") :artifacts (nil nil nil) :colored (:line "ClutTeX rejected broken citation Ω" :face (:foreground "red3")) :abnormal "TeX Output exited abnormally with code 7 at <time>" :raw-escape nil :extension nil :next "View" :still-compiling nil :messages ("ClutTeX failed.  Type ‘C-c C-l’ to display output."))"####
    ]];
    ParityBatchCase::value(
        "async_failure_preserves_artifact_state_and_reports_the_public_error",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn command_default_advice_suppresses_real_bibtex_and_biber_followups() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-cluttex-command-default"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "main.tex" root))
       (bibliography (expand-file-name "refs.bib" root))
       (bbl (expand-file-name "main.bbl" root))
       (pdf (expand-file-name "main.pdf" root))
       (default-directory root)
       buffer result)
  (unwind-protect
      (progn
        (neomacs-auctex-cluttex-test--cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\n"
           "Release Ω cites \\cite{stable}.\n"
           "\\bibliographystyle{plain}\n"
           "\\bibliography{refs}\n"
           "\\end{document}\n"))
        (with-temp-file bibliography
          (insert
           "@book{stable, author={Ada Lovelace}, title={Release}, year={1843}}\n"))
        (with-temp-file bbl (insert "\\begin{thebibliography}{1}\n\\end{thebibliography}\n"))
        (with-temp-file pdf (insert "%PDF deterministic fixture\n"))
        (set-file-times source (seconds-to-time 100))
        (set-file-times bbl (seconds-to-time 200))
        (set-file-times bibliography (seconds-to-time 300))
        (set-file-times pdf (seconds-to-time 400))
        (setq buffer (find-file-noselect source))
        (with-current-buffer buffer
          (let ((TeX-parse-self t))
            (LaTeX-mode)
            (TeX-auto-apply))
          (setq TeX-PDF-mode t
                TeX-output-extension "pdf")
          (let ((bibtex-disabled (TeX-command-default #'TeX-master-file)))
            (auctex-cluttex-mode 1)
            (let ((bibtex-enabled (TeX-command-default #'TeX-master-file)))
              (auctex-cluttex-mode 0)
              (let ((bibtex-restored (TeX-command-default #'TeX-master-file)))
                (setq LaTeX-using-Biber t)
                (let ((biber-disabled (TeX-command-default #'TeX-master-file)))
                  (auctex-cluttex-mode 1)
                  (let ((biber-enabled (TeX-command-default #'TeX-master-file)))
                    (setq result
                          (list
                           :mode major-mode
                           :bibliographies
                           (copy-tree (LaTeX-bibliography-list))
                           :times
                           (mapcar
                            (lambda (file)
                              (float-time
                               (file-attribute-modification-time
                                (file-attributes file))))
                            (list source bbl bibliography pdf))
                           :bibtex
                           (list bibtex-disabled bibtex-enabled bibtex-restored)
                           :biber (list biber-disabled biber-enabled)
                           :mode-enabled auctex-cluttex-mode
                           :default TeX-command-default
                           :source
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))))))))
    (neomacs-auctex-cluttex-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r####"OK (:mode LaTeX-mode :bibliographies (("refs")) :times (100.0 200.0 300.0 400.0) :bibtex ("BibTeX" "View" "BibTeX") :biber ("Biber" "View") :mode-enabled t :default "ClutTeX" :source "\\documentclass{article}\n\\begin{document}\nRelease Ω cites \\cite{stable}.\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n")"####
    ]];
    ParityBatchCase::value(
        "command_default_advice_suppresses_real_bibtex_and_biber_followups",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_is_buffer_local_idempotent_and_preserves_later_user_choice(),
        expands_real_latex_and_plain_tex_projects_for_each_supported_engine(),
        interactive_synchronous_command_builds_a_real_document_and_colorizes_output(),
        async_command_runs_a_real_local_process_and_colorizes_its_output(),
        async_failure_preserves_artifact_state_and_reports_the_public_error(),
        command_default_advice_suppresses_real_bibtex_and_biber_followups(),
    ]
}
