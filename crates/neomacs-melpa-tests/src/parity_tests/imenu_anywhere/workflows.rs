use expect_test::expect;

use super::ParityBatchCase;

fn real_minibuffer_default_duplicate_jump_widens_and_round_trips_xref() -> ParityBatchCase {
    let elisp_form = r##"
(ia352-test-run
 "real-minibuffer-xref"
 (lambda (root)
   (let* ((origin
           (ia352-test-write-buffer
            root "dispatch.el"
            ";; deploy-order origin Ω\n(deploy-order)\n"
            #'lisp-interaction-mode))
          (orders
           (ia352-test-write-buffer
            root "orders Ω.el"
            "(defvar deploy-timeout 30)\n\n(defun deploy-order (order)\n  (message \"deploy %s\" order))\n\n(defun deploy-order-with-audit (order actor)\n  (list order actor))\n"
            #'emacs-lisp-mode))
          (billing
           (ia352-test-write-buffer
            root "billing.el"
            "(defun bill-order (order)\n  (* order 2))\n\n(defun deploy-order (order)\n  (bill-order order))\n"
            #'emacs-lisp-mode)))
     (setq ia352-test-buffer-list (list orders billing origin))
     (setq imenu-anywhere-buffer-filter-functions
           '(imenu-anywhere-same-mode-p imenu-anywhere-friendly-mode-p))
     (dolist (buffer ia352-test-buffer-list)
       (with-current-buffer buffer
         (setq-local imenu-anywhere-buffer-list-function
                     #'ia352-test-buffer-list-function)))
     (ia352-test-position origin "deploy-order")
     (let* ((origin-before (ia352-test-location))
            (primed
             (ia352-test-candidate-names (imenu-anywhere-candidates))))
       (let (restricted)
         (with-current-buffer orders
         (goto-char (point-min))
         (search-forward "(defun deploy-order-with-audit")
           (narrow-to-region (match-beginning 0) (point-max))
           (setq restricted (list (point-min) (point-max))))
         (let ((imenu-after-jump-hook '(ia352-test-after-jump)))
           (ia352-test-real-select "" "RET"))
         (let ((target (ia352-test-location))
               (full-restriction
                (list :bounds (list (point-min) (point-max))
                      :full (and (= (point-min) 1)
                                 (= (point-max) (1+ (buffer-size))))))
               (mark (with-current-buffer origin
                       (list :position (mark t) :active (and mark-active t))))
               (pushed (ia352-test-xref-state)))
           (xref-go-back)
           (let ((back (ia352-test-location))
                 (back-history (ia352-test-xref-state)))
             (xref-go-forward)
             (list :primed primed :origin origin-before
                   :before-restriction restricted :target target
                   :after-restriction full-restriction
                   :origin-mark mark :pushed pushed
                   :back back :back-history back-history
                   :forward (ia352-test-location)
                   :forward-history (ia352-test-xref-state)))))))))
"##;
    let expect = expect![[
        r#"OK (:result (:primed ("deploy-order" "deploy-order-with-audit" "deploy-timeout/Variables" "bill-order" "deploy-order") :origin (:buffer "dispatch.el" :file "dispatch.el" :point 4 :line 1 :column 3 :text ";; deploy-order origin Ω" :restriction (1 41) :selected t) :before-restriction (89 156) :target (:buffer "orders Ω.el" :file "orders Ω.el" :point 29 :line 3 :column 0 :text "(defun deploy-order (order)" :restriction (1 156) :selected t) :after-restriction (:bounds (1 156) :full t) :origin-mark (:position 4 :active t) :pushed (:backward ((:file "dispatch.el" :point 4 :line 1 :column 3)) :forward nil) :back (:buffer "dispatch.el" :file "dispatch.el" :point 4 :line 1 :column 3 :text ";; deploy-order origin Ω" :restriction (1 41) :selected t) :back-history (:backward nil :forward ((:file "orders Ω.el" :point 29 :line 3 :column 0))) :forward (:buffer "orders Ω.el" :file "orders Ω.el" :point 29 :line 3 :column 0 :text "(defun deploy-order (order)" :restriction (1 156) :selected t) :forward-history (:backward ((:file "dispatch.el" :point 4 :line 1 :column 3)) :forward nil)) :interactions ((:prompt "Imenu: " :initial-input "" :require-match t :must-match-map t :collection ("deploy-order" "deploy-order-with-audit" "deploy-timeout/Variables" "bill-order" "deploy-order") :predicate nil :default "deploy-order" :reader completing-read-default :final-input "" :selected "deploy-order")) :ido nil :after-jump ((:buffer "orders Ω.el" :file "orders Ω.el" :point 29 :line 3 :column 0 :text "(defun deploy-order (order)" :restriction (1 156) :selected t)) :messages nil :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :xref (:backward nil :forward nil) :input-events nil :unread-events nil :minibuffer-active nil :root-exists nil :root-owned nil :window-restored t :completion-restored t :filters-restored t :friendly-restored t :preprocessor-restored t :delimiter-restored t :jump-hook-restored t :ido-hook-restored t :ido-minibuffer-hook-restored t :minibuffer-hook-restored t :choose-completion-hook-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value("real-minibuffer-default-duplicate-xref", elisp_form, expect)
}

fn python_hierarchy_filter_delimiter_and_live_edit_refresh() -> ParityBatchCase {
    let elisp_form = r##"
(ia352-test-run
 "python-hierarchy-refresh"
 (lambda (root)
   (let* ((service
           (ia352-test-write-buffer
            root "order_service.py"
            "class OrderService:\n    def deploy(self, order):\n        return order\n\n    async def audit(self, actor):\n        return actor\n\ndef health_check():\n    return 'ok'\n"
            #'python-mode))
          (jobs
           (ia352-test-write-buffer
            root "jobs.py"
            "def enqueue_order(order):\n    return order\n"
            #'python-mode))
          (omitted
           (ia352-test-write-buffer
            root "internal.py"
            "def omitted_secret():\n    return 'secret'\n"
            #'python-mode))
          (notes
           (ia352-test-write-buffer
            root "definitions.txt"
            "def false_positive(): pass\n" #'text-mode)))
     (setq ia352-test-buffer-list (list service jobs notes))
     (dolist (buffer (list service jobs omitted notes))
       (with-current-buffer buffer
         (setq-local imenu-anywhere-buffer-list-function
                     #'ia352-test-buffer-list-function)))
     (let ((imenu-anywhere-buffer-filter-functions
            '(imenu-anywhere-same-mode-p))
           (imenu-anywhere-delimiter "::"))
       (switch-to-buffer service)
       (goto-char (point-min))
       (let* ((initial (imenu-anywhere-candidates))
              (service-cache-1
               (buffer-local-value 'imenu-anywhere--cached-candidates service))
              (jobs-cache-1
               (buffer-local-value 'imenu-anywhere--cached-candidates jobs))
              (service-marker-1
               (ia352-test-cached-marker service "health_check (def)"))
              (jobs-marker-1
               (ia352-test-cached-marker jobs "enqueue_order (def)"))
              (unchanged (imenu-anywhere-candidates))
              (service-cache-2
               (buffer-local-value 'imenu-anywhere--cached-candidates service))
              (jobs-cache-2
               (buffer-local-value 'imenu-anywhere--cached-candidates jobs))
              (service-marker-2
               (ia352-test-cached-marker service "health_check (def)"))
              (jobs-marker-2
               (ia352-test-cached-marker jobs "enqueue_order (def)"))
              edit-observation
              (unchanged-observation
               (list :initial-names (ia352-test-candidate-names initial)
                     :aggregate-rebuilt (not (eq initial unchanged))
                     :service
                     (list :list-reused (eq service-cache-1 service-cache-2)
                           :marker-reused
                           (eq service-marker-1 service-marker-2))
                     :jobs
                     (list :list-reused (eq jobs-cache-1 jobs-cache-2)
                           :marker-reused (eq jobs-marker-1 jobs-marker-2)))))
         (ia352-test-real-select "audit (async def)::OrderService (class)")
         (let ((nested (ia352-test-location)))
         (ia352-test-clear-xref)
         (with-current-buffer jobs
           (goto-char (point-max))
           (insert "\nasync def reconcile_queue(batch):\n    return batch\n"))
         (switch-to-buffer service)
         (goto-char (point-min))
         (let* ((after-edit (imenu-anywhere-candidates))
                (service-cache-after-edit
                 (buffer-local-value 'imenu-anywhere--cached-candidates service))
                (jobs-cache-after-edit
                 (buffer-local-value 'imenu-anywhere--cached-candidates jobs))
                (service-marker-after-edit
                 (ia352-test-cached-marker service "health_check (def)"))
                (jobs-marker-after-edit
                 (ia352-test-cached-marker jobs "enqueue_order (def)")))
           (setq edit-observation
                 (list
                  :names (ia352-test-candidate-names after-edit)
                  :service
                  (list :list-reused
                        (eq service-cache-2 service-cache-after-edit)
                        :marker-reused
                        (eq service-marker-2 service-marker-after-edit))
                  :jobs
                  (list :list-replaced
                        (not (eq jobs-cache-2 jobs-cache-after-edit))
                        :marker-replaced
                        (not (eq jobs-marker-2 jobs-marker-after-edit)))
                  :new-definition-present
                  (and (assoc-string "reconcile_queue (async def)"
                                     after-edit nil)
                       t))))
         (ia352-test-real-select "reconcile_queue (async def)")
         (let* ((_completion-form (imenu-anywhere-candidates))
                (service-completion-cache
                 (buffer-local-value 'imenu-anywhere--cached-candidates service))
                (jobs-completion-cache
                 (buffer-local-value 'imenu-anywhere--cached-candidates jobs))
                (service-completion-marker
                 (ia352-test-cached-marker service "health_check (def)"))
                (jobs-completion-marker
                 (ia352-test-cached-marker jobs "enqueue_order (def)"))
                (service-tick
                 (buffer-local-value 'imenu-anywhere--cached-tick service))
                (jobs-tick
                 (buffer-local-value 'imenu-anywhere--cached-tick jobs))
                (imenu-anywhere-preprocess-entry-function
                 #'imenu-anywhere-preprocess-for-listing)
                (listing-form (imenu-anywhere-candidates))
                (service-listing-cache
                 (buffer-local-value 'imenu-anywhere--cached-candidates service))
                (jobs-listing-cache
                 (buffer-local-value 'imenu-anywhere--cached-candidates jobs))
                (service-listing-marker
                 (ia352-test-cached-marker
                  service "order_service.py: health_check (def)"))
                (jobs-listing-marker
                 (ia352-test-cached-marker jobs "jobs.py: enqueue_order (def)")))
           (list :initial-names (plist-get unchanged-observation :initial-names)
                 :aggregate-rebuilt
                 (plist-get unchanged-observation :aggregate-rebuilt)
                 :unchanged-service-list
                 (plist-get (plist-get unchanged-observation :service)
                            :list-reused)
                 :unchanged-service-marker
                 (plist-get (plist-get unchanged-observation :service)
                            :marker-reused)
                 :unchanged-jobs-list
                 (plist-get (plist-get unchanged-observation :jobs)
                            :list-reused)
                 :unchanged-jobs-marker
                 (plist-get (plist-get unchanged-observation :jobs)
                            :marker-reused)
                 :after-edit-names (plist-get edit-observation :names)
                 :after-edit-service-list
                 (plist-get (plist-get edit-observation :service) :list-reused)
                 :after-edit-service-marker
                 (plist-get (plist-get edit-observation :service)
                            :marker-reused)
                 :after-edit-jobs-list
                 (plist-get (plist-get edit-observation :jobs) :list-replaced)
                 :after-edit-jobs-marker
                 (plist-get (plist-get edit-observation :jobs) :marker-replaced)
                 :new-definition-present
                 (plist-get edit-observation :new-definition-present)
                 :service-same-tick
                 (eq service-tick
                     (buffer-local-value 'imenu-anywhere--cached-tick service))
                 :jobs-same-tick
                 (eq jobs-tick
                     (buffer-local-value 'imenu-anywhere--cached-tick jobs))
                 :preprocessor-service-list
                 (not (eq service-completion-cache service-listing-cache))
                 :preprocessor-service-marker
                 (not (eq service-completion-marker service-listing-marker))
                 :preprocessor-jobs-list
                 (not (eq jobs-completion-cache jobs-listing-cache))
                 :preprocessor-jobs-marker
                 (not (eq jobs-completion-marker jobs-listing-marker))
                 :preprocessor-names
                 (ia352-test-candidate-names listing-form)
                 :nested nested :refreshed (ia352-test-location)
                 :omitted-live (buffer-live-p omitted)
                 :notes-live (buffer-live-p notes)
                 :jobs-modified
                 (with-current-buffer jobs (buffer-modified-p))))))))))
"##;
    let expect = expect![[
        r#"OK (:result (:initial-names ("health_check (def)" "deploy (def)::OrderService (class)" "audit (async def)::OrderService (class)" "OrderService (class)::*class definition*" "enqueue_order (def)") :aggregate-rebuilt t :unchanged-service-list t :unchanged-service-marker t :unchanged-jobs-list t :unchanged-jobs-marker t :after-edit-names ("health_check (def)" "deploy (def)::OrderService (class)" "audit (async def)::OrderService (class)" "OrderService (class)::*class definition*" "enqueue_order (def)" "reconcile_queue (async def)") :after-edit-service-list t :after-edit-service-marker t :after-edit-jobs-list t :after-edit-jobs-marker t :new-definition-present t :service-same-tick t :jobs-same-tick t :preprocessor-service-list t :preprocessor-service-marker t :preprocessor-jobs-list t :preprocessor-jobs-marker t :preprocessor-names ("order_service.py: health_check (def)" "order_service.py: OrderService (class)::deploy (def)" "order_service.py: OrderService (class)::audit (async def)" "order_service.py: OrderService (class)::*class definition*" "jobs.py: enqueue_order (def)" "jobs.py: reconcile_queue (async def)") :nested (:buffer "order_service.py" :file "order_service.py" :point 72 :line 5 :column 0 :text "    async def audit(self, actor):" :restriction (1 164) :selected t) :refreshed (:buffer "jobs.py" :file "jobs.py" :point 45 :line 4 :column 0 :text "async def reconcile_queue(batch):" :restriction (1 96) :selected t) :omitted-live t :notes-live t :jobs-modified t) :interactions ((:prompt "Imenu: " :initial-input "" :require-match t :must-match-map t :collection ("health_check (def)" "deploy (def)::OrderService (class)" "audit (async def)::OrderService (class)" "OrderService (class)::*class definition*" "enqueue_order (def)") :predicate nil :default "deploy (def)::OrderService (class)" :reader completing-read-default :final-input "audit (async def)::OrderService (class)" :selected "audit (async def)::OrderService (class)") (:prompt "Imenu: " :initial-input "" :require-match t :must-match-map t :collection ("health_check (def)" "deploy (def)::OrderService (class)" "audit (async def)::OrderService (class)" "OrderService (class)::*class definition*" "enqueue_order (def)" "reconcile_queue (async def)") :predicate nil :default "deploy (def)::OrderService (class)" :reader completing-read-default :final-input "reconcile_queue (async def)" :selected "reconcile_queue (async def)")) :ido nil :after-jump nil :messages nil :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :xref (:backward nil :forward nil) :input-events nil :unread-events nil :minibuffer-active nil :root-exists nil :root-owned nil :window-restored t :completion-restored t :filters-restored t :friendly-restored t :preprocessor-restored t :delimiter-restored t :jump-hook-restored t :ido-hook-restored t :ido-minibuffer-hook-restored t :minibuffer-hook-restored t :choose-completion-hook-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value("python-hierarchy-filter-and-refresh", elisp_form, expect)
}

fn built_in_ido_wrapper_uses_real_matches_and_navigation() -> ParityBatchCase {
    let elisp_form = r##"
(ia352-test-run
 "real-ido-wrapper"
 (lambda (root)
   (let* ((origin
           (ia352-test-write-buffer
            root "ido-origin.el" "(deploy-order)\n" #'lisp-interaction-mode))
          (orders
           (ia352-test-write-buffer
            root "ido-orders.el"
            "(defun deploy-order (order) order)\n\n(defun deploy-order-with-audit (order) (list order :audit))\n"
            #'emacs-lisp-mode))
          (billing
           (ia352-test-write-buffer
            root "ido-billing.el"
            "(defun deploy-order (order) (* order 2))\n"
            #'emacs-lisp-mode)))
     (setq ia352-test-buffer-list (list orders billing origin))
     (setq imenu-anywhere-buffer-filter-functions
           '(imenu-anywhere-same-mode-p imenu-anywhere-friendly-mode-p))
     (dolist (buffer ia352-test-buffer-list)
       (with-current-buffer buffer
         (setq-local imenu-anywhere-buffer-list-function
                     #'ia352-test-buffer-list-function)))
     (ia352-test-position origin "deploy-order")
     (let ((observations
            (ia352-test-ido-select "deploy" "C-s RET")))
       (list :observations observations
             :target (ia352-test-location)
             :xref (ia352-test-xref-state))))))
"##;
    let expect = expect![[
        r#"OK (:result (:observations ((:phase setup :prompt "Imenu: " :choices ("deploy-order" "deploy-order-with-audit") :reader ido-completing-read :preprocessor imenu-anywhere-preprocess-for-completion :next-match-key ido-next-match :final-input "deploy" :selected "deploy-order-with-audit")) :target (:buffer "ido-orders.el" :file "ido-orders.el" :point 37 :line 3 :column 0 :text "(defun deploy-order-with-audit (order) (list order :audit))" :restriction (1 97) :selected t) :xref (:backward ((:file "ido-origin.el" :point 2 :line 1 :column 1)) :forward nil)) :interactions nil :ido nil :after-jump nil :messages nil :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :xref (:backward nil :forward nil) :input-events nil :unread-events nil :minibuffer-active nil :root-exists nil :root-owned nil :window-restored t :completion-restored t :filters-restored t :friendly-restored t :preprocessor-restored t :delimiter-restored t :jump-hook-restored t :ido-hook-restored t :ido-minibuffer-hook-restored t :minibuffer-hook-restored t :choose-completion-hook-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value("built-in-ido-real-session", elisp_form, expect)
}

fn provider_failure_retains_real_index_then_recovers() -> ParityBatchCase {
    let elisp_form = r##"
(ia352-test-run
 "provider-failure-recovery"
 (lambda (root)
   (let ((source
          (ia352-test-write-buffer
           root "provider.el"
           "(defun preserved-definition () :stable)\n"
           #'emacs-lisp-mode)))
     (setq ia352-test-buffer-list (list source))
     (setq imenu-anywhere-buffer-filter-functions
           '(imenu-anywhere-same-mode-p))
     (with-current-buffer source
       (setq-local imenu-anywhere-buffer-list-function
                   #'ia352-test-buffer-list-function))
     (switch-to-buffer source)
     (let* ((before (imenu-anywhere-candidates))
            (before-marker (cdar before))
            (real-provider imenu-create-index-function)
            after-failure)
       (with-current-buffer source
         (setq-local imenu-create-index-function
                     #'ia352-test-failing-index-provider)
         (goto-char (point-max))
         (insert "; force provider refresh\n"))
       (ia352-test-public-message
        (lambda () (setq after-failure (imenu-anywhere-candidates))))
       (with-current-buffer source
         (setq-local imenu-create-index-function real-provider)
         (goto-char (point-max))
         (insert "\n(defun recovered-definition () :recovered)\n"))
       (ia352-test-real-select "recovered-definition")
       (list :before (ia352-test-candidate-names before)
             :failure
             (list :candidates (ia352-test-candidate-names after-failure)
                            :same-marker (eq before-marker
                                             (cdar after-failure)))
             :recovered (ia352-test-location))))))
"##;
    let expect = expect![[
        r#"OK (:result (:before ("preserved-definition") :failure (:candidates ("preserved-definition") :same-marker t) :recovered (:buffer "provider.el" :file "provider.el" :point 67 :line 4 :column 0 :text "(defun recovered-definition () :recovered)" :restriction (1 110) :selected t)) :interactions ((:prompt "Imenu: " :initial-input "" :require-match t :must-match-map t :collection ("preserved-definition" "recovered-definition") :predicate nil :default "preserved-definition" :reader completing-read-default :final-input "recovered-definition" :selected "recovered-definition")) :ido nil :after-jump nil :messages ("Imenu error in provider.el. Keeping old index. (IA352 provider exploded Ω)") :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :xref (:backward nil :forward nil) :input-events nil :unread-events nil :minibuffer-active nil :root-exists nil :root-owned nil :window-restored t :completion-restored t :filters-restored t :friendly-restored t :preprocessor-restored t :delimiter-restored t :jump-hook-restored t :ido-hook-restored t :ido-minibuffer-hook-restored t :minibuffer-hook-restored t :choose-completion-hook-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value("provider-failure-retains-and-recovers", elisp_form, expect)
}

fn no_reachable_tags_reports_without_navigation() -> ParityBatchCase {
    let elisp_form = r##"
(ia352-test-run
 "no-reachable-tags"
 (lambda (root)
   (let ((notes
          (ia352-test-write-buffer root "empty-notes.txt"
                                   "deployment notes Ω\n" #'fundamental-mode)))
     (setq ia352-test-buffer-list (list notes))
     (setq imenu-anywhere-buffer-filter-functions
           '(imenu-anywhere-same-mode-p))
     (with-current-buffer notes
       (setq-local imenu-anywhere-buffer-list-function
                   #'ia352-test-buffer-list-function))
     (switch-to-buffer notes)
     (goto-char 5)
     (let ((before (list :location (ia352-test-location)
                         :mark (mark t)
                         :xref (ia352-test-xref-state)
                         :events unread-command-events)))
       (let ((public-return
              (ia352-test-public-message
               (lambda () (call-interactively #'imenu-anywhere)))))
         (list :return public-return
               :before before
               :after (list :location (ia352-test-location)
                            :mark (mark t)
                            :xref (ia352-test-xref-state)
                            :events unread-command-events
                            :minibuffer
                            (and (active-minibuffer-window) t))))))))
"##;
    let expect = expect![[
        r#"OK (:result (:return "No imenu tags" :before (:location (:buffer "empty-notes.txt" :file "empty-notes.txt" :point 5 :line 1 :column 4 :text "deployment notes Ω" :restriction (1 20) :selected t) :mark nil :xref (:backward nil :forward nil) :events nil) :after (:location (:buffer "empty-notes.txt" :file "empty-notes.txt" :point 5 :line 1 :column 4 :text "deployment notes Ω" :restriction (1 20) :selected t) :mark nil :xref (:backward nil :forward nil) :events nil :minibuffer nil)) :interactions nil :ido nil :after-jump nil :messages ("No imenu tags") :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :xref (:backward nil :forward nil) :input-events nil :unread-events nil :minibuffer-active nil :root-exists nil :root-owned nil :window-restored t :completion-restored t :filters-restored t :friendly-restored t :preprocessor-restored t :delimiter-restored t :jump-hook-restored t :ido-hook-restored t :ido-minibuffer-hook-restored t :minibuffer-hook-restored t :choose-completion-hook-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value("no-reachable-tags-no-side-effects", elisp_form, expect)
}

fn missing_optional_frontends_fail_then_default_command_recovers() -> ParityBatchCase {
    let elisp_form = r##"
(ia352-test-run
 "missing-frontends-recovery"
 (lambda (root)
   (let* ((origin
           (ia352-test-write-buffer root "frontend-origin.el"
                                    "(ia352-frontend-target)\n"
                                    #'emacs-lisp-mode))
          (target
           (ia352-test-write-buffer root "frontend-target.el"
                                    "(defun ia352-frontend-target () :ok)\n"
                                    #'emacs-lisp-mode))
          (helm-registration
           (ia352-test-helm-after-load-registration)))
     (setq ia352-test-buffer-list (list target origin))
     (setq imenu-anywhere-buffer-filter-functions
           '(imenu-anywhere-same-mode-p))
     (dolist (buffer ia352-test-buffer-list)
       (with-current-buffer buffer
         (setq-local imenu-anywhere-buffer-list-function
                     #'ia352-test-buffer-list-function)))
     (unless (and (not (featurep 'ivy)) (not (locate-library "ivy"))
                  (not (featurep 'helm)) (not (locate-library "helm")))
       (error "IMENU-ANYWHERE optional frontend unexpectedly available"))
     (let ((ivy (ia352-test-capture #'ivy-imenu-anywhere))
           (helm (ia352-test-capture #'helm-imenu-anywhere)))
       (ia352-test-position origin "ia352-frontend-target")
       (ia352-test-real-select "" "RET")
       (list :ivy ivy :helm helm
             :features (list (featurep 'ivy) (featurep 'helm))
             :helm-registration
             (list :same-entry
                   (eq helm-registration
                       (ia352-test-helm-after-load-registration)))
             :target (ia352-test-location)
             :xref (ia352-test-xref-state))))))
"##;
    let expect = expect![[
        r#"OK (:result (:ivy (:signal error :data ("[imenu-anywhere]: This command requires ’ivy’ package") :message "[imenu-anywhere]: This command requires ’ivy’ package") :helm (:signal error :data ("[imenu-anywhere]: This command requires ’helm’ package") :message "[imenu-anywhere]: This command requires ’helm’ package") :features (nil nil) :helm-registration (:same-entry t) :target (:buffer "frontend-target.el" :file "frontend-target.el" :point 1 :line 1 :column 0 :text "(defun ia352-frontend-target () :ok)" :restriction (1 38) :selected t) :xref (:backward ((:file "frontend-origin.el" :point 2 :line 1 :column 1)) :forward nil)) :interactions ((:prompt "Imenu: " :initial-input "" :require-match t :must-match-map t :collection ("ia352-frontend-target") :predicate nil :default "ia352-frontend-target" :reader completing-read-default :final-input "" :selected "ia352-frontend-target")) :ido nil :after-jump nil :messages nil :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :xref (:backward nil :forward nil) :input-events nil :unread-events nil :minibuffer-active nil :root-exists nil :root-owned nil :window-restored t :completion-restored t :filters-restored t :friendly-restored t :preprocessor-restored t :delimiter-restored t :jump-hook-restored t :ido-hook-restored t :ido-minibuffer-hook-restored t :minibuffer-hook-restored t :choose-completion-hook-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "missing-optional-frontends-and-recovery",
        elisp_form,
        expect,
    )
}

fn real_projectile_project_filter_crosses_modes_but_excludes_outsider() -> ParityBatchCase {
    let elisp_form = r##"
(ia352-test-run
 "real-projectile-filter"
 (lambda (root)
   (let* ((project (file-name-as-directory (expand-file-name "checkout" root)))
          (outsider-root
           (file-name-as-directory (expand-file-name "other-checkout" root))))
     (make-directory project t)
     (make-directory outsider-root t)
     (with-temp-file (expand-file-name ".projectile" project))
     (with-temp-file (expand-file-name ".projectile" outsider-root))
     (let* ((origin
             (ia352-test-write-buffer
              root "checkout/src/origin.el"
              "(reconcile_order)\n" #'emacs-lisp-mode))
            (python-peer
             (ia352-test-write-buffer
              root "checkout/services/orders.py"
              "def reconcile_order(order):\n    return order\n"
              #'python-mode))
            (outsider
             (ia352-test-write-buffer
              root "other-checkout/services/outsider.py"
              "def outsider_reconcile(order):\n    return order\n"
              #'python-mode)))
       (setq ia352-test-buffer-list (list python-peer outsider origin)
             imenu-anywhere-buffer-filter-functions
             '(imenu-anywhere-same-project-p))
       (dolist (buffer ia352-test-buffer-list)
         (with-current-buffer buffer
           (setq-local imenu-anywhere-buffer-list-function
                       #'ia352-test-buffer-list-function)))
       (ia352-test-position origin "reconcile_order")
       (let ((project-root (projectile-project-root)))
         (ia352-test-real-select "reconcile_order (def)")
         (list :project-root-relative (file-relative-name project-root root)
               :package-project-buffers
               (with-current-buffer origin
                 (mapcar (lambda (buffer)
                           (file-name-nondirectory
                            (buffer-file-name buffer)))
                         imenu-anywhere--project-buffers))
               :target (ia352-test-location)
               :outsider-live (buffer-live-p outsider)
               :xref (ia352-test-xref-state)))))))
"##;
    let expect = expect![[
        r#"OK (:result (:project-root-relative "checkout/" :package-project-buffers ("origin.el" "orders.py") :target (:buffer "orders.py" :file "orders.py" :point 1 :line 1 :column 0 :text "def reconcile_order(order):" :restriction (1 46) :selected t) :outsider-live t :xref (:backward ((:file "origin.el" :point 2 :line 1 :column 1)) :forward nil)) :interactions ((:prompt "Imenu: " :initial-input "" :require-match t :must-match-map t :collection ("reconcile_order (def)") :predicate nil :default "reconcile_order (def)" :reader completing-read-default :final-input "reconcile_order (def)" :selected "reconcile_order (def)")) :ido nil :after-jump nil :messages nil :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :xref (:backward nil :forward nil) :input-events nil :unread-events nil :minibuffer-active nil :root-exists nil :root-owned nil :window-restored t :completion-restored t :filters-restored t :friendly-restored t :preprocessor-restored t :delimiter-restored t :jump-hook-restored t :ido-hook-restored t :ido-minibuffer-hook-restored t :minibuffer-hook-restored t :choose-completion-hook-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value("real-projectile-cross-mode-filter", elisp_form, expect)
}

pub(super) fn public_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        real_minibuffer_default_duplicate_jump_widens_and_round_trips_xref(),
        python_hierarchy_filter_delimiter_and_live_edit_refresh(),
        built_in_ido_wrapper_uses_real_matches_and_navigation(),
        provider_failure_retains_real_index_then_recovers(),
        no_reachable_tags_reports_without_navigation(),
        missing_optional_frontends_fail_then_default_command_recovers(),
        real_projectile_project_filter_crosses_modes_but_excludes_outsider(),
    ]
}
