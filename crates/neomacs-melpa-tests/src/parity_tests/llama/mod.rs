use std::time::Duration;

use expect_test::expect;

use crate::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, LLAMA_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const LLAMA_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const LLAMA_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'seq)
(require 'llama)

(defvar llama-test-pristine-buffers (buffer-list))

(defun llama-test-reset ()
  (when llama-fontify-mode
    (llama-fontify-mode -1))
  (dolist (buffer (buffer-list))
    (unless (memq buffer llama-test-pristine-buffers)
      (kill-buffer buffer))))

(defun llama-test-eval-outcome (form)
  (condition-case problem
      (list :value (eval form t))
    (error
     (list :signal (car problem)
           (error-message-string problem)))))

(defun llama-test-face-at (token)
  (goto-char (point-min))
  (search-forward token)
  (get-text-property (- (point) (length token)) 'face))

(defun llama-test-fontified-faces ()
  (font-lock-ensure)
  (list :macro (llama-test-face-at "##")
        :special (llama-test-face-at "when")
        :mandatory (llama-test-face-at "%1")
        :deleted (llama-test-face-at "_%2")
        :optional (llama-test-face-at "&3")))
"###;

fn llama_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LLAMA_MELPA_PIN, "llama.el")
        .expect("prepare pinned Llama source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare pinned Compat dependency below ./tmp")
        .with_prelude(LLAMA_TEST_PRELUDE)
        .with_timeout(LLAMA_TEST_TIMEOUT)
}

fn compact_callbacks_drive_a_real_order_filter_projection_and_reduction_pipeline() -> ParityBatchCase
{
    let elisp_form = r##"
(progn
  (llama-test-reset)
  (let* ((orders '((:id "ORD-417" :state ready :amount 1299 :owner "ana")
                   (:id "ORD-418" :state held :amount 8450 :owner "bea")
                   (:id "ORD-419" :state ready :amount 3200 :owner "cy")
                   (:id "ORD-420" :state ready :amount 0 :owner "dan")))
         (ready
          (seq-filter
           (##and (eq (plist-get % :state) 'ready)
                  (> (plist-get % :amount) 0))
           orders))
         (manifest
          (mapcar
           (##list :id (plist-get % :id)
                   :owner (upcase (plist-get % :owner))
                   :cents (plist-get % :amount))
           ready))
         (total
          (seq-reduce (##+ %1 (plist-get %2 :amount)) ready 0)))
    (list :ready-count (length ready)
          :manifest manifest
          :total total
          :ids (mapcar (##plist-get % :id) ready))))
"##;
    let expect = expect![[
        r##"OK (:ready-count 2 :manifest ((:id "ORD-417" :owner "ANA" :cents 1299) (:id "ORD-419" :owner "CY" :cents 3200)) :total 4499 :ids ("ORD-417" "ORD-419"))"##
    ]];
    ParityBatchCase::value(
        "compact_callbacks_drive_a_real_order_filter_projection_and_reduction_pipeline",
        elisp_form,
        expect,
    )
}

fn optional_and_rest_placeholders_build_a_reusable_deployment_event_factory() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (llama-test-reset)
  (let ((event
         (##list :service %1
                 :region &2
                 :owner &3
                 :labels &*)))
    (list :arity (help-function-arglist event t)
          :minimal (funcall event "api")
          :region (funcall event "worker" "eu-west")
          :complete (funcall event "billing" "us-east" "ana"
                             "urgent" "customer-visible"))))
"##;
    let expect = expect![[
        r##"OK (:arity (%1 &optional &2 &3 &rest &*) :minimal (:service "api" :region nil :owner nil :labels nil) :region (:service "worker" :region "eu-west" :owner nil :labels nil) :complete (:service "billing" :region "us-east" :owner "ana" :labels ("urgent" "customer-visible")))"##
    ]];
    ParityBatchCase::value(
        "optional_and_rest_placeholders_build_a_reusable_deployment_event_factory",
        elisp_form,
        expect,
    )
}

fn quoted_backquoted_and_nested_forms_keep_placeholder_scopes_independent() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (llama-test-reset)
  (let* ((environment "production")
         (render
          (##list :id %1
                  :quoted '(%2)
                  :payload `(,environment ,%1 %2 ,%3)))
         (nested
          (##list :outer %1
                  :inner (funcall (##list :name % :metadata &2)
                                  "worker" "nightly"))))
    (list :render-arity (help-function-arglist render t)
          :rendered (funcall render "REL-417" :ignored 1299)
          :nested-arity (help-function-arglist nested t)
          :nested (funcall nested "deployment"))))
"##;
    let expect = expect![[
        r##"OK (:render-arity (%1 _%2 %3) :rendered (:id "REL-417" :quoted (%2) :payload ("production" "REL-417" %2 1299)) :nested-arity (%1) :nested (:outer "deployment" :inner (:name "worker" :metadata "nightly")))"##
    ]];
    ParityBatchCase::value(
        "quoted_backquoted_and_nested_forms_keep_placeholder_scopes_independent",
        elisp_form,
        expect,
    )
}

fn lexical_callbacks_and_left_and_right_partial_application_compose_service_steps()
-> ParityBatchCase {
    let elisp_form = r##"
(progn
  (llama-test-reset)
  (let* ((tax-rate 0.2)
         (price-with-tax (##round (* % (+ 1 tax-rate))))
         (service-event
          (llama--left-apply-partially #'list :service "api"))
         (with-audit
          (llama--right-apply-partially #'append '((audit . enabled))))
         (callbacks
          (mapcar (##let ((offset %)) (##+ offset %)) '(1 10 100))))
    (list :prices (mapcar price-with-tax '(100 250 999))
          :event (funcall service-event :state 'queued :attempt 2)
          :audit (funcall with-audit '((id . "REL-417") (state . ready)))
          :closures (cl-mapcar #'funcall callbacks '(5 5 5)))))
"##;
    let expect = expect![[
        r##"OK (:prices (120 300 1199) :event (:service "api" :state queued :attempt 2) :audit ((id . "REL-417") (state . ready) (audit . enabled)) :closures (6 15 105))"##
    ]];
    ParityBatchCase::value(
        "lexical_callbacks_and_left_and_right_partial_application_compose_service_steps",
        elisp_form,
        expect,
    )
}

fn macro_expansion_alias_completion_and_author_errors_define_the_compact_syntax_contract()
-> ParityBatchCase {
    let elisp_form = r##"
(progn
  (llama-test-reset)
  (list
   :expansions
   (list
    (macroexpand '(## list %1 _%3 &5 _&6))
    (macroexpand '(llama list `(,%1 %2 ,%3)))
    (macroexpand '(## % %2 2)))
   :aliases
   (list (eq (indirect-function '##) (indirect-function 'llama))
         (symbol-name '##)
         (macrop '##)
         (macrop 'llama))
   :empty-symbol-completion (member "" (all-completions "" obarray))
   :errors
   (mapcar
    #'llama-test-eval-outcome
    '((## list % &1)
      (## list &2 %2)
      (## list % _%1)
      (llama (list %1) %1)))))
"##;
    let expect = expect![[
        r##"OK (:expansions (#'(lambda (%1 _%2 _%3 &optional _&4 &5 _&6) (list %1 &5)) #'(lambda (%1 _%2 %3) (list `(,%1 %2 ,%3))) #'(lambda (_%1 %2) (% %2 2))) :aliases (t "" t t) :empty-symbol-completion nil :errors ((:signal error "‘%’ and ‘&1’ are mutually exclusive") (:signal error "‘&2’ and ‘%2’ are mutually exclusive") (:signal error "‘%’ and ‘_%1’ are mutually exclusive") (:signal wrong-type-argument "Wrong type argument: symbolp, \\`, (list %1)")))"##
    ]];
    ParityBatchCase::value(
        "macro_expansion_alias_completion_and_author_errors_define_the_compact_syntax_contract",
        elisp_form,
        expect,
    )
}

fn fontify_mode_updates_existing_and_future_elisp_buffers_and_removes_global_integration()
-> ParityBatchCase {
    let elisp_form = r##"
(progn
  (llama-test-reset)
  (let ((existing (generate-new-buffer "*llama-existing*"))
        future before enabled existing-faces future-faces disabled)
    (unwind-protect
        (progn
          (with-current-buffer existing
            (emacs-lisp-mode)
            (insert "(mapcar (##list %1 _%2 &3) rows)\n"
                    "(mapcar (##when %1 %1) rows)\n"))
          (setq before
                (list llama-fontify-mode
                      (and (advice-member-p #'lisp--el-match-keyword@llama
                                            'lisp--el-match-keyword)
                           t)
                      (and (advice-member-p #'elisp-mode-syntax-propertize@llama
                                            'elisp-mode-syntax-propertize)
                           t)
                      (and (memq #'llama--add-font-lock-keywords
                                 emacs-lisp-mode-hook)
                           t)))
          (llama-fontify-mode 1)
          (setq enabled
                (list llama-fontify-mode
                      (and (advice-member-p #'lisp--el-match-keyword@llama
                                            'lisp--el-match-keyword)
                           t)
                      (and (advice-member-p #'elisp-mode-syntax-propertize@llama
                                            'elisp-mode-syntax-propertize)
                           t)
                      (and (memq #'llama--add-font-lock-keywords
                                 emacs-lisp-mode-hook)
                           t)))
          (setq existing-faces
                (with-current-buffer existing
                  (llama-test-fontified-faces)))
          (setq future (generate-new-buffer "*llama-future*"))
          (with-current-buffer future
            (emacs-lisp-mode)
            (insert "(mapcar (##list %1 _%2 &3) rows)\n"
                    "(mapcar (##when %1 %1) rows)\n")
            (setq future-faces (llama-test-fontified-faces)))
          (llama-fontify-mode -1)
          (setq disabled
                (list llama-fontify-mode
                      (and (advice-member-p #'lisp--el-match-keyword@llama
                                            'lisp--el-match-keyword)
                           t)
                      (and (advice-member-p #'elisp-mode-syntax-propertize@llama
                                            'elisp-mode-syntax-propertize)
                           t)
                      (and (memq #'llama--add-font-lock-keywords
                                 emacs-lisp-mode-hook)
                           t)))
          (list :lifecycle (list before enabled disabled)
                :existing existing-faces
                :future future-faces))
      (dolist (buffer (list existing future))
        (when (buffer-live-p buffer)
          (kill-buffer buffer))))))
"##;
    let expect = expect![[
        r##"OK (:lifecycle ((nil nil nil nil) (t t t t) (nil nil nil nil)) :existing (:macro llama-\#\#-macro :special font-lock-keyword-face :mandatory llama-mandatory-argument :deleted (llama-deleted-argument llama-mandatory-argument) :optional llama-optional-argument) :future (:macro llama-\#\#-macro :special font-lock-keyword-face :mandatory llama-mandatory-argument :deleted (llama-deleted-argument llama-mandatory-argument) :optional llama-optional-argument))"##
    ]];
    ParityBatchCase::value(
        "fontify_mode_updates_existing_and_future_elisp_buffers_and_removes_global_integration",
        elisp_form,
        expect,
    )
}

#[test]
fn llama_package_batch() {
    let cases = vec![
        compact_callbacks_drive_a_real_order_filter_projection_and_reduction_pipeline(),
        optional_and_rest_placeholders_build_a_reusable_deployment_event_factory(),
        quoted_backquoted_and_nested_forms_keep_placeholder_scopes_independent(),
        lexical_callbacks_and_left_and_right_partial_application_compose_service_steps(),
        macro_expansion_alias_completion_and_author_errors_define_the_compact_syntax_contract(),
        fontify_mode_updates_existing_and_future_elisp_buffers_and_removes_global_integration(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed llama parity test");
    assert_oracle_batch_cases(llama_oracle(), test_name, "llama_parity", &cases);
}
