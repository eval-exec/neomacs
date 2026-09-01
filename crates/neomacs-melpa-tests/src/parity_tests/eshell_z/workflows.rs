use expect_test::expect;

use super::ParityBatchCase;

fn real_eshell_commands_persist_jump_delete_and_survive_restart() -> ParityBatchCase {
    let elisp_form = r##"(esz-test-run
 "eshell-z-persistence"
 (lambda (root)
   (let* ((alpha (expand-file-name "release alpha Ω/" root))
          (beta (expand-file-name "release beta/" root))
          (data (expand-file-name "shared-z-data" root))
          (eshell-z-freq-dir-hash-table-file-name data)
          (eshell-z-freq-dir-hash-table nil)
          (eshell-z-exclude-dirs nil)
          (eshell-z-change-dir-hook
           (list #'esz-test-change-directory-observer))
          first restart)
     (make-directory alpha t)
     (make-directory beta t)
     (let ((buffer (esz-test-new-session " *esz-persistence-first*" alpha)))
       (with-current-buffer buffer
         (let ((after-start (esz-test-table-rows))
               (start-file (esz-test-file-rows data))
               (echo-alpha (esz-test-send "echo alpha-work"))
               cd-beta echo-beta jump-alpha delete-alpha)
           (setq cd-beta
                 (esz-test-send
                  (concat "cd " (eshell-quote-argument beta)))
                 echo-beta (esz-test-send "echo beta-work")
                 jump-alpha (esz-test-send "z alpha")
                 delete-alpha (esz-test-send "z -x")
                 first
                 (list :after-start after-start :start-file start-file
                       :transitions
                       (list echo-alpha cd-beta echo-beta jump-alpha
                             delete-alpha)
                       :cwd (esz-test-directory)
                       :remove eshell-z--remove-p
                       :table (esz-test-table-rows)
                       :file (esz-test-file-rows data)
                       :history (esz-test-history)))))
     (setq eshell-z-freq-dir-hash-table nil)
     (let ((buffer (esz-test-new-session " *esz-persistence-restart*" beta)))
       (with-current-buffer buffer
         (let ((before-list
                (list :cwd (esz-test-directory)
                      :table (esz-test-table-rows)
                      :file (esz-test-file-rows data)))
               (listing (esz-test-send "z -l")))
           (setq restart
                 (list :before-list before-list :list listing
                       :after-list-table (esz-test-table-rows)
                       :after-list-file (esz-test-file-rows data))))))
     (list :first first :restart restart
           :directory-events (nreverse esz-test-directory-events))))))"##;
    let expected = expect![[
        r#"OK (:result (:first (:after-start (("[ROOT]/release alpha Ω" 1 "2000000000")) :start-file ("[ROOT]/release alpha Ω|1|2000000000") :transitions ((:input "echo alpha-work" :tail "echo alpha-work\nalpha-work\nZ> " :before "[ROOT]/release alpha Ω" :after "[ROOT]/release alpha Ω" :point-at-end t :history ("echo alpha-work")) (:input "cd [ROOT]/release\\ beta/" :tail "cd [ROOT]/release\\ beta/\nZ> " :before "[ROOT]/release alpha Ω" :after "[ROOT]/release beta" :point-at-end t :history ("cd [ROOT]/release\\ beta/" "echo alpha-work")) (:input "echo beta-work" :tail "echo beta-work\nbeta-work\nZ> " :before "[ROOT]/release beta" :after "[ROOT]/release beta" :point-at-end t :history ("echo beta-work" "cd [ROOT]/release\\ beta/" "echo alpha-work")) (:input "z alpha" :tail "z alpha\nZ> " :before "[ROOT]/release beta" :after "[ROOT]/release alpha Ω" :point-at-end t :history ("z alpha" "echo beta-work" "cd [ROOT]/release\\ beta/" "echo alpha-work")) (:input "z -x" :tail "z -x\nZ> " :before "[ROOT]/release alpha Ω" :after "[ROOT]/release alpha Ω" :point-at-end t :history ("z -x" "z alpha" "echo beta-work" "cd [ROOT]/release\\ beta/" "echo alpha-work"))) :cwd "[ROOT]/release alpha Ω" :remove nil :table (("[ROOT]/release beta" 2 "2000000000")) :file ("[ROOT]/release beta|2|2000000000") :history ("z -x" "z alpha" "echo beta-work" "cd [ROOT]/release\\ beta/" "echo alpha-work")) :restart (:before-list (:cwd "[ROOT]/release beta" :table (("[ROOT]/release beta" 3 "2000000000")) :file ("[ROOT]/release beta|3|2000000000")) :list (:input "z -l" :tail "z -l\ncommon:    [ROOT]/release beta\n12         [ROOT]/release betaZ> " :before "[ROOT]/release beta" :after "[ROOT]/release beta" :point-at-end t :history ("z -l")) :after-list-table (("[ROOT]/release beta" 4 "2000000000")) :after-list-file ("[ROOT]/release beta|4|2000000000")) :directory-events ("[ROOT]/release beta")) :cleanup (:owned-reference-live nil :new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :package-hooks-restored t :package-hook-shape (add remove) :query-functions-restored t :completion-remaining nil :commands ("echo alpha-work" "cd [ROOT]/release\\ beta/" "echo beta-work" "z alpha" "z -x" "z -l") :cleanup-error nil))"#
    ]];
    ParityBatchCase::value(
        "real_eshell_commands_persist_jump_delete_and_survive_restart",
        elisp_form,
        expected,
    )
}

fn public_z_navigation_distinguishes_every_selection_mode() -> ParityBatchCase {
    let elisp_form = r##"(esz-test-run
 "eshell-z-navigation"
 (lambda (root)
   (let* ((projects (expand-file-name "projects/" root))
          (client (expand-file-name "client/" projects))
          (api (expand-file-name "api/" client))
          (ui (expand-file-name "ui Ω/" client))
          (archive (expand-file-name "archive/client-api-old/" root))
          (numeric (expand-file-name "archive/2026/" root))
          (other (expand-file-name "other/api/" root))
          (table (make-hash-table :test #'equal))
          (eshell-z-freq-dir-hash-table-file-name nil)
          (eshell-z-freq-dir-hash-table table)
          (eshell-z-exclude-dirs (list root))
          (eshell-z-change-dir-hook
           (list #'esz-test-change-directory-observer))
          transitions)
     (dolist (directory (list api ui archive numeric other))
       (make-directory directory t))
     (esz-test-put table (directory-file-name client) 2 1999300000)
     (esz-test-put table (directory-file-name api) 9 1999992800)
     (esz-test-put table (directory-file-name ui) 30 1999300000)
     (esz-test-put table (directory-file-name archive) 1 1999999970)
     (esz-test-put table (directory-file-name other) 40 1999300000)
     (esz-test-put table (directory-file-name numeric) 3 1999999900)
     (let ((buffer (esz-test-new-session " *esz-navigation*" root)))
       (with-current-buffer buffer
         (push (esz-test-send "z projects.client") transitions)
         (eshell/cd root)
         (push (esz-test-send "z -r api") transitions)
         (eshell/cd root)
         (push (esz-test-send "z -t api") transitions)
         (eshell/cd projects)
         (push (esz-test-send "z -c api") transitions)
         (eshell/cd root)
         (push (esz-test-send "z client api") transitions)
         (eshell/cd projects)
         (push (esz-test-send "z client api") transitions)
         (eshell/cd root)
         (push (esz-test-send
                (concat "z " (eshell-quote-argument ui)))
               transitions)
         (eshell/cd root)
         (push (esz-test-send "z 2026") transitions)
         (eshell/cd root)
         (push (esz-test-send "z no-such-target") transitions)
         (eshell/cd root)
         (setq esz-test-completion-plan
               (list (list :prompt "pattern "
                           :choice (directory-file-name other))))
         (let ((completing-read-function
                #'esz-test-strict-completing-read))
           (push (esz-test-send "z") transitions))
         (list
          :transitions (nreverse transitions)
          :directory-events (nreverse esz-test-directory-events)
          :completion (nreverse esz-test-completion-ledger)
          :final-table (esz-test-table-rows)
          :final-cwd (esz-test-directory)
          :final-history (esz-test-history)))))))"##;
    let expected = expect![[
        r#"OK (:result (:transitions ((:input "z projects.client" :tail "z projects.client\nZ> " :before "[ROOT]" :after "[ROOT]/projects/client" :point-at-end t :history ("z projects.client")) (:input "z -r api" :tail "z -r api\nZ> " :before "[ROOT]" :after "[ROOT]/other/api" :point-at-end t :history ("z -r api" "z projects.client")) (:input "z -t api" :tail "z -t api\nZ> " :before "[ROOT]" :after "[ROOT]/archive/client-api-old" :point-at-end t :history ("z -t api" "z -r api" "z projects.client")) (:input "z -c api" :tail "z -c api\nZ> " :before "[ROOT]/projects" :after "[ROOT]/projects/client/api" :point-at-end t :history ("z -c api" "z -t api" "z -r api" "z projects.client")) (:input "z client api" :tail "z client api\nZ> " :before "[ROOT]" :after "[ROOT]/projects/client/api" :point-at-end t :history ("z client api" "z -c api" "z -t api" "z -r api" "z projects.client")) (:input "z client api" :tail "z client api\nZ> " :before "[ROOT]/projects" :after "[ROOT]/projects/client" :point-at-end t :history ("z client api" "z client api" "z -c api" "z -t api" "z -r api" "z projects.client")) (:input "z [ROOT]/projects/client/ui\\ Ω/" :tail "z [ROOT]/projects/client/ui\\ Ω/\nZ> " :before "[ROOT]" :after "[ROOT]/projects/client/ui Ω" :point-at-end t :history ("z [ROOT]/projects/client/ui\\ Ω/" "z client api" "z client api" "z -c api" "z -t api" "z -r api" "z projects.client")) (:input "z 2026" :tail "z 2026\nZ> " :before "[ROOT]" :after "[ROOT]/archive/2026" :point-at-end t :history ("z 2026" "z [ROOT]/projects/client/ui\\ Ω/" "z client api" "z client api" "z -c api" "z -t api" "z -r api" "z projects.client")) (:input "z no-such-target" :tail "z no-such-target\nZ> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("z no-such-target" "z 2026" "z [ROOT]/projects/client/ui\\ Ω/" "z client api" "z client api" "z -c api" "z -t api" "z -r api" "z projects.client")) (:input "z" :tail "z\nZ> " :before "[ROOT]" :after "[ROOT]/other/api" :point-at-end t :history ("z" "z no-such-target" "z 2026" "z [ROOT]/projects/client/ui\\ Ω/" "z client api" "z client api" "z -c api" "z -t api" "z -r api" "z projects.client"))) :directory-events ("[ROOT]" "[ROOT]" "[ROOT]" "[ROOT]/projects" "[ROOT]" "[ROOT]/projects" "[ROOT]" "[ROOT]" "[ROOT]") :completion (("pattern " (("[ROOT]/projects/client/api" :rank 9 :time "1999992800") ("[ROOT]/archive/2026" :rank 3 :time "1999999900") ("[ROOT]/other/api" :rank 40 :time "1999300000") ("[ROOT]/projects/client/ui Ω" :rank 30 :time "1999300000") ("[ROOT]/archive/client-api-old" :rank 1 :time "1999999970") ("[ROOT]/projects/client" :rank 2 :time "1999300000")) nil t nil nil nil nil)) :final-table (("[ROOT]/archive/2026" 3 "1999999900") ("[ROOT]/archive/client-api-old" 1 "1999999970") ("[ROOT]/other/api" 40 "1999300000") ("[ROOT]/projects/client" 2 "1999300000") ("[ROOT]/projects/client/api" 9 "1999992800") ("[ROOT]/projects/client/ui Ω" 30 "1999300000")) :final-cwd "[ROOT]/other/api" :final-history ("z" "z no-such-target" "z 2026" "z [ROOT]/projects/client/ui\\ Ω/" "z client api" "z client api" "z -c api" "z -t api" "z -r api" "z projects.client")) :cleanup (:owned-reference-live nil :new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :package-hooks-restored t :package-hook-shape (add remove) :query-functions-restored t :completion-remaining nil :commands ("z projects.client" "z -r api" "z -t api" "z -c api" "z client api" "z client api" "z [ROOT]/projects/client/ui\\ Ω/" "z 2026" "z no-such-target" "z") :cleanup-error nil))"#
    ]];
    ParityBatchCase::value(
        "public_z_navigation_distinguishes_every_selection_mode",
        elisp_form,
        expected,
    )
}

fn listing_help_and_eshell_errors_preserve_complete_output() -> ParityBatchCase {
    let elisp_form = r##"(esz-test-run
 "eshell-z-output-errors"
 (lambda (root)
   (let* ((projects (expand-file-name "projects/" root))
          (client (expand-file-name "client/" projects))
          (api (expand-file-name "api/" client))
          (ui (expand-file-name "ui Ω/" client))
          (archive (expand-file-name "archive/client-api-old/" root))
          (other (expand-file-name "other/api/" root))
          (table (make-hash-table :test #'equal))
          (eshell-z-freq-dir-hash-table-file-name nil)
          (eshell-z-freq-dir-hash-table table)
          (eshell-z-exclude-dirs (list root))
          reports)
     (dolist (directory (list api ui archive other))
       (make-directory directory t))
     (esz-test-put table (directory-file-name client) 2 1999300000)
     (esz-test-put table (directory-file-name api) 9 1999992800)
     (esz-test-put table (directory-file-name ui) 30 1999300000)
     (esz-test-put table (directory-file-name archive) 1 1999999970)
     (esz-test-put table (directory-file-name other) 40 1999300000)
     (let ((buffer (esz-test-new-session " *esz-output-errors*" root)))
       (with-current-buffer buffer
         (dolist (command '("z -l client" "z -l -r client"
                            "z -l -t client" "z -h" "z --bogus"
                            "z [" "echo recovered"))
           (push (esz-test-send command) reports))
         (list :reports (nreverse reports)
               :cwd (esz-test-directory)
               :table (esz-test-table-rows)
               :history (esz-test-history)))))))"##;
    let expected = expect![[
        r#"OK (:result (:reports ((:input "z -l client" :tail "z -l client\n0.5        [ROOT]/projects/client\n4          [ROOT]/archive/client-api-old\n7.5        [ROOT]/projects/client/ui Ω\n18         [ROOT]/projects/client/apiZ> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("z -l client")) (:input "z -l -r client" :tail "z -l -r client\n1          [ROOT]/archive/client-api-old\n2          [ROOT]/projects/client\n9          [ROOT]/projects/client/api\n30         [ROOT]/projects/client/ui ΩZ> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("z -l -r client" "z -l client")) (:input "z -l -t client" :tail "z -l -t client\n-700000    [ROOT]/projects/client\n-700000    [ROOT]/projects/client/ui Ω\n-7200      [ROOT]/projects/client/api\n-30        [ROOT]/archive/client-api-oldZ> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("z -l -t client" "z -l -r client" "z -l client")) (:input "z -h" :tail "z -h\nz: usage: z [-chlrtx] [regex1 regex2 ... regexn]\n\n    -c, --current        estrict matches to subdirectories of the current directory\n    -h, --help           show a brief help message\n    -l, --list           list only\n    -r, --rank           match by rank only\n    -t, --time           match by recent access only\n    -x, --delete         remove the current directory from the datafile\n\nexamples:\n\n    z foo         cd to most frecent dir matching foo\n    z foo bar     cd to most frecent dir matching foo, then bar\n    z -r foo      cd to highest ranked dir matching foo\n    z -t foo      cd to most recently accessed dir matching foo\n    z -l foo      list all dirs matching foo (by frecency)\nZ> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("z -h" "z -l -t client" "z -l -r client" "z -l client")) (:input "z --bogus" :tail "z --bogus\nz: z: unrecognized option --bogus\nZ> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("z --bogus" "z -h" "z -l -t client" "z -l -r client" "z -l client")) (:input "z [" :tail "z [\nInvalid regexp: \"Unmatched [ or [^\"\nZ> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("z [" "z --bogus" "z -h" "z -l -t client" "z -l -r client" "z -l client")) (:input "echo recovered" :tail "echo recovered\nrecovered\nZ> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("echo recovered" "z [" "z --bogus" "z -h" "z -l -t client" "z -l -r client" "z -l client"))) :cwd "[ROOT]" :table (("[ROOT]/archive/client-api-old" 1 "1999999970") ("[ROOT]/other/api" 40 "1999300000") ("[ROOT]/projects/client" 2 "1999300000") ("[ROOT]/projects/client/api" 9 "1999992800") ("[ROOT]/projects/client/ui Ω" 30 "1999300000")) :history ("echo recovered" "z [" "z --bogus" "z -h" "z -l -t client" "z -l -r client" "z -l client")) :cleanup (:owned-reference-live nil :new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :package-hooks-restored t :package-hook-shape (add remove) :query-functions-restored t :completion-remaining nil :commands ("z -l client" "z -l -r client" "z -l -t client" "z -h" "z --bogus" "z [" "echo recovered") :cleanup-error nil))"#
    ]];
    ParityBatchCase::value(
        "listing_help_and_eshell_errors_preserve_complete_output",
        elisp_form,
        expected,
    )
}

fn interactive_eshell_z_and_completion_at_point_use_real_eshell() -> ParityBatchCase {
    let elisp_form = r##"(esz-test-run
 "eshell-z-interactive"
 (lambda (root)
   (let* ((start (expand-file-name "start/" root))
          (release (expand-file-name "client release Ω/" root))
          (apostrophe (expand-file-name "client's release/" root))
          (table (make-hash-table :test #'equal))
          (eshell-z-freq-dir-hash-table-file-name nil)
          (eshell-z-freq-dir-hash-table table)
          (eshell-z-exclude-dirs nil)
          (eshell-z-change-dir-hook
           (list #'esz-test-change-directory-observer))
          success option-completion path-completion path-execution failure)
     (dolist (directory (list start release apostrophe))
       (make-directory directory t))
     (esz-test-put table (directory-file-name release) 3 1999999990)
     (esz-test-put table (directory-file-name apostrophe) 2 1999999999)
     (let ((eshell-buffer (esz-test-new-session "*eshell*" start))
           (ordinary (generate-new-buffer " *esz-interactive-origin*")))
       (push ordinary esz-test-owned-buffers)
       (switch-to-buffer ordinary)
       (setq esz-test-completion-plan
             (list (list :prompt "pattern "
                         :choice (directory-file-name release))))
       (let ((completing-read-function #'esz-test-strict-completing-read))
         (call-interactively #'eshell-z))
       (with-current-buffer eshell-buffer
         (setq success
               (list :selected (eq (current-buffer) (window-buffer))
                     :mode major-mode :cwd (esz-test-directory)
                     :text (buffer-substring-no-properties
                            (point-min) (point-max))
                     :history (esz-test-history)
                     :table (esz-test-table-rows)
                     :process (and (get-buffer-process (current-buffer)) t)
                     :directory-events
                     (nreverse esz-test-directory-events)))
         (setq esz-test-directory-events nil)
         (setq option-completion
               (esz-test-completion-at-point "z --ra"))
         (setq path-completion
               (esz-test-completion-at-point
                (concat "z "
                        (eshell-quote-argument
                         (concat root "client r")))))
         (let ((completed (plist-get path-completion :after)))
           (delete-region eshell-last-output-end (point-max))
           (setq path-execution (esz-test-send completed))))
       (switch-to-buffer ordinary)
       (esz-test-observe-messages
        (lambda () (eshell-z (directory-file-name apostrophe))))
       (with-current-buffer eshell-buffer
         (setq failure
               (list :cwd (esz-test-directory)
                     :text (buffer-substring-no-properties
                            (point-min) (point-max))
                     :history (esz-test-history)
                     :table (esz-test-table-rows)
                     :process (and (get-buffer-process (current-buffer)) t))))
       (list :completion (nreverse esz-test-completion-ledger)
             :success success
             :option-completion option-completion
             :path-completion path-completion
             :path-execution path-execution
             :apostrophe-failure failure
             :messages (nreverse esz-test-message-ledger)
             :directory-events
             (nreverse esz-test-directory-events))))))"##;
    let expected = expect![[
        r#"OK (:result (:completion (("pattern " (("[ROOT]/client release Ω" :rank 3 :time "1999999990") ("[ROOT]/client's release" :rank 2 :time "1999999999") ("[ROOT]/start" :rank 1 :time "2000000000")) nil t nil nil nil nil)) :success (:selected t :mode eshell-mode :cwd "[ROOT]/client release Ω" :text "Z> cd '[ROOT]/client release Ω'\nZ> " :history ("cd '[ROOT]/client release Ω'") :table (("[ROOT]/client release Ω" 4 "2000000000") ("[ROOT]/client's release" 2 "1999999999") ("[ROOT]/start" 1 "2000000000")) :process nil :directory-events nil) :option-completion (:before "z --ra" :return t :after "z --rank " :point-at-end t) :path-completion (:before "z [ROOT]/client\\ r" :return t :after "z [ROOT]/client\\ release\\ Ω " :point-at-end t) :path-execution (:input "z [ROOT]/client\\ release\\ Ω " :tail "z [ROOT]/client\\ release\\ Ω \nZ> " :before "[ROOT]/client release Ω" :after "[ROOT]/client release Ω" :point-at-end t :history ("z [ROOT]/client\\ release\\ Ω " "cd '[ROOT]/client release Ω'")) :apostrophe-failure (:cwd "[ROOT]/client release Ω" :text "Z> cd '[ROOT]/client release Ω'\nZ> z [ROOT]/client\\ release\\ Ω \nZ> cd '[ROOT]/client's release'\n" :history ("z [ROOT]/client\\ release\\ Ω " "cd '[ROOT]/client release Ω'") :table (("[ROOT]/client release Ω" 5 "2000000000") ("[ROOT]/client's release" 2 "1999999999") ("[ROOT]/start" 1 "2000000000")) :process nil) :messages ("Expecting completion of delimiter ' ...") :directory-events ("[ROOT]/client release Ω")) :cleanup (:owned-reference-live nil :new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :package-hooks-restored t :package-hook-shape (add remove) :query-functions-restored t :completion-remaining nil :commands ("z [ROOT]/client\\ release\\ Ω ") :cleanup-error nil))"#
    ]];
    ParityBatchCase::value(
        "interactive_eshell_z_and_completion_at_point_use_real_eshell",
        elisp_form,
        expected,
    )
}

fn real_pcomplete_exposes_options_and_directory_records() -> ParityBatchCase {
    let elisp_form = r##"(esz-test-run
 "eshell-z-pcomplete"
 (lambda (root)
   (let* ((client (expand-file-name "projects/client api/" root))
          (archive (expand-file-name "archive/client-old/" root))
          (table (make-hash-table :test #'equal))
          (eshell-z-freq-dir-hash-table-file-name nil)
          (eshell-z-freq-dir-hash-table table)
          (eshell-z-exclude-dirs (list root)))
     (make-directory client t)
     (make-directory archive t)
     (esz-test-put table (directory-file-name client) 9 100)
     (esz-test-put table (directory-file-name archive) 2 200)
     (let ((buffer (esz-test-new-session " *esz-pcomplete*" root)))
       (with-current-buffer buffer
         (list :short (esz-test-pcomplete "z -")
               :long (esz-test-pcomplete "z --")
               :prefix (esz-test-pcomplete "z cli")
               :full (esz-test-pcomplete "z client")
               :table (esz-test-table-rows)
               :process (and (get-buffer-process (current-buffer)) t)))))))"##;
    let expected = expect![[
        r#"OK (:result (:short (:input "z -" :point 7 :stub "-" :result ("-c" "-h" "-l" "-r" "-t" "-x")) :long (:input "z --" :point 8 :stub "--" :result ("--current" "--help" "--list" "--rank" "--time" "--delete")) :prefix (:input "z cli" :point 9 :stub "cli" :result (("[ROOT]/archive/client-old" :rank 2 :time "200") ("[ROOT]/projects/client api" :rank 9 :time "100"))) :full (:input "z client" :point 12 :stub "client" :result (("[ROOT]/archive/client-old" :rank 2 :time "200") ("[ROOT]/projects/client api" :rank 9 :time "100"))) :table (("[ROOT]/archive/client-old" 2 "200") ("[ROOT]/projects/client api" 9 "100")) :process nil) :cleanup (:owned-reference-live nil :new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :package-hooks-restored t :package-hook-shape (add remove) :query-functions-restored t :completion-remaining nil :commands nil :cleanup-error nil))"#
    ]];
    ParityBatchCase::value(
        "real_pcomplete_exposes_options_and_directory_records",
        elisp_form,
        expected,
    )
}

fn malformed_persistence_and_filesystem_failures_remain_visible() -> ParityBatchCase {
    let elisp_form = r##"(esz-test-run
 "eshell-z-failures"
 (lambda (root)
   (let* ((valid (expand-file-name "valid target/" root))
          (stale (expand-file-name "stale target/" root))
          (data (expand-file-name "broken-z-data" root))
          (missing (expand-file-name "missing-parent/z-data" root))
          (readonly (expand-file-name "readonly-z-data" root))
          malformed missing-parent readonly-state stale-state)
     (make-directory valid t)
     (write-region "broken-row\n" nil data nil 'silent)
     (let ((eshell-z-freq-dir-hash-table-file-name data)
           (eshell-z-freq-dir-hash-table nil))
       (setq malformed
             (list
              :outcome (esz-test-capture
                        (lambda () (eshell/z "target")))
              :table eshell-z-freq-dir-hash-table
              :file (esz-test-file-string data))))
     (let ((eshell-z-freq-dir-hash-table-file-name missing)
           (eshell-z-freq-dir-hash-table nil)
           (eshell-z-exclude-dirs nil))
       (setq missing-parent
             (list
              :outcome
              (esz-test-capture
               (lambda ()
                 (let ((buffer
                        (esz-test-new-session
                         " *esz-missing-parent*" valid)))
                   (with-current-buffer buffer
                     (list :mode major-mode
                           :table (esz-test-table-rows))))))
              :file-exists (file-exists-p missing)
              :live-owned
              (delq nil
                    (mapcar (lambda (buffer)
                              (and (buffer-live-p buffer)
                                   (list (buffer-name buffer)
                                         (buffer-local-value
                                          'major-mode buffer))))
                            esz-test-owned-buffers)))))
     (write-region
      (format "%s|4|100\n" (directory-file-name valid))
      nil readonly nil 'silent)
     (set-file-modes readonly #o444)
     (push readonly esz-test-readonly-files)
     (let ((eshell-z-freq-dir-hash-table-file-name readonly)
           (eshell-z-freq-dir-hash-table nil)
           (eshell-z-exclude-dirs nil))
       (setq readonly-state
             (esz-test-observe-messages
              (lambda ()
                (let ((buffer
                       (esz-test-new-session
                        " *esz-readonly*" valid)))
                  (with-current-buffer buffer
                    (let ((value
                           (gethash (directory-file-name valid)
                                    eshell-z-freq-dir-hash-table)))
                      (list :rank (plist-get (cdr value) :rank)
                            :time (plist-get (cdr value) :time)
                            :table (esz-test-table-rows)
                            :disk (esz-test-file-string readonly))))))))
     (let ((table (make-hash-table :test #'equal))
           (eshell-z-freq-dir-hash-table-file-name nil)
           (eshell-z-exclude-dirs (list root)))
       (esz-test-put table stale 100 2000000000)
       (esz-test-put table (directory-file-name valid) 1 2000000000)
       (let ((eshell-z-freq-dir-hash-table table)
             (buffer (esz-test-new-session " *esz-stale*" root)))
         (with-current-buffer buffer
           (setq stale-state
                 (list :jump (esz-test-send "z target")
                       :list (esz-test-send "z -l target")
                       :cwd (esz-test-directory)
                       :table (esz-test-table-rows))))))
     (list :malformed malformed :missing-parent missing-parent
           :readonly readonly-state
           :messages (nreverse esz-test-message-ledger)
           :stale stale-state)))))"##;
    let expected = expect![[
        r#"OK (:result (:malformed (:outcome (:signal wrong-type-argument :data (stringp nil)) :table nil :file "broken-row\n") :missing-parent (:outcome (:signal file-missing :data ("Opening output file" "No such file or directory" "[ROOT]/missing-parent/z-data")) :file-exists nil :live-owned ((" *esz-missing-parent*" eshell-mode))) :readonly (:rank 5 :time "2000000000" :table (("[ROOT]/valid target" 5 "2000000000")) :disk "[ROOT]/valid target|4|100\n") :messages ("Cannot write freq-dir-hash-table file [ROOT]/readonly-z-data") :stale (:jump (:input "z target" :tail "z target\nZ> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("z target")) :list (:input "z -l target" :tail "z -l target\n4          [ROOT]/valid target\n400        [ROOT]/stale target/Z> " :before "[ROOT]" :after "[ROOT]" :point-at-end t :history ("z -l target" "z target")) :cwd "[ROOT]" :table (("[ROOT]/stale target/" 100 "2000000000") ("[ROOT]/valid target" 1 "2000000000")))) :cleanup (:owned-reference-live nil :new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :package-hooks-restored t :package-hook-shape (add remove) :query-functions-restored t :completion-remaining nil :commands ("z target" "z -l target") :cleanup-error nil))"#
    ]];
    ParityBatchCase::value(
        "malformed_persistence_and_filesystem_failures_remain_visible",
        elisp_form,
        expected,
    )
}

fn deleting_the_only_record_resurrects_it_after_restart() -> ParityBatchCase {
    let elisp_form = r##"(esz-test-run
 "eshell-z-last-record"
 (lambda (root)
   (let* ((only (expand-file-name "only target/" root))
          (data (expand-file-name "z-data" root))
          (eshell-z-freq-dir-hash-table-file-name data)
          (eshell-z-freq-dir-hash-table nil)
          (eshell-z-exclude-dirs nil)
          deleted restarted)
     (make-directory only t)
     (write-region
      (format "%s|5|1999999900\n" (directory-file-name only))
      nil data nil 'silent)
     (let ((buffer (esz-test-new-session " *esz-delete-only*" only)))
       (with-current-buffer buffer
         (let ((transition (esz-test-send "z -x")))
           (setq deleted
                 (list :transition transition
                       :count (hash-table-count
                               eshell-z-freq-dir-hash-table)
                       :remove eshell-z--remove-p
                       :table (esz-test-table-rows)
                       :file (esz-test-file-string data))))))
     (setq eshell-z-freq-dir-hash-table nil)
     (let ((buffer (esz-test-new-session " *esz-delete-restart*" only)))
       (with-current-buffer buffer
         (let ((value (gethash (directory-file-name only)
                               eshell-z-freq-dir-hash-table)))
           (setq restarted
                 (list :count (hash-table-count
                               eshell-z-freq-dir-hash-table)
                       :rank (plist-get (cdr value) :rank)
                       :time (plist-get (cdr value) :time)
                       :table (esz-test-table-rows)
                       :file (esz-test-file-string data))))))
     (list :deleted deleted :restarted restarted))))"##;
    let expected = expect![[
        r#"OK (:result (:deleted (:transition (:input "z -x" :tail "z -x\nZ> " :before "[ROOT]/only target" :after "[ROOT]/only target" :point-at-end t :history ("z -x")) :count 0 :remove nil :table nil :file "[ROOT]/only target|7|2000000000\n") :restarted (:count 1 :rank 8 :time "2000000000" :table (("[ROOT]/only target" 8 "2000000000")) :file "[ROOT]/only target|8|2000000000\n")) :cleanup (:owned-reference-live nil :new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :package-hooks-restored t :package-hook-shape (add remove) :query-functions-restored t :completion-remaining nil :commands ("z -x") :cleanup-error nil))"#
    ]];
    ParityBatchCase::value(
        "deleting_the_only_record_resurrects_it_after_restart",
        elisp_form,
        expected,
    )
}

pub(super) fn public_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        real_eshell_commands_persist_jump_delete_and_survive_restart(),
        public_z_navigation_distinguishes_every_selection_mode(),
        listing_help_and_eshell_errors_preserve_complete_output(),
        interactive_eshell_z_and_completion_at_point_use_real_eshell(),
        real_pcomplete_exposes_options_and_directory_records(),
        malformed_persistence_and_filesystem_failures_remain_visible(),
        deleting_the_only_record_resurrects_it_after_restart(),
    ]
}
