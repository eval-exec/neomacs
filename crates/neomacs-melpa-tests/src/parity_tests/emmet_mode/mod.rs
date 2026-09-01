use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EMMET_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const EMMET_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn emmet_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EMMET_MODE_MELPA_PIN, "emmet-mode.el")
        .expect("prepare pinned Emmet Mode source below ./tmp")
        .with_timeout(EMMET_MODE_TEST_TIMEOUT)
}

fn checkout_component_abbreviation_expands_inside_an_existing_html_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "checkout_component_abbreviation_expands_inside_an_existing_html_document",
        r##"
(with-temp-buffer
  (html-mode)
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 2)
  (let ((emmet-preview-default nil)
        (emmet-indent-after-insert t)
        (emmet-insert-flash-time -1)
        (emmet-move-cursor-after-expanding t))
    (emmet-mode 1)
    (insert
     "<body>\n"
     "  main#checkout>section.summary>h2{Order Summary}+ul.items>li.item[data-sku=SKU-$]*3>{Item $}\n"
     "</body>\n")
    (goto-char (point-min))
    (forward-line 1)
    (end-of-line)
    (emmet-expand-line nil)
    (let ((flash
           (and (overlayp emmet-flash-ovl)
                (list (overlay-start emmet-flash-ovl)
                      (overlay-end emmet-flash-ovl)
                      (overlay-get emmet-flash-ovl 'face)))))
      (prog1
          (list :mode emmet-mode
                :source
                (buffer-substring-no-properties (point-min) (point-max))
                :point
                (list (line-number-at-pos)
                      (current-column)
                      (buffer-substring-no-properties
                       (line-beginning-position)
                       (line-end-position)))
                :flash flash)
        (emmet-remove-flash-ovl (current-buffer))))))
"##,
        expect![[
            r##"OK (:mode t :source "<body>\n  <main id=\"checkout\">\n    <section class=\"summary\">\n      <h2>Order Summary</h2>\n      <ul class=\"items\">\n        <li class=\"item\" data-sku=\"SKU-1\">Item 1</li>\n        <li class=\"item\" data-sku=\"SKU-2\">Item 2</li>\n        <li class=\"item\" data-sku=\"SKU-3\">Item 3</li>\n      </ul>\n    </section>\n  </main>\n</body>\n" :point (4 23 "      <h2>Order Summary</h2>") :flash (10 313 emmet-preview-output))"##
        ]],
    )
}

fn stylesheet_abbreviations_expand_through_the_css_editor_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "stylesheet_abbreviations_expand_through_the_css_editor_workflow",
        r##"
(with-temp-buffer
  (css-mode)
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 2)
  (let ((emmet-preview-default nil)
        (emmet-indent-after-insert t)
        (emmet-insert-flash-time -1)
        (emmet-move-cursor-after-expanding t)
        events)
    (emmet-mode 1)
    (insert
     ".checkout-card {\n"
     "  d:f\n"
     "  jc:sb\n"
     "  ai:c\n"
     "  p20-24\n"
     "  m10\n"
     "  bgc#fff\n"
     "  bdrs8\n"
     "}\n")
    (dolist (abbreviation '("d:f" "jc:sb" "ai:c" "p20-24" "m10" "bgc#fff" "bdrs8"))
      (goto-char (point-min))
      (search-forward abbreviation)
      (emmet-expand-line nil)
      (push
       (list abbreviation
             (line-number-at-pos)
             (current-column)
             (buffer-substring-no-properties
              (line-beginning-position)
              (line-end-position)))
       events))
    (let ((flash
           (and (overlayp emmet-flash-ovl)
                (list (overlay-start emmet-flash-ovl)
                      (overlay-end emmet-flash-ovl)
                      (overlay-get emmet-flash-ovl 'face)))))
      (prog1
          (list :mode emmet-mode
                :css-transform emmet-use-css-transform
                :source
                (buffer-substring-no-properties (point-min) (point-max))
                :events (nreverse events)
                :flash flash)
        (emmet-remove-flash-ovl (current-buffer))))))
"##,
        expect![[
            r##"OK (:mode t :css-transform t :source ".checkout-card {\n    display: flex;\n    justify-content: space-between;\n    align-items: center;\n    padding: 20px 24px;\n    margin: 10px;\n    background-color: #fff;\n    border-radius: 8px;\n}\n" :events (("d:f" 2 18 "    display: flex;") ("jc:sb" 3 35 "    justify-content: space-between;") ("ai:c" 4 24 "    align-items: center;") ("p20-24" 5 23 "    padding: 20px 24px;") ("m10" 6 17 "    margin: 10px;") ("bgc#fff" 7 27 "    background-color: #fff;") ("bdrs8" 8 23 "    border-radius: 8px;")) :flash (172 191 emmet-preview-output))"##
        ]],
    )
}

fn jsx_component_expansion_obeys_react_attribute_and_cursor_conventions() -> ParityBatchCase {
    ParityBatchCase::value(
        "jsx_component_expansion_obeys_react_attribute_and_cursor_conventions",
        r##"
(with-temp-buffer
  (js-jsx-mode)
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 2)
  (let ((emmet-preview-default nil)
        (emmet-indent-after-insert t)
        (emmet-insert-flash-time -1)
        (emmet-move-cursor-after-expanding t))
    (emmet-mode 1)
    (insert
     "function CheckoutPanel({items, total}) {\n"
     "  return (\n"
     "    section.checkout-panel>h2{Cart}+ul.items>li.item[data-id={item.id}]*2>{Row $}+label[for=coupon]{Coupon}+input#coupon[type=text]\n"
     "  );\n"
     "}\n")
    (goto-char (point-min))
    (forward-line 2)
    (end-of-line)
    (emmet-expand-line nil)
    (let ((flash
           (and (overlayp emmet-flash-ovl)
                (list (overlay-start emmet-flash-ovl)
                      (overlay-end emmet-flash-ovl)
                      (overlay-get emmet-flash-ovl 'face)))))
      (prog1
          (list :major-mode major-mode
                :mode emmet-mode
                :css-transform emmet-use-css-transform
                :source
                (buffer-substring-no-properties (point-min) (point-max))
                :point
                (list (line-number-at-pos)
                      (current-column)
                      (buffer-substring-no-properties
                       (line-beginning-position)
                       (line-end-position)))
                :flash flash)
        (emmet-remove-flash-ovl (current-buffer))))))
"##,
        expect![[
            r##"OK (:major-mode js-jsx-mode :mode t :css-transform nil :source "function CheckoutPanel({items, total}) {\n  return (\n      <section className=\"checkout-panel\">\n          <h2>Cart</h2>\n          <ul className=\"items\">\n              <li className=\"item\" data-id={item.id}>\n                  Row 1\n                  <label htmlFor=\"coupon\">Coupon</label>\n                  <input id=\"coupon\" name=\"\" type=\"text\" value=\"\"/>\n              </li>\n              <li className=\"item\" data-id={item.id}>\n                  Row 2\n                  <label htmlFor=\"coupon\">Coupon</label>\n                  <input id=\"coupon\" name=\"\" type=\"text\" value=\"\"/>\n              </li>\n          </ul>\n      </section>\n  );\n}\n" :point (4 18 "          <h2>Cart</h2>") :flash (59 631 emmet-preview-output))"##
        ]],
    )
}

fn active_multiline_selection_wraps_into_a_practical_shopping_list() -> ParityBatchCase {
    ParityBatchCase::value(
        "active_multiline_selection_wraps_into_a_practical_shopping_list",
        r##"
(with-temp-buffer
  (html-mode)
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 2)
  (let ((emmet-postwrap-goto-edit-point t))
    (emmet-mode 1)
    (insert "Whole bean coffee\nOat milk\nFresh berries")
    (goto-char (point-min))
    (push-mark (point-max) t t)
    (emmet-wrap-with-markup
     "section.shopping-list>h2{Groceries}+ul>li.item*")
    (list :mode emmet-mode
          :source
          (buffer-substring-no-properties (point-min) (point-max))
          :point
          (list (point)
                (line-number-at-pos)
                (current-column)
                (buffer-substring-no-properties
                 (line-beginning-position)
                 (line-end-position)))
          :mark (mark t)
          :region-active (use-region-p))))
"##,
        expect![[
            r##"OK (:mode t :source "<section class=\"shopping-list\">\n  <h2>Groceries</h2>\n  <ul>\n    <li class=\"item\">Whole bean coffee</li>\n    <li class=\"item\">Oat milk</li>\n    <li class=\"item\">Fresh berries</li>\n  </ul>\n</section>" :point (198 8 10 "</section>") :mark 1 :region-active nil)"##
        ]],
    )
}

fn interactive_preview_exposes_then_accepts_the_navigation_expansion() -> ParityBatchCase {
    ParityBatchCase::value(
        "interactive_preview_exposes_then_accepts_the_navigation_expansion",
        r##"
(with-temp-buffer
  (html-mode)
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 2)
  (let ((emmet-indent-after-insert t)
        (emmet-insert-flash-time -1)
        (emmet-move-cursor-after-expanding t))
    (emmet-mode 1)
    (insert
     "<main>\n"
     "  nav.primary>ul>li*3>a[href=/section-$]{Section $}\n"
     "</main>\n")
    (goto-char (point-min))
    (forward-line 1)
    (back-to-indentation)
    (let ((beg (point))
          (end (line-end-position)))
      (goto-char end)
      (emmet-preview beg end)
      (emmet-update-preview 2)
      (let* ((input emmet-preview-input)
             (output emmet-preview-output)
             (preview
              (list
               :source
               (buffer-substring-no-properties (point-min) (point-max))
               :input
               (list (overlay-start input)
                     (overlay-end input)
                     (overlay-get input 'face)
                     (lookup-key (overlay-get input 'keymap) (kbd "RET")))
               :output
               (list (overlay-start output)
                     (overlay-end output)
                     (substring-no-properties
                      (overlay-get output 'before-string))
                     (substring-no-properties
                      (overlay-get output 'after-string))
                     (overlay-get output 'face))
               :hooks
               (list (memq 'emmet-preview-before-change before-change-functions)
                     (memq 'emmet-preview-post-command post-command-hook)))))
        (emmet-preview-accept)
        (let ((flash
               (and (overlayp emmet-flash-ovl)
                    (list (overlay-start emmet-flash-ovl)
                          (overlay-end emmet-flash-ovl)
                          (overlay-get emmet-flash-ovl 'face)))))
          (prog1
              (list :preview preview
                    :accepted-source
                    (buffer-substring-no-properties (point-min) (point-max))
                    :point
                    (list (line-number-at-pos)
                          (current-column)
                          (buffer-substring-no-properties
                           (line-beginning-position)
                           (line-end-position)))
                    :preview-state
                    (list emmet-preview-input
                          emmet-preview-output
                          (memq 'emmet-preview-before-change
                                before-change-functions)
                          (memq 'emmet-preview-post-command post-command-hook))
                    :flash flash)
            (emmet-remove-flash-ovl (current-buffer))))))))
"##,
        expect![[
            r##"OK (:preview (:source "<main>\n  nav.primary>ul>li*3>a[href=/section-$]{Section $}\n</main>\n" :input (10 59 emmet-preview-input emmet-preview-accept) :output (60 60 " Emmet preview. Choose with RET. Cancel by stepping out. \n" "<nav class=\"primary\">\n    <ul>\n        <li><a href=\"/section-1\">Section 1</a></li>\n        <li><a href=\"/section-2\">Section 2</a></li>\n        <li><a href=\"/section-3\">Section 3</a></li>\n    </ul>\n</nav>\n" emmet-preview-output) :hooks ((emmet-preview-before-change syntax-ppss-flush-cache) (emmet-preview-post-command))) :accepted-source "<main>\n  <nav class=\"primary\">\n    <ul>\n      <li><a href=\"/section-1\">Section 1</a></li>\n      <li><a href=\"/section-2\">Section 2</a></li>\n      <li><a href=\"/section-3\">Section 3</a></li>\n    </ul>\n  </nav>\n</main>\n" :point (4 44 "      <li><a href=\"/section-1\">Section 1</a></li>") :preview-state (nil nil nil nil) :flash (10 209 emmet-preview-output))"##
        ]],
    )
}

#[test]
fn emmet_mode_package_batch() {
    let cases = vec![
        checkout_component_abbreviation_expands_inside_an_existing_html_document(),
        stylesheet_abbreviations_expand_through_the_css_editor_workflow(),
        jsx_component_expansion_obeys_react_attribute_and_cursor_conventions(),
        active_multiline_selection_wraps_into_a_practical_shopping_list(),
        interactive_preview_exposes_then_accepts_the_navigation_expansion(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Emmet Mode parity test");
    assert_oracle_batch_cases(emmet_mode_oracle(), test_name, "emmet_mode_parity", &cases);
}
