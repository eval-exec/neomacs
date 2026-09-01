use expect_test::expect;

use super::ParityBatchCase;

fn configured_eldoc_presents_real_eshell_workflows() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-esh-help-test-with-sandbox "eldoc-workflow"
  (setup-esh-help-eldoc)
  (let (first-session second-session)
    (neomacs-esh-help-test-with-eshell-buffer
      (setq first-session
            (list
             :setup
             (list :local (local-variable-p 'eldoc-documentation-function)
                   :provider eldoc-documentation-function
                   :mode major-mode)
             :eshell-command
             (neomacs-esh-help-test-visible-eldoc "echo release-ready")
             :ordinary-lisp
             (neomacs-esh-help-test-visible-eldoc "length release-items")
             :pipeline
             (neomacs-esh-help-test-visible-eldoc
              "kill %1 | which release-tool")
             :quoted-pipe-limitation
             (neomacs-esh-help-test-visible-eldoc "echo \"blue|green\"")
             :relative-program
             (neomacs-esh-help-test-visible-eldoc "./release-tool deploy")
             :mode-enabled eldoc-mode)))
    ;; One global setup call must configure every subsequently created Eshell
    ;; session, not only the first buffer which happened to run its hook.
    (neomacs-esh-help-test-with-eshell-buffer
      (setq second-session
            (list
             :setup
             (list :local (local-variable-p 'eldoc-documentation-function)
                   :provider eldoc-documentation-function
                   :mode major-mode)
             :workflow
             (neomacs-esh-help-test-visible-eldoc
              "echo second-session-ready"))))
    (list :first-session first-session :second-session second-session)))
"####;
    let expect = expect![[
        r#"OK (:first-session (:setup (:local t :provider esh-help-eldoc-command :mode eshell-mode) :eshell-command (:input "echo release-ready" :eldoc-message "(&rest ARGS)" :point 24 :text "OPS> echo release-ready" :eldoc-mode t :timer-deliveries 1 :scheduled t) :ordinary-lisp (:input "length release-items" :eldoc-message "(SEQUENCE)" :point 26 :text "OPS> length release-items" :eldoc-mode t :timer-deliveries 1 :scheduled t) :pipeline (:input "kill %1 | which release-tool" :eldoc-message "(COMMAND &rest NAMES)" :point 34 :text "OPS> kill %1 | which release-tool" :eldoc-mode t :timer-deliveries 1 :scheduled t) :quoted-pipe-limitation (:input "echo \"blue|green\"" :eldoc-message nil :point 23 :text "OPS> echo \"blue|green\"" :eldoc-mode t :timer-deliveries 1 :scheduled t) :relative-program (:input "./release-tool deploy" :eldoc-message nil :point 27 :text "OPS> ./release-tool deploy" :eldoc-mode t :timer-deliveries 1 :scheduled t) :mode-enabled t) :second-session (:setup (:local t :provider esh-help-eldoc-command :mode eshell-mode) :workflow (:input "echo second-session-ready" :eldoc-message "(&rest ARGS)" :point 31 :text "OPS> echo second-session-ready" :eldoc-mode t :timer-deliveries 1 :scheduled t)))"#
    ]];
    ParityBatchCase::value(
        "configured_eldoc_presents_real_eshell_workflows",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn run_help_renders_alias_and_lisp_help_through_m_x() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-esh-help-test-with-eshell "run-help-help-ui"
  (let ((origin (current-buffer)) alias-help lisp-help unknown)
    ;; The Eshell alias/function branch must win over similarly named programs.
    (neomacs-esh-help-test-run-help "which release-tool --all")
    (setq alias-help
          (neomacs-esh-help-test-buffer-state (get-buffer "*Help*")))
    (kill-buffer "*Help*")

    ;; Ordinary Lisp functions use the same public command but a different
    ;; dispatch branch and GNU Help document.
    (switch-to-buffer origin)
    (neomacs-esh-help-test-run-help "length release-items")
    (setq lisp-help
          (neomacs-esh-help-test-buffer-state (get-buffer "*Help*")))
    (kill-buffer "*Help*")

    ;; An unknown command is deliberately a no-op and must not steal focus or
    ;; resurrect either documentation UI.
    (switch-to-buffer origin)
    (neomacs-esh-help-test-run-help "definitely-not-installed")
    (setq unknown
          (list :help-buffer (get-buffer "*Help*")
                :man-buffers
                (mapcar #'buffer-name
                        (seq-filter
                         (lambda (buffer)
                           (with-current-buffer buffer
                             (derived-mode-p 'Man-mode)))
                         (buffer-list)))
                :origin-selected
                (eq (window-buffer (selected-window)) origin)
                :text (buffer-substring-no-properties
                       (point-min) (point-max))
                :point (point)))
    (list :alias alias-help :lisp lisp-help :unknown unknown)))
"####;
    let expect = expect![[
        r#"OK (:alias (:name "*Help*" :text "eshell/which is an interpreted-function in ‘esh-cmd.el’.\n\n(eshell/which COMMAND &rest NAMES)\n\nIdentify the COMMAND, and where it is located.\n" :mode help-mode :read-only t :modified nil :point 1 :visible t :selected nil :buttons ("interpreted-function" "esh-cmd.el") :process nil) :lisp (:name "*Help*" :text "length is a primitive-function in ‘C source code’.\n\n(length SEQUENCE)\n\nDeclared type: (function (t) (integer 0 *))\n\nReturn the length of vector, list or string SEQUENCE.\nA byte-code function object is also allowed.\n\nIf the string contains multibyte characters, this is not necessarily\nthe number of bytes in the string; it is the number of characters.\nTo get the number of bytes, use ‘string-bytes’.\n\nIf the length of a list is being computed to compare to a (small)\nnumber, the ‘length<’, ‘length>’ and ‘length=’ functions may be more\nefficient.\n\n  Other relevant functions are documented in the vector, list and\n  string groups.\n  Probably introduced at or before Emacs version 20.1.\n" :mode help-mode :read-only t :modified nil :point 1 :visible t :selected nil :buttons ("primitive-function" "C source code" "string-bytes" "length<" "length>" "length=" "vector" "list" "string" "20.1") :process nil) :unknown (:help-buffer nil :man-buffers nil :origin-selected t :text "OPS> definitely-not-installed" :point 30))"#
    ]];
    ParityBatchCase::value(
        "run_help_renders_alias_and_lisp_help_through_m_x",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn run_help_renders_real_man_ui_for_an_explicit_topic() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-esh-help-test-with-man-eshell "run-help-explicit-man-ui"
  (let ((man-log (plist-get peers :man-log))
        (miss-log (plist-get peers :miss-log)))
    ;; A leading star explicitly requests a manual page even without PATH
    ;; discovery.  Keep GNU Man's process, filter, sentinel, and UI real.
    (let ((input
           (neomacs-esh-help-test-command-execute-run-help
            "*printf --version"))
          (manual
           (neomacs-esh-help-test-buffer-state
            (neomacs-esh-help-test-wait-for-man "printf"))))
      (list :manual manual
            :input input
            :man-requests (neomacs-esh-help-test-read-lines man-log)
            :fixture-misses (neomacs-esh-help-test-read-lines miss-log)
            :caller-lang (getenv "LANG")))))
"####;
    let expect = expect![[
        r#"OK (:manual (:name "*Man printf*" :text "PRINTF(1)                        User Commands                        PRINTF(1)\n\nNAME\n       printf - format and print data\n\nSYNOPSIS\n       printf FORMAT [ARGUMENT]...\n       printf OPTION\n\nDESCRIPTION\n       Print ARGUMENT(s) according to FORMAT, or execute according to OPTION:\n\n       --help display this help and exit\n\n       --version\n              output version information and exit\n\n       FORMAT controls the output as in C printf.  Interpreted sequences are:\n\n       \\\"     double quote\n\n       \\\\     backslash\n\n       \\a     alert (BEL)\n\n       \\b     backspace\n\n       \\c     produce no further output\n\n       \\e     escape\n\n       \\f     form feed\n\n       \\n     new line\n\n       \\r     carriage return\n\n       \\t     horizontal tab\n\n       \\v     vertical tab\n\n       \\NNN   byte with octal value NNN (1 to 3 digits)\n\n       \\xHH   byte with hexadecimal value HH (1 to 2 digits)\n\n       \\uHHHH Unicode (ISO/IEC 10646) character with hex value HHHH (4 digits)\n\n       \\UHHHHHHHH\n              Unicode character with hex value HHHHHHHH (8 digits)\n\n       %%     a single %\n\n       %b     ARGUMENT  as  a  string with '\\' escapes interpreted, except that\n              octal escapes should have a leading 0 like \\0NNN\n\n       %q     ARGUMENT is printed in a format that can be reused as  shell  in‐\n              put, escaping non-printable characters with the POSIX $'' syntax\n\n       and  all  C format specifications ending with one of diouxXfeEgGcs, with\n       ARGUMENTs converted to proper type first.  Variable widths are handled.\n\n       Your shell may have its own version of printf, which usually  supersedes\n       the  version described here.  Please refer to your shell's documentation\n       for details about the options it supports.\n\nAUTHOR\n       Written by David MacKenzie.\n\nREPORTING BUGS\n       Report bugs to: bug-coreutils@gnu.org\n       GNU coreutils home page: <https://www.gnu.org/software/coreutils/>\n       General help using GNU software: <https://www.gnu.org/gethelp/>\n       Report any translation bugs to <https://translationproject.org/team/>\n\nSEE ALSO\n       printf(3)\n\n       Full documentation <https://www.gnu.org/software/coreutils/printf>\n       or available locally via: info '(coreutils) printf invocation'\n\n       Packaged by https://nixos.org\n       Copyright © 2025 Free Software Foundation, Inc.\n       License  GPLv3+:  GNU  GPL  version  3  or  later   <https://gnu.org/li‐\n       censes/gpl.html>.\n       This is free software: you are free to change and redistribute it.\n       There is NO WARRANTY, to the extent permitted by law.\n\nGNU coreutils 9.8                September 2025                       PRINTF(1)\n" :mode Man-mode :read-only t :modified nil :point 1 :visible t :selected nil :buttons ("printf") :process nil) :input (:text "OPS> *printf --version" :point 23 :selected t) :man-requests ("LANG=<fr_FR.UTF-8> ARGC=<1> ARGV=<printf>") :fixture-misses nil :caller-lang "fr_FR.UTF-8")"#
    ]];
    ParityBatchCase::value(
        "run_help_renders_real_man_ui_for_an_explicit_topic",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn run_help_resolves_a_path_command_before_opening_man_ui() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-esh-help-test-with-man-eshell "run-help-path-man-ui"
  (let* ((man-log (plist-get peers :man-log))
         (miss-log (plist-get peers :miss-log))
         manual input)

    ;; The executable branch reaches the same real GNU Man UI only after
    ;; Eshell's real PATH lookup recognizes the sandbox program.
    (setq input
          (neomacs-esh-help-test-command-execute-run-help
           "printf '%s' release-42"))
    (setq manual
          (neomacs-esh-help-test-buffer-digest-state
           (neomacs-esh-help-test-wait-for-man "printf")))

    (list :manual manual
          :input input
          :path-before-lisp
          (list :lisp-function (fboundp 'printf)
                :lisp-arglist (help-function-arglist 'printf))
          :man-requests (neomacs-esh-help-test-read-lines man-log)
          :fixture-misses (neomacs-esh-help-test-read-lines miss-log)
          :caller-lang (getenv "LANG"))))
"####;
    let expect = expect![[
        r#"OK (:manual (:name "*Man printf*" :characters 2674 :sha256 "77092880ecfa0ebb765a28383b2afa9612328dda9a6a4a27bed91ca07af8ff02" :prefix "PRINTF(1)                        User Commands                        PRINTF(1)\n\nNAME\n       printf - format and print data\n\nSYNOPSIS\n       printf FORMAT [ARGUMENT]...\n       prin" :suffix "e free to change and redistribute it.\n       There is NO WARRANTY, to the extent permitted by law.\n\nGNU coreutils 9.8                September 2025                       PRINTF(1)\n" :mode Man-mode :read-only t :modified nil :point 1 :visible t :selected nil :process nil) :input (:text "OPS> printf '%s' release-42" :point 28 :selected t) :path-before-lisp (:lisp-function t :lisp-arglist (lisp-only-argument)) :man-requests ("LANG=<fr_FR.UTF-8> ARGC=<1> ARGV=<printf>") :fixture-misses nil :caller-lang "fr_FR.UTF-8")"#
    ]];
    ParityBatchCase::value(
        "run_help_resolves_a_path_command_before_opening_man_ui",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn external_eldoc_uses_real_pipeline_cache_and_missing_page_failure() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-esh-help-test-with-sandbox "external-man-cache"
  (let* ((peers (neomacs-esh-help-test-install-man-peers root))
         (man-log (plist-get peers :man-log))
         (col-log (plist-get peers :col-log))
         (miss-log (plist-get peers :miss-log)))
    (setenv "LANG" "fr_FR.UTF-8")
    (setup-esh-help-eldoc)
    (neomacs-esh-help-test-with-eshell-buffer
      (let* ((first
              (neomacs-esh-help-test-visible-eldoc
               "printf 'release %s' release-42"))
             (cached
              (neomacs-esh-help-test-visible-eldoc
               "printf 'environment %s' production"))
             (calls-before-clear
              (list :man (neomacs-esh-help-test-read-lines man-log)
                    :col (neomacs-esh-help-test-read-lines col-log))))
        ;; Use the package's documented interactive cache reset, then prove a
        ;; real external request is made again.
        (execute-kbd-macro (kbd "M-x esh-help-clear-man-cache RET"))
        (let* ((after-clear
                (neomacs-esh-help-test-visible-eldoc
                 "printf 'verify %s' release-42"))
               ;; man-db 2.13.1 writes this failure to stderr and exits 16.
               ;; The shell pipeline therefore gives Esh Help no SYNOPSIS;
               ;; capture the exact public Eldoc failure and prove the peer was
               ;; actually called with the expected command.
               (missing-first
                (condition-case problem
                    (list :value
                          (neomacs-esh-help-test-visible-eldoc
                           "*missing-tool --all"))
                  (error
                   (list :signal (car problem)
                         :data (cdr problem)
                         :message (error-message-string problem)
                         :text (buffer-substring-no-properties
                                (point-min) (point-max))
                         :point (point)))))
               (missing-second
                (condition-case problem
                    (list :value
                          (neomacs-esh-help-test-visible-eldoc
                           "*missing-tool --verbose"))
                  (error
                   (list :signal (car problem)
                         :data (cdr problem)
                         :message (error-message-string problem)
                         :text (buffer-substring-no-properties
                                (point-min) (point-max))
                         :point (point)))))
               (malformed
                (condition-case problem
                    (list :value
                          (neomacs-esh-help-test-request-eldoc
                           "*malformed-tool --verbose"))
                  (error
                   (list :signal (car problem)
                         :data (cdr problem)
                         :message (error-message-string problem)
                         :text (buffer-substring-no-properties
                                (point-min) (point-max))
                         :point (point))))))
          (list
           :first first
           :cached cached
           :calls-before-clear calls-before-clear
           :after-clear after-clear
           :missing (list missing-first missing-second)
           :malformed malformed
           :calls
           (list :man (neomacs-esh-help-test-read-lines man-log)
                 :col (neomacs-esh-help-test-read-lines col-log))
           :cache
           (list (gethash "printf" esh-help-man-cache)
                 (gethash "missing-tool" esh-help-man-cache)
                 (gethash "malformed-tool" esh-help-man-cache))
           :path-before-lisp
           (list :lisp-function (fboundp 'printf)
                 :lisp-arglist (help-function-arglist 'printf))
           :malformed-fixture (plist-get peers :malformed-fixture)
           :fixture-misses (neomacs-esh-help-test-read-lines miss-log)
           :caller-lang (getenv "LANG")
           :help-buffer (get-buffer "*Help*")
           :man-buffers
           (mapcar #'buffer-name
                   (seq-filter
                    (lambda (buffer)
                      (with-current-buffer buffer
                        (derived-mode-p 'Man-mode)))
                    (buffer-list)))))))))
"####;
    let expect = expect![[
        r#"OK (:first (:input "printf 'release %s' release-42" :eldoc-message "printf FORMAT [ARGUMENT]..." :point 36 :text "OPS> printf 'release %s' release-42" :eldoc-mode t :timer-deliveries 1 :scheduled t) :cached (:input "printf 'environment %s' production" :eldoc-message "printf FORMAT [ARGUMENT]..." :point 40 :text "OPS> printf 'environment %s' production" :eldoc-mode t :timer-deliveries 1 :scheduled t) :calls-before-clear (:man ("LANG=<fr_FR.UTF-8> ARGC=<1> ARGV=<printf>") :col ("ARGC=<1> ARGV=<-b>")) :after-clear (:input "printf 'verify %s' release-42" :eldoc-message "printf FORMAT [ARGUMENT]..." :point 35 :text "OPS> printf 'verify %s' release-42" :eldoc-mode t :timer-deliveries 1 :scheduled t) :missing ((:value (:input "*missing-tool --all" :eldoc-message nil :point 25 :text "OPS> *missing-tool --all" :eldoc-mode t :timer-deliveries 1 :scheduled t)) (:value (:input "*missing-tool --verbose" :eldoc-message nil :point 29 :text "OPS> *missing-tool --verbose" :eldoc-mode t :timer-deliveries 1 :scheduled t))) :malformed (:signal wrong-type-argument :data (stringp nil) :message "Wrong type argument: stringp, nil" :text "OPS> *malformed-tool --verbose" :point 31) :calls (:man ("LANG=<fr_FR.UTF-8> ARGC=<1> ARGV=<printf>" "LANG=<fr_FR.UTF-8> ARGC=<1> ARGV=<printf>" "LANG=<fr_FR.UTF-8> ARGC=<1> ARGV=<missing-tool>" "LANG=<fr_FR.UTF-8> ARGC=<1> ARGV=<malformed-tool>") :col ("ARGC=<1> ARGV=<-b>" "ARGC=<1> ARGV=<-b>" "ARGC=<1> ARGV=<-b>" "ARGC=<1> ARGV=<-b>")) :cache ("printf FORMAT [ARGUMENT]..." none nil) :path-before-lisp (:lisp-function t :lisp-arglist (lisp-only-argument)) :malformed-fixture (:source "man-db 2.13.1/coreutils 9.8 printf(1), util-linux col 2.41.4 recordings" :transformation delete-synopsis-section :raw-sha256 "a41663d35da427537fda33b203d125f2cc6e347ed3ced79db21ff8cfc08c5efc" :post-col-sha256 "53c06b72bcc99a1640768e108c4ddaada99b1bcc0bde928371f80a225365ab34") :fixture-misses nil :caller-lang "fr_FR.UTF-8" :help-buffer nil :man-buffers nil)"#
    ]];
    ParityBatchCase::value(
        "external_eldoc_uses_real_pipeline_cache_and_missing_page_failure",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn run_help_rejects_a_blank_eshell_prompt() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-esh-help-test-with-eshell "blank-run-help"
  (neomacs-esh-help-test-run-help ""))
"####;
    let expect = expect!["ERR (wrong-type-argument stringp nil)"];
    ParityBatchCase::signal("run_help_rejects_a_blank_eshell_prompt", elisp_form, expect)
        .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        configured_eldoc_presents_real_eshell_workflows(),
        run_help_renders_alias_and_lisp_help_through_m_x(),
        run_help_renders_real_man_ui_for_an_explicit_topic(),
        run_help_resolves_a_path_command_before_opening_man_ui(),
        external_eldoc_uses_real_pipeline_cache_and_missing_page_failure(),
        run_help_rejects_a_blank_eshell_prompt(),
    ]
}
