use expect_test::expect;

use super::ParityBatchCase;

fn apparmor_mode_authors_indents_and_saves_a_real_nested_service_policy() -> ParityBatchCase {
    ParityBatchCase::value(
        "apparmor_mode_authors_indents_and_saves_a_real_nested_service_policy",
        r##"(let* ((root
                  (apparmor-mode-test-root
                   "apparmor-authoring"))
                 (path
                  (expand-file-name
                   "service.profile"
                   root))
                 buffer
                 result)
         (unwind-protect
             (progn
               (apparmor-mode-test-cleanup root)
               (make-directory root t)
               (with-temp-file path
                 (insert
                  "# -*- mode: apparmor; -*-\n"
                  "abi <abi/4.0>,\n"
                  "include <tunables/global>\n"
                  "\n"
                  "@{a}=/srv/service\n"
                  "\n"
                  "profile service /usr/bin/service flags=(complain) {\n"
                  "capability dac_override,\n"
                  "network inet stream,\n"
                  "file Cx /usr/libexec/service-helper -> service-helper,\n"
                  "file mrix /usr/lib/@{multiarch}/service/plugins/**,\n"
                  "file r /dev/{,urandom,null},\n"
                  "dbus send\n"
                  "bus=session\n"
                  "path=/org/example/Service\n"
                  "interface=org.example.Service\n"
                  "member=Run\n"
                  "peer=(name=org.example.Client),\n"
                  "profile helper /usr/libexec/service-helper {\n"
                  "allow network,\n"
                  "}\n"
                  "}\n"))
               (setq buffer
                     (find-file-noselect path))
               (with-current-buffer buffer
                 (goto-char
                  (point-min))
                 (search-forward "complain")
                 (replace-match
                  "enforce"
                  t
                  t)
                 (indent-region
                  (point-min)
                  (point-max))
                 (font-lock-ensure)
                 (save-buffer)
                 (setq result
                       (list
                        :mode major-mode
                        :policy
                        (buffer-substring-no-properties
                         (point-min)
                         (point-max))
                        :disk
                        (apparmor-mode-test-read-file
                         path)))))
           (when
               (buffer-live-p buffer)
             (with-current-buffer buffer
               (set-buffer-modified-p nil))
             (kill-buffer buffer))
           (apparmor-mode-test-cleanup root))
         result)"##,
        expect![[
            r##"OK (:mode apparmor-mode :policy "# -*- mode: apparmor; -*-\nabi <abi/4.0>,\ninclude <tunables/global>\n\n@{a}=/srv/service\n\nprofile service /usr/bin/service flags=(enforce) {\n  capability dac_override,\n  network inet stream,\n  file Cx /usr/libexec/service-helper -> service-helper,\n  file mrix /usr/lib/@{multiarch}/service/plugins/**,\n  file r /dev/{,urandom,null},\n  dbus send\n      bus=session\n      path=/org/example/Service\n      interface=org.example.Service\n      member=Run\n      peer=(name=org.example.Client),\n  profile helper /usr/libexec/service-helper {\n    allow network,\n  }\n}\n" :disk "# -*- mode: apparmor; -*-\nabi <abi/4.0>,\ninclude <tunables/global>\n\n@{a}=/srv/service\n\nprofile service /usr/bin/service flags=(enforce) {\n  capability dac_override,\n  network inet stream,\n  file Cx /usr/libexec/service-helper -> service-helper,\n  file mrix /usr/lib/@{multiarch}/service/plugins/**,\n  file r /dev/{,urandom,null},\n  dbus send\n      bus=session\n      path=/org/example/Service\n      interface=org.example.Service\n      member=Run\n      peer=(name=org.example.Client),\n  profile helper /usr/libexec/service-helper {\n    allow network,\n  }\n}\n")"##
        ]],
    )
}

fn apparmor_mode_refontifies_security_rules_after_practical_policy_edits() -> ParityBatchCase {
    ParityBatchCase::value(
        "apparmor_mode_refontifies_security_rules_after_practical_policy_edits",
        r##"(with-temp-buffer
         (apparmor-mode)
         (insert
          "@{a}=/srv/service\n"
          "profile service /usr/bin/service flags=(enforce) {\n"
          "  capability sys_admin,\n"
          "  file r \"@{HOME}/My Documents/report\",\n"
          "  /usr/lib/libfoo.so.1#2 mr,\n"
          "  network inet stream,\n"
          "}\n")
         (font-lock-ensure)
         (goto-char
          (point-min))
         (search-forward
          "file r")
         (replace-match
          "file rw"
          t
          t)
         (forward-line 1)
         (beginning-of-line)
         (insert
          "  @{HOME}/cache/** rw,\n")
         (goto-char
          (point-min))
         (search-forward
          "network inet stream,")
         (beginning-of-line)
         (let ((comment-start-position
                (point)))
           (forward-line 1)
           (comment-region
            comment-start-position
            (point)))
         (font-lock-flush
          (point-min)
          (point-max))
         (font-lock-ensure)
         (let ((commented-face
                (save-excursion
                  (goto-char
                   (point-min))
                  (search-forward
                   "network inet stream,")
                  (search-backward
                   "network")
                  (get-text-property
                   (point)
                   'face))))
           (goto-char
            (point-min))
           (search-forward
            "network inet stream,")
           (beginning-of-line)
           (let ((commented-line-start
                  (point)))
             (forward-line 1)
             (uncomment-region
              commented-line-start
              (point)))
           (font-lock-flush
            (point-min)
            (point-max))
           (font-lock-ensure)
           (cl-labels
             ((face-at
               (line token)
               (save-excursion
                 (goto-char
                  (point-min))
                 (search-forward line)
                 (let ((line-start
                        (match-beginning 0))
                       (line-end
                        (match-end 0)))
                   (goto-char line-start)
                   (search-forward
                    token
                    line-end)
                   (get-text-property
                    (match-beginning 0)
                    'face)))))
             (list
              :comment-cycle
              (list
               commented-face
               (face-at
                "  network inet stream,"
                "network"))
              :edited-faces
              (list
               (list "@{a}"
                     (face-at
                      "@{a}=/srv/service"
                      "@{a}"))
               (list "profile-name"
                     (face-at
                      "profile service /usr/bin/service flags=(enforce) {"
                      "service"))
               (list "edited-permission"
                     (face-at
                      "  file rw \"@{HOME}/My Documents/report\","
                      "rw"))
               (list "embedded-hash"
                     (face-at
                      "  /usr/lib/libfoo.so.1#2 mr,"
                      "#"))
               (list "inserted-wildcard"
                     (face-at
                      "  @{HOME}/cache/** rw,"
                      "**")))))))"##,
        expect![[
            r#"OK (:comment-cycle (font-lock-comment-face font-lock-keyword-face) :edited-faces (("@{a}" font-lock-variable-name-face) ("profile-name" font-lock-function-name-face) ("edited-permission" font-lock-constant-face) ("embedded-hash" nil) ("inserted-wildcard" font-lock-regexp-grouping-construct)))"#
        ]],
    )
}

fn apparmor_mode_completes_keyword_but_leaves_nested_local_include_unresolved() -> ParityBatchCase {
    ParityBatchCase::value(
        "apparmor_mode_completes_keyword_but_leaves_nested_local_include_unresolved",
        r##"(let* ((root
                  (apparmor-mode-test-root
                   "apparmor-completion"))
                 (default-directory root)
                 (local-directory
                  (expand-file-name
                   "local/"
                   root)))
         (unwind-protect
             (progn
               (apparmor-mode-test-cleanup root)
               (make-directory
                local-directory
                t)
               (dolist
                   (name
                    '("service-base"
                      "service-extra"))
                 (with-temp-file
                     (expand-file-name
                      name
                      local-directory)
                   (insert
                    "# local policy fragment\n")))
               (with-temp-buffer
                 (apparmor-mode)
                 (insert "capab")
                 (let ((keyword-status
                        (completion-at-point))
                       keyword-buffer
                       keyword-point
                       include-status
                       include-buffer
                       include-point
                       capf)
                   (setq keyword-buffer
                         (buffer-string)
                         keyword-point
                         (point))
                   (erase-buffer)
                   (insert
                    "include \"local/service-b")
                   (setq include-status
                         (completion-at-point)
                         include-buffer
                         (buffer-string)
                         include-point
                         (point))
                   (erase-buffer)
                   (insert
                    "include \"local/service")
                   (setq capf
                         (run-hook-with-args-until-success
                          'completion-at-point-functions))
                   (list
                    :keyword
                    (list
                     keyword-status
                     keyword-buffer
                     keyword-point)
                    :include
                    (list
                     include-status
                     include-buffer
                     include-point)
                    :choices
                    (list
                     (buffer-substring-no-properties
                      (nth 0 capf)
                      (nth 1 capf))
                     (sort
                      (all-completions
                       (buffer-substring-no-properties
                        (nth 0 capf)
                        (nth 1 capf))
                       (nth 2 capf))
                      #'string<))
                    :resolved
                    (sort
                     (apparmor-mode-complete-include
                      "local/service"
                      t)
                     #'string<)))))
           (apparmor-mode-test-cleanup root)))"##,
        expect![[
            r#"OK (:keyword (t "capability" 11) :include (nil "include \"local/service-b" 25) :choices ("service" nil) :resolved ("local/service-base" "local/service-extra"))"#
        ]],
    )
}

fn apparmor_mode_flymake_discards_a_slow_obsolete_result_after_a_rapid_edit() -> ParityBatchCase {
    ParityBatchCase::value(
        "apparmor_mode_flymake_discards_a_slow_obsolete_result_after_a_rapid_edit",
        r##"(let* ((root
                  (apparmor-mode-test-root
                   "apparmor-flymake"))
                 (path
                  (expand-file-name
                   "service.profile"
                   root))
                 (parser
                  (expand-file-name
                   "fake-apparmor-parser"
                   root))
                 (capture
                  (expand-file-name
                   "parser-capture"
                   root))
                 buffer
                 result)
         (unwind-protect
             (progn
               (apparmor-mode-test-cleanup root)
               (make-directory root t)
               (with-temp-file path
                 (insert
                  "profile service /usr/bin/service {\n"
                  "  capability net_bind_service,\n"
                  "  STALE_BROKEN rule,\n"
                  "}\n"))
               (with-temp-file parser
                 (insert
                  "#!/bin/sh\n"
                  "capture="
                  (shell-quote-argument capture)
                  "\n"
                  "cat > \"${capture}.input\"\n"
                  "printf '%s\\n' \"$@\" > \"${capture}.args\"\n"
                  "if grep -q STALE_BROKEN \"${capture}.input\"; then\n"
                  "  : > \"${capture}.started\"\n"
                  "  sleep 0.35\n"
                  "  printf '%s\\n' 'AppArmor parser error at line 3: stale diagnostic'\n"
                  "  exit 1\n"
                  "fi\n"
                  "if grep -q LATEST_BROKEN \"${capture}.input\"; then\n"
                  "  printf '%s\\n' 'AppArmor parser error at line 3: latest diagnostic'\n"
                  "  exit 1\n"
                  "fi\n"
                  "exit 0\n"))
               (set-file-modes
                parser
                #o755)
               (setq buffer
                     (find-file-noselect path))
               (with-current-buffer buffer
                 (apparmor-mode)
                 (setq-local
                 apparmor-mode-apparmor-parser-executable
                  parser)
                 (setq-local
                  flymake-no-changes-timeout
                  nil)
                 (flymake-mode 1)
                 (flymake-start nil t)
                 (apparmor-mode-test-await
                  (lambda ()
                    (file-exists-p
                     (concat
                      capture
                      ".started")))
                  "slow parser to start")
                 (goto-char
                  (point-min))
                 (search-forward
                  "STALE_BROKEN rule")
                 (replace-match
                  "LATEST_BROKEN rule"
                  t
                  t)
                 (save-buffer)
                 (apparmor-mode-test-start-flymake
                  (lambda ()
                    (let ((diagnostics
                           (apparmor-mode-test-diagnostics)))
                      (and
                       (= (length diagnostics) 1)
                       (equal
                        (plist-get
                         (car diagnostics)
                         :text)
                        "latest diagnostic"))))
                  "latest diagnostic")
                 (let ((deadline
                        (+
                         (float-time)
                         0.5)))
                   (while
                       (<
                        (float-time)
                        deadline)
                     (accept-process-output
                      nil
                      0.01)))
                 (setq result
                       (list
                        :diagnostics
                        (apparmor-mode-test-diagnostics)
                        :latest-input
                        (apparmor-mode-test-read-file
                         (concat
                          capture
                          ".input"))
                        :arguments
                        (apparmor-mode-test-read-file
                         (concat
                          capture
                          ".args"))
                        :buffer
                        (buffer-string)
                        :disk
                        (apparmor-mode-test-read-file
                         path)
                        :modified
                        (buffer-modified-p)))))
           (when
               (buffer-live-p buffer)
             (with-current-buffer buffer
               (flymake-mode -1)
               (set-buffer-modified-p nil))
             (kill-buffer buffer))
           (apparmor-mode-test-cleanup root))
         result)"##,
        expect![[
            r#"OK (:diagnostics ((:type :error :text "latest diagnostic" :begin (3 2) :end (3 21))) :latest-input "profile service /usr/bin/service {\n  capability net_bind_service,\n  LATEST_BROKEN rule,\n}\n" :arguments "-Q\n-K\n/dev/stdin\n" :buffer "profile service /usr/bin/service {\n  capability net_bind_service,\n  LATEST_BROKEN rule,\n}\n" :disk "profile service /usr/bin/service {\n  capability net_bind_service,\n  LATEST_BROKEN rule,\n}\n" :modified nil)"#
        ]],
    )
}

fn apparmor_mode_flymake_wraps_and_validates_a_real_abstraction_fragment() -> ParityBatchCase {
    ParityBatchCase::value(
        "apparmor_mode_flymake_wraps_and_validates_a_real_abstraction_fragment",
        r##"(let* ((root
                  (apparmor-mode-test-root
                   "apparmor-abstraction"))
                 (path
                  (expand-file-name
                   "abstractions/base"
                   root))
                 (parser
                  (expand-file-name
                   "fake-apparmor-parser"
                   root))
                 (capture
                  (expand-file-name
                   "parser-capture"
                   root))
                 buffer
                 result)
         (unwind-protect
             (progn
               (apparmor-mode-test-cleanup root)
               (make-directory
                (file-name-directory path)
                t)
               (with-temp-file path
                 (insert
                  "/etc/ssl/certs/** r,\n"
                  "BROKEN abstraction rule,\n"))
               (with-temp-file parser
                 (insert
                  "#!/bin/sh\n"
                  "capture="
                  (shell-quote-argument capture)
                  "\n"
                  "cat > \"${capture}.input\"\n"
                  "printf '%s\\n' \"$@\" > \"${capture}.args\"\n"
                  "printf '%s\\n' 'AppArmor parser error for /dev/stdin in profile base at line 2: invalid abstraction rule'\n"
                  "exit 1\n"))
               (set-file-modes
                parser
                #o755)
               (setq buffer
                     (find-file-noselect path))
               (with-current-buffer buffer
                 (apparmor-mode)
                 (setq-local
                  apparmor-mode-apparmor-parser-executable
                  parser)
                 (setq-local
                 flymake-no-changes-timeout
                  nil)
                 (flymake-mode 1)
                 (apparmor-mode-test-start-flymake
                  (lambda ()
                    (let ((diagnostics
                           (apparmor-mode-test-diagnostics)))
                      (and
                       (= (length diagnostics) 1)
                       (equal
                        (plist-get
                         (car diagnostics)
                         :text)
                        "invalid abstraction rule"))))
                  "abstraction diagnostic")
                 (setq result
                       (list
                        :diagnostics
                        (apparmor-mode-test-diagnostics)
                        :parser-input
                        (apparmor-mode-test-read-file
                         (concat
                          capture
                          ".input"))
                        :arguments
                        (apparmor-mode-test-read-file
                         (concat
                          capture
                          ".args"))
                        :source
                        (buffer-string)))))
           (when
               (buffer-live-p buffer)
             (with-current-buffer buffer
               (flymake-mode -1)
               (set-buffer-modified-p nil))
             (kill-buffer buffer))
           (apparmor-mode-test-cleanup root))
         result)"##,
        expect![[
            r#"OK (:diagnostics ((:type :error :text "invalid abstraction rule" :begin (2 0) :end (2 24))) :parser-input "profile base { /etc/ssl/certs/** r,\nBROKEN abstraction rule,\n }" :arguments "-Q\n-K\n/dev/stdin\n" :source "/etc/ssl/certs/** r,\nBROKEN abstraction rule,\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        apparmor_mode_authors_indents_and_saves_a_real_nested_service_policy(),
        apparmor_mode_refontifies_security_rules_after_practical_policy_edits(),
        apparmor_mode_completes_keyword_but_leaves_nested_local_include_unresolved(),
        apparmor_mode_flymake_discards_a_slow_obsolete_result_after_a_rapid_edit(),
        apparmor_mode_flymake_wraps_and_validates_a_real_abstraction_fragment(),
    ]
}
