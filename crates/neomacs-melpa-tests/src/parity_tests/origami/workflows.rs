use expect_test::expect;

use super::ParityBatchCase;

fn fold_tree_construction_and_open_set_are_deterministic() -> ParityBatchCase {
    ParityBatchCase::value(
        "fold_tree_construction_and_open_set_are_deterministic",
        r####"
;; Non-root fold nodes store an overlay in the data slot; root uses the 'root symbol.
(with-temp-buffer
  (insert (make-string 50 ?x))
  (let* ((child-ov (make-overlay 10 20))
         (parent-ov (make-overlay 5 30))
         (child (origami-fold-node 10 20 1 nil nil child-ov))
         (parent (origami-fold-node 5 30 1 t (list child) parent-ov))
         (root (origami-fold-root-node (list parent)))
         (closed (origami-fold-open-set parent nil)))
    (list :rootp (and (origami-fold-is-root-node? root) t)
          :parent-open (and (origami-fold-open? parent) t)
          :child-open (and (origami-fold-open? child) t)
          :closed-open (and (origami-fold-open? closed) t)
          :beg (origami-fold-beg parent)
          :end (origami-fold-end parent)
          :offset (origami-fold-offset parent)
          :child-count (length (origami-fold-children parent))
          :data-overlayp (overlayp (origami-fold-data parent)))))
"####,
        expect![
            "OK (:rootp t :parent-open t :child-open nil :closed-open nil :beg 4 :end 30 :offset 1 :child-count 1 :data-overlayp t)"
        ],
    )
}

fn history_push_undo_and_redo_preserve_present_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "history_push_undo_and_redo_preserve_present_values",
        r####"
(let* ((h0 (origami-h-new 'a))
       (h1 (origami-h-push h0 'b))
       (h2 (origami-h-push h1 'c))
       (undone (origami-h-undo h2))
       (redone (origami-h-redo undone)))
  (list :present0 (origami-h-present h0)
        :present1 (origami-h-present h1)
        :present2 (origami-h-present h2)
        :after-undo (origami-h-present undone)
        :after-redo (origami-h-present redone)))
"####,
        expect!["OK (:present0 a :present1 b :present2 c :after-undo b :after-redo c)"],
    )
}

fn create_hide_and_show_overlay_control_invisibility() -> ParityBatchCase {
    ParityBatchCase::value(
        "create_hide_and_show_overlay_control_invisibility",
        r####"
(with-temp-buffer
  (insert "abcdefghij")
  (let ((ov (origami-create-overlay 2 8 1 (current-buffer))))
    (list :before-inv (overlay-get ov 'invisible)
          :hidden
          (progn
            (origami-hide-overlay ov)
            (overlay-get ov 'invisible))
          :shown
          (progn
            (origami-show-overlay ov)
            (overlay-get ov 'invisible))
          :start (overlay-start ov)
          :end (overlay-end ov)
          :isearch (overlay-get ov 'isearch-open-invisible))))
"####,
        expect![
            "OK (:before-inv nil :hidden origami :shown nil :start 3 :end 8 :isearch origami-isearch-show)"
        ],
    )
}

fn mode_builds_fold_tree_and_close_open_node_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_builds_fold_tree_and_close_open_node_at_point",
        r####"
(neomacs-origami-test-with-buffer
 (lambda (buffer)
   (goto-char (point-min))
   (search-forward "return 1")
   (let* ((tree-before (origami-get-fold-tree buffer))
          (path (and tree-before
                     (origami-fold-find-path-containing tree-before (point))))
          (node (and path (car (last path)))))
     (list :mode (and origami-mode t)
           :has-tree (and tree-before t)
           :path-depth (and path (length path))
           :node-open-before (and node (origami-fold-open? node))
           :closed
           (progn
             (when node
               (origami-close-node buffer (point)))
             (let* ((tree (origami-get-fold-tree buffer))
                    (p (origami-fold-find-path-containing tree (point)))
                    (n (and p (car (last p)))))
               (and n (origami-fold-open? n))))
           :reopened
           (progn
             (origami-open-node buffer (point))
             (let* ((tree (origami-get-fold-tree buffer))
                    (p (origami-fold-find-path-containing tree (point)))
                    (n (and p (car (last p)))))
               (and n (origami-fold-open? n))))))))
"####,
        expect![
            "OK (:mode t :has-tree t :path-depth 3 :node-open-before t :closed nil :reopened t)"
        ],
    )
}

fn find_path_containing_range_locates_nested_fold() -> ParityBatchCase {
    ParityBatchCase::value(
        "find_path_containing_range_locates_nested_fold",
        r####"
(with-temp-buffer
  (insert (make-string 50 ?x))
  (let* ((inner-ov (make-overlay 15 25))
         (outer-ov (make-overlay 10 40))
         (inner (origami-fold-node 15 25 1 t nil inner-ov))
         (outer (origami-fold-node 10 40 1 t (list inner) outer-ov))
         (root (origami-fold-root-node (list outer)))
         (path (origami-fold-find-path-containing-range root 16 20)))
    (list :depth (length path)
          :leaf-is-inner
          (eq (origami-fold-data (car (last path))) inner-ov)
          :missing (origami-fold-find-path-containing-range root 1 2))))
"####,
        expect![
            "OK (:depth 3 :leaf-is-inner t :missing ([1 2305843009213693951 0 t ([10 40 1 t ([15 25 1 t nil #<overlay in no buffer>]) #<overlay in no buffer>]) root]))"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        fold_tree_construction_and_open_set_are_deterministic(),
        history_push_undo_and_redo_preserve_present_values(),
        create_hide_and_show_overlay_control_invisibility(),
        mode_builds_fold_tree_and_close_open_node_at_point(),
        find_path_containing_range_locates_nested_fold(),
    ]
}
