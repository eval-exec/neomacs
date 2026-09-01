use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, POPUP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const POPUP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const POPUP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'popup)

(defun popup-test-posn-col-row (_position)
  (cons (current-column)
        (1- (line-number-at-pos (point)))))

(defun popup-test-with-visible-buffer (thunk)
  (save-window-excursion
    (with-temp-buffer
      (switch-to-buffer (current-buffer))
      (delete-other-windows)
      (cl-letf (((symbol-function 'posn-col-row)
                 #'popup-test-posn-col-row))
        (funcall thunk)))))

(defun popup-test-property-runs (string property)
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((value (get-text-property position property string))
             (next
              (or (next-single-property-change
                   position property string)
                  (length string))))
        (when value
          (push (list position next value) runs))
        (setq position next)))
    (nreverse runs)))

(defun popup-test-normalize-item (item)
  (list
   :text (substring-no-properties (popup-x-to-string item))
   :value (copy-tree (popup-item-value item))
   :summary (popup-item-summary item)
   :symbol (popup-item-symbol item)
   :sublist (copy-tree (popup-item-sublist item))
   :document (popup-item-documentation item)
   :faces
   (and (stringp item)
        (popup-test-property-runs item 'face))))

(defun popup-test-rendered-lines (popup)
  (cl-loop
   for overlay across (popup-overlays popup)
   for content =
   (or (overlay-get overlay 'display)
       (overlay-get overlay 'after-string))
   collect
   (and content
        (list
         :text (substring-no-properties content)
         :faces (popup-test-property-runs content 'face)
         :mouse-faces
         (popup-test-property-runs content 'mouse-face)))))

(defun popup-test-navigation-state (popup operation)
  (list operation
        :cursor (popup-cursor popup)
        :scroll-top (popup-scroll-top popup)
        :selected (popup-selected-item popup)
        :selected-line (popup-selected-line popup)))
"##;

fn popup_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(POPUP_MELPA_PIN, "popup.el")
        .expect("prepare pinned Popup source below ./tmp")
        .with_prelude(POPUP_TEST_PRELUDE)
        .with_timeout(POPUP_TEST_TIMEOUT)
}

fn tooltip_content_fills_tabs_paragraphs_and_wide_characters_by_display_width() -> ParityBatchCase {
    let elisp_form = r##"
(let ((samples
       (list
        (list :name 'release-note
              :result
              (popup-fill-string
               "Deploy\torders and invoices after validation."
               16 nil nil nil))
        (list :name 'multilingual
              :result
              (popup-fill-string
               "状态正常，准备发布。Next batch is ready."
               nil 12 nil nil))
        (list :name 'paragraphs
              :result
              (popup-fill-string
               "First paragraph wraps cleanly.\n\nSecond paragraph stays separate."
               14 nil 'left nil)))))
  (mapcar
   (lambda (sample)
     (let ((result (plist-get sample :result)))
       (list
        :name (plist-get sample :name)
        :width (car result)
        :rows
        (mapcar
         (lambda (row)
           (list row :columns (string-width row)))
         (cdr result)))))
   samples))
"##;
    let expect = expect![[
        r####"OK ((:name release-note :width 14 :rows (("Deploy  orders" :columns 14) ("and invoices" :columns 12) ("after" :columns 5) ("validation." :columns 11))) (:name multilingual :width 12 :rows (("状态正常，准" :columns 12) ("备发布。Next" :columns 12) (" batch is re" :columns 12) ("ady." :columns 4))) (:name paragraphs :width 14 :rows (("First" :columns 5) ("paragraph" :columns 9) ("wraps cleanly." :columns 14) ("" :columns 0) ("Second" :columns 6) ("paragraph" :columns 9) ("stays" :columns 5) ("separate." :columns 9))))"####
    ]];
    ParityBatchCase::value(
        "tooltip_content_fills_tabs_paragraphs_and_wide_characters_by_display_width",
        elisp_form,
        expect,
    )
}

fn rich_menu_items_preserve_values_faces_lazy_documentation_and_submenus() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((base
        (propertize "Existing" 'face 'font-lock-keyword-face))
       (preserved
        (popup-item-propertize
         base
         'face nil
         'value 'existing-value
         'summary nil))
       (deploy
        (popup-make-item
         "Deploy staging"
         :value '(:environment staging :safe t)
         :face 'font-lock-function-name-face
         :selection-face 'highlight
         :document
         (lambda (value)
           (format "Run deployment for %s"
                   (plist-get value :environment)))
         :summary "safe"
         :symbol "D"
         :sublist '("Preview" "Confirm"))))
  (list
   :preserved
   (list
    :item (popup-test-normalize-item preserved)
    :face (get-text-property 0 'face preserved))
   :deploy (popup-test-normalize-item deploy)
   :value-or-self
   (list
    (copy-tree (popup-item-value-or-self deploy))
    (popup-item-value-or-self "plain")
    (popup-item-value-or-self 42))))
"##;
    let expect = expect![[
        r####"OK (:preserved (:item (:text "Existing" :value existing-value :summary nil :symbol nil :sublist nil :document nil :faces ((0 8 font-lock-keyword-face))) :face font-lock-keyword-face) :deploy (:text "Deploy staging" :value (:environment staging :safe t) :summary "safe" :symbol "D" :sublist ("Preview" "Confirm") :document "Run deployment for staging" :faces nil) :value-or-self ((:environment staging :safe t) "plain" 42))"####
    ]];
    ParityBatchCase::value(
        "rich_menu_items_preserve_values_faces_lazy_documentation_and_submenus",
        elisp_form,
        expect,
    )
}

fn completion_rows_balance_truncation_summary_symbol_and_margins() -> ParityBatchCase {
    let elisp_form = r##"
(let ((popup (make-popup :width 18)))
  (mapcar
   (lambda (spec)
     (let* ((line
             (popup-create-line-string
              popup
              (plist-get spec :text)
              :margin-left "| "
              :margin-right " |"
              :symbol (plist-get spec :symbol)
              :summary (plist-get spec :summary)
              :summary-face 'font-lock-comment-face)))
       (list
        :input spec
        :line (substring-no-properties line)
        :columns (string-width line)
        :summary-faces
        (popup-test-property-runs
         line 'face))))
   '((:text "deploy-production-service"
      :summary "safe" :symbol " D")
     (:text "状态检查" :summary "ready" :symbol " ✓")
     (:text "rollback" :summary "" :symbol " R"))))
"##;
    let expect = expect![[
        r####"OK ((:input (:text "deploy-production-service" :summary "safe" :symbol " D") :line "| deploy-production- D |" :columns 24 :summary-faces nil) (:input (:text "状态检查" :summary "ready" :symbol " ✓") :line "| 状态检查     ready ✓ |" :columns 24 :summary-faces ((11 16 font-lock-comment-face))) (:input (:text "rollback" :summary "" :symbol " R") :line "| rollback           R |" :columns 24 :summary-faces nil))"####
    ]];
    ParityBatchCase::value(
        "completion_rows_balance_truncation_summary_symbol_and_margins",
        elisp_form,
        expect,
    )
}

fn incremental_filtering_handles_literal_patterns_values_and_match_faces() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((items
        (list
         (popup-make-item
          "deploy.prod" :value 'production :summary "remote")
         (popup-make-item
          "deploy-staging" :value 'staging :summary "remote")
         "open deploy.prod logs"
         'deploy.prod
         2026))
       (literal-dot
        (popup-isearch-filter-list "deploy.prod" items))
       (staging
        (popup-isearch-filter-list "staging" items)))
  (list
   :literal-dot
   (mapcar #'popup-test-normalize-item literal-dot)
   :staging
   (mapcar #'popup-test-normalize-item staging)
   :source-values
   (mapcar #'popup-item-value items)))
"##;
    let expect = expect![[
        r####"OK (:literal-dot ((:text "deploy.prod" :value production :summary "remote" :symbol nil :sublist nil :document nil :faces ((0 11 popup-isearch-match))) (:text "open deploy.prod logs" :value nil :summary nil :symbol nil :sublist nil :document nil :faces ((5 16 popup-isearch-match))) (:text "deploy.prod" :value deploy.prod :summary nil :symbol nil :sublist nil :document nil :faces ((0 11 popup-isearch-match)))) :staging ((:text "deploy-staging" :value staging :summary "remote" :symbol nil :sublist nil :document nil :faces ((7 14 popup-isearch-match)))) :source-values (production staging nil nil nil))"####
    ]];
    ParityBatchCase::value(
        "incremental_filtering_handles_literal_patterns_values_and_match_faces",
        elisp_form,
        expect,
    )
}

fn overlay_menu_renders_selection_scrollbar_hide_redraw_and_delete_lifecycle() -> ParityBatchCase {
    let elisp_form = r##"
(popup-test-with-visible-buffer
 (lambda ()
   (insert "command> ")
   (let ((popup-scroll-bar-foreground-char "F")
         (popup-scroll-bar-background-char "B")
         (menu
          (popup-menu*
           (list
            (popup-make-item
             "Deploy" :value 'deploy :summary "safe" :symbol "D")
            (popup-make-item
             "Preview" :value 'preview :summary "dry-run" :symbol "P")
            (popup-make-item
             "Rollback" :value 'rollback :summary "urgent" :symbol "R")
            (popup-make-item
             "Logs" :value 'logs :summary "tail" :symbol "L"))
           :nowait t
           :width 16
           :height 3
           :margin t
           :scroll-bar t
           :symbol t
           :initial-index 1)))
     (unwind-protect
         (let ((drawn
                (list
                 :selected
                 (popup-test-normalize-item
                  (popup-selected-item menu))
                 :cursor (popup-cursor menu)
                 :scroll-top (popup-scroll-top menu)
                 :current-height (popup-current-height menu)
                 :lines (popup-test-rendered-lines menu))))
           (popup-hide menu)
           (let ((hidden
                  (list :hidden (popup-hidden-p menu)
                        :lines (popup-test-rendered-lines menu))))
             (popup-draw menu)
             (let ((redrawn
                    (list :hidden (popup-hidden-p menu)
                          :lines (popup-test-rendered-lines menu))))
               (popup-delete menu)
               (list
                :drawn drawn
                :hidden hidden
                :redrawn redrawn
                :deleted
                (list :live (popup-live-p menu)
                      :overlays (popup-overlays menu)
                      :instances
                      (memq menu popup-instances))))))
       (popup-delete menu)))))
"##;
    let expect = expect![[
        r####"OK (:drawn (:selected (:text "Preview" :value preview :summary "dry-run" :symbol "P" :sublist nil :document nil :faces nil) :cursor 1 :scroll-top 0 :current-height 3 :lines ((:text "         Deploy      safe D " :faces ((8 21 popup-menu-face) (21 25 popup-menu-summary-face) (25 27 popup-menu-face) (27 28 popup-scroll-bar-foreground-face)) :mouse-faces ((8 27 popup-menu-mouse-face))) (:text "         Preview  dry-run P " :faces ((8 27 popup-menu-selection-face) (27 28 popup-scroll-bar-foreground-face)) :mouse-faces ((8 27 popup-menu-mouse-face))) (:text "         Rollback  urgent R " :faces ((8 19 popup-menu-face) (19 25 popup-menu-summary-face) (25 27 popup-menu-face) (27 28 popup-scroll-bar-background-face)) :mouse-faces ((8 27 popup-menu-mouse-face))))) :hidden (:hidden t :lines (nil nil nil)) :redrawn (:hidden nil :lines ((:text "         Deploy      safe DF" :faces ((8 21 popup-menu-face) (21 25 popup-menu-summary-face) (25 27 popup-menu-face)) :mouse-faces ((8 27 popup-menu-mouse-face))) (:text "         Preview  dry-run PF" :faces ((8 27 popup-menu-selection-face)) :mouse-faces ((8 27 popup-menu-mouse-face))) (:text "         Rollback  urgent RB" :faces ((8 19 popup-menu-face) (19 25 popup-menu-summary-face) (25 27 popup-menu-face)) :mouse-faces ((8 27 popup-menu-mouse-face))))) :deleted (:live nil :overlays nil :instances nil))"####
    ]];
    ParityBatchCase::value(
        "overlay_menu_renders_selection_scrollbar_hide_redraw_and_delete_lifecycle",
        elisp_form,
        expect,
    )
}

fn menu_navigation_wraps_pages_and_clamps_explicit_scrolling() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((items '("one" "two" "three" "four" "five" "six" "seven"))
       (popup
        (make-popup
         :height 3
         :direction 1
         :offset 0
         :cursor 0
         :scroll-top 0
       :list items))
       states)
  (cl-letf (((symbol-function 'popup-draw)
             (lambda (_popup) nil)))
    (push (popup-test-navigation-state popup 'initial) states)
    (popup-next popup)
    (popup-next popup)
    (push (popup-test-navigation-state popup 'two-next) states)
    (popup-next popup)
    (push (popup-test-navigation-state popup 'page-edge) states)
    (popup-page-next popup)
    (push (popup-test-navigation-state popup 'page-next) states)
    (popup-previous popup)
    (push (popup-test-navigation-state popup 'previous) states)
    (popup-scroll-up popup 2)
    (push (popup-test-navigation-state popup 'scroll-up) states)
    (popup-scroll-down popup 99)
    (push
     (popup-test-navigation-state popup 'scroll-down-clamped)
     states)
    (popup-next popup)
    (push
     (popup-test-navigation-state popup 'next-in-last-page)
     states)
    (popup-next popup)
    (push (popup-test-navigation-state popup 'wrapped) states))
  (nreverse states))
"##;
    let expect = expect![[
        r####"OK ((initial :cursor 0 :scroll-top 0 :selected "one" :selected-line 0) (two-next :cursor 2 :scroll-top 0 :selected "three" :selected-line 2) (page-edge :cursor 3 :scroll-top 1 :selected "four" :selected-line 2) (page-next :cursor 5 :scroll-top 3 :selected "six" :selected-line 2) (previous :cursor 4 :scroll-top 3 :selected "five" :selected-line 1) (scroll-up :cursor 1 :scroll-top 1 :selected "two" :selected-line 0) (scroll-down-clamped :cursor 4 :scroll-top 4 :selected "five" :selected-line 0) (next-in-last-page :cursor 5 :scroll-top 4 :selected "six" :selected-line 1) (wrapped :cursor 6 :scroll-top 4 :selected "seven" :selected-line 2))"####
    ]];
    ParityBatchCase::value(
        "menu_navigation_wraps_pages_and_clamps_explicit_scrolling",
        elisp_form,
        expect,
    )
}

fn cascade_menu_converts_only_submenus_and_preserves_forwarded_options() -> ParityBatchCase {
    let elisp_form = r##"
(let (captured-list captured-options)
  (cl-letf (((symbol-function 'popup-menu*)
             (lambda (list &rest options)
               (setq captured-list list
                     captured-options options)
               'captured-menu)))
    (let ((result
           (popup-cascade-menu
            '(("Deploy"
               "Staging"
               "Production")
              "Preview"
              ("Maintenance"
               "Enable"
               "Disable"))
            :height 7
            :margin t
            :nowait t)))
      (list
       :result result
       :items
       (mapcar #'popup-test-normalize-item captured-list)
       :options captured-options))))
"##;
    let expect = expect![[
        r####"OK (:result captured-menu :items ((:text "Deploy" :value nil :summary nil :symbol ">" :sublist ("Staging" "Production") :document nil :faces nil) (:text "Preview" :value nil :summary nil :symbol nil :sublist nil :document nil :faces nil) (:text "Maintenance" :value nil :summary nil :symbol ">" :sublist ("Enable" "Disable") :document nil :faces nil)) :options (:symbol t :height 7 :margin t :nowait t))"####
    ]];
    ParityBatchCase::value(
        "cascade_menu_converts_only_submenus_and_preserves_forwarded_options",
        elisp_form,
        expect,
    )
}

#[test]
fn popup_package_batch() {
    let cases = vec![
        tooltip_content_fills_tabs_paragraphs_and_wide_characters_by_display_width(),
        rich_menu_items_preserve_values_faces_lazy_documentation_and_submenus(),
        completion_rows_balance_truncation_summary_symbol_and_margins(),
        incremental_filtering_handles_literal_patterns_values_and_match_faces(),
        overlay_menu_renders_selection_scrollbar_hide_redraw_and_delete_lifecycle(),
        menu_navigation_wraps_pages_and_clamps_explicit_scrolling(),
        cascade_menu_converts_only_submenus_and_preserves_forwarded_options(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Popup parity test");
    assert_oracle_batch_cases(popup_oracle(), test_name, "popup_parity", &cases);
}
