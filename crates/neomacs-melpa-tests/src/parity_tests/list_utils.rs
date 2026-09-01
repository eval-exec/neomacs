use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LIST_UTILS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'list-utils)

(defun neomacs-list-utils-test-cycle (values cycle-start)
  "Copy VALUES and point its last cdr at CYCLE-START."
  (let ((result (copy-sequence values)))
    (setcdr (last result) (nthcdr cycle-start result))
    result))

(defun neomacs-list-utils-test-error (thunk)
  "Call THUNK and return a stable description of any error."
  (condition-case error-data
      (progn (funcall thunk) 'no-error)
    (error (list (car error-data) (error-message-string error-data)))))

(defun neomacs-list-utils-test-case-fold-equal (left right)
  "Compare LEFT and RIGHT as case-insensitive strings."
  (string-equal (downcase left) (downcase right)))
"####;

fn append_only_pipeline_builder_preserves_order_tail_and_shared_head() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((builder (make-tconc))
       (front (list 'checkout 'compile))
       (back (list 'publish 'notify)))
  (tconc-list builder front)
  (tconc builder 'unit-test 'integration-test)
  (let ((head-before-final-append (tconc-head builder)))
    (tconc-list builder back)
    (list :pipeline (tconc builder)
          :front-now front
          :head-shared (eq head-before-final-append (tconc-head builder))
          :tail-value (car (tconc-tail builder))
          :tail-is-final-cell (eq (tconc-tail builder)
                                  (last (tconc-head builder)))
          :shape (list (tconc-p builder)
                       (length (tconc-head builder))))))
"####;
    let expected = expect![
        "OK (:pipeline #1=(checkout compile unit-test integration-test publish notify) :front-now #1# :head-shared t :tail-value notify :tail-is-final-cell t :shape (t 6))"
    ];
    ParityBatchCase::value(
        "append_only_pipeline_builder_preserves_order_tail_and_shared_head",
        elisp_form,
        expected,
    )
}

fn proper_and_improper_configuration_conversions_preserve_copy_and_inplace_contracts()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((improper (cons 'root (cons (cons 'child 'leaf) 'tail)))
       (improper-backup (copy-tree improper))
       (proper-copy (list-utils-make-proper-copy improper 'tree))
       (proper-inplace-input (copy-tree improper))
       (proper-inplace
        (list-utils-make-proper-inplace proper-inplace-input 'tree))
       (proper (copy-tree '(root (child leaf) tail)))
       (proper-backup (copy-tree proper))
       (improper-copy (list-utils-make-improper-copy proper 'tree))
       (improper-inplace-input (copy-tree proper))
       (improper-inplace
        (list-utils-make-improper-inplace improper-inplace-input 'tree)))
  (list :improper-tail (list-utils-cons-cell-p improper)
        :proper-copy proper-copy
        :copy-left-source-unchanged (equal improper improper-backup)
        :proper-inplace proper-inplace
        :proper-inplace-same-root (eq proper-inplace proper-inplace-input)
        :improper-copy improper-copy
        :improper-copy-left-source-unchanged (equal proper proper-backup)
        :improper-inplace improper-inplace
        :improper-inplace-same-root
        (eq improper-inplace improper-inplace-input)
        :invalid-inputs
        (list
         (neomacs-list-utils-test-error
          (lambda () (list-utils-make-proper-copy 42)))
         (neomacs-list-utils-test-error
          (lambda () (list-utils-make-improper-copy '(only)))))))
"####;
    let expected = expect![[
        r#"OK (:improper-tail tail :proper-copy (root (child leaf) tail) :copy-left-source-unchanged t :proper-inplace (root (child leaf) tail) :proper-inplace-same-root t :improper-copy (root (child . leaf) . tail) :improper-copy-left-source-unchanged t :improper-inplace (root (child . leaf) . tail) :improper-inplace-same-root t :invalid-inputs ((error "LIST is not a list") (error "LIST has only one element")))"#
    ]];
    ParityBatchCase::value(
        "proper_and_improper_configuration_conversions_preserve_copy_and_inplace_contracts",
        elisp_form,
        expected,
    )
}

fn cyclic_dependency_graphs_are_measured_compared_and_linearized_without_looping() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((partial (neomacs-list-utils-test-cycle
                 '(bootstrap parse compile package) 1))
       (partial-peer (neomacs-list-utils-test-cycle
                      '(bootstrap parse compile package) 1))
       (partial-different (neomacs-list-utils-test-cycle
                           '(bootstrap parse lint package) 1))
       (perfect (neomacs-list-utils-test-cycle '(parse compile package) 0))
       (inplace (neomacs-list-utils-test-cycle
                 '(bootstrap parse compile package) 1))
       (inplace-root inplace)
       (cycle-from-start (list-utils-cyclic-subseq partial 'from-start)))
  (let ((result
         (list :cycle-length (list-utils-cyclic-length partial)
               :safe-length (list-utils-safe-length partial)
               :linear-prefix (list-utils-linear-subseq partial)
               :cycle-elements (list-utils-make-linear-copy cycle-from-start)
               :partial-cyclic (not (null (list-utils-cyclic-p partial)))
               :partial-perfect (not (null (list-utils-cyclic-p partial 'perfect)))
               :perfect-cycle (not (null (list-utils-cyclic-p perfect 'perfect)))
               :linear-detected (list-utils-linear-p '(a b c))
               :same-graph (list-utils-safe-equal partial partial-peer)
               :different-graph
               (list-utils-safe-equal partial partial-different)
               :linear-copy (list-utils-make-linear-copy partial))))
    (list-utils-make-linear-inplace inplace)
    (append result
            (list :inplace-result inplace
                  :inplace-root-preserved (eq inplace inplace-root)
                  :inplace-now-linear (list-utils-linear-p inplace)))))
"####;
    let expected = expect![
        "OK (:cycle-length 3 :safe-length 4 :linear-prefix (bootstrap) :cycle-elements (parse compile package) :partial-cyclic t :partial-perfect nil :perfect-cycle t :linear-detected t :same-graph t :different-graph nil :linear-copy (bootstrap parse compile package) :inplace-result (bootstrap parse compile package) :inplace-root-preserved t :inplace-now-linear t)"
    ];
    ParityBatchCase::value(
        "cyclic_dependency_graphs_are_measured_compared_and_linearized_without_looping",
        elisp_form,
        expected,
    )
}

fn nested_release_data_flattens_differently_for_values_and_alist_entries() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((release-plan
        '(deploy (api nil (worker cache)) ((verify . smoke) notify)))
       (metadata
        '((service . api)
          (((region . us-east) (replicas . 3)))
          ((labels (tier . backend) (owner . platform)))))
       (cyclic-plan
        (neomacs-list-utils-test-cycle
         '(prepare (compile package) nil) 1)))
  (list :flat-plan (list-utils-flatten release-plan)
        :depth (list-utils-depth release-plan)
        :flat-prefix-length
        (list-utils-flat-length '(prepare nil verify (deploy) notify))
        :alist-prefix-length
        (list-utils-alist-or-flat-length
         '((service . api) (region . us-east) ((owner . platform))))
        :alist-flat (list-utils-alist-flatten metadata)
        :cyclic-flat (list-utils-flatten cyclic-plan)
        :cyclic-depth (list-utils-depth cyclic-plan)))
"####;
    let expected = expect![
        "OK (:flat-plan (deploy api nil worker cache verify smoke notify) :depth 3 :flat-prefix-length 3 :alist-prefix-length 2 :alist-flat ((service . api) (region . us-east) (replicas . 3) labels (tier . backend) (owner . platform)) :cyclic-flat (prepare compile package nil) :cyclic-depth 2)"
    ];
    ParityBatchCase::value(
        "nested_release_data_flattens_differently_for_values_and_alist_entries",
        elisp_form,
        expected,
    )
}

fn ordered_insertions_edit_proper_and_improper_workflows_and_report_invalid_targets()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((schedule (list 'checkout 'compile 'deploy))
       (improper (cons 'parse (cons 'compile 'deploy)))
       (case-folded (list "Build" "Deploy"))
       (prepend (list-utils-insert-before-pos
                 (list 'compile 'deploy) 0 'checkout)))
  (list-utils-insert-before schedule 'deploy 'test)
  (list-utils-insert-after schedule 'deploy 'notify)
  (list-utils-insert-after-pos schedule 1 'lint)
  (setq improper
        (list-utils-insert-before improper 'deploy 'validate))
  (setq case-folded
        (list-utils-insert-before
         case-folded "deploy" "Test"
         #'neomacs-list-utils-test-case-fold-equal))
  (list :schedule schedule
        :prepend prepend
        :improper improper
        :improper-tail (list-utils-improper-p improper)
        :case-folded case-folded
        :missing-element
        (neomacs-list-utils-test-error
         (lambda ()
           (list-utils-insert-after (list 'a 'b) 'missing 'value)))
        :invalid-position
        (neomacs-list-utils-test-error
         (lambda ()
           (list-utils-insert-before-pos (list 'a 'b) 8 'value)))))
"####;
    let expected = expect![[
        r#"OK (:schedule (checkout compile lint test deploy notify) :prepend (checkout compile deploy) :improper (parse compile validate . deploy) :improper-tail deploy :case-folded ("Build" "Test" "Deploy") :missing-element (error "Element not found: missing") :invalid-position (error "No such position 8"))"#
    ]];
    ParityBatchCase::value(
        "ordered_insertions_edit_proper_and_improper_workflows_and_report_invalid_targets",
        elisp_form,
        expected,
    )
}

fn deployment_set_analytics_preserve_order_duplicates_and_custom_equivalence() -> ParityBatchCase {
    let elisp_form = r####"
(let ((requested '(api worker api cache metrics worker))
      (available '(worker cache scheduler api)))
  (list
   :intersection (list-utils-and requested available)
   :missing (list-utils-not requested available)
   :exclusive (list-utils-xor requested available)
   :unique (list-utils-uniq requested)
   :duplicates (list-utils-dupes requested)
   :singlets (list-utils-singlets requested)
   :partition (list-utils-partition-dupes requested)
   :case-fold-intersection
   (list-utils-and '("API" "Worker" "CACHE" "worker")
                   '("api" "cache")
                   'list-utils-htt-case-fold-equal)
   :numeric-unique
   (list-utils-uniq '(1 1.0 2 2.0 3) 'list-utils-htt-=)
   :whitespace-unique
   (list-utils-uniq '("release candidate"
                      "release\tcandidate"
                      "production")
                    'list-utils-htt-ignore-whitespace-equal)
   :flipped-intersection
   (list-utils-and '(api worker api) '(worker api worker)
                   nil nil 'flip)))
"####;
    let expected = expect![[
        r#"OK (:intersection (api worker api cache worker) :missing (metrics) :exclusive (metrics scheduler) :unique (api worker cache metrics) :duplicates (api worker api worker) :singlets (cache metrics) :partition ((dupes api worker api worker) (singlets cache metrics)) :case-fold-intersection ("API" "CACHE") :numeric-unique (1 2 3) :whitespace-unique ("release candidate" "production") :flipped-intersection (worker api worker))"#
    ]];
    ParityBatchCase::value(
        "deployment_set_analytics_preserve_order_duplicates_and_custom_equivalence",
        elisp_form,
        expected,
    )
}

fn plist_maintenance_reverses_pairs_deletes_properties_and_rejects_malformed_input()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((settings '(:environment production
                    :retries 3
                    :notify t
                    :owner platform))
       (reversed (list-utils-plist-reverse settings))
       (without-retries (list-utils-plist-del
                         (copy-sequence settings) :retries))
       (missing-source (copy-sequence settings))
       (missing-result (list-utils-plist-del missing-source :timeout)))
  (list :original settings
        :reversed reversed
        :without-retries without-retries
        :missing-result missing-result
        :missing-preserves-root (eq missing-source missing-result)
        :odd-plist
        (neomacs-list-utils-test-error
         (lambda () (list-utils-plist-reverse '(:a 1 :b))))))
"####;
    let expected = expect![[
        r#"OK (:original (:environment production :retries 3 :notify t :owner platform) :reversed (:owner platform :notify t :retries 3 :environment production) :without-retries (:environment production :notify t :owner platform) :missing-result (:environment production :retries 3 :notify t :owner platform) :missing-preserves-root t :odd-plist (error "Not a PLIST"))"#
    ]];
    ParityBatchCase::value(
        "plist_maintenance_reverses_pairs_deletes_properties_and_rejects_malformed_input",
        elisp_form,
        expected,
    )
}

#[test]
fn list_utils_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(LIST_UTILS_MELPA_PIN, "list-utils.el")
            .expect("prepare revision-pinned List Utils source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "list-utils-package-batch",
        "List Utils",
        &[
            append_only_pipeline_builder_preserves_order_tail_and_shared_head(),
            proper_and_improper_configuration_conversions_preserve_copy_and_inplace_contracts(),
            cyclic_dependency_graphs_are_measured_compared_and_linearized_without_looping(),
            nested_release_data_flattens_differently_for_values_and_alist_entries(),
            ordered_insertions_edit_proper_and_improper_workflows_and_report_invalid_targets(),
            deployment_set_analytics_preserve_order_duplicates_and_custom_equivalence(),
            plist_maintenance_reverses_pairs_deletes_properties_and_rejects_malformed_input(),
        ],
    );
}
