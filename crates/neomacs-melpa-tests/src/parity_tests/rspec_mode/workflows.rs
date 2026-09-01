use expect_test::expect;

use super::ParityBatchCase;

fn source_and_spec_switching_follows_the_real_method() -> ParityBatchCase {
    ParityBatchCase::value(
        "source_and_spec_switching_follows_the_real_method",
        r####"
(rspec360-test-run
 "navigation"
 (lambda (world)
   (let* ((source (rspec360-test-visit world "lib/inventory/ledger.rb"))
          (source-window (selected-window))
          source-state spec-state back-state other-state)
     (switch-to-buffer source)
     (goto-char (point-min))
     (search-forward "def label")
     (setq source-state
           (append
            (rspec360-test-relative-state world source)
            (list :key-e (key-binding (kbd "C-c , e"))
                  :key-other (key-binding (kbd "C-c , 4 e"))
                  :lighter (assq 'rspec-verifiable-mode minor-mode-alist))))
     (rspec360-test-command-loop "C-c , e")
     (rspec360-test-own-buffer (current-buffer))
     (setq spec-state
           (append
            (rspec360-test-relative-state world (current-buffer))
            (list :key-d (key-binding (kbd "C-c , d"))
                  :lighter (assq 'rspec-mode minor-mode-alist)
                  :imenu (rspec360-test-index-positions
                          (imenu--make-index-alist t)))))
     (rspec360-test-command-loop "C-c , e")
     (setq back-state (rspec360-test-relative-state world (current-buffer)))
     (goto-char (point-min))
     (search-forward "def total")
     (rspec360-test-command-loop "C-c , 4 e")
     (rspec360-test-own-buffer (current-buffer))
     (setq other-state
           (list :selected (rspec360-test-relative-state world (current-buffer))
                 :windows
                 (mapcar
                  (lambda (window)
                    (with-current-buffer (window-buffer window)
                      (list :selected (eq window (selected-window))
                            :file (file-relative-name
                                   buffer-file-name (plist-get world :project))
                            :point (window-point window))))
                  (window-list nil 'no-minibuf))
                 :source-window-live (window-live-p source-window)))
     (list :source source-state :spec spec-state :back back-state
           :other other-state))))
"####,
        expect![[
            r##"OK (:result (:source (:file "lib/inventory/ledger.rb" :major ruby-mode :rspec nil :verifiable t :line 7 :point 92 :text "    def label(id)" :key-e rspec-toggle-spec-and-target-find-example :key-other rspec-find-spec-or-target-find-example-other-window :lighter (rspec-verifiable-mode "")) :spec (:file "spec/inventory/ledger_spec.rb" :major ruby-mode :rspec t :verifiable nil :line 18 :point 471 :text "  describe \"#label\" do" :key-d rspec-toggle-example-pendingness :lighter (rspec-mode " RSpec") :imenu (("*Rescan*" . -99) ("Examples" ("  describe \"#total\" do" . 157) ("    it \"adds realistic line items\" do" . 180) ("    it \"reports a bad expected total\" do" . 276) ("  describe \"#label\" do" . 452) ("    it \"preserves Unicode identifiers\" do" . 475)))) :back (:file "lib/inventory/ledger.rb" :major ruby-mode :rspec nil :verifiable t :line 7 :point 92 :text "    def label(id)") :other (:selected (:file "spec/inventory/ledger_spec.rb" :major ruby-mode :rspec t :verifiable nil :line 7 :point 176 :text "  describe \"#total\" do") :windows ((:selected t :file "spec/inventory/ledger_spec.rb" :point 176) (:selected nil :file "lib/inventory/ledger.rb" :point 46)) :source-window-live t)) :cleanup clean)"##
        ]],
    )
}

fn pendingness_is_a_real_reversible_spec_edit() -> ParityBatchCase {
    ParityBatchCase::value(
        "pendingness_is_a_real_reversible_spec_edit",
        r####"
(rspec360-test-run
 "pending"
 (lambda (world)
   (let ((buffer (rspec360-test-visit world "spec/inventory/ledger_spec.rb")))
     (switch-to-buffer buffer)
     (goto-char (point-min))
     (search-forward "adds realistic line items")
     (search-forward "ledger.total")
     (set-buffer-modified-p nil)
     (setq buffer-undo-list nil)
     (let ((before (buffer-string))
           (before-point (point))
           (before-undo buffer-undo-list)
           on off failure)
       (rspec360-test-command-loop "C-c , d")
       (setq on
             (list :bytes (buffer-string) :point (point)
                   :modified (buffer-modified-p)
                   :pending (rspec-example-pending-p)
                   :undo-changed (not (equal buffer-undo-list before-undo))))
       (rspec360-test-command-loop "C-c , d")
       (setq off
             (list :bytes (buffer-string) :point (point)
                   :modified (buffer-modified-p)
                   :pending (rspec-example-pending-p)
                   :restored (equal (buffer-string) before)))
       (set-buffer-modified-p nil)
       (setq buffer-undo-list nil)
       (goto-char (point-min))
       (let ((bytes (buffer-string)) (point (point))
             (undo buffer-undo-list) (modified (buffer-modified-p)))
         (setq failure
               (list :condition
                     (rspec360-test-condition
                      (lambda ()
                        (call-interactively #'rspec-toggle-example-pendingness)))
                     :bytes-same (equal bytes (buffer-string))
                     :point-same (= point (point))
                     :undo-same (equal undo buffer-undo-list)
                     :modified-same (eq modified (buffer-modified-p)))))
       (list :before-point before-point :on on :off off :failure failure)))))
"####,
        expect![[
            r##"OK (:result (:before-point 243 :on (:bytes "require_relative \"../../lib/inventory/ledger\"\nrequire \"rspec/expectations\"\n\nRSpec.describe Inventory::Ledger do\n  subject(:ledger) { described_class.new }\n\n  describe \"#total\" do\n    it \"adds realistic line items\" do\n      pending\n      expect(ledger.total([12, 8, 5])).to eq(25)\n    end\n\n    it \"reports a bad expected total\" do\n      puts \"     [Screenshot Image]: ./capybara/order receipt_123.png\"\n      expect(ledger.total([12, 8, 5])).to eq(24)\n    end\n  end\n\n  describe \"#label\" do\n    it \"preserves Unicode identifiers\" do\n      expect(ledger.label(7)).to eq(\"order-界-8\")\n    end\n  end\nend\n" :point 257 :modified t :pending 231 :undo-changed t) :off (:bytes "require_relative \"../../lib/inventory/ledger\"\nrequire \"rspec/expectations\"\n\nRSpec.describe Inventory::Ledger do\n  subject(:ledger) { described_class.new }\n\n  describe \"#total\" do\n    it \"adds realistic line items\" do\n      expect(ledger.total([12, 8, 5])).to eq(25)\n    end\n\n    it \"reports a bad expected total\" do\n      puts \"     [Screenshot Image]: ./capybara/order receipt_123.png\"\n      expect(ledger.total([12, 8, 5])).to eq(24)\n    end\n  end\n\n  describe \"#label\" do\n    it \"preserves Unicode identifiers\" do\n      expect(ledger.label(7)).to eq(\"order-界-8\")\n    end\n  end\nend\n" :point 243 :modified t :pending nil :restored t) :failure (:condition (:signal error :data ("Unable to find an example") :message "Unable to find an example") :bytes-same t :point-same t :undo-same t :modified-same t)) :cleanup clean)"##
        ]],
    )
}

fn single_example_verification_runs_real_compilation() -> ParityBatchCase {
    ParityBatchCase::value(
        "single_example_verification_runs_real_compilation",
        r####"
(rspec360-test-run
 "single"
 (lambda (world)
   (let ((spec (rspec360-test-visit world "spec/inventory/ledger_spec.rb")))
     (switch-to-buffer spec)
     (goto-char (point-min))
     (forward-line 7)
     (rspec360-test-command-loop "C-c , s")
     (let* ((compilation (rspec360-test-owned-compilation-buffer))
            (settled (rspec360-test-wait world compilation 1)))
       (list
        :runner (list :bundler rspec-use-bundler-when-possible
                      :rake rspec-use-rake-when-possible
                      :zeus rspec-use-zeus-when-possible
                      :spring rspec-use-spring-when-possible
                      :opts rspec-use-opts-file-when-available)
        :trace (rspec360-test-invocations world)
        :misses (rspec360-test-misses world)
        :mode (with-current-buffer compilation major-mode)
        :name (buffer-name compilation)
        :directory (with-current-buffer compilation default-directory)
        :ansi-left
        (with-current-buffer compilation
          (and (string-match-p "\e\\[" (buffer-string)) t))
        :observations
        (rspec360-test-compilation-observations
         compilation '("adds realistic line items" "1 example, 0 failures"))
        :last-directory rspec-last-directory
        :settled settled)))))
"####,
        expect![[
            r#"OK (:result (:runner (:bundler t :rake nil :zeus nil :spring nil :opts t) :trace ("{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb:8\"],\"cwd\":\"[ROOT]\",\"debug\":\"true\"}") :misses nil :mode rspec-compilation-mode :name "*rspec-compilation*" :directory "[ROOT]/" :ansi-left nil :observations ((:pattern "adds realistic line items" :line 9 :runs ((:columns (0 . 29) :text "    adds realistic line items" :face nil :font-lock-face nil :message nil)) :overlays ((:columns (0 . 29) :face (:foreground "green3") :font-lock-face nil :priority nil))) (:pattern "1 example, 0 failures" :line 12 :runs ((:columns (0 . 21) :text "1 example, 0 failures" :face compilation-info :font-lock-face nil :message nil)) :overlays ((:columns (0 . 21) :face (:foreground "green3") :font-lock-face nil :priority nil)))) :last-directory "[ROOT]/spec/inventory/" :settled (:process exit :buffer-process nil :finish (("*rspec-compilation*" "finished")) :hooks ((:before nil) (:after nil)) :failed nil :text "-*- mode: rspec-compilation; default-directory: \"[ROOT]/\" -*-\nRSpec Compilation started at <TIME>\n\nbundle exec rspec --options .rspec spec/inventory/ledger_spec.rb:8\nRun options: include {:locations=>{\"./spec/inventory/ledger_spec.rb\"=>[8]}}\n\nInventory::Ledger\n  #total\n    adds realistic line items\n\nFinished in <DURATION> seconds (files took <DURATION> seconds to load)\n1 example, 0 failures\n\n\nRSpec Compilation finished at <TIME>, duration <DURATION>\n")) :cleanup clean)"#
        ]],
    )
}

fn failed_verification_is_navigable_and_records_failures() -> ParityBatchCase {
    ParityBatchCase::value(
        "failed_verification_is_navigable_and_records_failures",
        r####"
(rspec360-test-run
 "failure-navigation"
 (lambda (world)
   (let ((spec (rspec360-test-visit world "spec/inventory/ledger_spec.rb")))
     (switch-to-buffer spec)
     (goto-char (point-min))
     (rspec360-test-command-loop "C-c , v")
     (let* ((compilation (rspec360-test-owned-compilation-buffer))
            (settled (rspec360-test-wait world compilation 1))
            navigation terminal reset)
       (with-current-buffer compilation
         (setq-local compilation-skip-threshold 0)
         (setq next-error-last-buffer compilation))
       (next-error 1 t)
       (push (rspec360-test-navigation-state world) navigation)
       (next-error 1)
       (push (rspec360-test-navigation-state world) navigation)
       (next-error 1)
       (push (rspec360-test-navigation-state world) navigation)
       (next-error 1)
       (push (rspec360-test-navigation-state world) navigation)
       (next-error 1)
       (push (rspec360-test-navigation-state world) navigation)
       (setq terminal
             (rspec360-test-condition (lambda () (next-error 1))))
       (switch-to-buffer compilation)
       (goto-char (point-min))
       (setq next-error-last-buffer compilation)
       (next-error 1 t)
       (setq reset (rspec360-test-navigation-state world))
       (list
        :trace (rspec360-test-invocations world)
        :misses (rspec360-test-misses world)
        :ansi-left
        (with-current-buffer compilation
          (and (string-match-p "\e\\[" (buffer-string)) t))
        :observations
        (rspec360-test-compilation-observations
         compilation
         '("[Screenshot Image]: ./capybara/order receipt_123.png"
           "reports a bad expected total (FAILED - 1)"
           "# ./spec/inventory/ledger_spec.rb:14:in"
           "3 examples, 2 failures"
           "rspec ./spec/inventory/ledger_spec.rb:12"))
        :navigation (nreverse navigation) :terminal terminal :reset reset
        :settled settled)))))
"####,
        expect![[
            r##"OK (:result (:trace ("{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb\"],\"cwd\":\"[ROOT]\",\"debug\":\"true\"}") :misses nil :ansi-left nil :observations ((:pattern "[Screenshot Image]: ./capybara/order receipt_123.png" :line 9 :runs ((:columns (0 . 25) :text "     [Screenshot Image]: " :face nil :font-lock-face nil :message nil) (:columns (25 . 57) :text "./capybara/order receipt_123.png" :face nil :font-lock-face (compilation-info underline) :message (:type 0 :rule rspec-capybara-screenshot :file "./capybara/order receipt_123.png" :line nil :column nil))) :overlays nil) (:pattern "reports a bad expected total (FAILED - 1)" :line 10 :runs ((:columns (0 . 45) :text "    reports a bad expected total (FAILED - 1)" :face nil :font-lock-face nil :message nil)) :overlays ((:columns (0 . 45) :face (:foreground "red3") :font-lock-face nil :priority nil))) (:pattern "# ./spec/inventory/ledger_spec.rb:14:in" :line 23 :runs ((:columns (0 . 7) :text "     # " :face nil :font-lock-face nil :message nil) (:columns (7 . 38) :text "./spec/inventory/ledger_spec.rb" :face nil :font-lock-face (compilation-error underline) :message (:type 2 :rule rspec :file "./spec/inventory/ledger_spec.rb" :line 14 :column nil)) (:columns (38 . 39) :text ":" :face nil :font-lock-face nil :message nil) (:columns (39 . 41) :text "14" :face nil :font-lock-face compilation-line-number :message nil) (:columns (41 . 83) :text ":in `block (3 levels) in <top (required)>'" :face nil :font-lock-face nil :message nil)) :overlays ((:columns (5 . 83) :face (:foreground "cyan3") :font-lock-face nil :priority nil))) (:pattern "3 examples, 2 failures" :line 35 :runs ((:columns (0 . 12) :text "3 examples, " :face nil :font-lock-face nil :message nil) (:columns (12 . 22) :text "2 failures" :face compilation-error :font-lock-face nil :message nil)) :overlays ((:columns (0 . 22) :face (:foreground "red3") :font-lock-face nil :priority nil))) (:pattern "rspec ./spec/inventory/ledger_spec.rb:12" :line 39 :runs ((:columns (0 . 6) :text "rspec " :face nil :font-lock-face nil :message nil) (:columns (6 . 37) :text "./spec/inventory/ledger_spec.rb" :face nil :font-lock-face (compilation-error underline) :message (:type 2 :rule rspec-summary :file "./spec/inventory/ledger_spec.rb" :line 12 :column nil)) (:columns (37 . 38) :text ":" :face nil :font-lock-face nil :message nil) (:columns (38 . 40) :text "12" :face nil :font-lock-face compilation-line-number :message nil) (:columns (40 . 95) :text " # Inventory::Ledger#total reports a bad expected total" :face nil :font-lock-face nil :message nil)) :overlays ((:columns (0 . 40) :face (:foreground "red3") :font-lock-face nil :priority nil) (:columns (41 . 95) :face (:foreground "cyan3") :font-lock-face nil :priority nil)))) :navigation ((:file "capybara/order receipt_123.png" :major fundamental-mode :line 1 :point 1 :text :binary-image) (:file "spec/inventory/ledger_spec.rb" :major ruby-mode :line 14 :point 394 :text "      expect(ledger.total([12, 8, 5])).to eq(24)") (:file "spec/inventory/ledger_spec.rb" :major ruby-mode :line 20 :point 523 :text "      expect(ledger.label(7)).to eq(\"order-界-8\")") (:file "spec/inventory/ledger_spec.rb" :major ruby-mode :line 12 :point 280 :text "    it \"reports a bad expected total\" do") (:file "spec/inventory/ledger_spec.rb" :major ruby-mode :line 19 :point 479 :text "    it \"preserves Unicode identifiers\" do")) :terminal (:signal user-error :data ("Past last error") :message "Past last error") :reset (:file "capybara/order receipt_123.png" :major fundamental-mode :line 1 :point 1 :text :binary-image) :settled (:process exit :buffer-process nil :finish (("*rspec-compilation*" "exited abnormally with code 1")) :hooks ((:before nil) (:after ("./spec/inventory/ledger_spec.rb:12" "./spec/inventory/ledger_spec.rb:19"))) :failed ("./spec/inventory/ledger_spec.rb:12" "./spec/inventory/ledger_spec.rb:19") :text "-*- mode: rspec-compilation; default-directory: \"[ROOT]/\" -*-\nRSpec Compilation started at <TIME>\n\nbundle exec rspec --options .rspec spec/inventory/ledger_spec.rb\n\nInventory::Ledger\n  #total\n    adds realistic line items\n     [Screenshot Image]: ./capybara/order receipt_123.png\n    reports a bad expected total (FAILED - 1)\n  #label\n    preserves Unicode identifiers (FAILED - 2)\n\nFailures:\n\n  1) Inventory::Ledger#total reports a bad expected total\n     Failure/Error: expect(ledger.total([12, 8, 5])).to eq(24)\n     \n       expected: 24\n            got: 25\n     \n       (compared using ==)\n     # ./spec/inventory/ledger_spec.rb:14:in `block (3 levels) in <top (required)>'\n\n  2) Inventory::Ledger#label preserves Unicode identifiers\n     Failure/Error: expect(ledger.label(7)).to eq(\"order-界-8\")\n     \n       expected: \"order-界-8\"\n            got: \"order-界-7\"\n     \n       (compared using ==)\n     # ./spec/inventory/ledger_spec.rb:20:in `block (3 levels) in <top (required)>'\n\nFinished in <DURATION> seconds (files took <DURATION> seconds to load)\n3 examples, 2 failures\n\nFailed examples:\n\nrspec ./spec/inventory/ledger_spec.rb:12 # Inventory::Ledger#total reports a bad expected total\nrspec ./spec/inventory/ledger_spec.rb:19 # Inventory::Ledger#label preserves Unicode identifiers\n\n\nRSpec Compilation exited abnormally with code 1 at <TIME>, duration <DURATION>\n")) :cleanup clean)"##
        ]],
    )
}

fn last_failed_rerun_and_yank_preserve_session_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "last_failed_rerun_and_yank_preserve_session_state",
        r####"
(rspec360-test-run
 "session"
 (lambda (world)
   (let ((spec (rspec360-test-visit world "spec/inventory/ledger_spec.rb")))
     (switch-to-buffer spec)
     (rspec360-test-command-loop "C-c , v")
     (let ((compilation (rspec360-test-owned-compilation-buffer)))
       (rspec360-test-wait world compilation 1)
       (switch-to-buffer spec)
       (rspec360-test-command-loop "C-c , f")
       (rspec360-test-wait world compilation 2)
       (switch-to-buffer spec)
       (rspec360-test-command-loop "C-c , r")
       (rspec360-test-wait world compilation 3)
       (let ((calls-before-yank (length (rspec360-test-invocations world)))
             (kill-ring nil) (kill-ring-yank-pointer nil)
             command)
         (switch-to-buffer spec)
         (rspec360-test-command-loop "C-c , y")
         (setq command (current-kill 0 t))
         (list
          :trace (rspec360-test-invocations world)
          :misses (rspec360-test-misses world)
          :calls-before-yank calls-before-yank
          :calls-after-yank (length (rspec360-test-invocations world))
          :hooks rspec360-test-hook-events
          :finish rspec360-test-finish-events
          :failed rspec-last-failed-specs
          :last-directory rspec-last-directory
          :last-targets
          (rspec-compile-target-specs (car rspec-last-arguments))
          :last-target-directory
          (rspec-compile-target-directory (car rspec-last-arguments))
          :last-options (cadr rspec-last-arguments)
          :yank command))))))
"####,
        expect![[
            r#"OK (:result (:trace ("{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb\"],\"cwd\":\"[ROOT]\",\"debug\":\"true\"}" "{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb:12\",\"spec/inventory/ledger_spec.rb:19\"],\"cwd\":\"[ROOT]\",\"debug\":\"true\"}" "{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb:12\",\"spec/inventory/ledger_spec.rb:19\"],\"cwd\":\"[ROOT]\",\"debug\":\"true\"}") :misses nil :calls-before-yank 3 :calls-after-yank 3 :hooks ((:before nil) (:after ("./spec/inventory/ledger_spec.rb:12" "./spec/inventory/ledger_spec.rb:19")) (:before ("./spec/inventory/ledger_spec.rb:12" "./spec/inventory/ledger_spec.rb:19")) (:after ("./spec/inventory/ledger_spec.rb:12" "./spec/inventory/ledger_spec.rb:19")) (:before ("./spec/inventory/ledger_spec.rb:12" "./spec/inventory/ledger_spec.rb:19")) (:after ("./spec/inventory/ledger_spec.rb:12" "./spec/inventory/ledger_spec.rb:19"))) :finish (("*rspec-compilation*" "exited abnormally with code 1") ("*rspec-compilation*" "exited abnormally with code 1") ("*rspec-compilation*" "exited abnormally with code 1")) :failed ("./spec/inventory/ledger_spec.rb:12" "./spec/inventory/ledger_spec.rb:19") :last-directory "[ROOT]/spec/inventory/" :last-targets (("./spec/inventory/ledger_spec.rb:12") ("./spec/inventory/ledger_spec.rb:19")) :last-target-directory "[ROOT]/" :last-options ("--options .rspec") :yank "bundle exec rspec --options .rspec spec/inventory/spec/inventory/ledger_spec.rb\\:12 spec/inventory/spec/inventory/ledger_spec.rb\\:19") :cleanup clean)"#
        ]],
    )
}

fn preflight_and_empty_selection_fail_without_side_effects() -> ParityBatchCase {
    ParityBatchCase::value(
        "preflight_and_empty_selection_fail_without_side_effects",
        r####"
(rspec360-test-run
 "preflight"
 (lambda (world)
   (let* ((no-project-file
           (rspec360-test-write
            (expand-file-name "spec/lonely_spec.rb" (plist-get world :no-project))
            "RSpec.describe 'lonely' do\nend\n"))
          (no-project-buffer
           (let ((enable-local-variables nil) (enable-dir-local-variables nil))
             (rspec360-test-own-buffer (find-file-noselect no-project-file))))
          (spec (rspec360-test-visit world "spec/inventory/ledger_spec.rb"))
          (buffers-before (buffer-list))
          (processes-before (process-list))
          (kill-before kill-ring)
          rerun yank empty no-project states)
     (switch-to-buffer spec)
     (cl-labels
         ((state
           (phase)
           (push
            (list :phase phase :calls (rspec360-test-invocations world)
                  :new-buffers
                  (mapcar #'buffer-name
                          (seq-difference (buffer-list) buffers-before #'eq))
                  :new-processes
                  (seq-difference (process-list) processes-before #'eq)
                  :compilation (get-buffer rspec-compilation-buffer-name-base)
                  :kill-same (eq kill-ring kill-before)
                  :hooks rspec360-test-hook-events
                  :last-directory rspec-last-directory
                  :last-arguments rspec-last-arguments)
            states)))
       (setq rerun (rspec360-test-condition
                    (lambda () (call-interactively #'rspec-rerun))))
       (state 'rerun)
       (setq yank (rspec360-test-condition
                   (lambda () (call-interactively #'rspec-yank-last-command))))
       (state 'yank)
       (setq empty (rspec-run-last-failed))
       (state 'empty)
       (switch-to-buffer no-project-buffer)
       (setq no-project
             (rspec360-test-condition
              (lambda () (call-interactively #'rspec-verify))))
       (state 'no-project))
     (let ((failure-states (nreverse states)))
       (progn
         (switch-to-buffer spec)
         (goto-char (point-min))
         (forward-line 7)
         (rspec360-test-command-loop "C-c , s")
         (let* ((compilation (rspec360-test-owned-compilation-buffer))
                (settled (rspec360-test-wait world compilation 1)))
           (list
            :rerun rerun :yank yank :empty empty :no-project no-project
            :failure-states failure-states
            :recovery (list :trace (rspec360-test-invocations world)
                            :misses (rspec360-test-misses world)
                            :settled settled))))))))
"####,
        expect![[
            r#"OK (:result (:rerun (:signal error :data ("No previous verification") :message "No previous verification") :yank (:signal error :data ("No previous verification") :message "No previous verification") :empty "No spec files found!" :no-project (:signal error :data ("Could not determine the project root.") :message "Could not determine the project root.") :failure-states ((:phase rerun :calls nil :new-buffers nil :new-processes nil :compilation nil :kill-same t :hooks nil :last-directory nil :last-arguments nil) (:phase yank :calls nil :new-buffers nil :new-processes nil :compilation nil :kill-same t :hooks nil :last-directory nil :last-arguments nil) (:phase empty :calls nil :new-buffers nil :new-processes nil :compilation nil :kill-same t :hooks nil :last-directory nil :last-arguments nil) (:phase no-project :calls nil :new-buffers nil :new-processes nil :compilation nil :kill-same t :hooks nil :last-directory nil :last-arguments nil)) :recovery (:trace ("{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb:8\"],\"cwd\":\"[ROOT]\",\"debug\":\"true\"}") :misses nil :settled (:process exit :buffer-process nil :finish (("*rspec-compilation*" "finished")) :hooks ((:before nil) (:after nil)) :failed nil :text "-*- mode: rspec-compilation; default-directory: \"[ROOT]/\" -*-\nRSpec Compilation started at <TIME>\n\nbundle exec rspec --options .rspec spec/inventory/ledger_spec.rb:8\nRun options: include {:locations=>{\"./spec/inventory/ledger_spec.rb\"=>[8]}}\n\nInventory::Ledger\n  #total\n    adds realistic line items\n\nFinished in <DURATION> seconds (files took <DURATION> seconds to load)\n1 example, 0 failures\n\n\nRSpec Compilation finished at <TIME>, duration <DURATION>\n"))) :cleanup clean)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        source_and_spec_switching_follows_the_real_method(),
        pendingness_is_a_real_reversible_spec_edit(),
        single_example_verification_runs_real_compilation(),
        failed_verification_is_navigable_and_records_failures(),
        last_failed_rerun_and_yank_preserve_session_state(),
        preflight_and_empty_selection_fail_without_side_effects(),
    ]
}
