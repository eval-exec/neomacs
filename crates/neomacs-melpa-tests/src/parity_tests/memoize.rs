use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MEMOIZE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'memoize)

(defun neomacs-memoize-test-error (function)
  "Return FUNCTION's value or stable error details."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"###;

fn package_contract_exposes_cache_policies_and_definition_forms() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'memoize package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'memoize) t))
   :default-timeout memoize-default-timeout
   :entry-points
   (mapcar #'fboundp
           '(memoize memoize-restore defmemoize
             memoize-by-buffer-contents
             defmemoize-by-buffer-contents))
   :macro-shapes
   (list (car (macroexpand-1 '(defmemoize cached-sum (a b) (+ a b))))
         (car (macroexpand-1
               '(defmemoize-by-buffer-contents line-count ()
                  (count-lines (point-min) (point-max))))))))
"###;
    let expected = expect![[
        r#"OK (:package (:name memoize :version "20200103.2036" :requirements nil :feature t) :default-timeout "2 hours" :entry-points (t t t t t) :macro-shapes (progn progn))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_cache_policies_and_definition_forms",
        elisp_form,
        expected,
    )
}

fn structured_equal_requests_share_results_while_distinct_options_get_new_entries()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((calls 0)
       (memoize-default-timeout nil)
       (cached
        (memoize
         (lambda (request options)
           (setq calls (1+ calls))
           (list :call calls
                 :path (plist-get request :path)
                 :fields (plist-get options :fields)))
         nil))
       (first (funcall cached
                       '(:path "/users/42")
                       '(:fields (name email))))
       (equal-but-distinct-arguments
        (funcall cached
                 (list :path (concat "/users/" "42"))
                 (list :fields (list 'name 'email))))
       (different-options
        (funcall cached
                 '(:path "/users/42")
                 '(:fields (name teams))))
       (second-hit
        (funcall cached
                 '(:path "/users/42")
                 '(:fields (name teams)))))
  (list :calls calls
        :values (list first equal-but-distinct-arguments
                      different-options second-hit)
        :identity (list (eq first equal-but-distinct-arguments)
                        (eq different-options second-hit))))
"###;
    let expected = expect![[
        r#"OK (:calls 2 :values (#1=(:call 1 :path "/users/42" :fields (name email)) #1# #2=(:call 2 :path "/users/42" :fields (name teams)) #2#) :identity (t t))"#
    ]];
    ParityBatchCase::value(
        "structured_equal_requests_share_results_while_distinct_options_get_new_entries",
        elisp_form,
        expected,
    )
}

fn nil_results_and_signaled_failures_are_recomputed_instead_of_being_cached() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((calls 0)
       (memoize-default-timeout nil)
       (lookup
        (memoize
         (lambda (key)
           (setq calls (1+ calls))
           (if (eq key 'explode)
               (error "backend unavailable on call %s" calls)
             (cdr (assq key '((known . "ready"))))))
         nil))
       (missing-first (funcall lookup 'missing))
       (missing-second (funcall lookup 'missing))
       (known-first (funcall lookup 'known))
       (known-second (funcall lookup 'known))
       (failure-first
        (neomacs-memoize-test-error (lambda () (funcall lookup 'explode))))
       (failure-second
        (neomacs-memoize-test-error (lambda () (funcall lookup 'explode)))))
  (list :calls calls
        :missing (list missing-first missing-second)
        :known (list known-first known-second (eq known-first known-second))
        :failures (list failure-first failure-second)))
"###;
    let expected = expect![[
        r#"OK (:calls 5 :missing (nil nil) :known ("ready" "ready" t) :failures ((:error error :data ("backend unavailable on call 4") :message "backend unavailable on call 4") (:error error :data ("backend unavailable on call 5") :message "backend unavailable on call 5")))"#
    ]];
    ParityBatchCase::value(
        "nil_results_and_signaled_failures_are_recomputed_instead_of_being_cached",
        elisp_form,
        expected,
    )
}

fn timeout_access_renews_each_key_and_expiry_recomputes_only_that_entry() -> ParityBatchCase {
    let elisp_form = r###"
(let ((calls 0)
      scheduled
      canceled)
  (cl-letf (((symbol-function 'run-at-time)
             (lambda (timeout repeat function &rest arguments)
               (let ((token (intern (format "timer-%s" (1+ (length scheduled))))))
                 (push (list token timeout repeat function arguments) scheduled)
                 token)))
            ((symbol-function 'cancel-timer)
             (lambda (timer)
               (push timer canceled)
               nil)))
    (let* ((memoize-default-timeout "2 hours")
           (cached
            (memoize
             (lambda (key)
               (setq calls (1+ calls))
               (list key calls))
             "10 seconds"))
           (alpha-first (funcall cached 'alpha))
           (alpha-hit (funcall cached 'alpha))
           (beta-first (funcall cached 'beta))
           (alpha-expiry (nth 3 (nth 1 scheduled))))
      (funcall alpha-expiry)
      (let ((alpha-refreshed (funcall cached 'alpha))
            (beta-hit (funcall cached 'beta)))
        (list
         :calls calls
         :values (list alpha-first alpha-hit beta-first
                       alpha-refreshed beta-hit)
         :scheduled
         (mapcar (lambda (event) (list (nth 0 event) (nth 1 event) (nth 2 event)))
                 (nreverse scheduled))
         :canceled (nreverse canceled))))))
"###;
    let expected = expect![[
        r#"OK (:calls 3 :values (#1=(alpha 1) #1# #2=(beta 2) (alpha 3) #2#) :scheduled ((timer-1 "10 seconds" nil) (timer-2 "10 seconds" nil) (timer-3 "10 seconds" nil) (timer-4 "10 seconds" nil) (timer-5 "10 seconds" nil)) :canceled (timer-1 timer-2 timer-3))"#
    ]];
    ParityBatchCase::value(
        "timeout_access_renews_each_key_and_expiry_recomputes_only_that_entry",
        elisp_form,
        expected,
    )
}

fn named_function_memoization_updates_documentation_rejects_stacking_and_restores()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((calls 0)
      (memoize-default-timeout nil))
  (unwind-protect
      (progn
        (fset 'neomacs-memoize-test-price
              (lambda (quantity unit-price)
                "Compute the order price."
                (setq calls (1+ calls))
                (list :total (* quantity unit-price) :call calls)))
        (let ((original (symbol-function 'neomacs-memoize-test-price))
              before memoized restored)
          (setq before
                (list (documentation 'neomacs-memoize-test-price t)
                      (funcall 'neomacs-memoize-test-price 3 25)))
          (memoize 'neomacs-memoize-test-price nil)
          (setq memoized
                (list
                 (documentation 'neomacs-memoize-test-price t)
                 (funcall 'neomacs-memoize-test-price 3 25)
                 (funcall 'neomacs-memoize-test-price 3 25)
                 (funcall 'neomacs-memoize-test-price 4 25)
                 (neomacs-memoize-test-error
                  (lambda () (memoize 'neomacs-memoize-test-price nil)))))
          (memoize-restore 'neomacs-memoize-test-price)
          (setq restored
                (list
                 (eq original (symbol-function 'neomacs-memoize-test-price))
                 (documentation 'neomacs-memoize-test-price t)
                 (funcall 'neomacs-memoize-test-price 3 25)
                 (neomacs-memoize-test-error
                  (lambda () (memoize-restore 'neomacs-memoize-test-price)))))
          (list :before before
                :memoized memoized
                :restored restored
                :calls calls)))
    (fmakunbound 'neomacs-memoize-test-price)
    (setplist 'neomacs-memoize-test-price nil)))
"###;
    let expected = expect![[
        r#"OK (:before ("Compute the order price." (:total 75 :call 1)) :memoized ("Compute the order price. (memoized)" #1=(:total 75 :call 2) #1# (:total 100 :call 3) (:error user-error :data ("neomacs-memoize-test-price is already memoized") :message "neomacs-memoize-test-price is already memoized")) :restored (t "Compute the order price." (:total 75 :call 4) (:error user-error :data ("neomacs-memoize-test-price is not memoized") :message "neomacs-memoize-test-price is not memoized")) :calls 4)"#
    ]];
    ParityBatchCase::value(
        "named_function_memoization_updates_documentation_rejects_stacking_and_restores",
        elisp_form,
        expected,
    )
}

fn recursive_defmemoize_reuses_subproblems_across_successive_business_calculations()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((calls 0)
      (memoize-default-timeout nil))
  (unwind-protect
      (progn
        (defmemoize neomacs-memoize-test-factorial (number)
          "Compute factorial while recording actual evaluations."
          (setq calls (1+ calls))
          (if (zerop number)
              1
            (* number (neomacs-memoize-test-factorial (1- number)))))
        (let ((eight (neomacs-memoize-test-factorial 8))
              (after-eight calls)
              (ten (neomacs-memoize-test-factorial 10))
              (after-ten calls)
              (ten-again (neomacs-memoize-test-factorial 10)))
          (list :values (list eight ten ten-again)
                :calls (list after-eight after-ten calls)
                :documentation
                (documentation 'neomacs-memoize-test-factorial t))))
    (fmakunbound 'neomacs-memoize-test-factorial)
    (setplist 'neomacs-memoize-test-factorial nil)))
"###;
    let expected = expect![[
        r#"OK (:values (40320 3628800 3628800) :calls (9 11 11) :documentation "Compute factorial while recording actual evaluations. (memoized)")"#
    ]];
    ParityBatchCase::value(
        "recursive_defmemoize_reuses_subproblems_across_successive_business_calculations",
        elisp_form,
        expected,
    )
}

fn buffer_content_cache_shares_equal_documents_and_invalidates_semantic_edits() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((calls 0)
       (analyze
        (memoize-by-buffer-contents--wrap
         (lambda (severity)
           (setq calls (1+ calls))
           (list :call calls
                 :severity severity
                 :todos (how-many "TODO" (point-min) (point-max))
                 :hash (secure-hash 'md5 (buffer-string))))))
       first first-hit styled-hit second-same second-edited second-hit)
  (with-temp-buffer
    (insert "TODO: parse request\nDONE: validate input\n")
    (setq first (funcall analyze 'warning)
          first-hit (funcall analyze 'warning))
    (add-text-properties (point-min) (+ (point-min) 4)
                         '(face font-lock-warning-face))
    (setq styled-hit (funcall analyze 'warning)))
  (with-temp-buffer
    (insert "TODO: parse request\nDONE: validate input\n")
    (setq second-same (funcall analyze 'warning))
    (goto-char (point-max))
    (insert "TODO: document edge case\n")
    (setq second-edited (funcall analyze 'warning)
          second-hit (funcall analyze 'warning)))
  (list :calls calls
        :first (list first first-hit styled-hit)
        :second (list second-same second-edited second-hit)
        :identity
        (list (eq first first-hit)
              (eq first styled-hit)
              (eq first second-same)
              (eq second-edited second-hit))))
"###;
    let expected = expect![[
        r#"OK (:calls 2 :first (#1=(:call 1 :severity warning :todos 1 :hash "365d8cb082cb69d033e705feb77e3fc8") #1# #1#) :second (#1# #2=(:call 2 :severity warning :todos 2 :hash "e4dd170bd90424d2e6db990e705aeea5") #2#) :identity (t t t t))"#
    ]];
    ParityBatchCase::value(
        "buffer_content_cache_shares_equal_documents_and_invalidates_semantic_edits",
        elisp_form,
        expected,
    )
}

fn buffer_content_macro_keeps_argument_variants_separate_within_each_revision() -> ParityBatchCase {
    let elisp_form = r###"
(let ((calls 0))
  (unwind-protect
      (progn
        (defmemoize-by-buffer-contents neomacs-memoize-test-lines (include-empty)
          "Summarize the current document revision."
          (setq calls (1+ calls))
          (let ((lines (split-string (buffer-string) "\n" (not include-empty))))
            (list :call calls :lines lines)))
        (with-temp-buffer
          (insert "alpha\n\nbeta\n")
          (let ((compact (neomacs-memoize-test-lines nil))
                (compact-hit (neomacs-memoize-test-lines nil))
                (with-empty (neomacs-memoize-test-lines t)))
            (goto-char (point-max))
            (insert "gamma\n")
            (let ((edited (neomacs-memoize-test-lines nil)))
              (list :values (list compact compact-hit with-empty edited)
                    :identity (eq compact compact-hit)
                    :calls calls
                    :documentation
                    (documentation 'neomacs-memoize-test-lines t))))))
    (fmakunbound 'neomacs-memoize-test-lines)
    (setplist 'neomacs-memoize-test-lines nil)))
"###;
    let expected = expect![[
        r#"OK (:values (#1=(:call 1 :lines ("alpha" "beta")) #1# (:call 2 :lines ("alpha" "" "beta" "")) (:call 3 :lines ("alpha" "beta" "gamma"))) :identity t :calls 3 :documentation "Summarize the current document revision. (memoized by buffer contents)")"#
    ]];
    ParityBatchCase::value(
        "buffer_content_macro_keeps_argument_variants_separate_within_each_revision",
        elisp_form,
        expected,
    )
}

#[test]
fn memoize_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(MEMOIZE_MELPA_PIN, "memoize.el")
            .expect("prepare revision-pinned Memoize below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "memoize-package-batch",
        "Memoize",
        &[
            package_contract_exposes_cache_policies_and_definition_forms(),
            structured_equal_requests_share_results_while_distinct_options_get_new_entries(),
            nil_results_and_signaled_failures_are_recomputed_instead_of_being_cached(),
            timeout_access_renews_each_key_and_expiry_recomputes_only_that_entry(),
            named_function_memoization_updates_documentation_rejects_stacking_and_restores(),
            recursive_defmemoize_reuses_subproblems_across_successive_business_calculations(),
            buffer_content_cache_shares_equal_documents_and_invalidates_semantic_edits(),
            buffer_content_macro_keeps_argument_variants_separate_within_each_revision(),
        ],
    );
}
