use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TREEPY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'map)
(require 'treepy)

(defun neomacs-treepy-test-node (name &rest children)
  "Create a named workflow node with CHILDREN."
  (list (cons :name name) (cons :children children)))

(defun neomacs-treepy-test-branch-p (node)
  "Return non-nil when NODE is a workflow branch."
  (and (listp node) (assq :children node)))

(defun neomacs-treepy-test-children (node)
  "Return NODE's workflow children."
  (map-elt node :children))

(defun neomacs-treepy-test-make-node (node children)
  "Return NODE with a persistent replacement for CHILDREN."
  (mapcar (lambda (entry)
            (if (eq (car entry) :children)
                (cons :children children)
              entry))
          node))

(defun neomacs-treepy-test-zip (root)
  "Create a zipper for a named workflow ROOT."
  (treepy-zipper #'neomacs-treepy-test-branch-p
                 #'neomacs-treepy-test-children
                 #'neomacs-treepy-test-make-node
                 root))

(defun neomacs-treepy-test-label (node)
  "Return the user-facing label for NODE."
  (if (neomacs-treepy-test-branch-p node)
      (map-elt node :name)
    node))

(defun neomacs-treepy-test-enumerate (root order)
  "Return ROOT's labels in depth-first ORDER."
  (let ((loc (neomacs-treepy-test-zip root))
        labels)
    (when (eq order :postorder)
      (setq loc (treepy-leftmost-descendant loc)))
    (while (not (treepy-end-p loc))
      (push (neomacs-treepy-test-label (treepy-node loc)) labels)
      (setq loc (treepy-next loc order)))
    (nreverse labels)))

(defun neomacs-treepy-test-find (loc label)
  "Return the first preorder LOC whose node has LABEL."
  (while (and (not (treepy-end-p loc))
              (not (equal label
                          (neomacs-treepy-test-label (treepy-node loc)))))
    (setq loc (treepy-next loc :preorder)))
  loc)
"####;

fn mixed_release_configuration_walks_in_order_and_preserves_inputs() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((metadata (make-hash-table :test #'equal))
       (_ (puthash :region "us-east" metadata))
       (config `((:service . "api")
                 (:limits . [2 4])
                 (:metadata . ,metadata)))
       (walk-tree '(release [build test] (deploy canary)))
       (result
        (treepy-postwalk
         (lambda (value)
           (cond ((stringp value) (upcase value))
                 ((integerp value) (1+ value))
                 (t value)))
         config)))
  (list
   :transformed result
   :types
   (list :root (type-of result)
         :limits (type-of (map-elt result :limits))
         :metadata (type-of (map-elt result :metadata)))
   :original
   (list :service (map-elt config :service)
         :limits (map-elt config :limits)
         :region (gethash :region metadata))
   :preorder (treepy-prewalk-demo walk-tree)
   :postorder (treepy-postwalk-demo walk-tree)))
"####;
    let expected = expect![[
        r#"OK (:transformed ((:service . "API") (:limits . [3 5]) (:metadata (:region . "US-EAST"))) :types (:root cons :limits vector :metadata cons) :original (:service "api" :limits [2 4] :region "us-east") :preorder ((release #1=[build test] #2=(deploy canary)) release #1# build test #2# deploy canary) :postorder (release build test #3=[build test] deploy canary #4=(deploy canary) (release #3# #4#)))"#
    ]];
    ParityBatchCase::value(
        "mixed_release_configuration_walks_in_order_and_preserves_inputs",
        elisp_form,
        expected,
    )
}

fn prewalk_and_postwalk_replacement_model_recursive_and_single_pass_expansion() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((expansions '((:release . (:deploy)) (:deploy . :production)))
       (case-fold
        (lambda (left right)
          (and (stringp left) (stringp right)
               (string-equal (downcase left) (downcase right))))))
  (list
   :root-expansion
   (list :pre (treepy-prewalk-replace expansions :release)
         :post (treepy-postwalk-replace expansions :release))
   :nested
   (list
    :pre
    (treepy-prewalk-replace
     expansions '(:release (:deploy :queued)))
    :post
    (treepy-postwalk-replace
     expansions '(:release (:deploy :queued))))
   :custom-key-test
   (treepy-postwalk-replace
    '(("production" . "live") ("staging" . "preview"))
    '("PRODUCTION" ("Staging" "other"))
    case-fold)))
"####;
    let expected = expect![[
        r#"OK (:root-expansion (:pre (:production) :post #1=(:deploy)) :nested (:pre ((:production) (:production :queued)) :post (#1# (:production :queued))) :custom-key-test (nil (nil "other")))"#
    ]];
    ParityBatchCase::value(
        "prewalk_and_postwalk_replacement_model_recursive_and_single_pass_expansion",
        elisp_form,
        expected,
    )
}

fn vector_expression_edits_rebuild_the_root_without_mutating_the_original() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((expression '[[price * quantity] + [tax * rate]])
       (zip (treepy-vector-zip expression))
       (operator (treepy-right (treepy-down (treepy-down zip))))
       (second-term
        (treepy-right (treepy-right (treepy-down zip))))
       (tax (treepy-down second-term)))
  (list
   :context
   (list :node (treepy-node operator)
         :path (treepy-path operator)
         :lefts (treepy-lefts operator)
         :rights (treepy-rights operator))
   :replace (treepy-root (treepy-replace operator '/))
   :insert-left (treepy-root (treepy-insert-left operator 'gross))
   :insert-right (treepy-root (treepy-insert-right operator 'discount))
   :remove-tax (treepy-root (treepy-remove tax))
   :append-audit (treepy-root (treepy-append-child zip '[audit]))
   :original expression))
"####;
    let expected = expect![
        "OK (:context (:node * :path (#3=[#1=[price * quantity] + #2=[tax * rate]] #1#) :lefts (price) :rights (quantity)) :replace [[price / quantity] + #2#] :insert-left [[price gross * quantity] + #2#] :insert-right [[price * discount quantity] + #2#] :remove-tax [#1# + [[* rate] * rate]] :append-audit [#1# + #2# [audit]] :original #3#)"
    ];
    ParityBatchCase::value(
        "vector_expression_edits_rebuild_the_root_without_mutating_the_original",
        elisp_form,
        expected,
    )
}

fn custom_pipeline_zipper_enumerates_and_edits_real_workflow_nodes() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((pipeline
        (neomacs-treepy-test-node
         'pipeline
         (neomacs-treepy-test-node 'build 'compile 'lint)
         (neomacs-treepy-test-node 'test 'unit 'integration)
         'ship))
       (zip (neomacs-treepy-test-zip pipeline))
       (test-loc (neomacs-treepy-test-find zip 'test))
       (with-system
        (treepy-root (treepy-append-child test-loc 'system)))
       (integration-loc
        (neomacs-treepy-test-find
         (neomacs-treepy-test-zip with-system) 'integration))
       (renamed
        (treepy-root (treepy-replace integration-loc 'contract))))
  (list
   :preorder (neomacs-treepy-test-enumerate pipeline :preorder)
   :postorder (neomacs-treepy-test-enumerate pipeline :postorder)
   :test-context
   (list :path (mapcar #'neomacs-treepy-test-label
                       (treepy-path test-loc))
         :lefts (mapcar #'neomacs-treepy-test-label
                        (treepy-lefts test-loc))
         :rights (mapcar #'neomacs-treepy-test-label
                         (treepy-rights test-loc)))
   :with-system (neomacs-treepy-test-enumerate with-system :preorder)
   :renamed (neomacs-treepy-test-enumerate renamed :preorder)
   :original (neomacs-treepy-test-enumerate pipeline :preorder)))
"####;
    let expected = expect![
        "OK (:preorder (pipeline build compile lint test unit integration ship) :postorder (compile lint build unit integration test ship pipeline) :test-context (:path (pipeline) :lefts (build) :rights (ship)) :with-system (pipeline build compile lint test unit integration system ship) :renamed (pipeline build compile lint test unit contract system ship) :original (pipeline build compile lint test unit integration ship))"
    ];
    ParityBatchCase::value(
        "custom_pipeline_zipper_enumerates_and_edits_real_workflow_nodes",
        elisp_form,
        expected,
    )
}

fn dotted_lists_round_trip_through_preorder_navigation_and_backtracking() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((zip (treepy-list-zip '(alpha beta . release)))
       (loc zip)
       visited locations)
  (while (not (treepy-end-p loc))
    (push (treepy-node loc) visited)
    (push loc locations)
    (setq loc (treepy-next loc)))
  (let* ((end loc)
         (release-loc (car locations))
         (beta-loc (treepy-prev release-loc)))
    (list :visited (nreverse visited)
          :end (treepy-end-p end)
          :backtracked (list (treepy-node release-loc)
                             (treepy-node beta-loc))
          :round-trip (treepy-root beta-loc))))
"####;
    let expected = expect![
        "OK (:visited (#1=(alpha beta . release) alpha beta release) :end t :backtracked (release beta) :round-trip #1#)"
    ];
    ParityBatchCase::value(
        "dotted_lists_round_trip_through_preorder_navigation_and_backtracking",
        elisp_form,
        expected,
    )
}

fn children_rejects_leaf_locations() -> ParityBatchCase {
    let elisp_form = r####"
(treepy-children (treepy-down (treepy-list-zip '(leaf))))
"####;
    let expected = expect![[r#"ERR (error "Called children on a leaf node")"#]];
    ParityBatchCase::signal("children_rejects_leaf_locations", elisp_form, expected)
}

fn inserting_a_left_sibling_at_the_root_is_rejected() -> ParityBatchCase {
    let elisp_form = r####"
(treepy-insert-left (treepy-list-zip '(root child)) 'before)
"####;
    let expected = expect![[r#"ERR (error "Insert at top")"#]];
    ParityBatchCase::signal(
        "inserting_a_left_sibling_at_the_root_is_rejected",
        elisp_form,
        expected,
    )
}

fn removing_the_root_is_rejected() -> ParityBatchCase {
    let elisp_form = r####"
(treepy-remove (treepy-vector-zip '[root child]))
"####;
    let expected = expect![[r#"ERR (error "Remove at top")"#]];
    ParityBatchCase::signal("removing_the_root_is_rejected", elisp_form, expected)
}

fn traversal_rejects_an_unknown_order() -> ParityBatchCase {
    let elisp_form = r####"
(treepy-next (treepy-list-zip '(root child)) :breadth-first)
"####;
    let expected = expect![[r#"ERR (error "Unrecognized order")"#]];
    ParityBatchCase::signal("traversal_rejects_an_unknown_order", elisp_form, expected)
}

#[test]
fn treepy_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(TREEPY_MELPA_PIN, "treepy.el")
            .expect("prepare revision-pinned Treepy source below ./tmp")
            .with_timeout(Duration::from_secs(300))
            .with_prelude(PRELUDE),
        "treepy-package-batch",
        "Treepy",
        &[
            mixed_release_configuration_walks_in_order_and_preserves_inputs(),
            prewalk_and_postwalk_replacement_model_recursive_and_single_pass_expansion(),
            vector_expression_edits_rebuild_the_root_without_mutating_the_original(),
            custom_pipeline_zipper_enumerates_and_edits_real_workflow_nodes(),
            dotted_lists_round_trip_through_preorder_navigation_and_backtracking(),
            children_rejects_leaf_locations(),
            inserting_a_left_sibling_at_the_root_is_rejected(),
            removing_the_root_is_rejected(),
            traversal_rejects_an_unknown_order(),
        ],
    );
}
