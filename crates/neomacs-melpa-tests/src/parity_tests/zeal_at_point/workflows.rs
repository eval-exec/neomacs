use expect_test::expect;

use super::ParityBatchCase;

fn reports_the_modern_version_and_docset_detected_during_package_load() -> ParityBatchCase {
    let elisp_form = r####"
(let ((elisp-docset
       (cdr (assq 'emacs-lisp-mode zeal-at-point-mode-alist))))
  (list :version zeal-at-point-zeal-version
        :emacs-lisp-docset elisp-docset
        :known (and (member elisp-docset zeal-at-point-docsets) t)
        :executable (and (executable-find "zeal") t)
        :feature (featurep 'zeal-at-point)))
"####;
    let expect = expect![[
        r##"OK (:version "0.6.1" :emacs-lisp-docset "elisp" :known t :executable t :feature t)"##
    ]];
    ParityBatchCase::value(
        "reports_the_modern_version_and_docset_detected_during_package_load",
        elisp_form,
        expect,
    )
}

fn searches_a_symbol_and_a_unicode_region_with_modern_docsets() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "zeal-at-point-real-process"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (bin (expand-file-name "bin" sandbox))
        (program (expand-file-name "zeal" bin))
        (log (expand-file-name "argv.log" sandbox))
        (exec-path (cons bin exec-path))
        (zeal-at-point-zeal-version "0.6.1")
        (zeal-at-point-mode-alist
         '((emacs-lisp-mode . "elisp")
           (python-mode . ("python" "django"))))
        prompts symbol-state region-state result)
   (unwind-protect
       (progn
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory bin t)
         (with-temp-file program
           (insert
            "#!/bin/sh\n"
            "printf 'argc=%s\\n' \"$#\" >> \"$NEOMACS_TEST_SANDBOX_ROOT/zeal-at-point-real-process/argv.log\"\n"
            "for arg in \"$@\"; do printf '<%s>\\n' \"$arg\" >> \"$NEOMACS_TEST_SANDBOX_ROOT/zeal-at-point-real-process/argv.log\"; done\n"))
         (set-file-modes program #o755)
         (cl-letf (((symbol-function 'read-string)
                    (lambda (&rest args)
                      (push args prompts)
                      "unexpected prompt")))
           (with-temp-buffer
             (emacs-lisp-mode)
             (insert "(mapcar #'string-trim records)")
             (goto-char (point-min))
             (search-forward "string-trim")
             (backward-char 2)
             (let* ((before (list (buffer-string) (point) mark-active))
                    (process (call-interactively #'zeal-at-point)))
               (setq symbol-state
                     (list
                      :process
                      (neomacs-melpa-zeal-at-point--wait-process process)
                      :before before
                      :after
                      (list (buffer-string) (point) mark-active)))))
           (with-temp-buffer
             (setq major-mode 'python-mode)
             (insert "result = pathlib.Path / λ & cache\n")
             (goto-char (point-min))
             (search-forward "pathlib")
             (set-mark (match-beginning 0))
             (search-forward "cache")
             (setq mark-active t)
             (let* ((selected
                     (buffer-substring (region-beginning) (region-end)))
                    (before
                     (list (buffer-string) (point) (mark) mark-active))
                    (process (call-interactively #'zeal-at-point)))
               (setq region-state
                     (list
                      :selected selected
                      :process
                      (neomacs-melpa-zeal-at-point--wait-process process)
                      :before before
                      :after
                      (list (buffer-string) (point) (mark) mark-active))))))
         (setq result
               (list
                :argv
                (with-temp-buffer
                  (insert-file-contents log)
                  (buffer-string))
                :prompts (nreverse prompts)
                :symbol symbol-state
                :region region-state))
         result)
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r##"OK (:argv "argc=1\n<dash-plugin://keys=elisp&query=string-trim>\nargc=1\n<dash-plugin://keys=python,django&query=pathlib.Path / λ & cache>\n" :prompts nil :symbol (:process (exit 0) :before ("(mapcar #'string-trim records)" 20 nil) :after ("(mapcar #'string-trim records)" 20 nil)) :region (:selected "pathlib.Path / λ & cache" :process (exit 0) :before ("result = pathlib.Path / λ & cache\n" 34 10 t) :after ("result = pathlib.Path / λ & cache\n" 34 10 t)))"##
    ]];
    ParityBatchCase::value(
        "searches_a_symbol_and_a_unicode_region_with_modern_docsets",
        elisp_form,
        expect,
    )
}

fn sets_a_buffer_local_docset_then_edits_the_prefilled_query() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((zeal-at-point-zeal-version "0.6.1")
       (zeal-at-point-mode-alist
        '((rust-mode . "rust")
          (go-mode . "go")))
       (zeal-at-point-docsets '("rust" "go" "kotlin"))
       (zeal-at-point-docset nil)
       (zeal-at-point--docset-history '("python"))
       completion-call read-call start-call state sibling-state
       (original-space-binding
        (lookup-key minibuffer-local-completion-map (kbd "SPC"))))
   (cl-letf (((symbol-function 'completing-read)
              (lambda (prompt collection predicate require-match
                       initial-input history default &optional inherit)
                (setq completion-call
                      (list :prompt prompt
                            :collection collection
                            :predicate predicate
                            :require-match require-match
                            :initial-input initial-input
                            :history history
                            :default default
                            :inherit inherit
                            :space-binding
                            (lookup-key minibuffer-local-completion-map
                                        (kbd "SPC"))))
                "rust,tokio"))
             ((symbol-function 'read-string)
              (lambda (prompt &optional initial-input &rest _)
                (setq read-call (list prompt initial-input))
                "dash-plugin://keys=rust,tokio&query=Result.map_err"))
             ((symbol-function 'executable-find)
              (lambda (program)
                (and (string= program "zeal") "/opt/zeal/bin/zeal")))
             ((symbol-function 'start-process)
              (lambda (name buffer program &rest args)
                (setq start-call
                      (append (list name buffer program) args))
                'zeal-edited-query)))
     (with-temp-buffer
       (setq major-mode 'rust-mode)
       (insert "let result = Result.map_err(handle_error);")
       (goto-char (point-min))
       (let ((case-fold-search nil))
         (search-forward "Result"))
       (backward-char 2)
       (call-interactively #'zeal-at-point-set-docset)
       (let ((before (list (buffer-string) (point))))
         (setq state
               (list
                :return
                (let ((current-prefix-arg '(4)))
                  (call-interactively #'zeal-at-point))
                :docset zeal-at-point-docset
                :local (local-variable-p 'zeal-at-point-docset)
                :before before
                :after (list (buffer-string) (point))))))
     (with-temp-buffer
       (setq major-mode 'rust-mode)
       (setq sibling-state
             (list zeal-at-point-docset
                   (local-variable-p 'zeal-at-point-docset)
                   (zeal-at-point-get-docset))))
     (list :completion completion-call
           :configured-docsets zeal-at-point-docsets
           :read read-call
           :start start-call
           :state state
           :sibling sibling-state
           :default (default-value 'zeal-at-point-docset)
           :space-binding-before original-space-binding
           :space-binding-after
           (lookup-key minibuffer-local-completion-map (kbd "SPC"))))))
"####;
    let expect = expect![[
        r##"OK (:completion (:prompt "Zeal docset[Default: rust]: " :collection ("rust" "go") :predicate nil :require-match nil :initial-input nil :history zeal-at-point--docset-history :default "rust" :inherit nil :space-binding nil) :configured-docsets ("rust" "go" "kotlin") :read ("Zeal search: " "dash-plugin://keys=rust,tokio&query=Result") :start ("Zeal" nil "zeal" "dash-plugin://keys=rust,tokio&query=Result.map_err") :state (:return zeal-edited-query :docset ("rust" "tokio") :local t :before ("let result = Result.map_err(handle_error);" 18) :after ("let result = Result.map_err(handle_error);" 18)) :sibling (nil nil "rust") :default nil :space-binding-before minibuffer-complete-word :space-binding-after minibuffer-complete-word)"##
    ]];
    ParityBatchCase::value(
        "sets_a_buffer_local_docset_then_edits_the_prefilled_query",
        elisp_form,
        expect,
    )
}

fn selects_scalar_empty_and_list_docsets_from_all_prompt_shapes() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((zeal-at-point-mode-alist
        '((rust-mode . "rust")
          (python-mode . ("python" "django"))))
       (zeal-at-point-docsets '("rust" "python" "django" "kotlin"))
       (zeal-at-point-docset nil)
       (answers '("kotlin" "" "python,django"))
       calls states)
   (cl-letf (((symbol-function 'completing-read)
              (lambda (prompt collection predicate require-match
                       initial-input history default &optional inherit)
                (push
                 (list :prompt prompt
                       :collection collection
                       :predicate predicate
                       :require-match require-match
                       :initial-input initial-input
                       :history history
                       :default default
                       :inherit inherit
                       :space-binding
                       (lookup-key minibuffer-local-completion-map
                                   (kbd "SPC")))
                 calls)
                (pop answers))))
     (with-temp-buffer
       (setq major-mode 'fundamental-mode)
       (call-interactively #'zeal-at-point-set-docset)
       (push
        (list :scenario 'unmatched-scalar
              :docset zeal-at-point-docset
              :resolved (zeal-at-point-get-docset)
              :local (local-variable-p 'zeal-at-point-docset))
        states))
     (with-temp-buffer
       (setq major-mode 'fundamental-mode)
       (call-interactively #'zeal-at-point-set-docset)
       (push
        (list :scenario 'unmatched-empty
              :docset zeal-at-point-docset
              :resolved (zeal-at-point-get-docset)
              :local (local-variable-p 'zeal-at-point-docset))
        states))
     (with-temp-buffer
       (setq major-mode 'python-mode)
       (call-interactively #'zeal-at-point-set-docset)
       (push
        (list :scenario 'list-default-and-result
              :docset zeal-at-point-docset
              :resolved (zeal-at-point-get-docset)
              :local (local-variable-p 'zeal-at-point-docset))
        states))
     (list :calls (nreverse calls)
           :states (nreverse states)
           :configured-docsets zeal-at-point-docsets
           :answers-left answers))))
"####;
    let expect = expect![[
        r##"OK (:calls ((:prompt "Zeal docset: " :collection ("rust" #1=("python" "django")) :predicate nil :require-match nil :initial-input nil :history zeal-at-point--docset-history :default nil :inherit nil :space-binding nil) (:prompt "Zeal docset: " :collection ("rust" #1#) :predicate nil :require-match nil :initial-input nil :history zeal-at-point--docset-history :default nil :inherit nil :space-binding nil) (:prompt "Zeal docset[Default: (python django)]: " :collection ("rust" #1#) :predicate nil :require-match nil :initial-input nil :history zeal-at-point--docset-history :default #1# :inherit nil :space-binding nil)) :states ((:scenario unmatched-scalar :docset "kotlin" :resolved "kotlin" :local t) (:scenario unmatched-empty :docset "" :resolved "" :local t) (:scenario list-default-and-result :docset #2=("python" "django") :resolved #2# :local t)) :configured-docsets ("rust" "python" "django" "kotlin") :answers-left nil)"##
    ]];
    ParityBatchCase::value(
        "selects_scalar_empty_and_list_docsets_from_all_prompt_shapes",
        elisp_form,
        expect,
    )
}

fn accepts_an_empty_minibuffer_as_the_first_list_valued_default() -> ParityBatchCase {
    let elisp_form = r####"
(let ((zeal-at-point-mode-alist
       '((python-mode . ("python" "django"))))
      (zeal-at-point-docset nil)
      minibuffer-state result)
  (cl-letf (((symbol-function 'read-from-minibuffer)
             (lambda (prompt &optional initial keymap read history
                             default inherit)
               (setq minibuffer-state
                     (list :prompt prompt
                           :initial initial
                           :read read
                           :history history
                           :default default
                           :inherit inherit
                           :space-binding
                           (lookup-key keymap (kbd "SPC"))))
               "")))
    (with-temp-buffer
      (setq major-mode 'python-mode)
      (call-interactively #'zeal-at-point-set-docset)
      (setq result
            (list :minibuffer minibuffer-state
                  :docset zeal-at-point-docset
                  :resolved (zeal-at-point-get-docset)
                  :local (local-variable-p 'zeal-at-point-docset)))
      result)))
"####;
    let expect = expect![[
        r##"OK (:minibuffer (:prompt "Zeal docset[Default: (python django)]: " :initial nil :read nil :history zeal-at-point--docset-history :default ("python" "django") :inherit nil :space-binding nil) :docset "python" :resolved "python" :local t)"##
    ]];
    ParityBatchCase::value(
        "accepts_an_empty_minibuffer_as_the_first_list_valued_default",
        elisp_form,
        expect,
    )
}

fn launches_queries_across_supported_zeal_cli_generations() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((zeal-at-point-mode-alist '((c++-mode . "cpp")))
       versions prompts starts returns)
   (cl-letf (((symbol-function 'read-string)
              (lambda (prompt &optional initial-input &rest _)
                (push (list zeal-at-point-zeal-version prompt initial-input)
                      prompts)
                (concat initial-input "std::vector<T>")))
             ((symbol-function 'executable-find)
              (lambda (program)
                (and (string= program "zeal") "/opt/zeal/bin/zeal")))
             ((symbol-function 'start-process)
              (lambda (name buffer program &rest args)
                (push
                 (cons zeal-at-point-zeal-version
                       (append (list name buffer program) args))
                 starts)
                (intern (format "zeal-%s" zeal-at-point-zeal-version)))))
     (with-temp-buffer
       (setq major-mode 'c++-mode)
       (dolist (version '("0.1.9" "0.2.0" "0.2.1" "0.6.1"))
         (setq zeal-at-point-zeal-version version)
         (push
          (list
           version
           (let ((current-prefix-arg '(4)))
             (call-interactively #'zeal-at-point-search)))
          returns)))
     (list :prompts (nreverse prompts)
           :starts (nreverse starts)
           :returns (nreverse returns)))))
"####;
    let expect = expect![[
        r##"OK (:prompts (("0.1.9" "Zeal search: " "cpp:") ("0.2.0" "Zeal search: " "cpp:") ("0.2.1" "Zeal search: " "dash-plugin://keys=cpp&query=") ("0.6.1" "Zeal search: " "dash-plugin://keys=cpp&query=")) :starts (("0.1.9" "Zeal" nil "zeal" "--query" "cpp:std::vector<T>") ("0.2.0" "Zeal" nil "zeal" "--query" "cpp:std::vector<T>") ("0.2.1" "Zeal" nil "zeal" "dash-plugin://keys=cpp&query=std::vector<T>") ("0.6.1" "Zeal" nil "zeal" "dash-plugin://keys=cpp&query=std::vector<T>")) :returns (("0.1.9" zeal-0.1.9) ("0.2.0" zeal-0.2.0) ("0.2.1" zeal-0.2.1) ("0.6.1" zeal-0.6.1)))"##
    ]];
    ParityBatchCase::value(
        "launches_queries_across_supported_zeal_cli_generations",
        elisp_form,
        expect,
    )
}

fn prompts_for_a_query_without_a_symbol_or_matching_mode() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let ((zeal-at-point-zeal-version "0.6.1")
       (zeal-at-point-mode-alist '((rust-mode . "rust")))
       prompt-call start-call)
   (cl-letf (((symbol-function 'read-string)
              (lambda (prompt &optional initial-input &rest _)
                (setq prompt-call (list prompt initial-input))
                "dash-plugin://keys=&query=C%2B%2B λ & ownership"))
             ((symbol-function 'executable-find)
              (lambda (program)
                (and (string= program "zeal") "/opt/zeal/bin/zeal")))
             ((symbol-function 'start-process)
              (lambda (name buffer program &rest args)
                (setq start-call
                      (append (list name buffer program) args))
                'zeal-fallback-query)))
     (with-temp-buffer
       (setq major-mode 'fundamental-mode)
       (insert "   \n")
       (goto-char 2)
       (let ((before (list (buffer-string) (point))))
         (list :return (call-interactively #'zeal-at-point)
               :prompt prompt-call
               :start start-call
               :docset (zeal-at-point-get-docset)
               :before before
               :after (list (buffer-string) (point))))))))
"####;
    let expect = expect![[
        r##"OK (:return zeal-fallback-query :prompt ("Zeal search: " "dash-plugin://keys=&query=nil") :start ("Zeal" nil "zeal" "dash-plugin://keys=&query=C%2B%2B λ & ownership") :docset nil :before ("   \n" 2) :after ("   \n" 2))"##
    ]];
    ParityBatchCase::value(
        "prompts_for_a_query_without_a_symbol_or_matching_mode",
        elisp_form,
        expect,
    )
}

fn reports_a_disappeared_executable_and_an_unversioned_install() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let (messages starts prompts disappeared unversioned)
   (cl-letf (((symbol-function 'read-string)
              (lambda (prompt &optional initial-input &rest _)
                (push (list prompt initial-input) prompts)
                (concat initial-input "offline API guide")))
             ((symbol-function 'executable-find) (lambda (&rest _) nil))
             ((symbol-function 'start-process)
              (lambda (&rest args)
                (push args starts)
                'unexpected-process))
             ((symbol-function 'message)
              (lambda (format-string &rest args)
                (let ((rendered (apply #'format format-string args)))
                  (push rendered messages)
                  rendered))))
     (setq disappeared
           (let ((zeal-at-point-zeal-version "0.6.1")
                 (zeal-at-point-mode-alist '((rust-mode . "rust"))))
             (with-temp-buffer
               (setq major-mode 'rust-mode)
               (zeal-at-point-search))))
     (setq unversioned
           (let ((zeal-at-point-zeal-version nil)
                 (zeal-at-point-mode-alist '((emacs-lisp-mode . "elisp"))))
             (with-temp-buffer
               (emacs-lisp-mode)
               (insert "mapcar")
               (goto-char 3)
               (neomacs-melpa-zeal-at-point--capture-signal
                (lambda () (zeal-at-point))))))
     (list :disappeared disappeared
           :unversioned unversioned
           :prompts (nreverse prompts)
           :messages (nreverse messages)
           :starts (nreverse starts)))))
"####;
    let expect = expect![[
        r##"OK (:disappeared "Zeal is not found. Please install it from http://zealdocs.org" :unversioned (:signal error ("Version must be a string")) :prompts (("Zeal search: " "dash-plugin://keys=rust&query=")) :messages ("Zeal is not found. Please install it from http://zealdocs.org") :starts nil)"##
    ]];
    ParityBatchCase::value(
        "reports_a_disappeared_executable_and_an_unversioned_install",
        elisp_form,
        expect,
    )
}

fn propagates_legacy_docset_and_process_start_failures() -> ParityBatchCase {
    let elisp_form = r####"
(save-match-data
 (let* ((sandbox
         (expand-file-name
          "zeal-at-point-disappeared-process"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (bin (expand-file-name "bin" sandbox))
        (program (expand-file-name "zeal" bin))
        (exec-path (list bin))
        old-list process-failure prompts lookup-count result)
   (unwind-protect
       (progn
         (when (file-directory-p sandbox)
           (delete-directory sandbox t))
         (make-directory bin t)
         (with-temp-file program
           (insert "#!/bin/sh\nexit 0\n"))
         (set-file-modes program #o755)
         (cl-letf (((symbol-function 'read-string)
                    (lambda (prompt &optional initial-input &rest _)
                      (push (list prompt initial-input) prompts)
                      (concat initial-input "HashMap")))
                   ((symbol-function 'executable-find)
                    (lambda (_program)
                      (setq lookup-count (1+ (or lookup-count 0)))
                      (when (file-exists-p program)
                        (delete-file program))
                      program)))
           (setq old-list
                 (let ((zeal-at-point-zeal-version "0.2.0")
                       (zeal-at-point-docset '("rust" "tokio")))
                   (with-temp-buffer
                     (setq major-mode 'rust-mode)
                     (neomacs-melpa-zeal-at-point--capture-signal
                      (lambda () (zeal-at-point-search))))))
           (setq process-failure
                 (let ((zeal-at-point-zeal-version "0.6.1")
                       (zeal-at-point-docset "rust"))
                   (with-temp-buffer
                     (setq major-mode 'rust-mode)
                     (insert "HashMap")
                     (goto-char 4)
                     (neomacs-melpa-zeal-at-point--capture-signal
                      (lambda () (zeal-at-point))))))
           (setq result
                 (list :old-list old-list
                       :process-failure process-failure
                       :prompts (nreverse prompts)
                       :lookup-count lookup-count))
           result))
     (when (file-directory-p sandbox)
       (delete-directory sandbox t)))))
"####;
    let expect = expect![[
        r##"OK (:old-list (:signal wrong-type-argument (characterp "rust")) :process-failure (:signal file-missing ("Searching for program" "No such file or directory" "zeal")) :prompts nil :lookup-count 1)"##
    ]];
    ParityBatchCase::value(
        "propagates_legacy_docset_and_process_start_failures",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        reports_the_modern_version_and_docset_detected_during_package_load(),
        searches_a_symbol_and_a_unicode_region_with_modern_docsets(),
        sets_a_buffer_local_docset_then_edits_the_prefilled_query(),
        selects_scalar_empty_and_list_docsets_from_all_prompt_shapes(),
        accepts_an_empty_minibuffer_as_the_first_list_valued_default(),
        launches_queries_across_supported_zeal_cli_generations(),
        prompts_for_a_query_without_a_symbol_or_matching_mode(),
        reports_a_disappeared_executable_and_an_unversioned_install(),
        propagates_legacy_docset_and_process_start_failures(),
    ]
}
