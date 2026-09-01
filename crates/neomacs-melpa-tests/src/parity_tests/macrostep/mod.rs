use std::time::Duration;

use expect_test::expect;

use crate::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, MACROSTEP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const MACROSTEP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const MACROSTEP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'macrostep)

(defun macrostep-test-reset ()
  (when (bound-and-true-p macrostep-mode)
    (macrostep-collapse-all))
  (dolist (buffer (buffer-list))
    (when (string-prefix-p "*macro expansion*" (buffer-name buffer))
      (kill-buffer buffer))))

(defmacro macrostep-test-deploy (release &rest steps)
  `(let ((ticket ,release))
     (list :release ticket :steps (list ,@steps))))

(defmacro macrostep-test-wrap (&rest body)
  `(progn (macrostep-test-deploy "REL-417" ,@body)))

(defun macrostep-test-resolve (service region)
  (list :runtime service region))

(cl-define-compiler-macro macrostep-test-resolve
    (&whole _form service region)
  `(list :optimized-service ,service :optimized-region ,region))

(defmacro macrostep-test-release-plan (release service region)
  `(list :release ,release
         :resolution (macrostep-test-resolve ,service ,region)))

(defmacro macrostep-test-evaluate-once (value &rest body)
  (let ((ticket (make-symbol "ticket"))
        (result (make-symbol "result")))
    `(let* ((,ticket ,value)
             (,result (progn ,@body)))
       (list :ticket ,ticket
             :result ,result
             :quoted '(left . right)))))

(defun macrostep-test-overlay-state (overlay)
  (let ((highlight
         (overlay-get overlay 'macrostep-highlight-overlay))
        (original
         (overlay-get overlay 'macrostep-original-text)))
    (list
     :range (list (overlay-start overlay) (overlay-end overlay))
     :priority (overlay-get overlay 'priority)
     :original
     (if (stringp original)
         (substring-no-properties original)
       original)
     :gensym-depth (overlay-get overlay 'macrostep-gensym-depth)
     :highlight
     (and highlight
          (list :range
                (list (overlay-start highlight) (overlay-end highlight))
                :face (overlay-get highlight 'face)
                :priority (overlay-get highlight 'priority))))))

(defun macrostep-test-overlay-states ()
  (mapcar
   #'macrostep-test-overlay-state
   (sort (copy-sequence macrostep-overlays)
         (lambda (left right)
           (< (overlay-get left 'priority)
              (overlay-get right 'priority))))))

(defun macrostep-test-gensym-occurrences ()
  (save-excursion
    (goto-char (point-min))
    (let (occurrences)
      (while (re-search-forward "\\_<\\(ticket\\|result\\)\\_>" nil t)
        (let ((start (match-beginning 0)))
          (when-let ((face
                      (get-text-property start 'font-lock-face)))
            (push
             (list :name (match-string-no-properties 1)
                   :face face)
             occurrences))))
      (nreverse occurrences))))
"##;

fn macrostep_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MACROSTEP_MELPA_PIN, "macrostep.el")
        .expect("prepare pinned Macrostep source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare pinned Compat dependency below ./tmp")
        .with_prelude(MACROSTEP_TEST_PRELUDE)
        .with_timeout(MACROSTEP_TEST_TIMEOUT)
}

fn inline_release_expansion_preserves_editor_state_and_collapses_exactly() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (macrostep-test-reset)
  (with-temp-buffer
    (emacs-lisp-mode)
    (buffer-enable-undo)
    (insert
     "(let ((owner \"ana\"))\n"
     "  (macrostep-test-deploy \"REL-417\"\n"
     "    (list :owner owner :state 'ready)))\n")
    (setq buffer-undo-list nil)
    (set-buffer-modified-p nil)
    (goto-char (point-min))
    (search-forward "(macrostep-test-deploy")
    (goto-char (match-beginning 0))
    (let ((original (buffer-string))
          (original-point (point)))
      (macrostep-expand)
      (let ((expanded
             (list
              :content (buffer-substring-no-properties
                        (point-min) (point-max))
              :point (point)
              :mode macrostep-mode
              :read-only buffer-read-only
              :undo-disabled (eq buffer-undo-list t)
              :modified (buffer-modified-p)
              :hook-installed
              (and (memq #'macrostep-command-hook post-command-hook) t)
              :keys
              (mapcar
               (lambda (key)
                 (lookup-key macrostep-mode-map (kbd key)))
               '("e" "u" "n" "p" "q"))
              :overlays (macrostep-test-overlay-states))))
        (macrostep-collapse)
        (list
         :expanded expanded
         :collapsed
         (list :content (buffer-string)
               :original-restored (equal (buffer-string) original)
               :point (point)
               :original-point original-point
               :mode macrostep-mode
               :read-only buffer-read-only
               :undo-restored (null buffer-undo-list)
               :modified (buffer-modified-p)
               :hook-removed
               (not (memq #'macrostep-command-hook post-command-hook))
               :overlays macrostep-overlays))))))
"##;
    let expect = expect![[
        r##"OK (:expanded (:content "(let ((owner \"ana\"))\n  (let ((ticket \"REL-417\"))\n    (list :release ticket :steps\n\11  (list (list :owner owner :state 'ready)))))\n" :point 24 :mode t :read-only t :undo-disabled t :modified nil :hook-installed t :keys (macrostep-expand macrostep-collapse macrostep-next-macro macrostep-prev-macro macrostep-collapse-all) :overlays ((:range (24 128) :priority 1 :original "(macrostep-test-deploy \"REL-417\"\n    (list :owner owner :state 'ready))" :gensym-depth -1 :highlight (:range (24 128) :face macrostep-expansion-highlight-face :priority -1)))) :collapsed (:content "(let ((owner \"ana\"))\n  (macrostep-test-deploy \"REL-417\"\n    (list :owner owner :state 'ready)))\n" :original-restored t :point 24 :original-point 24 :mode nil :read-only nil :undo-restored t :modified nil :hook-removed t :overlays nil))"##
    ]];
    ParityBatchCase::value(
        "inline_release_expansion_preserves_editor_state_and_collapses_exactly",
        elisp_form,
        expect,
    )
}

fn nested_expansions_can_be_stepped_and_collapsed_one_level_at_a_time() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (macrostep-test-reset)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(macrostep-test-wrap (message \"verify\"))")
    (goto-char (point-min))
    (let ((original (buffer-string)))
      (macrostep-expand)
      (let ((outer-content
             (buffer-substring-no-properties (point-min) (point-max)))
            (outer-overlays (macrostep-test-overlay-states)))
        (goto-char (point-min))
        (search-forward "(macrostep-test-deploy")
        (goto-char (match-beginning 0))
        (macrostep-expand)
        (let ((inner-content
               (buffer-substring-no-properties (point-min) (point-max)))
              (nested-overlays (macrostep-test-overlay-states)))
          (macrostep-collapse)
          (let ((after-inner-collapse
                 (buffer-substring-no-properties (point-min) (point-max)))
                (remaining-overlays (macrostep-test-overlay-states))
                (mode-after-inner macrostep-mode))
            (macrostep-collapse)
            (list
             :outer-content outer-content
             :outer-overlays outer-overlays
             :inner-content inner-content
             :nested-overlays nested-overlays
             :after-inner-collapse after-inner-collapse
             :remaining-overlays remaining-overlays
             :mode-after-inner mode-after-inner
             :final
             (list :content (buffer-string)
                   :original-restored (equal (buffer-string) original)
                   :mode macrostep-mode
                   :overlays macrostep-overlays))))))))
"##;
    let expect = expect![[
        r##"OK (:outer-content "(progn (macrostep-test-deploy \"REL-417\" (message \"verify\")))" :outer-overlays ((:range (1 61) :priority 1 :original "(macrostep-test-wrap (message \"verify\"))" :gensym-depth -1 :highlight (:range (1 61) :face macrostep-expansion-highlight-face :priority -1))) :inner-content "(progn (let ((ticket \"REL-417\"))\n\11 (list :release ticket :steps (list (message \"verify\")))))" :nested-overlays ((:range (1 93) :priority 1 :original "(macrostep-test-wrap (message \"verify\"))" :gensym-depth -1 :highlight (:range (1 93) :face macrostep-expansion-highlight-face :priority -1)) (:range (8 92) :priority 2 :original "(macrostep-test-deploy \"REL-417\" (message \"verify\"))" :gensym-depth -1 :highlight (:range (8 92) :face macrostep-expansion-highlight-face :priority -1))) :after-inner-collapse "(progn (macrostep-test-deploy \"REL-417\" (message \"verify\")))" :remaining-overlays ((:range (1 61) :priority 1 :original "(macrostep-test-wrap (message \"verify\"))" :gensym-depth -1 :highlight (:range (1 61) :face macrostep-expansion-highlight-face :priority -1))) :mode-after-inner t :final (:content "(macrostep-test-wrap (message \"verify\"))" :original-restored t :mode nil :overlays nil))"##
    ]];
    ParityBatchCase::value(
        "nested_expansions_can_be_stepped_and_collapsed_one_level_at_a_time",
        elisp_form,
        expect,
    )
}

fn local_macro_environment_drives_a_record_projection_workflow() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (macrostep-test-reset)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert
     "(cl-macrolet\n"
     "    ((field (record key) `(plist-get ,record ,key))\n"
     "     (emit (id amount) `(list :id ,id :amount ,amount)))\n"
     "  (let ((order '(:id \"REL-417\" :amount 1299)))\n"
     "    (emit (field order :id) (field order :amount))))")
    (goto-char (point-min))
    (search-forward "(emit (field")
    (goto-char (match-beginning 0))
    (let* ((environment (macrostep-environment-at-point))
           (field-definition (cdr (assq 'field environment)))
           (emit-definition (cdr (assq 'emit environment)))
           (environment-summary
            (list
             :names
             (mapcar
              #'car
              (cl-remove-if-not
               (lambda (entry) (memq (car entry) '(field emit)))
               environment))
             :field-expansion
             (apply field-definition '(order :amount))
             :emit-expansion
             (apply emit-definition
                    '((field order :id) (field order :amount))))))
      (macrostep-expand)
      (let ((after-emit
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "(field order :id)")
        (goto-char (match-beginning 0))
        (let* ((saved-environment
                (get-text-property (point) 'macrostep-environment))
               (property-summary
                (list
                 :macro-start
                 (get-text-property (point) 'macrostep-macro-start)
                 :expanded-text
                 (get-text-property (point) 'macrostep-expanded-text)
                 :face
                 (get-text-property (1+ (point)) 'font-lock-face)
                 :saved-names
                 (mapcar
                  #'car
                  (cl-remove-if-not
                   (lambda (entry) (memq (car entry) '(field emit)))
                   saved-environment))
                 :recognized
                 (macrostep-macro-form-p
                  (macrostep-sexp-at-point) saved-environment))))
          (macrostep-expand)
          (let ((after-field
                 (buffer-substring-no-properties (point-min) (point-max)))
                (overlays (macrostep-test-overlay-states)))
            (macrostep-collapse-all)
            (list
             :environment environment-summary
             :after-emit after-emit
             :property property-summary
             :after-field after-field
             :overlays overlays
             :restored
             (buffer-substring-no-properties
              (point-min) (point-max)))))))))
"##;
    let expect = expect![[
        r##"OK (:environment (:names (emit field) :field-expansion (plist-get order :amount) :emit-expansion (list :id (field order :id) :amount (field order :amount))) :after-emit "(cl-macrolet\n    ((field (record key) `(plist-get ,record ,key))\n     (emit (id amount) `(list :id ,id :amount ,amount)))\n  (let ((order '(:id \"REL-417\" :amount 1299)))\n    (list :id (field order :id) :amount (field order :amount))))" :property (:macro-start t :expanded-text (field order :id) :face macrostep-macro-face :saved-names (emit field) :recognized macro) :after-field "(cl-macrolet\n    ((field (record key) `(plist-get ,record ,key))\n     (emit (id amount) `(list :id ,id :amount ,amount)))\n  (let ((order '(:id \"REL-417\" :amount 1299)))\n    (list :id (plist-get order :id) :amount (field order :amount))))" :overlays ((:range (174 236) :priority 1 :original "(emit (field order :id) (field order :amount))" :gensym-depth -1 :highlight (:range (174 236) :face macrostep-expansion-highlight-face :priority -1)) (:range (184 205) :priority 2 :original "(field order :id)" :gensym-depth -1 :highlight (:range (184 205) :face macrostep-expansion-highlight-face :priority -1))) :restored "(cl-macrolet\n    ((field (record key) `(plist-get ,record ,key))\n     (emit (id amount) `(list :id ,id :amount ,amount)))\n  (let ((order '(:id \"REL-417\" :amount 1299)))\n    (emit (field order :id) (field order :amount))))")"##
    ]];
    ParityBatchCase::value(
        "local_macro_environment_drives_a_record_projection_workflow",
        elisp_form,
        expect,
    )
}

fn compiler_macro_navigation_exposes_and_expands_an_optimized_resolution_step() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (macrostep-test-reset)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert
     "(macrostep-test-release-plan \"REL-417\" \"api\" \"us-east\")")
    (goto-char (point-min))
    (let ((original (buffer-string)))
      (macrostep-expand)
      (let ((after-plan
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "(macrostep-test-resolve")
        (goto-char (match-beginning 0))
        (let* ((form (macrostep-sexp-at-point))
               (classification
                (list
                 :enabled (macrostep-macro-form-p form nil)
                 :disabled
                 (let ((macrostep-expand-compiler-macros nil))
                   (macrostep-macro-form-p form nil))
                 :macro-start
                 (get-text-property (point) 'macrostep-macro-start)
                 :expanded-text
                 (get-text-property (point) 'macrostep-expanded-text)
                 :head-face
                 (get-text-property (1+ (point)) 'font-lock-face))))
          (macrostep-expand)
          (let ((after-compiler
                 (buffer-substring-no-properties (point-min) (point-max)))
                (overlays (macrostep-test-overlay-states)))
            (macrostep-collapse-all)
            (list
             :after-plan after-plan
             :classification classification
             :after-compiler after-compiler
             :overlays overlays
             :restored
             (list :content (buffer-string)
                   :matches-original (equal (buffer-string) original)
                   :mode macrostep-mode))))))))
"##;
    let expect = expect![[
        r##"OK (:after-plan "(list :release \"REL-417\" :resolution\n      (macrostep-test-resolve \"api\" \"us-east\"))" :classification (:enabled compiler-macro :disabled nil :macro-start t :expanded-text (macrostep-test-resolve "api" "us-east") :head-face macrostep-compiler-macro-face) :after-compiler "(list :release \"REL-417\" :resolution\n      (list :optimized-service \"api\" :optimized-region \"us-east\"))" :overlays ((:range (1 104) :priority 1 :original "(macrostep-test-release-plan \"REL-417\" \"api\" \"us-east\")" :gensym-depth -1 :highlight (:range (1 104) :face macrostep-expansion-highlight-face :priority -1)) (:range (44 103) :priority 2 :original "(macrostep-test-resolve \"api\" \"us-east\")" :gensym-depth -1 :highlight (:range (44 103) :face macrostep-expansion-highlight-face :priority -1))) :restored (:content "(macrostep-test-release-plan \"REL-417\" \"api\" \"us-east\")" :matches-original t :mode nil))"##
    ]];
    ParityBatchCase::value(
        "compiler_macro_navigation_exposes_and_expands_an_optimized_resolution_step",
        elisp_form,
        expect,
    )
}

fn pretty_printer_keeps_gensyms_consistent_and_preserves_quoted_data() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (macrostep-test-reset)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert
     "(macrostep-test-evaluate-once\n"
     "    (fetch-ticket \"REL-417\")\n"
     "  (validate-ticket))")
    (goto-char (point-min))
    (macrostep-expand)
    (let* ((content
            (buffer-substring-no-properties (point-min) (point-max)))
           (occurrences (macrostep-test-gensym-occurrences))
           (faces (mapcar (lambda (entry) (plist-get entry :face))
                          occurrences))
           (overlay-state (macrostep-test-overlay-states)))
      (macrostep-collapse-all)
      (list
       :content content
       :occurrences occurrences
       :same-level-face
       (and faces
            (cl-every
             (lambda (face) (eq face (car faces)))
             (cdr faces)))
       :overlay overlay-state
       :restored
       (buffer-substring-no-properties (point-min) (point-max))))))
"##;
    let expect = expect![[
        r##"OK (:content "(let*\n    ((ticket (fetch-ticket \"REL-417\"))\n     (result (progn (validate-ticket))))\n  (list :ticket ticket :result result :quoted '(left . right)))" :occurrences ((:name "ticket" :face macrostep-gensym-1) (:name "result" :face macrostep-gensym-1) (:name "ticket" :face macrostep-gensym-1) (:name "result" :face macrostep-gensym-1)) :same-level-face t :overlay ((:range (1 150) :priority 1 :original "(macrostep-test-evaluate-once\n    (fetch-ticket \"REL-417\")\n  (validate-ticket))" :gensym-depth 0 :highlight (:range (1 150) :face macrostep-expansion-highlight-face :priority -1))) :restored "(macrostep-test-evaluate-once\n    (fetch-ticket \"REL-417\")\n  (validate-ticket))")"##
    ]];
    ParityBatchCase::value(
        "pretty_printer_keeps_gensyms_consistent_and_preserves_quoted_data",
        elisp_form,
        expect,
    )
}

fn separate_expansion_buffer_steps_without_mutating_the_source_buffer() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (macrostep-test-reset)
  (let ((source (generate-new-buffer " *macrostep-source*"))
        result)
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer source)
          (emacs-lisp-mode)
          (insert "(macrostep-test-wrap (message \"verify\"))")
          (goto-char (point-min))
          (let ((source-content (buffer-string))
                (source-point (point)))
            (macrostep-expand t)
            (let* ((expansion-buffer (current-buffer))
                   (first-step
                    (list
                     :different-buffer (not (eq expansion-buffer source))
                     :buffer-name (buffer-name expansion-buffer)
                     :expansion-buffer macrostep-expansion-buffer
                     :mode macrostep-mode
                     :read-only buffer-read-only
                     :content
                     (buffer-substring-no-properties
                      (point-min) (point-max))
                     :overlays (macrostep-test-overlay-states)
                     :source
                     (with-current-buffer source
                       (list :content (buffer-string)
                             :unchanged
                             (equal (buffer-string) source-content)
                             :point (point)
                             :point-unchanged (= (point) source-point))))))
              (goto-char (point-min))
              (search-forward "(macrostep-test-deploy")
              (goto-char (match-beginning 0))
              (macrostep-expand)
              (let ((second-step
                     (list
                      :content
                      (buffer-substring-no-properties
                       (point-min) (point-max))
                      :overlays (macrostep-test-overlay-states))))
                (macrostep-collapse)
                (let ((after-inner-collapse
                       (buffer-substring-no-properties
                        (point-min) (point-max))))
                  (macrostep-collapse-all)
                  (setq result
                        (list
                         :first first-step
                         :second second-step
                         :after-inner-collapse after-inner-collapse
                         :closed
                         (list
                          :expansion-buffer-live
                          (buffer-live-p expansion-buffer)
                          :returned-to-source
                          (eq (current-buffer) source)
                          :source-content
                          (buffer-substring-no-properties
                           (point-min) (point-max))
                          :source-unchanged
                          (equal
                           (buffer-substring-no-properties
                            (point-min) (point-max))
                           source-content)))))))))
      (when (buffer-live-p source)
        (kill-buffer source))
      (macrostep-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:first (:different-buffer t :buffer-name "*macro expansion*" :expansion-buffer t :mode t :read-only t :content "(progn (macrostep-test-deploy \"REL-417\" (message \"verify\")))" :overlays ((:range (1 61) :priority 1 :original "(macrostep-test-wrap (message \"verify\"))" :gensym-depth -1 :highlight nil)) :source (:content "(macrostep-test-wrap (message \"verify\"))" :unchanged t :point 1 :point-unchanged t)) :second (:content "(progn (let ((ticket \"REL-417\"))\n\11 (list :release ticket :steps (list (message \"verify\")))))" :overlays ((:range (1 93) :priority 1 :original "(macrostep-test-wrap (message \"verify\"))" :gensym-depth -1 :highlight nil) (:range (8 92) :priority 2 :original "(macrostep-test-deploy \"REL-417\" (message \"verify\"))" :gensym-depth -1 :highlight nil))) :after-inner-collapse "(progn (macrostep-test-deploy \"REL-417\" (message \"verify\")))" :closed (:expansion-buffer-live nil :returned-to-source t :source-content "(macrostep-test-wrap (message \"verify\"))" :source-unchanged t))"##
    ]];
    ParityBatchCase::value(
        "separate_expansion_buffer_steps_without_mutating_the_source_buffer",
        elisp_form,
        expect,
    )
}

fn failing_custom_printer_rolls_back_partial_text_and_restores_mode_state() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (macrostep-test-reset)
  (with-temp-buffer
    (emacs-lisp-mode)
    (buffer-enable-undo)
    (insert "(deployment REL-417)")
    (setq buffer-undo-list nil)
    (set-buffer-modified-p nil)
    (goto-char (point-min))
    (let ((macrostep-environment-at-point-function #'ignore)
          (macrostep-expand-1-function
           (lambda (_form _environment)
             '(list :release "REL-417")))
          (macrostep-macro-form-p-function
           (lambda (_form _environment) t))
          (macrostep-print-function
           (lambda (_form _environment)
             (insert "(partial-render")
             (error "macrostep test renderer failed"))))
      (let ((failure
             (condition-case problem
                 (progn
                   (macrostep-expand)
                   :not-signaled)
               (error
                (list (car problem) (error-message-string problem))))))
        (let ((failed-state
               (list
                :failure failure
                :content (buffer-string)
                :point (point)
                :mode macrostep-mode
                :read-only buffer-read-only
                :undo-disabled (eq buffer-undo-list t)
                :modified (buffer-modified-p)
                :overlays macrostep-overlays
                :hook-installed
                (and (memq #'macrostep-command-hook post-command-hook) t))))
          (macrostep-mode 0)
          (list
           :failed failed-state
           :cleaned
           (list :content (buffer-string)
                 :mode macrostep-mode
                 :read-only buffer-read-only
                 :undo-restored (null buffer-undo-list)
                 :modified (buffer-modified-p)
                 :hook-removed
                 (not
                  (memq #'macrostep-command-hook post-command-hook)))))))))
"##;
    let expect = expect![[
        r##"OK (:failed (:failure (error "macrostep test renderer failed") :content "(deployment REL-417)" :point 1 :mode t :read-only t :undo-disabled t :modified nil :overlays nil :hook-installed t) :cleaned (:content "(deployment REL-417)" :mode nil :read-only nil :undo-restored t :modified nil :hook-removed t))"##
    ]];
    ParityBatchCase::value(
        "failing_custom_printer_rolls_back_partial_text_and_restores_mode_state",
        elisp_form,
        expect,
    )
}

#[test]
fn macrostep_package_batch() {
    let cases = vec![
        inline_release_expansion_preserves_editor_state_and_collapses_exactly(),
        nested_expansions_can_be_stepped_and_collapsed_one_level_at_a_time(),
        local_macro_environment_drives_a_record_projection_workflow(),
        compiler_macro_navigation_exposes_and_expands_an_optimized_resolution_step(),
        pretty_printer_keeps_gensyms_consistent_and_preserves_quoted_data(),
        separate_expansion_buffer_steps_without_mutating_the_source_buffer(),
        failing_custom_printer_rolls_back_partial_text_and_restores_mode_state(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Macrostep parity test");
    assert_oracle_batch_cases(macrostep_oracle(), test_name, "macrostep_parity", &cases);
}
