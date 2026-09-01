use expect_test::expect;

use super::ParityBatchCase;

fn mode_starts_the_real_ruby_server_and_stops_with_its_console() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-robe-test-root "lifecycle"))
       (project (neomacs-robe-test-write-project root))
       (app (car project))
       (definition (cadr project)))
  (neomacs-robe-test-with-console
   root definition
   (lambda (console process)
     (let ((source-buffer (find-file-noselect app)))
       (unwind-protect
           (with-current-buffer source-buffer
             (ruby-mode)
             (setq-local eldoc-documentation-function
                         #'neomacs-robe-test-yard-eldoc)
             (robe-mode 1)
             (robe-mode 1)
             (robe-start)
             (let* ((port (buffer-local-value 'robe-port console))
                    (started
                     (list
                      :mode robe-mode
                      :capf (memq #'robe-complete-at-point
                                  completion-at-point-functions)
                      :capf-count
                      (cl-count #'robe-complete-at-point
                                completion-at-point-functions)
                      :xref (memq #'robe--xref-backend xref-backend-functions)
                      :xref-count
                      (cl-count #'robe--xref-backend xref-backend-functions)
                      :eldoc-provider
                      (and (advice-function-member-p
                            #'robe-eldoc eldoc-documentation-function)
                           t)
                      :eldoc-composed-result
                      (funcall eldoc-documentation-function)
                      :keys
                      (mapcar
                       (lambda (key)
                         (list key (lookup-key robe-mode-map (kbd key))))
                       '("C-c C-d" "C-c C-k"))
                      :console-mode (buffer-local-value 'major-mode console)
                      :process-status (process-status process)
                      :running (buffer-local-value 'robe-running console)
                      :host (buffer-local-value 'robe-host console)
                      :port-positive (and (integerp port) (> port 0))
                      :project-on-load-path
                      (and (member (file-name-as-directory
                                    (expand-file-name "lib" root))
                                   (buffer-local-value 'robe-load-path console))
                           t)
                      :sentinel (process-sentinel process)
                      :console
                      (with-current-buffer console
                        (replace-regexp-in-string
                         "robe on [0-9]+" "robe on <port>"
                         (buffer-substring-no-properties
                          (point-min) (point-max))))
                      :requests (neomacs-robe-test-access-paths console)))
                    (_ (process-send-eof process)))
               (neomacs-robe-test-wait-until
                (lambda () (not (process-live-p process)))
                "the Robe console to exit after EOF")
               (let ((stopped
                      (list :status (process-status process)
                            :live (process-live-p process)
                            :console-flag
                            (buffer-local-value 'robe-running console)
                            :running-p (robe-running-p))))
                 (robe-mode -1)
                 (list
                  :started started
                  :stopped stopped
                  :disabled
                  (list
                   :mode robe-mode
                   :capf (memq #'robe-complete-at-point
                               completion-at-point-functions)
                   :xref (memq #'robe--xref-backend xref-backend-functions)
                   :eldoc-provider
                   (and (advice-function-member-p
                         #'robe-eldoc eldoc-documentation-function)
                        t)
                   :eldoc-restored
                   (eq eldoc-documentation-function
                       #'neomacs-robe-test-yard-eldoc)
                   :eldoc-local
                   (local-variable-p 'eldoc-documentation-function))))))
         (when (buffer-live-p source-buffer)
           (with-current-buffer source-buffer (set-buffer-modified-p nil))
           (kill-buffer source-buffer)))))))
"####;
    let expected = expect![[
        r#"OK (:started (:mode t :capf (robe-complete-at-point t) :capf-count 1 :xref (robe--xref-backend t) :xref-count 1 :eldoc-provider t :eldoc-composed-result "YARD: release client" :keys (("C-c C-d" robe-doc) ("C-c C-k" robe-rails-refresh)) :console-mode inf-ruby-mode :process-status run :running t :host "127.0.0.1" :port-positive t :project-on-load-path t :sentinel robe-process-sentinel :console "NEOMACS-ROBE-CONSOLE:ready\n\"robe on <port>\"\n" :requests ("GET /ping/" "GET /load_path/")) :stopped (:status exit :live nil :console-flag t :running-p nil) :disabled (:mode nil :capf nil :xref nil :eldoc-provider nil :eldoc-restored t :eldoc-local t))"#
    ]];
    ParityBatchCase::value(
        "mode_starts_the_real_ruby_server_and_stops_with_its_console",
        elisp_form,
        expected,
    )
}

fn method_and_constant_completion_edit_a_real_release_client() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-robe-test-root "completion"))
       (project (neomacs-robe-test-write-project root))
       (app (car project))
       (definition (cadr project)))
  (neomacs-robe-test-with-console
   root definition
   (lambda (console _process)
     (let ((buffer (find-file-noselect app)))
       (unwind-protect
           (with-current-buffer buffer
             (ruby-mode)
             (robe-mode 1)
             (robe-start)
             (goto-char (point-min))
             (search-forward "plan.publish")
             (let* ((capf (robe-complete-at-point))
                    (begin (nth 0 capf))
                    (end (nth 1 capf))
                    (table (nth 2 capf))
                    (properties (nthcdr 3 capf))
                    (prefix (buffer-substring-no-properties begin end))
                    (candidates (all-completions prefix table))
                    (candidate (car candidates))
                    (annotation
                     (funcall (plist-get properties :annotation-function)
                              candidate))
                    (docsig
                     (funcall (plist-get properties :company-dogsig)
                              candidate))
                    (location
                     (funcall (plist-get properties :company-location)
                              candidate))
                    (metadata
                     (list
                      :bounds (list begin end prefix)
                      :candidates (mapcar #'substring-no-properties candidates)
                      :candidate-faces
                      (neomacs-robe-test-face-runs candidate)
                      :annotation (substring-no-properties annotation)
                      :annotation-faces
                      (neomacs-robe-test-face-runs annotation)
                      :docsig (substring-no-properties docsig)
                      :docsig-faces (neomacs-robe-test-face-runs docsig)
                      :kind
                      (funcall (plist-get properties :company-kind) candidate)
                      :location
                      (list (file-relative-name (car location) root)
                            (cdr location))))
                    (method-result (completion-at-point))
                   method-state)
               (setq method-state
                     (list :capf metadata
                           :result method-result
                           :text (buffer-substring-no-properties
                                  (line-beginning-position)
                                  (line-end-position))
                           :line (line-number-at-pos)
                           :column (current-column)))
               (goto-char (point-max))
               (search-backward "Deploy::Rel")
               (goto-char (match-end 0))
               (let ((constant-result (completion-at-point)))
                 (list
                  :method method-state
                  :constant
                  (list :result constant-result
                        :text (buffer-substring-no-properties
                               (line-beginning-position)
                               (line-end-position))
                        :line (line-number-at-pos)
                        :column (current-column))
                  :buffer (buffer-substring-no-properties
                           (point-min) (point-max))
                  :modified (buffer-modified-p)
                  :requests (neomacs-robe-test-access-paths console)))))
         (when (buffer-live-p buffer)
           (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer)))))))
"####;
    let expected = expect![[
        r#"OK (:method (:capf (:bounds (76 83 "publish") :candidates ("publish!") :candidate-faces ((0 8 font-lock-function-name-face)) :annotation "(artifact, [dry_run:])" :annotation-faces ((1 9 font-lock-variable-name-face) (11 21 font-lock-variable-name-face)) :docsig "Deploy::ReleasePlan#publish!(artifact, [dry_run:])" :docsig-faces ((0 6 font-lock-type-face) (8 19 font-lock-type-face) (20 28 font-lock-function-name-face) (29 37 font-lock-variable-name-face) (39 49 font-lock-variable-name-face)) :kind method :location ("lib/release_plan.rb" 12)) :result t :text "receipt = plan.publish!" :line 2 :column 23) :constant (:result t :text "Deploy::ReleasePlan" :line 4 :column 19) :buffer "plan = Deploy::ReleasePlan.new(owner: \"Ana Ng\", retries: 3)\nreceipt = plan.publish!\nplan.publish!(\"neomacs.tar\", dry_run: false)\nDeploy::ReleasePlan\n" :modified t :requests ("GET /ping/" "GET /load_path/" "GET /complete_method/publish/Deploy::ReleasePlan/-/yes" "GET /complete_method/publish/Deploy::ReleasePlan/-/yes" "GET /complete_const/Deploy::Rel/-"))"#
    ]];
    ParityBatchCase::value(
        "method_and_constant_completion_edit_a_real_release_client",
        elisp_form,
        expected,
    )
}

fn direct_eldoc_renders_signature_and_the_installed_provider_surfaces_its_failure()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-robe-test-root "eldoc"))
       (project (neomacs-robe-test-write-project root))
       (app (car project))
       (definition (cadr project)))
  (neomacs-robe-test-with-console
   root definition
   (lambda (console _process)
     (let ((buffer (find-file-noselect app)))
       (unwind-protect
           (with-current-buffer buffer
             (ruby-mode)
             (robe-mode 1)
             (robe-start)
             (goto-char (point-min))
             (search-forward "false")
             (let ((direct (robe-eldoc)))
               (list
                :direct (substring-no-properties direct)
                :faces (neomacs-robe-test-face-runs direct)
                :provider
                (neomacs-robe-test-signal
                 (lambda () (funcall eldoc-documentation-function)))
                :frame-width (frame-width)
                :requests (neomacs-robe-test-access-paths console))))
         (when (buffer-live-p buffer)
           (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer)))))))
"####;
    let expected = expect![[
        r#"OK (:direct "Deploy::ReleasePlan#publish!(artifact, [dry_run:]) Publish ARTIFACT for the conf" :faces ((0 6 font-lock-type-face) (8 19 font-lock-type-face) (20 28 font-lock-function-name-face) (29 37 font-lock-variable-name-face) (39 49 font-lock-variable-name-face)) :provider (:signal void-function :data (nil) :message "Symbol’s function definition is void: nil") :frame-width 80 :requests ("GET /ping/" "GET /load_path/" "GET /method_targets/publish!/Deploy::ReleasePlan/-/yes/-/yes" "GET /doc_for/Deploy::ReleasePlan/yes/publish!"))"#
    ]];
    ParityBatchCase::value(
        "direct_eldoc_renders_signature_and_the_installed_provider_surfaces_its_failure",
        elisp_form,
        expected,
    )
}

fn documentation_buffer_formats_real_pry_docs_source_and_definition_button() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (let* ((root (neomacs-robe-test-root "documentation"))
         (project (neomacs-robe-test-write-project root))
         (app (car project))
         (definition (cadr project)))
    (neomacs-robe-test-with-console
     root definition
     (lambda (console _process)
       (let ((buffer (find-file-noselect app))
             (robe-show-doc-source nil))
         (unwind-protect
             (with-current-buffer buffer
               (ruby-mode)
               (robe-mode 1)
               (robe-start)
               (goto-char (point-min))
               (search-forward "publish!")
               (robe-doc nil)
               (let ((doc (get-buffer "*robe-doc*")))
                 (with-current-buffer doc
                   (let ((position (point-min))
                         buttons
                         definition-button
                         source-button
                         source-hidden)
                     (while-let ((button (next-button position)))
                       (push (list (button-label button)
                                   (button-type button)
                                   (button-start button)
                                   (button-end button))
                             buttons)
                       (pcase (button-type button)
                         ('robe-method-def (setq definition-button button))
                         ('robe-toggle-source (setq source-button button)))
                       (setq position (button-end button)))
                     (goto-char (point-min))
                     (search-forward "Source")
                     (setq source-hidden
                           (and (get-text-property (1+ (point)) 'invisible) t))
                     (button-activate source-button)
                     (let ((source-visible
                            (not (get-text-property
                                  (1+ (button-end source-button))
                                  'invisible))))
                       (button-activate source-button)
                       (let ((source-hidden-again
                              (and (get-text-property
                                    (1+ (button-end source-button))
                                    'invisible)
                                   t)))
                         (let ((doc-state
                                (list
                                 :mode major-mode
                                 :read-only buffer-read-only
                                 :visual-line visual-line-mode
                                 :text (buffer-substring-no-properties
                                        (point-min) (point-max))
                                 :buttons (nreverse buttons)
                                 :source-toggle
                                 (list source-hidden source-visible
                                       source-hidden-again))))
                           (button-activate definition-button)
                           (append
                            doc-state
                            (list
                             :definition-destination
                             (with-current-buffer
                                 (window-buffer (selected-window))
                               (list
                                :file
                                (file-relative-name buffer-file-name root)
                                :line (line-number-at-pos)
                                :column (current-column)
                                :text
                                (buffer-substring-no-properties
                                 (line-beginning-position)
                                 (line-end-position))))
                             :requests
                             (neomacs-robe-test-access-paths console))))))))))
           (dolist (candidate (buffer-list))
             (when (or (eq candidate (get-buffer "*robe-doc*"))
                       (and (buffer-file-name candidate)
                            (string-prefix-p root
                                             (buffer-file-name candidate))))
               (with-current-buffer candidate (set-buffer-modified-p nil))
               (kill-buffer candidate)))))))))
"####;
    let expected = expect![[
        r##"OK (:mode help-mode :read-only t :visual-line t :text "Deploy::ReleasePlan#publish!(artifact, [dry_run:]) is defined in release_plan.rb\n\nPublish ARTIFACT for the configured owner.\nReturns a stable release receipt.\n\nSource\n\ndef publish!(artifact, dry_run: false)\n  \"#{owner}:#{artifact}:#{dry_run}:#{retries}\"\nend\n" :buttons (("release_plan.rb" robe-method-def 66 81) ("Source" robe-toggle-source 161 167)) :source-toggle (t t t) :definition-destination (:file "lib/release_plan.rb" :line 12 :column 4 :text "    def publish!(artifact, dry_run: false)") :requests ("GET /ping/" "GET /load_path/" "GET /method_targets/publish!/Deploy::ReleasePlan/-/yes/-/-" "GET /doc_for/Deploy::ReleasePlan/yes/publish!"))"##
    ]];
    ParityBatchCase::value(
        "documentation_buffer_formats_real_pry_docs_source_and_definition_button",
        elisp_form,
        expected,
    )
}

fn xref_method_and_constant_jumps_return_to_the_exact_editing_positions() -> ParityBatchCase {
    let elisp_form = r####"
(let (result)
  (save-window-excursion
    (let* ((root (neomacs-robe-test-root "xref"))
           (project (neomacs-robe-test-write-project root))
           (app (car project))
           (definition (cadr project))
           (workflow (expand-file-name "workflow.rb" root))
           (xref-history-storage #'xref-global-history))
      (neomacs-robe-test-write-file
       workflow
       (concat
        "workflow = Deploy::WorkflowRelease.new(\"nightly\")\n"
        "workflow.run(\"REL-42\")\n"))
      (neomacs-robe-test-with-console
       root definition
       (lambda (console _process)
         (let ((origin (find-file-noselect app))
               method-destination method-return
               constant-destination constant-return
               new-destination new-return super-destination super-return)
           (unwind-protect
               (progn
                 (xref-clear-marker-stack)
                 (switch-to-buffer origin)
                 (ruby-mode)
                 (robe-mode 1)
                 (robe-start)

                 (goto-char (point-min))
                 (search-forward "publish!")
                 (let ((origin-point (point)))
                   (xref-find-definitions
                    (xref-backend-identifier-at-point 'robe))
                   (setq method-destination
                         (list
                          :file (file-relative-name buffer-file-name root)
                          :line (line-number-at-pos)
                          :column (current-column)
                          :text (buffer-substring-no-properties
                                 (line-beginning-position)
                                 (line-end-position))))
                   (xref-go-back)
                   (setq method-return
                         (list :file (file-relative-name buffer-file-name root)
                               :same-buffer (eq (current-buffer) origin)
                               :same-point (= (point) origin-point)
                               :line (line-number-at-pos)
                               :column (current-column))))

                 (goto-char (point-min))
                 (search-forward "Deploy::ReleasePlan")
                 (let ((origin-point (point)))
                   (xref-find-definitions
                    (xref-backend-identifier-at-point 'robe))
                   (setq constant-destination
                         (list
                          :file (file-relative-name buffer-file-name root)
                          :line (line-number-at-pos)
                          :column (current-column)
                          :text (buffer-substring-no-properties
                                 (line-beginning-position)
                                 (line-end-position))))
                   (xref-go-back)
                   (setq constant-return
                         (list :file (file-relative-name buffer-file-name root)
                               :same-buffer (eq (current-buffer) origin)
                               :same-point (= (point) origin-point)
                               :line (line-number-at-pos)
                               :column (current-column))))

                 (let ((workflow-buffer (find-file-noselect workflow)))
                   (switch-to-buffer workflow-buffer)
                   (ruby-mode)
                   (robe-mode 1)
                   (goto-char (point-min))
                   (search-forward "new")
                   (let ((origin-point (point)))
                     (xref-find-definitions
                      (xref-backend-identifier-at-point 'robe))
                     (setq new-destination
                           (list
                            :file (file-relative-name buffer-file-name root)
                            :line (line-number-at-pos)
                            :column (current-column)
                            :text (buffer-substring-no-properties
                                   (line-beginning-position)
                                   (line-end-position))))
                     (xref-go-back)
                     (setq new-return
                           (list :file (file-relative-name buffer-file-name root)
                                 :same-buffer
                                 (eq (current-buffer) workflow-buffer)
                                 :same-point (= (point) origin-point)
                                 :line (line-number-at-pos)
                                 :column (current-column)))))

                 (let ((definition-buffer (find-file-noselect definition)))
                   (switch-to-buffer definition-buffer)
                   (ruby-mode)
                   (robe-mode 1)
                   (goto-char (point-min))
                   (search-forward "def run(label)\n      super")
                   (let ((origin-point (point)))
                     (xref-find-definitions
                      (xref-backend-identifier-at-point 'robe))
                     (setq super-destination
                           (list
                            :file (file-relative-name buffer-file-name root)
                            :line (line-number-at-pos)
                            :column (current-column)
                            :text (buffer-substring-no-properties
                                   (line-beginning-position)
                                   (line-end-position))))
                     (xref-go-back)
                     (setq super-return
                           (list :file (file-relative-name buffer-file-name root)
                                 :same-buffer
                                 (eq (current-buffer) definition-buffer)
                                 :same-point (= (point) origin-point)
                                 :line (line-number-at-pos)
                                 :column (current-column)))))

                 (setq result
                       (list
                        :method-destination method-destination
                        :method-return method-return
                        :constant-destination constant-destination
                        :constant-return constant-return
                        :new-destination new-destination
                        :new-return new-return
                        :super-destination super-destination
                        :super-return super-return
                        :history
                        (list :back-empty (xref-marker-stack-empty-p)
                              :forward-empty (xref-forward-history-empty-p))
                        :requests
                        (neomacs-robe-test-access-paths console))))
             (xref-clear-marker-stack)
             (dolist (candidate (buffer-list))
               (when (and (buffer-file-name candidate)
                          (string-prefix-p root (buffer-file-name candidate)))
                 (with-current-buffer candidate (set-buffer-modified-p nil))
                 (kill-buffer candidate)))))))))
  result)
"####;
    let expected = expect![[
        r#"OK (:method-destination (:file "lib/release_plan.rb" :line 12 :column 4 :text "    def publish!(artifact, dry_run: false)") :method-return (:file "app.rb" :same-buffer t :same-point t :line 3 :column 13) :constant-destination (:file "lib/release_plan.rb" :line 2 :column 2 :text "  class ReleasePlan") :constant-return (:file "app.rb" :same-buffer t :same-point t :line 1 :column 26) :new-destination (:file "lib/release_plan.rb" :line 32 :column 4 :text "    def initialize(name)") :new-return (:file "workflow.rb" :same-buffer t :same-point t :line 1 :column 38) :super-destination (:file "lib/release_plan.rb" :line 26 :column 4 :text "    def run(label)") :super-return (:file "lib/release_plan.rb" :same-buffer t :same-point t :line 37 :column 11) :history (:back-empty t :forward-empty nil) :requests ("GET /ping/" "GET /load_path/" "GET /method_targets/publish!/Deploy::ReleasePlan/-/yes/-/-" "GET /const_locations/publish!/-" "GET /method_targets/Deploy::ReleasePlan/-/-/yes/-/-" "GET /const_locations/Deploy::ReleasePlan/-" "GET /method_targets/new/Deploy::WorkflowRelease/-/-/-/-" "GET /const_locations/new/-" "GET /method_targets/run/-/Deploy::WorkflowRelease/yes/yes/-" "GET /const_locations/super/Deploy::WorkflowRelease"))"#
    ]];
    ParityBatchCase::value(
        "xref_method_and_constant_jumps_return_to_the_exact_editing_positions",
        elisp_form,
        expected,
    )
}

fn local_variable_definition_uses_the_public_xref_workflow() -> ParityBatchCase {
    let elisp_form = r####"
(let (result)
  (save-window-excursion
    (let* ((root (neomacs-robe-test-root "local-variable"))
           (project (neomacs-robe-test-write-project root))
           (definition (cadr project))
           (source (expand-file-name "batch_publisher.rb" root))
           (xref-history-storage #'xref-global-history))
      (neomacs-robe-test-write-file
       source
       (concat
        "class BatchPublisher\n"
        "  def publish(releases)\n"
        "    release_count = releases.length\n"
        "    audit = {count: release_count}\n"
        "    release_count\n"
        "  end\n"
        "end\n"))
      (neomacs-robe-test-with-console
       root definition
       (lambda (console process)
         (let ((buffer (find-file-noselect source)))
           (unwind-protect
               (progn
                 (xref-clear-marker-stack)
                 (switch-to-buffer buffer)
                 (ruby-mode)
                 (robe-mode 1)
                 (robe-start)
                 (goto-char (point-max))
                 (search-backward "release_count")
                 (goto-char (match-end 0))
                 (let* ((origin (point))
                        (identifier
                         (xref-backend-identifier-at-point 'robe)))
                   (xref-find-definitions identifier)
                   (let ((definition-state
                          (list
                           :same-buffer (eq (current-buffer) buffer)
                           :line (line-number-at-pos)
                           :column (current-column)
                           :text (buffer-substring-no-properties
                                  (line-beginning-position)
                                  (line-end-position))
                           :back-empty (xref-marker-stack-empty-p))))
                     (xref-go-back)
                     (setq result
                           (list
                            :identifier
                            (substring-no-properties identifier)
                            :definition definition-state
                            :return
                            (list :same-buffer (eq (current-buffer) buffer)
                                  :same-point (= (point) origin)
                                  :line (line-number-at-pos)
                                  :column (current-column)
                                  :back-empty (xref-marker-stack-empty-p)
                                  :forward-empty
                                  (xref-forward-history-empty-p))
                            :backend (robe--xref-backend)
                            :server-running (robe-running-p)
                            :process (process-status process)
                            :requests
                            (neomacs-robe-test-access-paths console)))))
             (xref-clear-marker-stack)
             (dolist (candidate (buffer-list))
               (when (and (buffer-file-name candidate)
                          (string-prefix-p root (buffer-file-name candidate)))
                 (with-current-buffer candidate (set-buffer-modified-p nil))
                 (kill-buffer candidate)))))))))
  result))
"####;
    let expected = expect![[
        r#"OK (:identifier "release_count" :definition (:same-buffer t :line 3 :column 4 :text "    release_count = releases.length" :back-empty nil) :return (:same-buffer t :same-point t :line 5 :column 17 :back-empty t :forward-empty nil) :backend robe :server-running t :process run :requests ("GET /ping/" "GET /load_path/"))"#
    ]];
    ParityBatchCase::value(
        "local_variable_definition_uses_the_public_xref_workflow",
        elisp_form,
        expected,
    )
}

fn missing_method_definition_preserves_the_live_editing_session() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (let* ((root (neomacs-robe-test-root "missing-definition"))
         (project (neomacs-robe-test-write-project root))
         (definition (cadr project))
         (source (expand-file-name "missing_release_action.rb" root))
         (xref-history-storage #'xref-global-history))
    (neomacs-robe-test-write-file
     source
     (concat
      "plan = Deploy::ReleasePlan.new(owner: \"Ana Ng\", retries: 3)\n"
      "plan.ship_missing\n"))
    (neomacs-robe-test-with-console
     root definition
     (lambda (console process)
       (let ((buffer (find-file-noselect source)))
         (unwind-protect
             (progn
               (xref-clear-marker-stack)
               (switch-to-buffer buffer)
               (ruby-mode)
               (robe-mode 1)
               (robe-start)
               (goto-char (point-max))
               (search-backward "ship_missing")
               (goto-char (match-end 0))
               (let* ((origin-buffer (current-buffer))
                      (origin-point (point))
                      (origin-text (buffer-string))
                      (identifier
                       (xref-backend-identifier-at-point 'robe))
                      (outcome
                       (neomacs-robe-test-signal
                        (lambda ()
                          (xref-find-definitions identifier)))))
                 (list
                  :identifier (substring-no-properties identifier)
                  :outcome outcome
                  :after
                  (list
                   :same-buffer (eq (current-buffer) origin-buffer)
                   :same-point (= (point) origin-point)
                   :same-text (equal (buffer-string) origin-text)
                   :line (line-number-at-pos)
                   :column (current-column)
                   :back-empty (xref-marker-stack-empty-p)
                   :server-running (robe-running-p)
                   :process (process-status process))
                  :requests (neomacs-robe-test-access-paths console))))
           (xref-clear-marker-stack)
           (when (buffer-live-p buffer)
             (with-current-buffer buffer (set-buffer-modified-p nil))
             (kill-buffer buffer))))))))
"####;
    let expected = expect![[
        r#"OK (:identifier "ship_missing" :outcome (:signal user-error :data (#("No definitions found for: ship_missing" 26 38 (robe-at-pt t))) :message "No definitions found for: ship_missing") :after (:same-buffer t :same-point t :same-text t :line 2 :column 17 :back-empty t :server-running t :process run) :requests ("GET /ping/" "GET /load_path/" "GET /method_targets/ship_missing/Deploy::ReleasePlan/-/yes/-/-" "GET /const_locations/ship_missing/-"))"#
    ]];
    ParityBatchCase::value(
        "missing_method_definition_preserves_the_live_editing_session",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_starts_the_real_ruby_server_and_stops_with_its_console(),
        method_and_constant_completion_edit_a_real_release_client(),
        direct_eldoc_renders_signature_and_the_installed_provider_surfaces_its_failure(),
        documentation_buffer_formats_real_pry_docs_source_and_definition_button(),
        xref_method_and_constant_jumps_return_to_the_exact_editing_positions(),
        local_variable_definition_uses_the_public_xref_workflow(),
        missing_method_definition_preserves_the_live_editing_session(),
    ]
}
