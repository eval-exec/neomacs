use expect_test::expect;

use super::ParityBatchCase;

fn insert_gt_closes_a_started_tag_and_places_point_inside() -> ParityBatchCase {
    ParityBatchCase::value(
        "insert_gt_closes_a_started_tag_and_places_point_inside",
        r####"
(neomacs-tagedit-test-with-html
 "<div"
 nil
 (lambda ()
   (goto-char (point-max))
   (tagedit-insert-gt)
   (neomacs-tagedit-test-state)))
"####,
        expect![[r#"OK (:text "<div" :point 5 :line 1 :column 4 :mode t)"#]],
    )
}

fn kill_attribute_removes_only_the_current_attribute() -> ParityBatchCase {
    ParityBatchCase::value(
        "kill_attribute_removes_only_the_current_attribute",
        r####"
(neomacs-tagedit-test-with-html
 "<div class=\"widget\" id=\"main\">body</div>"
 "class"
 (lambda ()
   (tagedit-kill-attribute)
   (neomacs-tagedit-test-state)))
"####,
        expect![[r#"OK (:text "<div id=\"main\">body</div>" :point 6 :line 1 :column 5 :mode t)"#]],
    )
}

fn forward_slurp_and_barf_rebalance_sibling_tags() -> ParityBatchCase {
    ParityBatchCase::value(
        "forward_slurp_and_barf_rebalance_sibling_tags",
        r####"
(neomacs-tagedit-test-with-html
 "<div><span>a</span></div><p>b</p>"
 "span"
 (lambda ()
   (tagedit-forward-slurp-tag)
   (let ((after-slurp (neomacs-tagedit-test-state)))
     (tagedit-forward-barf-tag)
     (list :slurp after-slurp
           :barf (neomacs-tagedit-test-state)))))
"####,
        expect![[
            r#"OK (:slurp (:text "<div><span>a</span><p>b</p></div>" :point 7 :line 1 :column 6 :mode t) :barf (:text "<div><span></span>a<p>b</p></div>" :point 7 :line 1 :column 6 :mode t))"#
        ]],
    )
}

fn raise_and_splice_reshape_nested_markup() -> ParityBatchCase {
    ParityBatchCase::value(
        "raise_and_splice_reshape_nested_markup",
        r####"
(list
 :raise
 (neomacs-tagedit-test-with-html
  "<div><span>inner</span></div>"
  "span"
  (lambda ()
    (tagedit-raise-tag)
    (neomacs-tagedit-test-state)))
 :splice
 (neomacs-tagedit-test-with-html
  "<div><span>inner</span></div>"
  "span"
  (lambda ()
    (tagedit-splice-tag)
    (neomacs-tagedit-test-state))))
"####,
        expect![[
            r#"OK (:raise (:text "<span>inner</span>" :point 1 :line 1 :column 0 :mode t) :splice (:text "<span>inner</span>" :point 2 :line 1 :column 1 :mode t))"#
        ]],
    )
}

fn split_and_join_tags_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "split_and_join_tags_at_point",
        r####"
(list
 :split
 (neomacs-tagedit-test-with-html
  "<div>hello world</div>"
  " "
  (lambda ()
    (tagedit-split-tag)
    (neomacs-tagedit-test-state)))
 :join
 (neomacs-tagedit-test-with-html
  "<div>hello</div><div>world</div>"
  "</div><div>"
  (lambda ()
    (search-forward "</div>")
    (tagedit-join-tags)
    (neomacs-tagedit-test-state))))
"####,
        expect![[
            r#"OK (:split (:text "<div>hello</div><div> world</div>" :point 22 :line 1 :column 21 :mode t) :join (:text "<div>helloworld</div>" :point 11 :line 1 :column 10 :mode t))"#
        ]],
    )
}

fn toggle_multiline_and_mode_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "toggle_multiline_and_mode_lifecycle",
        r####"
(neomacs-tagedit-test-with-html
 "<div class=\"x\"><span>y</span></div>"
 "div"
 (lambda ()
   (tagedit-toggle-multiline-tag)
   (let ((multi (neomacs-tagedit-test-state)))
     (tagedit-mode -1)
     (list :multi multi
           :disabled (and tagedit-mode t)
           :bindings
           (list (lookup-key tagedit-mode-map (kbd ">"))
                 (lookup-key tagedit-mode-map (kbd "C-c C-f"))
                 (lookup-key tagedit-mode-map (kbd "C-c C-b")))))))
"####,
        expect![[
            r#"OK (:multi (:text "<div class=\"x\">\n  <span>y</span>\n</div>" :point 2 :line 1 :column 1 :mode t) :disabled nil :bindings (nil 1 1))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        insert_gt_closes_a_started_tag_and_places_point_inside(),
        kill_attribute_removes_only_the_current_attribute(),
        forward_slurp_and_barf_rebalance_sibling_tags(),
        raise_and_splice_reshape_nested_markup(),
        split_and_join_tags_at_point(),
        toggle_multiline_and_mode_lifecycle(),
    ]
}
